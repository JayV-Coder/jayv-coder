//! Os agentes de linha de comando que o JayV sabe chamar e os modelos de cada
//! um. Moram no banco, não no `config.yaml`: a tela Configuração do LLM é a
//! única porta de entrada, e o que ela grava é o que o orquestrador usa.
//!
//! No MVP são três agentes, e só três: Claude Code, Codex e Copilot. Nenhum
//! deles recebe argumentos crus. Cada opção da tela tem um conjunto fechado de
//! valores, e é daqui que sai a linha de comando — um argumento digitado errado
//! era o jeito mais fácil de quebrar o agente sem saber por quê.

use crate::config::{ModelConfig, ProviderConfig};
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::{HashMap, HashSet}, env, path::{Path, PathBuf}, process::Stdio, time::Duration};

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS llm_agents (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL DEFAULT 1,
  command TEXT NOT NULL,
  timeout INTEGER NOT NULL,
  options TEXT NOT NULL DEFAULT '{}',
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS llm_models (
  agent TEXT NOT NULL REFERENCES llm_agents(id) ON DELETE CASCADE,
  model TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  capabilities TEXT NOT NULL DEFAULT '[]',
  cost_class TEXT NOT NULL,
  speed TEXT NOT NULL,
  context_window INTEGER NOT NULL,
  position INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (agent, model)
);";

/// O prazo de silêncio aceito, em segundos. Menos que meio minuto derruba um
/// agente que ainda está pensando; mais que uma hora esconde um travado.
pub const TIMEOUT_RANGE:(u64,u64)=(30,3_600);
pub const CONTEXT_RANGE:(usize,usize)=(8_000,2_000_000);
pub const CAPABILITIES:[&str;4]=["chat","code","reasoning","tools"];
pub const COSTS:[&str;4]=["free","low","medium","high"];
pub const SPEEDS:[&str;3]=["fast","medium","slow"];

#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum AgentId { Claude, Codex, Copilot }

impl AgentId {
    pub const ALL:[AgentId;3]=[AgentId::Claude,AgentId::Codex,AgentId::Copilot];
    pub fn key(self)->&'static str { match self { Self::Claude=>"claude", Self::Codex=>"codex", Self::Copilot=>"copilot" } }
    pub fn binary(self)->&'static str { self.key() }
    fn parse(key:&str)->Result<Self> { Self::ALL.into_iter().find(|agent|agent.key()==key).ok_or_else(||anyhow!("agente desconhecido: `{key}`")) }
    fn default_timeout(self)->u64 { 300 }
}

/// Um agente como a tela o edita. `options` é o JSON das opções próprias dele;
/// ao salvar, ele passa pela struct tipada do agente e volta limpo.
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct AgentSettings {
    pub id:AgentId,
    pub enabled:bool,
    pub command:String,
    pub timeout:u64,
    #[serde(default)] pub options:Value,
}

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct AgentModel {
    pub agent:AgentId,
    pub model:String,
    pub enabled:bool,
    pub capabilities:Vec<String>,
    pub cost_class:String,
    pub speed:String,
    pub context_window:usize,
}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct LlmSettings { pub agents:Vec<AgentSettings>, pub models:Vec<AgentModel> }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct ClaudeOptions {
    /// `default`, `plan`, `acceptEdits`, `auto` ou `bypassPermissions`.
    pub permission_mode:String,
    /// `default`, `low`, `medium`, `high`, `xhigh` ou `max`.
    pub effort:String,
    /// Um modelo do catálogo do Claude, ou vazio.
    pub fallback_model:String,
    pub max_budget_usd:Option<f64>,
    pub blocked_tools:Vec<String>,
    pub append_system_prompt:String,
    pub persist_sessions:bool,
    pub safe_mode:bool,
}
impl Default for ClaudeOptions { fn default()->Self { Self{permission_mode:"default".into(),effort:"default".into(),fallback_model:String::new(),max_budget_usd:None,blocked_tools:vec![],append_system_prompt:String::new(),persist_sessions:true,safe_mode:false} } }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CodexOptions {
    /// `read-only`, `workspace-write` ou `danger-full-access`.
    pub sandbox:String,
    /// `default`, `low`, `medium` ou `high`.
    pub reasoning_effort:String,
    /// Só vale com `workspace-write`: nos outros dois a rede já está decidida.
    pub network_access:bool,
    pub skip_git_repo_check:bool,
}
impl Default for CodexOptions { fn default()->Self { Self{sandbox:"read-only".into(),reasoning_effort:"default".into(),network_access:false,skip_git_repo_check:true} } }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CopilotOptions {
    /// `read`, `edits` ou `all`.
    pub tool_access:String,
    pub blocked_tools:Vec<String>,
    pub silent:bool,
}
impl Default for CopilotOptions { fn default()->Self { Self{tool_access:"read".into(),blocked_tools:vec![],silent:true} } }

const CLAUDE_PERMISSIONS:[&str;5]=["default","plan","acceptEdits","auto","bypassPermissions"];
const CLAUDE_EFFORTS:[&str;6]=["default","low","medium","high","xhigh","max"];
pub const CLAUDE_TOOLS:[&str;6]=["Bash","Edit","Write","NotebookEdit","WebFetch","WebSearch"];
const CODEX_SANDBOXES:[&str;3]=["read-only","workspace-write","danger-full-access"];
const CODEX_EFFORTS:[&str;4]=["default","low","medium","high"];
const COPILOT_ACCESS:[&str;3]=["read","edits","all"];
pub const COPILOT_TOOLS:[&str;4]=["shell","write","shell(git push)","shell(rm)"];

fn one_of(field:&str,value:&str,allowed:&[&str])->Result<()> {
    if allowed.contains(&value) { Ok(()) } else { bail!("valor inválido para {field}: `{value}`") }
}
fn tools_in(field:&str,tools:&[String],allowed:&[&str])->Result<Vec<String>> {
    let mut seen=Vec::new();
    for tool in tools { one_of(field,tool,allowed)?; if !seen.contains(tool) { seen.push(tool.clone()); } }
    Ok(seen)
}

impl ClaudeOptions {
    fn checked(mut self,models:&HashSet<&str>)->Result<Self> {
        one_of("o modo de permissão do Claude Code",&self.permission_mode,&CLAUDE_PERMISSIONS)?;
        one_of("o esforço do Claude Code",&self.effort,&CLAUDE_EFFORTS)?;
        self.fallback_model=self.fallback_model.trim().to_string();
        if !self.fallback_model.is_empty()&&!models.contains(self.fallback_model.as_str()) { bail!("o modelo reserva do Claude Code precisa ser um dos modelos cadastrados nele"); }
        if let Some(budget)=self.max_budget_usd { if !(budget.is_finite()&&budget>0.0&&budget<=1_000.0) { bail!("o limite de gasto do Claude Code precisa ficar entre US$ 0,01 e US$ 1.000"); } }
        self.blocked_tools=tools_in("as ferramentas bloqueadas do Claude Code",&self.blocked_tools,&CLAUDE_TOOLS)?;
        self.append_system_prompt=self.append_system_prompt.trim().to_string();
        if self.append_system_prompt.chars().count()>4_000 { bail!("as instruções extras do Claude Code passam de 4.000 caracteres"); }
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        let mut args=strings(&["--print","--output-format","stream-json","--verbose","--include-partial-messages","--model","{model}"]);
        if self.permission_mode!="default" { args.extend(strings(&["--permission-mode",&self.permission_mode])); }
        if self.effort!="default" { args.extend(strings(&["--effort",&self.effort])); }
        if !self.fallback_model.is_empty() { args.extend(strings(&["--fallback-model",&self.fallback_model])); }
        if let Some(budget)=self.max_budget_usd { args.extend(["--max-budget-usd".to_string(),format!("{budget:.2}")]); }
        if !self.blocked_tools.is_empty() { args.extend(["--disallowed-tools".to_string(),self.blocked_tools.join(",")]); }
        if !self.append_system_prompt.is_empty() { args.extend(["--append-system-prompt".to_string(),self.append_system_prompt.clone()]); }
        if !self.persist_sessions { args.push("--no-session-persistence".into()); }
        if self.safe_mode { args.push("--safe-mode".into()); }
        args
    }
}

impl CodexOptions {
    fn checked(mut self)->Result<Self> {
        one_of("o sandbox do Codex",&self.sandbox,&CODEX_SANDBOXES)?;
        one_of("o raciocínio do Codex",&self.reasoning_effort,&CODEX_EFFORTS)?;
        if self.sandbox!="workspace-write" { self.network_access=false; }
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        let mut args=strings(&["exec","--model","{model}","--sandbox",&self.sandbox]);
        if self.skip_git_repo_check { args.push("--skip-git-repo-check".into()); }
        if self.reasoning_effort!="default" { args.extend(["-c".to_string(),format!("model_reasoning_effort=\"{}\"",self.reasoning_effort)]); }
        if self.network_access { args.extend(strings(&["-c","sandbox_workspace_write.network_access=true"])); }
        // O pedido chega pela entrada padrão.
        args.push("-".into());
        args
    }
}

impl CopilotOptions {
    fn checked(mut self)->Result<Self> {
        one_of("o acesso a ferramentas do Copilot",&self.tool_access,&COPILOT_ACCESS)?;
        self.blocked_tools=tools_in("as ferramentas bloqueadas do Copilot",&self.blocked_tools,&COPILOT_TOOLS)?;
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        // O Copilot não lê o pedido da entrada padrão: ele vai no `-p`.
        let mut args=strings(&["-p","{prompt}","--model","{model}"]);
        match self.tool_access.as_str() { "edits"=>args.extend(strings(&["--allow-tool","write"])), "all"=>args.push("--allow-all-tools".into()), _=>{} }
        for tool in &self.blocked_tools { args.extend(["--deny-tool".to_string(),tool.clone()]); }
        if self.silent { args.push("--silent".into()); }
        args
    }
}

fn strings(items:&[&str])->Vec<String> { items.iter().map(|item|item.to_string()).collect() }

fn parse<T:for<'de> Deserialize<'de>+Default>(options:&Value)->Result<T> {
    if options.is_null() { return Ok(T::default()); }
    serde_json::from_value(options.clone()).map_err(|error|anyhow!("opções inválidas: {error}"))
}

impl AgentSettings {
    fn fresh(id:AgentId)->Self {
        let options=match id { AgentId::Claude=>serde_json::to_value(ClaudeOptions::default()), AgentId::Codex=>serde_json::to_value(CodexOptions::default()), AgentId::Copilot=>serde_json::to_value(CopilotOptions::default()) }.unwrap_or_default();
        Self{id,enabled:locate(id.binary()).is_some(),command:id.binary().into(),timeout:id.default_timeout(),options}
    }

    /// As opções limpas e tipadas, com o que faltava preenchido pelo padrão.
    fn checked(&self,models:&HashSet<&str>)->Result<Value> {
        Ok(match self.id {
            AgentId::Claude=>serde_json::to_value(parse::<ClaudeOptions>(&self.options)?.checked(models)?)?,
            AgentId::Codex=>serde_json::to_value(parse::<CodexOptions>(&self.options)?.checked()?)?,
            AgentId::Copilot=>serde_json::to_value(parse::<CopilotOptions>(&self.options)?.checked()?)?,
        })
    }

    pub fn args(&self)->Vec<String> {
        match self.id {
            AgentId::Claude=>parse::<ClaudeOptions>(&self.options).unwrap_or_default().args(),
            AgentId::Codex=>parse::<CodexOptions>(&self.options).unwrap_or_default().args(),
            AgentId::Copilot=>parse::<CopilotOptions>(&self.options).unwrap_or_default().args(),
        }
    }
}

/// Um modelo conhecido de um agente, com os números que a tela preenche
/// sozinha ao escolhê-lo.
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct KnownModel { pub id:&'static str, pub label:&'static str, pub context_window:usize, pub cost_class:&'static str, pub speed:&'static str }

const fn known(id:&'static str,label:&'static str,context_window:usize,cost_class:&'static str,speed:&'static str)->KnownModel { KnownModel{id,label,context_window,cost_class,speed} }

/// O catálogo de cada agente. Os apelidos do Claude Code apontam sempre para a
/// versão mais nova da família; os nomes completos fixam uma versão.
pub fn catalog(agent:AgentId)->Vec<KnownModel> {
    match agent {
        AgentId::Claude=>vec![
            known("sonnet","Sonnet (mais recente)",200_000,"medium","medium"),
            known("opus","Opus (mais recente)",200_000,"high","slow"),
            known("haiku","Haiku (mais recente)",200_000,"low","fast"),
            known("fable","Fable (mais recente)",200_000,"high","medium"),
            known("claude-opus-5-5","Claude Opus 5.5",200_000,"high","slow"),
            known("claude-sonnet-5","Claude Sonnet 5",200_000,"medium","medium"),
            known("claude-fable-5-1","Claude Fable 5.1",200_000,"high","medium"),
            known("claude-haiku-4-5-20251001","Claude Haiku 4.5",200_000,"low","fast"),
        ],
        AgentId::Codex=>vec![
            known("gpt-5-codex","GPT-5 Codex",272_000,"high","medium"),
            known("gpt-5.1-codex","GPT-5.1 Codex",272_000,"high","medium"),
            known("gpt-5.1-codex-max","GPT-5.1 Codex Max",272_000,"high","slow"),
            known("gpt-5.1-codex-mini","GPT-5.1 Codex Mini",272_000,"low","fast"),
            known("gpt-5.2-codex","GPT-5.2 Codex",272_000,"high","medium"),
            known("gpt-5","GPT-5",272_000,"high","medium"),
        ],
        AgentId::Copilot=>vec![
            known("claude-sonnet-4.5","Claude Sonnet 4.5",128_000,"medium","medium"),
            known("claude-opus-4.5","Claude Opus 4.5",128_000,"high","slow"),
            known("claude-haiku-4.5","Claude Haiku 4.5",128_000,"low","fast"),
            known("gpt-5","GPT-5",128_000,"high","medium"),
            known("gpt-5.1","GPT-5.1",128_000,"high","medium"),
            known("gpt-5.1-codex","GPT-5.1 Codex",128_000,"high","medium"),
            known("gpt-5-mini","GPT-5 mini",128_000,"low","fast"),
            known("gemini-3-pro-preview","Gemini 3 Pro (preview)",128_000,"medium","medium"),
        ],
    }
}

/// Os modelos mais recentes de cada agente, com que ele nasce. Os apelidos do
/// Claude Code já seguem a versão nova de cada família; no Codex são a
/// geração mais nova e as variantes max e mini mais novas.
const LATEST_CLAUDE:[&str;4]=["sonnet","opus","haiku","fable"];
const LATEST_CODEX:[&str;3]=["gpt-5.2-codex","gpt-5.1-codex-max","gpt-5.1-codex-mini"];

fn starter_models(agent:AgentId)->Vec<AgentModel> {
    let known=catalog(agent);
    let wanted:Vec<&str>=match agent {
        AgentId::Claude=>LATEST_CLAUDE.to_vec(),
        AgentId::Codex=>LATEST_CODEX.to_vec(),
        AgentId::Copilot=>vec![known[0].id],
    };
    wanted.into_iter().filter_map(|id|known.iter().find(|model|model.id==id)).map(|model|AgentModel{
        agent,model:model.id.into(),enabled:true,capabilities:strings(&CAPABILITIES),cost_class:model.cost_class.into(),speed:model.speed.into(),context_window:model.context_window,
    }).collect()
}

/// Cria as tabelas e, na primeira vez, cadastra os três agentes com os modelos
/// mais recentes de cada um. Um agente cujo binário não está no PATH nasce desligado: ligado, ele
/// seria escolhido pelo roteador e falharia no primeiro pedido.
pub fn ensure(connection:&Connection)->Result<()> {
    connection.execute_batch(SCHEMA)?;
    let known:i64=connection.query_row("SELECT COUNT(*) FROM llm_agents",[],|row|row.get(0))?;
    if known==0 {
        let agents=AgentId::ALL.into_iter().map(AgentSettings::fresh).collect();
        let models=AgentId::ALL.into_iter().flat_map(starter_models).collect();
        write(connection,&LlmSettings{agents,models})?;
    }
    Ok(())
}

pub fn load(connection:&Connection)->Result<LlmSettings> {
    let mut agents=Vec::new();
    {
        let mut statement=connection.prepare("SELECT id,enabled,command,timeout,options FROM llm_agents")?;
        let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?)))?;
        for row in rows {
            let (id,enabled,command,timeout,options)=row?;
            let Ok(id)=AgentId::parse(&id) else { continue };
            agents.push(AgentSettings{id,enabled,command,timeout:timeout.max(0) as u64,options:serde_json::from_str(&options).unwrap_or(Value::Null)});
        }
    }
    // Um agente que falte no banco volta com o padrão: a tela sempre tem as
    // três abas.
    for id in AgentId::ALL { if !agents.iter().any(|agent|agent.id==id) { agents.push(AgentSettings::fresh(id)); } }
    agents.sort_by_key(|agent|AgentId::ALL.iter().position(|id|*id==agent.id));
    let mut models=Vec::new();
    let mut statement=connection.prepare("SELECT agent,model,enabled,capabilities,cost_class,speed,context_window FROM llm_models ORDER BY agent,position,model")?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,i64>(6)?)))?;
    for row in rows {
        let (agent,model,enabled,capabilities,cost_class,speed,context_window)=row?;
        let Ok(agent)=AgentId::parse(&agent) else { continue };
        models.push(AgentModel{agent,model,enabled,capabilities:serde_json::from_str(&capabilities).unwrap_or_default(),cost_class,speed,context_window:context_window.max(0) as usize});
    }
    Ok(LlmSettings{agents,models})
}

