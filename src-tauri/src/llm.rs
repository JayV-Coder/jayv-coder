//! Os agentes de linha de comando que o JayV sabe chamar e os modelos de cada
//! um. Moram no banco, não no `config.yaml`: a tela Configuração do LLM é a
//! única porta de entrada, e o que ela grava é o que o orquestrador usa.
//!
//! São quatro agentes, e só quatro: Claude Code, Codex, Copilot e Cursor. Nenhum
//! deles recebe argumentos crus. Cada opção da tela tem um conjunto fechado de
//! valores, e é daqui que sai a linha de comando — um argumento digitado errado
//! era o jeito mais fácil de quebrar o agente sem saber por quê.

use crate::config::{ModelConfig, ProviderConfig};
use crate::i18n::Text;
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
pub enum AgentId { Claude, Codex, Copilot, Cursor }

impl AgentId {
    pub const ALL:[AgentId;4]=[AgentId::Claude,AgentId::Codex,AgentId::Copilot,AgentId::Cursor];
    pub fn key(self)->&'static str { match self { Self::Claude=>"claude", Self::Codex=>"codex", Self::Copilot=>"copilot", Self::Cursor=>"cursor" } }
    /// O nome do executável. O instalador do Cursor cria `agent` e
    /// `cursor-agent`; `agent` sozinho é genérico demais para achar no PATH.
    pub fn binary(self)->&'static str { match self { Self::Cursor=>"cursor-agent", other=>other.key() } }
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
    /// `auto` (o Jev escolhe por pedido), `low`, `medium`, `high`, `xhigh`
    /// ou `max`.
    pub effort:String,
    /// Um modelo do catálogo do Claude, ou vazio.
    pub fallback_model:String,
    pub max_budget_usd:Option<f64>,
    pub blocked_tools:Vec<String>,
    pub append_system_prompt:String,
    pub persist_sessions:bool,
    pub safe_mode:bool,
}
impl Default for ClaudeOptions { fn default()->Self { Self{permission_mode:"default".into(),effort:AUTO_EFFORT.into(),fallback_model:String::new(),max_budget_usd:None,blocked_tools:vec![],append_system_prompt:String::new(),persist_sessions:true,safe_mode:false} } }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CodexOptions {
    /// `read-only`, `workspace-write` ou `danger-full-access`.
    pub sandbox:String,
    /// `auto` (o Jev escolhe por pedido), `low`, `medium` ou `high`.
    pub reasoning_effort:String,
    /// Só vale com `workspace-write`: nos outros dois a rede já está decidida.
    pub network_access:bool,
    pub skip_git_repo_check:bool,
}
impl Default for CodexOptions { fn default()->Self { Self{sandbox:"read-only".into(),reasoning_effort:AUTO_EFFORT.into(),network_access:false,skip_git_repo_check:true} } }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CopilotOptions {
    /// `read`, `edits` ou `all`.
    pub tool_access:String,
    pub blocked_tools:Vec<String>,
    pub silent:bool,
}
impl Default for CopilotOptions { fn default()->Self { Self{tool_access:"read".into(),blocked_tools:vec![],silent:true} } }

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CursorOptions {
    /// `default`, `enabled` ou `disabled`. `default` segue a configuração
    /// do próprio Cursor.
    pub sandbox:String,
    /// Aplica as edições e roda os comandos sem pedir aprovação.
    pub force:bool,
    pub approve_mcps:bool,
}
impl Default for CursorOptions { fn default()->Self { Self{sandbox:"default".into(),force:false,approve_mcps:false} } }

const CLAUDE_PERMISSIONS:[&str;5]=["default","plan","acceptEdits","auto","bypassPermissions"];
/// O esforço que o Jev escolhe a cada pedido, pelo tamanho do que foi pedido.
pub const AUTO_EFFORT:&str="auto";
/// O argumento que vira o esforço escolhido pelo Jev.
pub const EFFORT:&str="{effort}";
/// O lugar da sessão do agente a retomar. Sem sessão, o argumento sai junto
/// da flag que o anuncia, como o `{effort}`.
pub const RESUME:&str="{resume}";
const CLAUDE_EFFORTS:[&str;6]=["auto","low","medium","high","xhigh","max"];
/// A ferramenta do Claude que abre um formulário no terminal interativo.
const INTERACTIVE_ONLY_TOOL:&str="AskUserQuestion";
pub const CLAUDE_TOOLS:[&str;6]=["Bash","Edit","Write","NotebookEdit","WebFetch","WebSearch"];
const CODEX_SANDBOXES:[&str;3]=["read-only","workspace-write","danger-full-access"];
const CODEX_EFFORTS:[&str;4]=["auto","low","medium","high"];

/// `default` era o nome antigo do `auto`: o esforço deixava a cargo do agente,
/// que pensava o máximo que o plano dele permitia em todo pedido.
fn effort_of(stored:&str)->String { if stored=="default" { AUTO_EFFORT.into() } else { stored.into() } }
const COPILOT_ACCESS:[&str;3]=["read","edits","all"];
pub const COPILOT_TOOLS:[&str;4]=["shell","write","shell(git push)","shell(rm)"];
const CURSOR_SANDBOXES:[&str;3]=["default","enabled","disabled"];

/// `field` é a chave do i18n do nome do campo, sem o prefixo
/// `settings.field.`.
fn one_of(field:&str,value:&str,allowed:&[&str])->Result<()> {
    if allowed.contains(&value) { Ok(()) } else { bail!(Text::new("settings.invalidValue").with("field",Text::new(&format!("settings.field.{field}"))).with("value",value)) }
}
fn tools_in(field:&str,tools:&[String],allowed:&[&str])->Result<Vec<String>> {
    let mut seen=Vec::new();
    for tool in tools { one_of(field,tool,allowed)?; if !seen.contains(tool) { seen.push(tool.clone()); } }
    Ok(seen)
}