/// Confere tudo antes de gravar e devolve a versão limpa. Nada chega ao banco
/// sem passar por aqui.
pub fn validate(settings:&LlmSettings)->Result<LlmSettings> {
    let mut agents=Vec::new();
    for id in AgentId::ALL {
        let agent=settings.agents.iter().find(|agent|agent.id==id).cloned().unwrap_or_else(||AgentSettings::fresh(id));
        let command=agent.command.trim().to_string();
        if command.is_empty() { bail!("informe o comando do {}",label(id)); }
        if command.chars().any(char::is_whitespace) { bail!("o comando do {} é só o nome ou o caminho do executável, sem argumentos",label(id)); }
        if !(TIMEOUT_RANGE.0..=TIMEOUT_RANGE.1).contains(&agent.timeout) { bail!("o tempo de silêncio do {} precisa ficar entre {} e {} segundos",label(id),TIMEOUT_RANGE.0,TIMEOUT_RANGE.1); }
        let own:HashSet<&str>=settings.models.iter().filter(|model|model.agent==id).map(|model|model.model.trim()).collect();
        let options=agent.checked(&own).map_err(|error|anyhow!("{error}"))?;
        if agent.enabled && !settings.models.iter().any(|model|model.agent==id&&model.enabled) { bail!("o {} está ligado mas não tem nenhum modelo ativo",label(id)); }
        agents.push(AgentSettings{id,enabled:agent.enabled,command,timeout:agent.timeout,options});
    }
    let mut seen=HashSet::new();
    let mut models=Vec::new();
    for model in &settings.models {
        let name=model.model.trim().to_string();
        if name.is_empty() { bail!("há um modelo sem identificador no {}",label(model.agent)); }
        if !name.chars().all(|char|char.is_ascii_alphanumeric()||"._-:/@".contains(char)) { bail!("o identificador `{name}` tem caracteres que nenhum agente aceita"); }
        if !seen.insert((model.agent,name.clone())) { bail!("o modelo `{name}` aparece duas vezes no {}",label(model.agent)); }
        let mut capabilities=Vec::new();
        for capability in &model.capabilities { one_of("a capacidade",capability,&CAPABILITIES)?; if !capabilities.contains(capability) { capabilities.push(capability.clone()); } }
        if capabilities.is_empty() { bail!("o modelo `{name}` precisa de ao menos uma capacidade"); }
        one_of("o custo",&model.cost_class,&COSTS)?;
        one_of("a velocidade",&model.speed,&SPEEDS)?;
        if !(CONTEXT_RANGE.0..=CONTEXT_RANGE.1).contains(&model.context_window) { bail!("a janela de contexto de `{name}` precisa ficar entre {} e {} tokens",CONTEXT_RANGE.0,CONTEXT_RANGE.1); }
        models.push(AgentModel{model:name,capabilities,..model.clone()});
    }
    if !agents.iter().any(|agent|agent.enabled) { bail!("ligue ao menos um agente"); }
    Ok(LlmSettings{agents,models})
}