impl ClaudeOptions {
    fn checked(mut self,models:&HashSet<&str>)->Result<Self> {
        one_of("claude.permissionMode",&self.permission_mode,&CLAUDE_PERMISSIONS)?;
        self.effort=effort_of(&self.effort);
        one_of("claude.effort",&self.effort,&CLAUDE_EFFORTS)?;
        self.fallback_model=self.fallback_model.trim().to_string();
        if !self.fallback_model.is_empty()&&!models.contains(self.fallback_model.as_str()) { bail!(Text::new("settings.claude.fallback")); }
        if let Some(budget)=self.max_budget_usd { if !(budget.is_finite()&&budget>0.0&&budget<=1_000.0) { bail!(Text::new("settings.claude.budget")); } }
        self.blocked_tools=tools_in("claude.blockedTools",&self.blocked_tools,&CLAUDE_TOOLS)?;
        self.append_system_prompt=self.append_system_prompt.trim().to_string();
        if self.append_system_prompt.chars().count()>4_000 { bail!(Text::new("settings.claude.instructions").with("max",4_000u32)); }
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        let mut args=strings(&["--print","--output-format","stream-json","--verbose","--include-partial-messages","--model","{model}"]);
        if self.permission_mode!="default" { args.extend(strings(&["--permission-mode",&self.permission_mode])); }
        let effort=effort_of(&self.effort);
        args.extend(strings(&["--effort",if effort==AUTO_EFFORT {EFFORT} else {&effort}]));
        if !self.fallback_model.is_empty() { args.extend(strings(&["--fallback-model",&self.fallback_model])); }
        if let Some(budget)=self.max_budget_usd { args.extend(["--max-budget-usd".to_string(),format!("{budget:.2}")]); }
        // O `AskUserQuestion` só existe no terminal interativo: no `--print` ele
        // falha, e o Claude despejava as perguntas em texto avisando que "não
        // conseguiu abrir o formulário". Sem ele, o Claude pergunta no fim da
        // resposta, e é de lá que o JayV monta o formulário.
        let blocked=[INTERACTIVE_ONLY_TOOL.to_string()].into_iter().chain(self.blocked_tools.iter().cloned()).collect::<Vec<_>>();
        args.extend(["--disallowed-tools".to_string(),blocked.join(",")]);
        if !self.append_system_prompt.is_empty() { args.extend(["--append-system-prompt".to_string(),self.append_system_prompt.clone()]); }
        // Com as sessões guardadas, o pedido seguinte do mesmo chat retoma a
        // sessão do anterior: o agente já leu o que leu e não explora tudo de
        // novo. Sem elas não há o que retomar.
        if self.persist_sessions { args.extend(["--resume".to_string(),RESUME.to_string()]); } else { args.push("--no-session-persistence".into()); }
        if self.safe_mode { args.push("--safe-mode".into()); }
        args
    }
}

impl CodexOptions {
    fn checked(mut self)->Result<Self> {
        one_of("codex.sandbox",&self.sandbox,&CODEX_SANDBOXES)?;
        self.reasoning_effort=effort_of(&self.reasoning_effort);
        one_of("codex.reasoning",&self.reasoning_effort,&CODEX_EFFORTS)?;
        if self.sandbox!="workspace-write" { self.network_access=false; }
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        // `--json` narra em eventos: a fala do agente, os passos e a conta
        // dos tokens chegam separados, e é dela que sai o uso informado.
        let mut args=strings(&["exec","--json","--model","{model}","--sandbox",&self.sandbox]);
        if self.skip_git_repo_check { args.push("--skip-git-repo-check".into()); }
        let effort=effort_of(&self.reasoning_effort);
        args.extend(["-c".to_string(),format!("model_reasoning_effort=\"{}\"",if effort==AUTO_EFFORT {EFFORT} else {&effort})]);
        if self.network_access { args.extend(strings(&["-c","sandbox_workspace_write.network_access=true"])); }
        // O pedido chega pela entrada padrão.
        args.push("-".into());
        args
    }
}

impl CopilotOptions {
    fn checked(mut self)->Result<Self> {
        one_of("copilot.toolAccess",&self.tool_access,&COPILOT_ACCESS)?;
        self.blocked_tools=tools_in("copilot.blockedTools",&self.blocked_tools,&COPILOT_TOOLS)?;
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        // O Copilot não lê o pedido da entrada padrão: ele vai no `-p`.
        let mut args=strings(&["-p","{prompt}","--model","{model}"]);
        match self.tool_access.as_str() { "edits"=>args.extend(strings(&["--allow-tool","write"])), "all"=>args.push("--allow-all-tools".into()), _=>{} }
        for tool in &self.blocked_tools { args.extend(["--deny-tool".to_string(),tool.clone()]); }
        if self.silent { args.push("--silent".into()); }
        // A conta do fim vai para um arquivo, que o provedor lê e apaga: o
        // `--silent` esconde o resumo da saída, e a saída é a resposta.
        args.extend(strings(&["--usage-output-file","{usage_file}"]));
        args
    }
}

impl CursorOptions {
    fn checked(self)->Result<Self> {
        one_of("cursor.sandbox",&self.sandbox,&CURSOR_SANDBOXES)?;
        Ok(self)
    }
    fn args(&self)->Vec<String> {
        // Em `stream-json` o Cursor narra como o Claude: a fala do assistente
        // num evento, as ferramentas em outro e a conta no `result`. O pedido
        // chega pela entrada padrão.
        let mut args=strings(&["--print","--output-format","stream-json","--model","{model}"]);
        if self.sandbox!="default" { args.extend(strings(&["--sandbox",&self.sandbox])); }
        if self.force { args.push("--force".into()); }
        if self.approve_mcps { args.push("--approve-mcps".into()); }
        args
    }
    fn plan_args(&self)->Vec<String> {
        let mut args=CursorOptions{force:false,..self.clone()}.args();
        args.extend(strings(&["--mode","plan"]));
        args
    }
}

fn strings(items:&[&str])->Vec<String> { items.iter().map(|item|item.to_string()).collect() }

fn parse<T:for<'de> Deserialize<'de>+Default>(options:&Value)->Result<T> {
    if options.is_null() { return Ok(T::default()); }
    serde_json::from_value(options.clone()).map_err(|error|anyhow::Error::new(Text::new("settings.invalidOptions").with("reason",error.to_string())))
}

impl AgentSettings {
    fn fresh(id:AgentId)->Self {
        let options=match id { AgentId::Claude=>serde_json::to_value(ClaudeOptions::default()), AgentId::Codex=>serde_json::to_value(CodexOptions::default()), AgentId::Copilot=>serde_json::to_value(CopilotOptions::default()), AgentId::Cursor=>serde_json::to_value(CursorOptions::default()) }.unwrap_or_default();
        Self{id,enabled:locate(id.binary()).is_some(),command:id.binary().into(),timeout:id.default_timeout(),options}
    }

    /// As opções limpas e tipadas, com o que faltava preenchido pelo padrão.
    fn checked(&self,models:&HashSet<&str>)->Result<Value> {
        Ok(match self.id {
            AgentId::Claude=>serde_json::to_value(parse::<ClaudeOptions>(&self.options)?.checked(models)?)?,
            AgentId::Codex=>serde_json::to_value(parse::<CodexOptions>(&self.options)?.checked()?)?,
            AgentId::Copilot=>serde_json::to_value(parse::<CopilotOptions>(&self.options)?.checked()?)?,
            AgentId::Cursor=>serde_json::to_value(parse::<CursorOptions>(&self.options)?.checked()?)?,
        })
    }

    pub fn args(&self)->Vec<String> {
        match self.id {
            AgentId::Claude=>parse::<ClaudeOptions>(&self.options).unwrap_or_default().args(),
            AgentId::Codex=>parse::<CodexOptions>(&self.options).unwrap_or_default().args(),
            AgentId::Copilot=>parse::<CopilotOptions>(&self.options).unwrap_or_default().args(),
            AgentId::Cursor=>parse::<CursorOptions>(&self.options).unwrap_or_default().args(),
        }
    }

    /// A linha de comando do modo planejamento: as opções do desenvolvedor,
    /// com a escrita desligada. O Claude entra no `--permission-mode plan`, o
    /// Codex no sandbox `read-only` sem rede, o Copilot só lê e o Cursor entra
    /// no `--mode plan`, sem `--force`. No modo build
    /// valem as opções como estão: o Jev nunca dá ao agente mais do que a
    /// configuração deu.
    pub fn plan_args(&self)->Vec<String> {
        match self.id {
            AgentId::Claude=>ClaudeOptions{permission_mode:"plan".into(),..parse::<ClaudeOptions>(&self.options).unwrap_or_default()}.args(),
            AgentId::Codex=>CodexOptions{sandbox:"read-only".into(),network_access:false,..parse::<CodexOptions>(&self.options).unwrap_or_default()}.args(),
            AgentId::Copilot=>CopilotOptions{tool_access:"read".into(),..parse::<CopilotOptions>(&self.options).unwrap_or_default()}.args(),
            AgentId::Cursor=>parse::<CursorOptions>(&self.options).unwrap_or_default().plan_args(),
        }
    }

    /// O mesmo agente sem os modos sem trava, para a política de LLM com
    /// `safe_agents`: o Claude sai do `bypassPermissions`, o Codex do
    /// `danger-full-access` (para `workspace-write`, sem rede), o Copilot do
    /// `all` (para `edits`) e o Cursor perde `--force`, `--approve-mcps` e o
    /// sandbox desligado. O que já tinha trava fica como está.
    pub fn without_unsafe_modes(&self)->Self {
        let options=match self.id {
            AgentId::Claude=>{
                let mut options=parse::<ClaudeOptions>(&self.options).unwrap_or_default();
                if options.permission_mode=="bypassPermissions" { options.permission_mode="default".into(); }
                serde_json::to_value(options)
            }
            AgentId::Codex=>{
                let mut options=parse::<CodexOptions>(&self.options).unwrap_or_default();
                if options.sandbox=="danger-full-access" { options.sandbox="workspace-write".into(); options.network_access=false; }
                serde_json::to_value(options)
            }
            AgentId::Copilot=>{
                let mut options=parse::<CopilotOptions>(&self.options).unwrap_or_default();
                if options.tool_access=="all" { options.tool_access="edits".into(); }
                serde_json::to_value(options)
            }
            AgentId::Cursor=>{
                let mut options=parse::<CursorOptions>(&self.options).unwrap_or_default();
                options.force=false;
                options.approve_mcps=false;
                if options.sandbox=="disabled" { options.sandbox="enabled".into(); }
                serde_json::to_value(options)
            }
        }.unwrap_or_default();
        Self{options,..self.clone()}
    }
}

/// Um modelo que o agente oferece, com os números que a tela preenche sozinha
/// ao escolhê-lo.
#[derive(Debug,Clone,Serialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct KnownModel { pub id:String, pub label:String, pub context_window:usize, pub cost_class:String, pub speed:String }

impl KnownModel {
    /// Custo e velocidade saem do nome: o CLI não os diz. A família pequena é
    /// barata e rápida; a grande, cara e lenta; o resto fica no meio.
    fn named(agent:AgentId,id:&str,label:Option<String>,context_window:Option<usize>)->Self {
        let lower=id.to_ascii_lowercase();
        let has=|words:&[&str]|words.iter().any(|word|lower.contains(word));
        let (cost_class,mut speed)=if has(&["haiku","mini","flash","luna","nano","lite"]) {("low","fast")}
            else if has(&["opus","max","astra","best","-pro"]) {("high","slow")}
            else if has(&["fable"]) {("high","medium")}
            else {("medium","medium")};
        if lower.ends_with("-fast") { speed="fast"; }
        let context=context_window.unwrap_or(if lower.contains("[1m]") {1_000_000} else {match agent { AgentId::Claude=>200_000, AgentId::Codex=>272_000, AgentId::Copilot=>128_000, AgentId::Cursor=>200_000 }});
        Self{id:id.into(),label:label.unwrap_or_else(||id.into()),context_window:context.clamp(CONTEXT_RANGE.0,CONTEXT_RANGE.1),cost_class:cost_class.into(),speed:speed.into()}
    }
}

/// Os modelos que vêm de fábrica, para a máquina em que o agente ainda não
/// respondeu à descoberta. Os apelidos do Claude Code seguem sozinhos a
/// versão mais nova de cada família.
fn starter_ids(agent:AgentId)->&'static [&'static str] {
    match agent {
        AgentId::Claude=>&["sonnet","opus","haiku","fable"],
        AgentId::Codex=>&["gpt-5.5"],
        AgentId::Copilot=>&["claude-sonnet-4.5"],
        AgentId::Cursor=>&["auto"],
    }
}

fn starter_models(agent:AgentId)->Vec<AgentModel> {
    starter_ids(agent).iter().map(|id|fresh_model(agent,&KnownModel::named(agent,id,None,None))).collect()
}

fn fresh_model(agent:AgentId,known:&KnownModel)->AgentModel {
    AgentModel{agent,model:known.id.clone(),enabled:true,capabilities:strings(&CAPABILITIES),cost_class:known.cost_class.clone(),speed:known.speed.clone(),context_window:known.context_window}
}

/// O que o último `/model` de cada agente devolveu nesta execução do app.
static DISCOVERED:std::sync::LazyLock<std::sync::Mutex<HashMap<AgentId,Vec<KnownModel>>>>=std::sync::LazyLock::new(Default::default);

/// O catálogo que a tela oferece: o que o CLI listou e, enquanto ele não
/// respondeu, os modelos já gravados do agente.
pub fn catalog(agent:AgentId,settings:&LlmSettings)->Vec<KnownModel> {
    if let Some(found)=DISCOVERED.lock().ok().and_then(|cache|cache.get(&agent).cloned()) { return found; }
    settings.models.iter().filter(|model|model.agent==agent).map(|model|KnownModel{
        id:model.model.clone(),label:model.model.clone(),context_window:model.context_window,cost_class:model.cost_class.clone(),speed:model.speed.clone(),
    }).collect()
}

/// Os argumentos que fazem o agente listar, sem gastar crédito, os modelos
/// do `/model`. O Claude Code responde ao `/model` no `--print` sem chamar o
/// modelo; o Codex tem o catálogo no `debug models`; o Copilot lista os
/// valores aceitos de `model` na ajuda de configuração; o Cursor tem o
/// `models`.
fn listing_args(agent:AgentId)->&'static [&'static str] {
    match agent {
        AgentId::Claude=>&["--print","/model","--output-format","json","--no-session-persistence"],
        AgentId::Codex=>&["debug","models"],
        AgentId::Copilot=>&["help","config"],
        AgentId::Cursor=>&["models"],
    }
}