pub fn save(connection:&mut Connection,settings:&LlmSettings)->Result<LlmSettings> {
    let settings=validate(settings)?;
    let transaction=connection.transaction()?;
    write(&transaction,&settings)?;
    transaction.commit()?;
    Ok(settings)
}

fn write(connection:&Connection,settings:&LlmSettings)->Result<()> {
    let now=chrono::Utc::now().to_rfc3339();
    connection.execute("DELETE FROM llm_models",[])?;
    for agent in &settings.agents {
        connection.execute(
            "INSERT INTO llm_agents(id,enabled,command,timeout,options,updated_at) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled,command=excluded.command,timeout=excluded.timeout,options=excluded.options,updated_at=excluded.updated_at",
            params![agent.id.key(),agent.enabled,agent.command,agent.timeout as i64,agent.options.to_string(),now],
        )?;
    }
    for (position,model) in settings.models.iter().enumerate() {
        connection.execute(
            "INSERT INTO llm_models(agent,model,enabled,capabilities,cost_class,speed,context_window,position) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![model.agent.key(),model.model,model.enabled,serde_json::to_string(&model.capabilities)?,model.cost_class,model.speed,model.context_window as i64,position as i64],
        )?;
    }
    Ok(())
}

/// O nome com que o roteador conhece o modelo: único entre os agentes.
pub fn model_key(model:&AgentModel)->String { format!("{}/{}",model.agent.key(),model.model) }