/// Pergunta ao CLI quais modelos ele oferece. `None` quando ele não está
/// instalado, não respondeu ou respondeu algo que esta versão não lê.
pub async fn discover(agent:AgentId,command:&str)->Option<Vec<KnownModel>> {
    let path=locate(command)?;
    let (program,lead)=launcher(&path);
    let mut process=tokio::process::Command::new(&program);
    if let Some(search)=agent_path(&path) { process.env("PATH",search); }
    let run=crate::providers::quiet(&mut process).args(lead).args(listing_args(agent)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).output();
    let output=tokio::time::timeout(Duration::from_secs(60),run).await.ok()?.ok()?;
    let text=String::from_utf8_lossy(&output.stdout);
    let found=match agent {
        AgentId::Claude=>parse_claude_listing(&text),
        AgentId::Codex=>parse_codex_listing(&text),
        AgentId::Copilot=>parse_copilot_listing(&text),
        AgentId::Cursor=>parse_cursor_listing(&text),
    };
    if found.is_empty() { eprintln!("[llm] {} listed no models",agent.key()); return None; }
    if let Ok(mut cache)=DISCOVERED.lock() { cache.insert(agent,found.clone()); }
    Some(found)
}

/// `Available: sonnet, opus, …, default, or a full model ID.` — `default` é
/// "o que a conta escolher", não um modelo.
fn parse_claude_listing(text:&str)->Vec<KnownModel> {
    let result=serde_json::from_str::<Value>(text).ok().and_then(|value|value["result"].as_str().map(str::to_string)).unwrap_or_else(||text.to_string());
    let Some(rest)=result.split("Available:").nth(1) else { return vec![] };
    let list=rest.split(" or a full model ID").next().unwrap_or(rest);
    let ids=list.split(',').map(|item|item.trim().trim_end_matches('.').trim()).filter(|id|!id.is_empty()&&*id!="default"&&valid_id(id));
    unique(ids.map(|id|KnownModel::named(AgentId::Claude,id,None,None)))
}

/// O catálogo do Codex, só com os que o `/model` mostra (`visibility: list`).
fn parse_codex_listing(text:&str)->Vec<KnownModel> {
    let Ok(value)=serde_json::from_str::<Value>(text) else { return vec![] };
    let mut models:Vec<&Value>=value["models"].as_array().map(|models|models.iter().filter(|model|model["visibility"].as_str()==Some("list")).collect()).unwrap_or_default();
    models.sort_by_key(|model|model["priority"].as_i64().unwrap_or(i64::MAX));
    unique(models.into_iter().filter_map(|model|{
        let id=model["slug"].as_str().filter(|id|valid_id(id))?;
        Some(KnownModel::named(AgentId::Codex,id,model["display_name"].as_str().map(str::to_string),model["context_window"].as_u64().map(|window|window as usize)))
    }))
}

/// Os valores da chave `model` na ajuda de configuração: uma linha
/// `- "id"` por modelo, até a linha em branco.
fn parse_copilot_listing(text:&str)->Vec<KnownModel> {
    let mut lines=text.lines().skip_while(|line|!line.trim_start().starts_with("`model`"));
    lines.next();
    let ids=lines.map(str::trim).take_while(|line|!line.is_empty()).filter_map(|line|line.strip_prefix("- \"").and_then(|rest|rest.strip_suffix('"'))).filter(|id|valid_id(id));
    unique(ids.map(|id|KnownModel::named(AgentId::Copilot,id,None,None)))
}

/// Uma linha `id - Nome` por modelo, depois do `Available models`. O
/// `(default)` e o `(current)` do fim do nome são da conta, não do modelo.
fn parse_cursor_listing(text:&str)->Vec<KnownModel> {
    let found=text.lines().map(str::trim).filter_map(|line|line.split_once(" - ")).filter(|(id,_)|valid_id(id)).map(|(id,name)|{
        let name=name.trim().trim_end_matches("(default)").trim_end_matches("(current)").trim();
        KnownModel::named(AgentId::Cursor,id,Some(name.to_string()).filter(|name|!name.is_empty()),None)
    });
    unique(found)
}

fn unique(models:impl Iterator<Item=KnownModel>)->Vec<KnownModel> {
    let mut seen=HashSet::new();
    models.filter(|model|seen.insert(model.id.clone())).collect()
}

fn valid_id(id:&str)->bool { !id.is_empty()&&id.chars().all(|char|char.is_ascii_alphanumeric()||"._-:/@[]".contains(char)) }

/// Troca os modelos do agente pela lista do CLI, na ordem dele. O que já
/// estava gravado guarda o que o usuário mudou (ligado, custo, capacidades);
/// o que é novo nasce ligado, para o Jev poder escolhê-lo; o que o CLI deixou
/// de oferecer sai — e sai também do modelo reserva do Claude.
pub fn adopt_listing(settings:&LlmSettings,agent:AgentId,found:&[KnownModel])->LlmSettings {
    let mut models:Vec<AgentModel>=settings.models.iter().filter(|model|model.agent!=agent).cloned().collect();
    models.extend(found.iter().map(|known|settings.models.iter().find(|model|model.agent==agent&&model.model==known.id).cloned().unwrap_or_else(||fresh_model(agent,known))));
    let mut agents=settings.agents.clone();
    if agent==AgentId::Claude {
        for entry in agents.iter_mut().filter(|entry|entry.id==AgentId::Claude) {
            let mut options=parse::<ClaudeOptions>(&entry.options).unwrap_or_default();
            if !options.fallback_model.is_empty()&&!found.iter().any(|known|known.id==options.fallback_model) {
                options.fallback_model.clear();
                entry.options=serde_json::to_value(options).unwrap_or(Value::Null);
            }
        }
    }
    LlmSettings{agents,models}
}

/// Cria as tabelas e, na primeira vez, cadastra os agentes com os modelos
/// de fábrica — a descoberta troca-os pela lista do CLI assim que ele
/// responde. Um agente cujo binário não está no PATH nasce desligado: ligado,
/// ele seria escolhido pelo roteador e falharia no primeiro pedido.
pub fn ensure(connection:&Connection)->Result<()> {
    connection.execute_batch(SCHEMA)?;
    let known:i64=connection.query_row("SELECT COUNT(*) FROM llm_agents",[],|row|row.get(0))?;
    if known==0 {
        let agents=AgentId::ALL.into_iter().map(AgentSettings::fresh).collect();
        let models=AgentId::ALL.into_iter().flat_map(starter_models).collect();
        write(connection,&LlmSettings{agents,models})?;
        return Ok(());
    }
    // Um agente que chegou numa versão nova entra no banco que já existia com
    // os modelos de fábrica: ligado e sem modelo, ele travaria o salvar.
    for id in AgentId::ALL {
        let present:i64=connection.query_row("SELECT COUNT(*) FROM llm_agents WHERE id=?1",[id.key()],|row|row.get(0))?;
        if present>0 { continue; }
        let mut settings=load(connection)?;
        settings.models.extend(starter_models(id));
        write(connection,&settings)?;
    }
    Ok(())
}

/// O esforço gravado como `default` é lido como `auto`, para a tela mostrar a
/// escolha certa.
fn legacy_effort(mut options:Value)->Value {
    for key in ["effort","reasoningEffort"] { if options.get(key).and_then(Value::as_str)==Some("default") { options[key]=Value::from(AUTO_EFFORT); } }
    options
}

pub fn load(connection:&Connection)->Result<LlmSettings> {
    let mut agents=Vec::new();
    {
        let mut statement=connection.prepare("SELECT id,enabled,command,timeout,options FROM llm_agents")?;
        let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?)))?;
        for row in rows {
            let (id,enabled,command,timeout,options)=row?;
            let Ok(id)=AgentId::parse(&id) else { continue };
            agents.push(AgentSettings{id,enabled,command,timeout:timeout.max(0) as u64,options:legacy_effort(serde_json::from_str(&options).unwrap_or(Value::Null))});
        }
    }
    // Um agente que falte no banco volta com o padrão: a tela sempre tem uma
    // aba por agente.
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
        if command.is_empty() { bail!(Text::new("settings.commandRequired").with("agent",label(id))); }
        if command.chars().any(char::is_whitespace) { bail!(Text::new("settings.commandArgs").with("agent",label(id))); }
        if !(TIMEOUT_RANGE.0..=TIMEOUT_RANGE.1).contains(&agent.timeout) { bail!(Text::new("settings.timeout").with("agent",label(id)).with("min",TIMEOUT_RANGE.0).with("max",TIMEOUT_RANGE.1)); }
        let own:HashSet<&str>=settings.models.iter().filter(|model|model.agent==id).map(|model|model.model.trim()).collect();
        let options=agent.checked(&own)?;
        if agent.enabled && !settings.models.iter().any(|model|model.agent==id&&model.enabled) { bail!(Text::new("settings.noActiveModel").with("agent",label(id))); }
        agents.push(AgentSettings{id,enabled:agent.enabled,command,timeout:agent.timeout,options});
    }
    let mut seen=HashSet::new();
    let mut models=Vec::new();
    for model in &settings.models {
        let name=model.model.trim().to_string();
        if name.is_empty() { bail!(Text::new("settings.modelNoId").with("agent",label(model.agent))); }
        if !valid_id(&name) { bail!(Text::new("settings.modelBadId").with("name",&name)); }
        if !seen.insert((model.agent,name.clone())) { bail!(Text::new("settings.modelDuplicate").with("name",&name).with("agent",label(model.agent))); }
        let mut capabilities=Vec::new();
        for capability in &model.capabilities { one_of("model.capability",capability,&CAPABILITIES)?; if !capabilities.contains(capability) { capabilities.push(capability.clone()); } }
        if capabilities.is_empty() { bail!(Text::new("settings.modelNoCapability").with("name",&name)); }
        one_of("model.cost",&model.cost_class,&COSTS)?;
        one_of("model.speed",&model.speed,&SPEEDS)?;
        if !(CONTEXT_RANGE.0..=CONTEXT_RANGE.1).contains(&model.context_window) { bail!(Text::new("settings.contextWindow").with("name",&name).with("min",CONTEXT_RANGE.0).with("max",CONTEXT_RANGE.1)); }
        models.push(AgentModel{model:name,capabilities,..model.clone()});
    }
    if !agents.iter().any(|agent|agent.enabled) { bail!(Text::new("settings.noAgent")); }
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
        enabled:agent.enabled,kind:"cli".into(),command:Some(agent.command.clone()),timeout:agent.timeout,args:agent.args(),plan_args:agent.plan_args(),..ProviderConfig::default()
    })).collect();
    let models=settings.models.iter().map(|model|(model_key(model),ModelConfig{
        enabled:model.enabled,provider:model.agent.key().into(),model:model.model.clone(),capabilities:model.capabilities.clone(),
        cost_class:model.cost_class.clone(),speed:model.speed.clone(),context_window:model.context_window,
    })).collect();
    (providers,models)
}

fn label(agent:AgentId)->&'static str { match agent { AgentId::Claude=>"Claude Code", AgentId::Codex=>"Codex", AgentId::Copilot=>"Copilot", AgentId::Cursor=>"Cursor" } }

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
        let has_extension=base.extension().is_some_and(|extension|extensions.iter().any(|known|known.trim_start_matches('.').eq_ignore_ascii_case(&extension.to_string_lossy())));
        // No Windows o arquivo sem extensão ao lado do `copilot.cmd` é o script
        // de shell que o npm deixa para o Git Bash: o `CreateProcess` não o abre.
        // Lá, só vale o nome com uma extensão do `PATHEXT`.
        if has_extension||extensions.is_empty() { return vec![base]; }
        extensions.iter().filter(|extension|!extension.is_empty()).map(|extension|PathBuf::from(format!("{}{extension}",base.display()))).collect::<Vec<_>>()
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

/// As pastas dos instaladores: o script nativo (`~/.local/bin`, ou
/// `/usr/local/bin` como root), o npm global — com o `prefix` do usuário e o
/// Node de cada gerenciador de versões —, o bun e o Homebrew do macOS e do
/// Linux.
pub(crate) fn install_dirs()->Vec<PathBuf> {
    let mut dirs=Vec::new();
    if let Some(prefix)=npm_prefix() { dirs.push(if cfg!(windows){prefix}else{prefix.join("bin")}); }
    if let Some(home)=dirs::home_dir() {
        dirs.extend([home.join(".local").join("bin"),home.join(".npm-global").join("bin"),home.join(".bun").join("bin"),home.join(".claude").join("local"),home.join(".volta").join("bin")]);
        // nvm e fnm guardam um Node por versão; a mais nova vem primeiro.
        dirs.extend(versions(&home.join(".nvm").join("versions").join("node"),&["bin"]));
        let fnm=env::var_os("FNM_DIR").map(PathBuf::from).unwrap_or_else(||dirs::data_dir().unwrap_or_else(||home.join(".local").join("share")).join("fnm"));
        dirs.extend(versions(&fnm.join("node-versions"),&["installation","bin"]));
        #[cfg(not(windows))] dirs.push(home.join(".linuxbrew").join("bin"));
    }
    #[cfg(windows)] {
        if let Some(appdata)=env::var_os("APPDATA") { dirs.push(PathBuf::from(appdata).join("npm")); }
        if let Some(local)=dirs::data_local_dir() { dirs.push(local.join("Programs").join("claude")); dirs.push(local.join("Microsoft").join("WinGet").join("Links")); }
    }
    #[cfg(not(windows))] { dirs.extend([PathBuf::from("/opt/homebrew/bin"),PathBuf::from("/usr/local/bin"),PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),PathBuf::from("/usr/bin")]); }
    dirs
}