/// Os provedores e modelos no formato que o orquestrador já entende.
pub fn to_config(settings:&LlmSettings)->(HashMap<String,ProviderConfig>,HashMap<String,ModelConfig>) {
    let providers=settings.agents.iter().map(|agent|(agent.id.key().to_string(),ProviderConfig{
        enabled:agent.enabled,kind:"cli".into(),command:Some(agent.command.clone()),timeout:agent.timeout,args:agent.args(),..ProviderConfig::default()
    })).collect();
    let models=settings.models.iter().map(|model|(model_key(model),ModelConfig{
        enabled:model.enabled,provider:model.agent.key().into(),model:model.model.clone(),capabilities:model.capabilities.clone(),
        cost_class:model.cost_class.clone(),speed:model.speed.clone(),context_window:model.context_window,
    })).collect();
    (providers,models)
}

fn label(agent:AgentId)->&'static str { match agent { AgentId::Claude=>"Claude Code", AgentId::Codex=>"Codex", AgentId::Copilot=>"Copilot" } }

/// Onde o executável está, procurando como o shell faria: no PATH e, depois,
/// nas pastas onde os instaladores dos agentes os põem — o app aberto pelo menu
/// não herda o PATH do terminal. No Windows, com as extensões do `PATHEXT`.
pub fn locate(command:&str)->Option<PathBuf> {
    let mut dirs:Vec<PathBuf>=env::var_os("PATH").map(|path|env::split_paths(&path).collect()).unwrap_or_default();
    dirs.extend(install_dirs());
    search(command,&dirs,&extensions())
}

/// A busca em si, sem ler o ambiente. Quem já escreveu a extensão, ou um
/// caminho com pasta, é conferido como está e também com cada extensão.
fn search(command:&str,dirs:&[PathBuf],extensions:&[String])->Option<PathBuf> {
    let command=command.trim();
    if command.is_empty() { return None; }
    let runnable=|path:&Path|path.is_file()&&executable(path);
    let variants=|base:PathBuf|->Vec<PathBuf> {
        let mut found=vec![base.clone()];
        let has_extension=base.extension().is_some_and(|extension|extensions.iter().any(|known|known.trim_start_matches('.').eq_ignore_ascii_case(&extension.to_string_lossy())));
        if !has_extension { found.extend(extensions.iter().filter(|extension|!extension.is_empty()).map(|extension|PathBuf::from(format!("{}{extension}",base.display())))); }
        found
    };
    if command.contains('/')||command.contains('\\') { return variants(PathBuf::from(command)).into_iter().find(|path|runnable(path)); }
    dirs.iter().flat_map(|directory|variants(directory.join(command))).find(|path|runnable(path))
}

#[cfg(windows)]
fn extensions()->Vec<String> {
    let pathext=env::var("PATHEXT").unwrap_or_else(|_|".COM;.EXE;.BAT;.CMD".into());
    pathext.split(';').map(str::trim).filter(|extension|!extension.is_empty()).map(str::to_lowercase).collect()
}
#[cfg(not(windows))]
fn extensions()->Vec<String> { Vec::new() }