/// As pastas `bin` de cada versão instalada por um gerenciador de Node, da
/// versão mais nova para a mais antiga.
fn versions(root:&Path,inner:&[&str])->Vec<PathBuf> {
    let Ok(entries)=std::fs::read_dir(root) else {return Vec::new()};
    let number=|name:&str|name.trim_start_matches('v').split('.').map(|part|part.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    let mut found:Vec<(Vec<u32>,PathBuf)>=entries.filter_map(Result::ok).map(|entry|{
        let name=entry.file_name().to_string_lossy().to_string();
        (number(&name),inner.iter().fold(entry.path(),|path,part|path.join(part)))
    }).filter(|(_,path)|path.is_dir()).collect();
    found.sort_by(|a,b|b.0.cmp(&a.0));
    found.into_iter().map(|(_,path)|path).collect()
}

/// O `prefix` do npm global, quando o usuário o mudou: pela variável de
/// ambiente ou pelo `~/.npmrc`.
fn npm_prefix()->Option<PathBuf> {
    if let Some(prefix)=env::var_os("NPM_CONFIG_PREFIX").or_else(||env::var_os("npm_config_prefix")) { return Some(PathBuf::from(prefix)); }
    let home=dirs::home_dir()?;
    let npmrc=std::fs::read_to_string(home.join(".npmrc")).ok()?;
    let value=npmrc.lines().find_map(|line|line.trim().strip_prefix("prefix").map(str::trim).and_then(|rest|rest.strip_prefix('=')).map(str::trim))?;
    let value=value.trim_matches('"');
    Some(match value.strip_prefix("~/") { Some(rest)=>home.join(rest), None=>PathBuf::from(value) })
}

/// Como abrir o executável achado. Quase sempre é ele mesmo; o atalho `.cmd`
/// que o npm cria no Windows vira o Node rodando o script do pacote, porque o
/// Windows não deixa um `.cmd` receber argumento com quebra de linha — e o
/// Copilot recebe o pedido inteiro no `-p`.
pub fn launcher(program:&Path)->(PathBuf,Vec<String>) {
    let shim=program.extension().is_some_and(|extension|extension.eq_ignore_ascii_case("cmd")||extension.eq_ignore_ascii_case("bat"));
    if shim {
        if let Some(launch)=std::fs::read_to_string(program).ok().and_then(|text|npm_shim(program,&text)) { return launch; }
    }
    (program.to_path_buf(),Vec::new())
}

/// O alvo de um atalho do `cmd-shim` do npm: `"%dp0%\node_modules\…\x.js" %*`.
fn npm_shim(program:&Path,text:&str)->Option<(PathBuf,Vec<String>)> {
    let dir=program.parent()?;
    let start=text.rfind("\"%dp0%\\")?;
    let rest=&text[start+7..];
    let end=rest.find('"')?;
    let target=rest[..end].split(['\\','/']).fold(dir.to_path_buf(),|path,part|path.join(part));
    if !target.is_file() { return None; }
    let script=target.extension().is_some_and(|extension|["js","cjs","mjs"].iter().any(|known|extension.eq_ignore_ascii_case(known)));
    if !script { return Some((target,Vec::new())); }
    let node=[dir.join("node.exe"),dir.join("node")].into_iter().find(|path|path.is_file()).or_else(||locate("node"))?;
    Some((node,vec![target.display().to_string()]))
}

/// O PATH do agente: a pasta dele primeiro, depois as dos instaladores e, por
/// fim, o PATH do app. Um agente do npm é um script `#!/usr/bin/env node`, e o
/// app aberto pelo menu não enxerga o Node do nvm.
pub fn agent_path(program:&Path)->Option<std::ffi::OsString> {
    let mut dirs:Vec<PathBuf>=program.parent().map(Path::to_path_buf).into_iter().collect();
    dirs.extend(install_dirs());
    dirs.extend(env::var_os("PATH").map(|path|env::split_paths(&path).collect::<Vec<_>>()).unwrap_or_default());
    let mut seen=HashSet::new();
    dirs.retain(|dir|seen.insert(dir.clone()));
    env::join_paths(dirs).ok()
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
    // O mesmo arranque do pedido: se a versão responde aqui, o pedido também abre.
    let (program,lead)=launcher(&path);
    let mut command=tokio::process::Command::new(&program);
    if let Some(search)=agent_path(&path) { command.env("PATH",search); }
    let run=crate::providers::quiet(&mut command).args(lead).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output();
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

    /// Antes de o CLI responder, cada agente tem os modelos de fábrica, todos
    /// ligados — os apelidos do Claude seguem a versão nova sozinhos.
    #[test] fn the_database_starts_with_the_starter_models() {
        let loaded=load(&memory()).expect("leitura");
        assert_eq!(loaded.agents.iter().map(|agent|agent.id).collect::<Vec<_>>(),AgentId::ALL);
        let of=|id:AgentId|loaded.models.iter().filter(|model|model.agent==id).map(|model|model.model.as_str()).collect::<Vec<_>>();
        assert_eq!(of(AgentId::Claude),["sonnet","opus","haiku","fable"]);
        assert_eq!(of(AgentId::Codex).len(),1);
        assert_eq!(of(AgentId::Copilot).len(),1);
        assert_eq!(of(AgentId::Cursor),["auto"]);
        assert!(loaded.models.iter().all(|model|model.enabled));
    }

    #[test] fn claude_lists_its_models_through_the_model_command() {
        let output=json!({"type":"result","result":"Current model: `Opus`\nUsage: /model <name>. Available: sonnet, opus, haiku, best, sonnet[1m], opusplan, default, or a full model ID."}).to_string();
        let found=parse_claude_listing(&output);
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["sonnet","opus","haiku","best","sonnet[1m]","opusplan"],"`default` não é um modelo");
        assert_eq!(found[4].context_window,1_000_000);
        assert_eq!((found[2].cost_class.as_str(),found[2].speed.as_str()),("low","fast"));
    }

    #[test] fn codex_lists_only_what_its_model_picker_shows() {
        let output=json!({"models":[
            {"slug":"model-b","display_name":"Model B","visibility":"list","priority":2,"context_window":400_000},
            {"slug":"internal","display_name":"Internal","visibility":"hide","priority":0},
            {"slug":"model-a-mini","display_name":"Model A mini","visibility":"list","priority":1},
        ]}).to_string();
        let found=parse_codex_listing(&output);
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["model-a-mini","model-b"]);
        assert_eq!(found[1].label,"Model B");
        assert_eq!(found[1].context_window,400_000);
        assert!(parse_codex_listing("not json").is_empty());
    }

    #[test] fn copilot_lists_the_values_of_its_model_setting() {
        let help="`logLevel`: log level.\n\n`model`: AI model to use; change it with /model.\n  - \"model-one\"\n  - \"model-two-fast\"\n\n`contextTier`: tier.\n  - \"default\"\n";
        let found=parse_copilot_listing(help);
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["model-one","model-two-fast"]);
        assert_eq!(found[1].speed,"fast");
    }

    #[test] fn cursor_lists_one_model_per_line() {
        let output="Available models\n\nauto - Auto (default)\nmodel-one - Model One\nmodel-two-fast - Model Two Fast (current)\n\nTip: use --model <id> (or /model <id> in interactive mode) to switch.\n";
        let found=parse_cursor_listing(output);
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["auto","model-one","model-two-fast"],"a dica do fim não é modelo");
        assert_eq!(found[0].label,"Auto");
        assert_eq!(found[2].label,"Model Two Fast");
        assert_eq!(found[2].speed,"fast");
    }

    /// O pedido do Cursor vai pela entrada padrão; o modo planejamento tira o
    /// `--force` e entra no `--mode plan`.
    #[test] fn cursor_builds_its_command_line_from_the_options() {
        let cursor=agent(AgentId::Cursor,json!({"sandbox":"enabled","force":true,"approveMcps":true}));
        let args=cursor.args();
        assert!(!args.iter().any(|arg|arg=="{prompt}"),"o pedido chega pela entrada padrão");
        assert!(args.windows(2).any(|pair|pair==["--output-format","stream-json"]));
        assert!(args.windows(2).any(|pair|pair==["--sandbox","enabled"]));
        assert!(args.iter().any(|arg|arg=="--force")&&args.iter().any(|arg|arg=="--approve-mcps"));
        let plan=cursor.plan_args();
        assert!(!plan.iter().any(|arg|arg=="--force"));
        assert!(plan.windows(2).any(|pair|pair==["--mode","plan"]));
        let plain=agent(AgentId::Cursor,Value::Null).args();
        assert!(!plain.iter().any(|arg|arg=="--sandbox"||arg=="--force"),"o padrão segue a configuração do próprio Cursor");
        assert!(agent(AgentId::Cursor,json!({"sandbox":"maybe"})).checked(&HashSet::new()).is_err());
    }

    /// Quem já tinha o banco antes do Cursor existir ganha a aba dele com o
    /// modelo de fábrica, sem perder o que já estava gravado.
    #[test] fn a_new_agent_joins_an_existing_database() {
        let connection=memory();
        connection.execute("DELETE FROM llm_models WHERE agent='cursor'",[]).expect("modelos");
        connection.execute("DELETE FROM llm_agents WHERE id='cursor'",[]).expect("agente");
        connection.execute("UPDATE llm_agents SET timeout=900 WHERE id='codex'",[]).expect("ajuste");
        ensure(&connection).expect("de novo");
        let loaded=load(&connection).expect("leitura");
        assert!(loaded.models.iter().any(|model|model.agent==AgentId::Cursor&&model.model=="auto"));
        assert_eq!(loaded.agents.iter().find(|agent|agent.id==AgentId::Codex).map(|agent|agent.timeout),Some(900));
    }

    /// A lista do CLI manda: o novo nasce ligado, o gravado guarda o que o
    /// usuário mudou, e o que sumiu sai — inclusive do modelo reserva.
    #[test] fn the_cli_listing_replaces_the_agent_models() {
        let mut current=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"haiku"})),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]);
        current.models.iter_mut().filter(|model|model.model=="opus").for_each(|model|model.enabled=false);
        let listing=[KnownModel::named(AgentId::Claude,"opus",None,None),KnownModel::named(AgentId::Claude,"sonnet[1m]",None,None)];
        let adopted=adopt_listing(&current,AgentId::Claude,&listing);
        let claude:Vec<&AgentModel>=adopted.models.iter().filter(|model|model.agent==AgentId::Claude).collect();
        assert_eq!(claude.iter().map(|model|model.model.as_str()).collect::<Vec<_>>(),["opus","sonnet[1m]"]);
        assert!(!claude[0].enabled,"a escolha do usuário fica");
        assert!(claude[1].enabled,"o modelo novo nasce ligado");
        assert_eq!(adopted.models.iter().filter(|model|model.agent!=AgentId::Claude).count(),current.models.iter().filter(|model|model.agent!=AgentId::Claude).count());
        let options:ClaudeOptions=serde_json::from_value(adopted.agents[0].options.clone()).expect("opções");
        assert_eq!(options.fallback_model,"");
        assert!(validate(&adopted).is_ok());
    }

    use std::fs;

    fn runnable_file(path:&Path) {
        fs::write(path,"").expect("arquivo");
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(path,fs::Permissions::from_mode(0o755)).expect("permissão"); }
    }

    /// No Windows o npm instala `claude.cmd` e o instalador nativo põe
    /// `claude.exe` em `~/.local/bin`, que o app aberto pelo menu nem sempre
    /// tem no PATH. Procurar só `claude`, só no PATH, dava "não encontrado".
    #[test] fn finds_the_agent_by_extension_and_in_install_folders() {
        let path_dir=tempfile::tempdir().expect("PATH");
        let install_dir=tempfile::tempdir().expect("instalação");
        runnable_file(&path_dir.path().join("codex.cmd"));
        runnable_file(&install_dir.path().join("claude.exe"));
        let dirs=vec![path_dir.path().to_path_buf(),install_dir.path().to_path_buf()];
        let windows=[".exe".to_string(),".cmd".to_string()];
        assert_eq!(search("codex",&dirs,&windows),Some(path_dir.path().join("codex.cmd")));
        assert_eq!(search("claude",&dirs,&windows),Some(install_dir.path().join("claude.exe")));
        assert_eq!(search("copilot",&dirs,&windows),None);
        // O npm deixa `copilot` (shell, para o Git Bash) ao lado do `copilot.cmd`.
        runnable_file(&path_dir.path().join("copilot"));
        runnable_file(&path_dir.path().join("copilot.cmd"));
        assert_eq!(search("copilot",&dirs,&windows),Some(path_dir.path().join("copilot.cmd")),"no Windows o atalho sem extensão nunca é o escolhido");
        assert_eq!(search("codex.cmd",&dirs,&windows),Some(path_dir.path().join("codex.cmd")),"quem já escreveu a extensão não ganha outra");
        let full=install_dir.path().join("claude");
        assert_eq!(search(&full.display().to_string(),&[],&windows),Some(install_dir.path().join("claude.exe")),"caminho completo sem extensão também vale");
    }

    #[test] fn save_then_load_returns_the_same() {
        let mut connection=memory();
        let wanted=settings(vec![agent(AgentId::Claude,json!({"effort":"high","blockedTools":["Bash","Bash"]})),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]);
        let saved=save(&mut connection,&wanted).expect("salvar");
        let loaded=load(&connection).expect("ler");
        assert_eq!(loaded.models,saved.models);
        let claude:ClaudeOptions=serde_json::from_value(loaded.agents[0].options.clone()).expect("opções");
        assert_eq!(claude.effort,"high");
        assert_eq!(claude.blocked_tools,["Bash"],"a ferramenta repetida vira uma só");
    }

    #[test] fn a_value_outside_the_list_never_reaches_the_database() {
        let wrong=settings(vec![agent(AgentId::Codex,json!({"sandbox":"tudo-liberado"}))]);
        assert!(validate(&wrong).unwrap_err().to_string().contains("sandbox"));
        let timeout=settings(vec![AgentSettings{timeout:5,..agent(AgentId::Claude,Value::Null)}]);
        assert!(validate(&timeout).is_err());
        let spaced=settings(vec![AgentSettings{command:"claude --print".into(),..agent(AgentId::Claude,Value::Null)}]);
        assert!(validate(&spaced).is_err());
    }

    #[test] fn an_enabled_agent_without_an_active_model_is_refused() {
        let mut lonely=settings(vec![agent(AgentId::Claude,Value::Null)]);
        lonely.models.retain(|model|model.agent!=AgentId::Claude);
        assert!(validate(&lonely).unwrap_err().to_string().contains("Claude Code"));
    }

    #[test] fn the_fallback_model_must_belong_to_the_same_agent() {
        let wrong=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"gpt-5"}))]);
        assert!(validate(&wrong).is_err());
        let right=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"sonnet"}))]);
        assert!(validate(&right).is_ok());
    }

    #[test] fn claude_always_speaks_stream_json() {
        let args=agent(AgentId::Claude,json!({"permissionMode":"plan","maxBudgetUsd":2.5})).args();
        for fixed in ["--print","stream-json","--include-partial-messages","{model}"] { assert!(args.iter().any(|arg|arg==fixed),"falta {fixed}"); }
        assert!(!args.iter().any(|arg|arg=="--no-session-persistence"),"guardar sessões é o padrão");
        let forgetful=agent(AgentId::Claude,json!({"persistSessions":false})).args();
        assert!(forgetful.iter().any(|arg|arg=="--no-session-persistence"));
        assert!(args.windows(2).any(|pair|pair==["--resume",RESUME]),"com sessões guardadas, o pedido seguinte retoma a do chat");
        assert!(!forgetful.iter().any(|arg|arg=="--resume"),"sem sessão guardada não há o que retomar");
        assert!(args.windows(2).any(|pair|pair==["--permission-mode","plan"]));
        assert!(args.windows(2).any(|pair|pair==["--max-budget-usd","2.50"]));
    }

    #[test] fn codex_reads_stdin_and_only_opens_the_network_when_it_can_write() {
        let args=agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true})).args();
        let cleaned:CodexOptions=serde_json::from_value(validate(&settings(vec![agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true}))])).expect("válido").agents[1].options.clone()).expect("opções");
        assert!(!cleaned.network_access);
        assert_eq!(args.last().map(String::as_str),Some("-"));
        assert!(args.windows(2).any(|pair|pair==["--sandbox","read-only"]));
    }

    #[test] fn copilot_receives_the_request_as_an_argument() {
        let args=agent(AgentId::Copilot,json!({"toolAccess":"all","blockedTools":["shell(rm)"]})).args();
        assert!(args.windows(2).any(|pair|pair==["-p","{prompt}"]));
        assert!(args.iter().any(|arg|arg=="--allow-all-tools"));
        assert!(args.windows(2).any(|pair|pair==["--deny-tool","shell(rm)"]));
    }

    /// O modo planejamento só tira a escrita: o resto do que foi configurado
    /// segue igual, e o que já era somente leitura continua somente leitura.
    /// Sem esforço escolhido, quem decide é o Jev a cada pedido; o `default`
    /// antigo vale como `auto`.
    #[test] fn an_automatic_effort_leaves_the_choice_to_jev() {
        for options in [Value::Null,json!({"effort":"default"})] {
            assert!(agent(AgentId::Claude,options).args().windows(2).any(|pair|pair==["--effort",EFFORT]));
        }
        assert!(agent(AgentId::Codex,json!({"reasoningEffort":"default"})).args().iter().any(|arg|arg=="model_reasoning_effort=\"{effort}\""));
        assert!(agent(AgentId::Claude,json!({"effort":"high"})).args().windows(2).any(|pair|pair==["--effort","high"]));
        assert_eq!(legacy_effort(json!({"effort":"default"}))["effort"],AUTO_EFFORT);
    }

    #[test] fn planning_runs_every_agent_read_only() {
        let claude=agent(AgentId::Claude,json!({"permissionMode":"bypassPermissions","effort":"high"})).plan_args();
        assert!(claude.windows(2).any(|pair|pair==["--permission-mode","plan"]));
        assert!(!claude.iter().any(|arg|arg=="bypassPermissions"));
        assert!(claude.windows(2).any(|pair|pair==["--effort","high"]));

        let codex=agent(AgentId::Codex,json!({"sandbox":"workspace-write","networkAccess":true})).plan_args();
        assert!(codex.windows(2).any(|pair|pair==["--sandbox","read-only"]));
        assert!(!codex.iter().any(|arg|arg.contains("network_access")));

        let copilot=agent(AgentId::Copilot,json!({"toolAccess":"all","blockedTools":["shell(rm)"]})).plan_args();
        assert!(!copilot.iter().any(|arg|arg=="--allow-all-tools"||arg=="--allow-tool"));
        assert!(copilot.windows(2).any(|pair|pair==["--deny-tool","shell(rm)"]));

        let (providers,_)=to_config(&settings(vec![agent(AgentId::Codex,json!({"sandbox":"workspace-write"}))]));
        let codex=&providers["codex"];
        assert!(codex.args.windows(2).any(|pair|pair==["--sandbox","workspace-write"]),"o build usa o que foi configurado");
        assert!(codex.for_planning().args.windows(2).any(|pair|pair==["--sandbox","read-only"]));
    }

    #[test] fn the_orchestrator_configuration_comes_from_the_database() {
        let (providers,models)=to_config(&settings(vec![agent(AgentId::Claude,Value::Null),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]));
        assert_eq!(providers.len(),3);
        assert!(providers.values().all(|provider|provider.kind=="cli"&&provider.local.is_none()));
        assert_eq!(models["claude/sonnet"].provider,"claude");
    }

    /// O atalho `.cmd` do npm não recebe quebra de linha no Windows; o pedido
    /// do Copilot vai inteiro no `-p`. O Node roda o script do pacote direto.
    #[test] fn the_npm_shim_becomes_node_with_the_package_script() {
        let dir=tempfile::tempdir().expect("pasta");
        let package=dir.path().join("node_modules").join("@github").join("copilot");
        std::fs::create_dir_all(&package).expect("pacote");
        std::fs::write(package.join("npm-loader.js"),"").expect("script");
        std::fs::write(dir.path().join("node.exe"),"").expect("node");
        let shim=dir.path().join("copilot.cmd");
        std::fs::write(&shim,"@ECHO off\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@github\\copilot\\npm-loader.js\" %*\r\n").expect("atalho");
        let (program,lead)=launcher(&shim);
        assert_eq!(program,dir.path().join("node.exe"));
        assert_eq!(lead,vec![package.join("npm-loader.js").display().to_string()]);
        let native=dir.path().join("copilot.exe");
        assert_eq!(launcher(&native),(native.clone(),vec![]),"binário nativo abre como está");
    }

    #[test] fn the_newest_nvm_node_comes_first() {
        let root=tempfile::tempdir().expect("nvm");
        for version in ["v18.20.0","v24.2.0","v22.11.0"] { std::fs::create_dir_all(root.path().join(version).join("bin")).expect("versão"); }
        let found=versions(root.path(),&["bin"]);
        assert_eq!(found.first(),Some(&root.path().join("v24.2.0").join("bin")));
        assert_eq!(found.len(),3);
    }

    #[test] fn the_agent_path_starts_with_its_own_folder() {
        let path=agent_path(Path::new("/opt/tools/bin/copilot")).expect("path");
        assert_eq!(env::split_paths(&path).next(),Some(PathBuf::from("/opt/tools/bin")));
    }
}