/// As pastas dos instaladores: o nativo do Claude Code (`~/.local/bin`), o npm
/// global, o bun e, no macOS, o Homebrew.
fn install_dirs()->Vec<PathBuf> {
    let mut dirs=Vec::new();
    if let Some(home)=dirs::home_dir() {
        dirs.extend([home.join(".local").join("bin"),home.join(".npm-global").join("bin"),home.join(".bun").join("bin"),home.join(".claude").join("local")]);
    }
    #[cfg(windows)] {
        if let Some(appdata)=env::var_os("APPDATA") { dirs.push(PathBuf::from(appdata).join("npm")); }
        if let Some(local)=dirs::data_local_dir() { dirs.push(local.join("Programs").join("claude")); dirs.push(local.join("Microsoft").join("WinGet").join("Links")); }
    }
    #[cfg(not(windows))] { dirs.extend([PathBuf::from("/opt/homebrew/bin"),PathBuf::from("/usr/local/bin")]); }
    dirs
}

#[cfg(unix)] fn executable(path:&Path)->bool { use std::os::unix::fs::PermissionsExt; path.metadata().is_ok_and(|meta|meta.permissions().mode()&0o111!=0) }
#[cfg(not(unix))] fn executable(_:&Path)->bool { true }

/// O que a tela mostra ao conferir um agente: onde ele está e qual versão
/// responde. Sem o binário, a tela avisa antes de o pedido falhar.
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Probe { pub path:Option<String>, pub version:Option<String> }

pub async fn probe(command:&str)->Probe {
    let Some(path)=locate(command) else { return Probe{path:None,version:None} };
    let run=crate::providers::quiet(&mut tokio::process::Command::new(&path)).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output();
    let version=match tokio::time::timeout(Duration::from_secs(10),run).await {
        Ok(Ok(output))=>{
            let text=if output.stdout.is_empty(){output.stderr}else{output.stdout};
            String::from_utf8_lossy(&text).lines().map(str::trim).find(|line|!line.is_empty()).map(str::to_string)
        }
        _=>None,
    };
    Probe{path:Some(path.display().to_string()),version}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn memory()->Connection { let connection=Connection::open_in_memory().expect("banco"); ensure(&connection).expect("tabelas"); connection }
    fn agent(id:AgentId,options:Value)->AgentSettings { AgentSettings{id,enabled:true,command:id.binary().into(),timeout:300,options} }
    fn settings(agents:Vec<AgentSettings>)->LlmSettings { LlmSettings{agents,models:AgentId::ALL.into_iter().flat_map(starter_models).collect()} }

    /// Claude e Codex nascem com todos os modelos mais recentes ligados — os
    /// apelidos do Claude seguem a versão nova sozinhos; o Copilot, com um.
    #[test] fn o_banco_nasce_com_os_modelos_mais_recentes() {
        let loaded=load(&memory()).expect("leitura");
        assert_eq!(loaded.agents.iter().map(|agent|agent.id).collect::<Vec<_>>(),AgentId::ALL);
        let of=|id:AgentId|loaded.models.iter().filter(|model|model.agent==id).map(|model|model.model.as_str()).collect::<Vec<_>>();
        assert_eq!(of(AgentId::Claude),["sonnet","opus","haiku","fable"]);
        assert_eq!(of(AgentId::Codex),["gpt-5.2-codex","gpt-5.1-codex-max","gpt-5.1-codex-mini"]);
        assert_eq!(of(AgentId::Copilot).len(),1);
        assert!(loaded.models.iter().all(|model|model.enabled));
    }

    use std::fs;

    fn runnable_file(path:&Path) {
        fs::write(path,"").expect("arquivo");
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(path,fs::Permissions::from_mode(0o755)).expect("permissão"); }
    }

    /// No Windows o npm instala `claude.cmd` e o instalador nativo põe
    /// `claude.exe` em `~/.local/bin`, que o app aberto pelo menu nem sempre
    /// tem no PATH. Procurar só `claude`, só no PATH, dava "não encontrado".
    #[test] fn acha_o_agente_pela_extensao_e_nas_pastas_de_instalacao() {
        let path_dir=tempfile::tempdir().expect("PATH");
        let install_dir=tempfile::tempdir().expect("instalação");
        runnable_file(&path_dir.path().join("codex.cmd"));
        runnable_file(&install_dir.path().join("claude.exe"));
        let dirs=vec![path_dir.path().to_path_buf(),install_dir.path().to_path_buf()];
        let windows=[".exe".to_string(),".cmd".to_string()];
        assert_eq!(search("codex",&dirs,&windows),Some(path_dir.path().join("codex.cmd")));
        assert_eq!(search("claude",&dirs,&windows),Some(install_dir.path().join("claude.exe")));
        assert_eq!(search("copilot",&dirs,&windows),None);
        assert_eq!(search("codex.cmd",&dirs,&windows),Some(path_dir.path().join("codex.cmd")),"quem já escreveu a extensão não ganha outra");
        let full=install_dir.path().join("claude");
        assert_eq!(search(&full.display().to_string(),&[],&windows),Some(install_dir.path().join("claude.exe")),"caminho completo sem extensão também vale");
    }

    #[test] fn salvar_e_ler_devolve_o_mesmo() {
        let mut connection=memory();
        let wanted=settings(vec![agent(AgentId::Claude,json!({"effort":"high","blockedTools":["Bash","Bash"]})),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]);
        let saved=save(&mut connection,&wanted).expect("salvar");
        let loaded=load(&connection).expect("ler");
        assert_eq!(loaded.models,saved.models);
        let claude:ClaudeOptions=serde_json::from_value(loaded.agents[0].options.clone()).expect("opções");
        assert_eq!(claude.effort,"high");
        assert_eq!(claude.blocked_tools,["Bash"],"a ferramenta repetida vira uma só");
    }

    #[test] fn valor_fora_da_lista_nao_chega_ao_banco() {
        let wrong=settings(vec![agent(AgentId::Codex,json!({"sandbox":"tudo-liberado"}))]);
        assert!(validate(&wrong).unwrap_err().to_string().contains("sandbox"));
        let timeout=settings(vec![AgentSettings{timeout:5,..agent(AgentId::Claude,Value::Null)}]);
        assert!(validate(&timeout).is_err());
        let spaced=settings(vec![AgentSettings{command:"claude --print".into(),..agent(AgentId::Claude,Value::Null)}]);
        assert!(validate(&spaced).is_err());
    }

    #[test] fn agente_ligado_sem_modelo_ativo_e_recusado() {
        let mut lonely=settings(vec![agent(AgentId::Claude,Value::Null)]);
        lonely.models.retain(|model|model.agent!=AgentId::Claude);
        assert!(validate(&lonely).unwrap_err().to_string().contains("Claude Code"));
    }

    #[test] fn o_modelo_reserva_precisa_ser_do_proprio_agente() {
        let wrong=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"gpt-5"}))]);
        assert!(validate(&wrong).is_err());
        let right=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"sonnet"}))]);
        assert!(validate(&right).is_ok());
    }

    #[test] fn o_claude_sempre_fala_em_stream_json() {
        let args=agent(AgentId::Claude,json!({"permissionMode":"plan","maxBudgetUsd":2.5})).args();
        for fixed in ["--print","stream-json","--include-partial-messages","{model}"] { assert!(args.iter().any(|arg|arg==fixed),"falta {fixed}"); }
        assert!(!args.iter().any(|arg|arg=="--no-session-persistence"),"guardar sessões é o padrão");
        let forgetful=agent(AgentId::Claude,json!({"persistSessions":false})).args();
        assert!(forgetful.iter().any(|arg|arg=="--no-session-persistence"));
        assert!(args.windows(2).any(|pair|pair==["--permission-mode","plan"]));
        assert!(args.windows(2).any(|pair|pair==["--max-budget-usd","2.50"]));
    }

    #[test] fn o_codex_le_da_entrada_e_so_abre_a_rede_quando_pode_escrever() {
        let args=agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true})).args();
        let cleaned:CodexOptions=serde_json::from_value(validate(&settings(vec![agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true}))])).expect("válido").agents[1].options.clone()).expect("opções");
        assert!(!cleaned.network_access);
        assert_eq!(args.last().map(String::as_str),Some("-"));
        assert!(args.windows(2).any(|pair|pair==["--sandbox","read-only"]));
    }

    #[test] fn o_copilot_recebe_o_pedido_no_argumento() {
        let args=agent(AgentId::Copilot,json!({"toolAccess":"all","blockedTools":["shell(rm)"]})).args();
        assert!(args.windows(2).any(|pair|pair==["-p","{prompt}"]));
        assert!(args.iter().any(|arg|arg=="--allow-all-tools"));
        assert!(args.windows(2).any(|pair|pair==["--deny-tool","shell(rm)"]));
    }

    #[test] fn a_configuracao_do_orquestrador_sai_do_banco() {
        let (providers,models)=to_config(&settings(vec![agent(AgentId::Claude,Value::Null),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]));
        assert_eq!(providers.len(),3);
        assert!(providers.values().all(|provider|provider.kind=="cli"&&provider.local.is_none()));
        assert_eq!(models["claude/sonnet"].provider,"claude");
    }
}
