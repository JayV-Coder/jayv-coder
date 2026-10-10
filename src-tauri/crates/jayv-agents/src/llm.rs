//! Os mods de integração com LLM que o JayV sabe chamar e os modelos de cada
//! um. Moram no banco, não no `config.yaml`: a tela de configurações é a única
//! porta de entrada, e o que ela grava é o que o orquestrador usa.
//!
//! Cada agente é um mod (`crate::mods`): sete vêm com o app — quatro de linha
//! de comando que editam o projeto (Claude Code, Codex, Copilot e Cursor), o
//! Kilo Code (também de linha de comando) e dois gateways de API (OpenRouter e
//! LiteLLM), que só respondem por texto — e a pessoa cria os seus em
//! Configurações › Mods. Nenhum recebe argumentos crus digitados numa opção:
//! cada opção dos mods do app tem um conjunto fechado de valores, e é do mod
//! que sai a linha de comando — um argumento digitado errado era o jeito mais
//! fácil de quebrar o agente sem saber por quê. O mod criado é a exceção de
//! propósito: a linha dele é a que a pessoa escreveu, conferida
//! (`mods::custom`).

use crate::config::{ModelConfig, ProviderConfig};
use crate::i18n::Text;
use crate::mods::{LlmMod, ModKind};
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{collections::{HashMap, HashSet}, env, path::{Path, PathBuf}, process::Stdio, time::Duration};

pub use crate::mods::claude::{ClaudeOptions, CLAUDE_MECHANISMS, CLAUDE_TOOLS};
pub use crate::mods::codex::{CodexOptions, CODEX_MECHANISMS};
pub use crate::mods::copilot::{CopilotOptions, COPILOT_MECHANISMS, COPILOT_TOOLS};
pub use crate::mods::cursor::CursorOptions;
pub use crate::mods::custom::CustomOptions;
pub use crate::mods::gateway::GatewayOptions;
pub use crate::mods::kilo::KiloOptions;

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
);
-- A chave de API dos gateways e dos mods de API. Só local: não está na lista
-- de tabelas sincronizadas (`TABLES` do `jayv-store`), então nunca sobe para a
-- nuvem.
CREATE TABLE IF NOT EXISTS llm_secrets (
  agent TEXT PRIMARY KEY,
  secret TEXT NOT NULL
);";

/// O prazo de silêncio aceito, em segundos. Menos que meio minuto derruba um
/// agente que ainda está pensando; mais que uma hora esconde um travado.
pub const TIMEOUT_RANGE:(u64,u64)=(30,3_600);
pub const CONTEXT_RANGE:(usize,usize)=(8_000,2_000_000);
pub const CAPABILITIES:[&str;4]=["chat","code","reasoning","tools"];
pub const COSTS:[&str;4]=["free","low","medium","high"];
pub const SPEEDS:[&str;3]=["fast","medium","slow"];

/// O mod de um agente, pelo id: um dos que vêm com o app (`claude`, `codex`…)
/// ou um criado pela pessoa (`mod-<nome>`). Os do app são constantes com o
/// nome de sempre (`AgentId::Claude`), e casam em `match` como antes; o id de
/// um mod criado é guardado uma vez (`intern`) e copiado à vontade, como o dos
/// mods do app.
#[derive(Clone,Copy,PartialEq,Eq,Hash,PartialOrd,Ord)]
pub struct AgentId(&'static str);

#[allow(non_upper_case_globals)]
impl AgentId {
    pub const Claude:AgentId=AgentId("claude");
    pub const Codex:AgentId=AgentId("codex");
    pub const Copilot:AgentId=AgentId("copilot");
    pub const Cursor:AgentId=AgentId("cursor");
    pub const Kilo:AgentId=AgentId("kilo");
    pub const Openrouter:AgentId=AgentId("openrouter");
    pub const Litellm:AgentId=AgentId("litellm");
    /// Os mods que vêm com o app, na ordem das abas. Os criados vêm depois,
    /// pelo nome.
    pub const ALL:[AgentId;7]=[AgentId::Claude,AgentId::Codex,AgentId::Copilot,AgentId::Cursor,AgentId::Kilo,AgentId::Openrouter,AgentId::Litellm];

    pub fn key(self)->&'static str { self.0 }
    /// O mod deste agente.
    pub fn module(self)->&'static dyn LlmMod { crate::mods::of(self) }
    /// Um mod criado pela pessoa (`mod-…`).
    pub fn is_custom(self)->bool { self.0.starts_with(crate::mods::CUSTOM_PREFIX) }
    /// O nome do executável padrão do mod; vazio no mod de API e no criado.
    pub fn binary(self)->&'static str { self.module().binary() }

    /// O id de um mod do app ou de um mod criado com o formato aceito
    /// (`mod-` e de 1 a 32 letras minúsculas, dígitos ou `-`).
    pub fn parse(key:&str)->Result<Self> {
        if let Some(known)=Self::ALL.into_iter().find(|agent|agent.key()==key) { return Ok(known); }
        if valid_custom(key) { if let Some(interned)=intern(key) { return Ok(AgentId(interned)); } }
        Err(anyhow!("unknown agent: `{key}`"))
    }
}

impl std::fmt::Debug for AgentId { fn fmt(&self,formatter:&mut std::fmt::Formatter<'_>)->std::fmt::Result { formatter.write_str(self.0) } }
impl std::fmt::Display for AgentId { fn fmt(&self,formatter:&mut std::fmt::Formatter<'_>)->std::fmt::Result { formatter.write_str(self.0) } }
impl Serialize for AgentId { fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> { serializer.serialize_str(self.0) } }
impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
        let key=String::deserialize(deserializer)?;
        AgentId::parse(&key).map_err(serde::de::Error::custom)
    }
}

/// O id de um mod criado: o prefixo e de 1 a 32 letras minúsculas, dígitos ou
/// `-`, sem `-` nas pontas.
fn valid_custom(key:&str)->bool {
    let Some(slug)=key.strip_prefix(crate::mods::CUSTOM_PREFIX) else { return false };
    (1..=32).contains(&slug.len())&&!slug.starts_with('-')&&!slug.ends_with('-')&&slug.chars().all(|char|char.is_ascii_lowercase()||char.is_ascii_digit()||char=='-')
}

/// Quantos ids de mods criados um processo guarda. Cada um vira texto que vive
/// até o app fechar; o teto segura um banco ou uma tela com ids sem fim.
const INTERN_LIMIT:usize=256;

/// O id guardado uma vez para o processo inteiro.
fn intern(key:&str)->Option<&'static str> {
    static INTERNED:std::sync::LazyLock<std::sync::Mutex<HashSet<&'static str>>>=std::sync::LazyLock::new(Default::default);
    let mut interned=INTERNED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(known)=interned.get(key) { return Some(known); }
    if interned.len()>=INTERN_LIMIT { return None; }
    let leaked:&'static str=Box::leak(key.to_string().into_boxed_str());
    interned.insert(leaked);
    Some(leaked)
}

/// Um agente como a tela o edita. `options` é o JSON das opções próprias dele;
/// ao salvar, ele passa pelas opções tipadas do mod e volta limpo.
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

impl LlmSettings {
    /// Todos os agentes com o que foi liberado para o pedido.
    pub fn with_grants(&self,grants:&Grants)->Self {
        if grants.is_empty() { return self.clone(); }
        Self{agents:self.agents.iter().map(|agent|agent.with_grants(grants)).collect(),models:self.models.clone()}
    }

    /// Todos os agentes sem poder rodar os comandos que a política bloqueia.
    pub fn without_commands(&self,blocked:&[String])->Self {
        if blocked.is_empty() { return self.clone(); }
        Self{agents:self.agents.iter().map(|agent|agent.without_commands(blocked)).collect(),models:self.models.clone()}
    }

    /// O que o pedido precisa em disco antes de abrir os agentes: o bloco do
    /// JayV no `~/.cursor/mcp.json` (o Cursor não recebe MCP pela linha de
    /// comando). Sem servidores, o que o JayV pôs antes sai.
    pub fn sync_mcp_files(&self) {
        let Some(cursor)=self.agents.iter().find(|agent|agent.id==AgentId::Cursor) else { return };
        let servers=parse::<CursorOptions>(&cursor.options).unwrap_or_default().mcp;
        if let Err(error)=crate::mcp::sync_cursor(&servers) { eprintln!("mcp: {error:#}"); }
    }

    /// Todos os agentes com os servidores MCP configurados.
    pub fn with_mcp(&self,servers:&[crate::mcp::McpServer])->Self {
        if servers.is_empty() { return self.clone(); }
        Self{agents:self.agents.iter().map(|agent|agent.with_mcp(servers)).collect(),models:self.models.clone()}
    }

    /// O agente deste id, se está nas configurações.
    pub fn agent(&self,id:AgentId)->Option<&AgentSettings> { self.agents.iter().find(|agent|agent.id==id) }

    /// O nome do agente nas mensagens: o do mod criado sai das opções dele.
    pub fn label_of(&self,id:AgentId)->String { self.agent(id).map_or_else(||label(id),AgentSettings::label) }
}

/// O modo que pergunta antes de cada ferramenta. Sem terminal, ninguém
/// responde, e o que pediria aprovação é negado. O Claude chamava de `default`
/// até a 2.1.200; a linha de comando só o recebe como padrão, sem a flag.
pub(crate) const MANUAL:&str="manual";
/// Quem responde aos pedidos de aprovação do Claude no `--print`: ninguém. O
/// que pediria aprovação é negado na hora, em vez de esperar um anfitrião
/// que não existe. As versões que não conhecem a flag a perdem (`OPTIONAL_FLAGS`).
pub const PERMISSION_PROMPTS:&str="--permission-prompts";
/// O lugar da flag que não guarda a sessão em disco. A chamada de apoio (o
/// plano, a divisão, a revisão, o título) não vira sessão que ninguém vai
/// retomar; o pedido do chat continua guardando. O argumento seguinte é a flag
/// do agente, e os dois somem quando a chamada é do chat.
pub const EPHEMERAL:&str="{ephemeral}";
/// As flags que só as versões mais novas das CLIs conhecem, com quantos
/// valores cada uma leva. Antes de abrir o agente, a ajuda dele diz se a flag
/// existe; se não existir, ela sai da linha de comando em vez de derrubar o
/// pedido com "unknown option".
pub const OPTIONAL_FLAGS:[(&str,usize);2]=[(PERMISSION_PROMPTS,1),("--ephemeral",0)];
/// O esforço que o Jev escolhe a cada pedido, pelo tamanho do que foi pedido.
pub const AUTO_EFFORT:&str="auto";
/// O argumento que vira o esforço escolhido pelo Jev.
pub const EFFORT:&str="{effort}";
/// O lugar da sessão do agente a retomar. Sem sessão, o argumento sai junto
/// da flag que o anuncia, como o `{effort}`.
pub const RESUME:&str="{resume}";
/// O subcomando que retoma uma sessão do Codex (`codex exec … resume <id> -`).
/// Com sessão, vira `resume` e o id; sem, some. As opções do `exec` vêm antes
/// dele: o `resume` não aceita `--sandbox`, e o do `exec` vale para a retomada.
pub const RESUME_THREAD:&str="{resume_thread}";

/// `default` era o nome antigo do `auto`: o esforço deixava a cargo do agente,
/// que pensava o máximo que o plano dele permitia em todo pedido.
pub(crate) fn effort_of(stored:&str)->String { if stored=="default" { AUTO_EFFORT.into() } else { stored.into() } }

/// Os mecanismos que dão ao agente o que fazer além de ler e editar o
/// projeto: buscar na web, abrir páginas, rodar comandos. Cada agente roda sem
/// terminal (`--print`/`exec`), e ninguém responde ao pedido de aprovação
/// dele: o que não vem liberado na linha de comando é negado. Cada mod só
/// mostra os que a CLI dele sabe ligar por flag.
pub const WEB_SEARCH:&str="webSearch";
pub const WEB_FETCH:&str="webFetch";
pub const SHELL:&str="shell";
/// O "Aprovar servidores MCP" como mecanismo que a política da organização
/// bloqueia (`claude/mcp`).
pub const MCP:&str="mcp";
pub const GITHUB_TOOLS:&str="githubTools";

/// Os mecanismos que a CLI do agente sabe ligar. O Cursor não tem flag para
/// nenhum: a busca na web dele é sempre dele, e os comandos só pelo `--force`.
pub fn mechanisms_of(agent:AgentId)->&'static [&'static str] { agent.module().mechanisms() }

/// `field` é a chave do i18n do nome do campo, sem o prefixo
/// `settings.field.`.
pub(crate) fn one_of(field:&str,value:&str,allowed:&[&str])->Result<()> {
    if allowed.contains(&value) { Ok(()) } else { bail!(Text::new("settings.invalidValue").with("field",Text::new(&format!("settings.field.{field}"))).with("value",value)) }
}
pub(crate) fn tools_in(field:&str,tools:&[String],allowed:&[&str])->Result<Vec<String>> {
    let mut seen=Vec::new();
    for tool in tools { one_of(field,tool,allowed)?; if !seen.contains(tool) { seen.push(tool.clone()); } }
    Ok(seen)
}

pub(crate) fn strings(items:&[&str])->Vec<String> { items.iter().map(|item|item.to_string()).collect() }

pub(crate) fn parse<T:for<'de> Deserialize<'de>+Default>(options:&Value)->Result<T> {
    if options.is_null() { return Ok(T::default()); }
    serde_json::from_value(options.clone()).map_err(|error|anyhow::Error::new(Text::new("settings.invalidOptions").with("reason",error.to_string())))
}

/// Os mecanismos com mais um, sem repetir.
pub(crate) fn with_mechanism(mechanisms:&[String],mechanism:&str)->Vec<String> {
    let mut all=mechanisms.to_vec();
    if !all.iter().any(|known|known==mechanism) { all.push(mechanism.into()); }
    all
}

impl AgentSettings {
    /// O agente como nasce: as opções de fábrica do mod e, para o de linha de
    /// comando, ligado só se o executável está no PATH.
    pub(crate) fn fresh(id:AgentId)->Self {
        let module=id.module();
        let options=module.fresh_options();
        let cli=module.kind(&options)==ModKind::Cli;
        Self{id,enabled:cli&&!module.binary().is_empty()&&locate(module.binary()).is_some(),command:module.binary().into(),timeout:300,options}
    }

    pub fn module(&self)->&'static dyn LlmMod { self.id.module() }
    /// O nome do agente: o do mod criado sai das opções dele.
    pub fn label(&self)->String { self.module().label(&self.options) }
    /// O mod fala por HTTP (endereço e chave), sem programa para abrir.
    pub fn is_api(&self)->bool { self.module().kind(&self.options)==ModKind::Api }
    /// O mod edita o projeto, ou só responde por texto.
    pub fn edits_project(&self)->bool { self.module().edits_project(&self.options) }

    /// As opções limpas e tipadas, com o que faltava preenchido pelo padrão.
    fn checked(&self,models:&HashSet<&str>)->Result<Value> { self.module().checked(&self.options,models) }

    pub fn args(&self)->Vec<String> { self.module().args(&self.options) }

    /// A linha de comando do modo desenvolvimento. O agente roda sem terminal,
    /// e ninguém responde ao pedido de aprovação dele: o que só se faz com
    /// aprovação é negado. Então o que na configuração só lê sobe para o
    /// degrau que escreve no projeto e nada além — o Claude para o
    /// `acceptEdits`, o Codex para o `workspace-write` (com a rede que a
    /// configuração deu) e o Copilot para o `edits`. O que já escrevia fica
    /// como está, e o Cursor também: o único degrau dele que escreve é o
    /// `--force`, que roda comandos sem perguntar. Quem não quer escrita usa o
    /// modo planejamento, e a regra de escrita do Jev em `deny` manda o pedido
    /// para ele.
    pub fn build_args(&self)->Vec<String> { self.module().build_args(&self.options) }

    /// A linha de comando do modo planejamento: as opções do desenvolvedor,
    /// com a escrita desligada — e sem rodar comandos sem pergunta, que
    /// também escrevem. O Claude entra no `--permission-mode plan`, o Codex no
    /// sandbox `read-only` sem rede, o Copilot só lê, o Cursor entra no
    /// `--mode plan`, sem `--force`, e o mod criado usa a linha de
    /// planejamento que a pessoa escreveu.
    pub fn plan_args(&self)->Vec<String> { self.module().plan_args(&self.options) }

    /// O mesmo agente sem os modos sem trava, para a política de LLM com
    /// `safe_agents`: o Claude sai do `bypassPermissions`, o Codex do
    /// `danger-full-access` (para `workspace-write`, sem rede), o Copilot do
    /// `all` (para `edits`), o Cursor perde `--force`, `--approve-mcps` e o
    /// sandbox desligado, e o mod criado roda com a linha que não escreve. O
    /// Claude e o Copilot perdem também os comandos sem pergunta (o mecanismo
    /// `shell`). O que já tinha trava fica como está.
    pub fn without_unsafe_modes(&self)->Self { Self{options:self.module().without_unsafe_modes(&self.options),..self.clone()} }

    /// O mesmo agente com os servidores MCP configurados que ele recebe, se o
    /// "Aprovar servidores MCP" dele está ligado: as ferramentas deles rodam
    /// sem pergunta, então sem a aprovação nenhum servidor chega. A política
    /// com `safe_agents`, aplicada depois, os tira.
    pub fn with_mcp(&self,servers:&[crate::mcp::McpServer])->Self {
        let mine=servers.iter().filter(|server|server.serves(self.id)).cloned().collect::<Vec<_>>();
        if mine.is_empty() { return self.clone(); }
        match self.module().with_mcp(&self.options,mine) { Some(options)=>Self{options,..self.clone()}, None=>self.clone() }
    }

    /// O mesmo agente com o que o desenvolvedor liberou para um pedido só
    /// (`Grants`). Só soma: o que a configuração já liberava continua, e a
    /// política da organização, aplicada depois, ainda tira o que não deixa.
    ///
    /// Cada CLI libera do jeito que sabe. O Claude e o Copilot liberam
    /// comando por comando (`Bash(git add:*)`, `shell(git add)`). O Codex não
    /// tem lista de comandos: ele já roda comandos no sandbox, e o que o
    /// sandbox nega é a rede (`network_access`) e o `.git`, que só o
    /// `danger-full-access` abre. O Cursor só tem o `--force`.
    pub fn with_grants(&self,grants:&Grants)->Self {
        if grants.is_empty() { return self.clone(); }
        match self.module().with_grants(&self.options,grants) { Some(options)=>Self{options,..self.clone()}, None=>self.clone() }
    }

    /// O mesmo agente sem poder rodar os comandos que a política da organização
    /// bloqueia (`git push`, `npm publish`…). O Claude e o Copilot negam cada
    /// regra por comando, e a negação vence a liberação. O Codex, o Cursor, o
    /// Kilo e os mods criados não têm lista de comandos: com qualquer comando
    /// bloqueado eles rodam no modo mais travado que têm (sem sandbox aberto,
    /// sem rede liberada, sem `--force` nem `--auto`, com a linha que não
    /// escreve), para que nenhum comando bloqueado passe. Só aperta: nunca
    /// libera nada.
    pub fn without_commands(&self,blocked:&[String])->Self {
        if blocked.is_empty() { return self.clone(); }
        match self.module().without_commands(&self.options,blocked) { Some(options)=>Self{options,..self.clone()}, None=>self.clone() }
    }

    /// O mesmo agente sem os mecanismos que a política bloqueia, escritos
    /// `agente/mecanismo`. A política só tira: nunca liga o que quem usa
    /// deixou desligado.
    pub fn without_mechanisms(&self,blocked:&[String])->Self {
        let prefix=format!("{}/",self.id.key());
        if !blocked.iter().any(|key|key.starts_with(&prefix)) { return self.clone(); }
        Self{options:self.module().without_mechanisms(&self.options,blocked),..self.clone()}
    }
}

/// O que o desenvolvedor libera para um pedido só, por cima das
/// configurações do agente: no seletor de permissões da caixa de mensagem, ou
/// ao responder "Executar" ou "Sempre permitir" ao comando que o agente pediu.
/// `commands` são os comandos aprovados, como o agente os citou (ou o começo
/// deles, para os sempre permitidos no projeto).
#[derive(Debug,Clone,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",default)]
pub struct Grants { pub shell:bool, pub git:bool, pub network:bool, pub commands:Vec<String> }

impl Grants {
    pub fn is_empty(&self)->bool { !self.shell&&!self.git&&!self.network&&self.commands.is_empty() }

    /// Os dois juntos: o pedido do seletor e as regras do projeto.
    pub fn merged(&self,other:&Grants)->Self {
        let mut commands=self.commands.clone();
        for command in &other.commands { if !commands.contains(command) { commands.push(command.clone()); } }
        Self{shell:self.shell||other.shell,git:self.git||other.git,network:self.network||other.network,commands}
    }

    /// O que sobra do liberado depois da política da organização: o pedido
    /// que libera "todos os comandos" ou "o Git inteiro" não vale com comando
    /// bloqueado (cada comando se libera um a um), e os comandos bloqueados
    /// saem da lista.
    pub fn without_blocked(&self,blocked:&[String])->Self {
        if blocked.is_empty() { return self.clone(); }
        Self{
            shell:false,
            git:self.git&&!blocked.iter().any(|rule|program(rule)=="git"),
            network:self.network,
            commands:self.commands.iter().filter(|command|!command_blocked(blocked,command)).cloned().collect(),
        }
    }

    /// Algum comando aprovado mexe no git. No Codex é o `.git` aberto.
    pub fn needs_git(&self)->bool { self.git||self.commands.iter().any(|command|program(command)=="git") }

    /// Algum comando aprovado busca coisa na rede: instalar pacote, baixar,
    /// falar com o remoto do git.
    pub fn needs_network(&self)->bool { self.network||self.commands.iter().any(|command|fetches(command)) }
}

/// O começo de um comando que vale para "sempre permitir": o programa e o
/// subcomando (`git add`, `npm run`), sem os argumentos que mudam a cada vez.
pub fn command_prefix(command:&str)->String {
    let words=command.split_whitespace().collect::<Vec<_>>();
    let take=match words.get(1) { Some(word) if !word.starts_with('-')&&!word.contains(['/','\\','.','=']) =>2, _=>1 };
    words.iter().take(take).copied().collect::<Vec<_>>().join(" ")
}

/// A regra da organização bloqueia o comando quando as palavras dela são o
/// começo das palavras dele: `git` bloqueia `git push -f`; `git push` não
/// bloqueia `git pull`.
pub fn command_blocked(blocked:&[String],command:&str)->bool {
    let words=command.split_whitespace().collect::<Vec<_>>();
    blocked.iter().any(|rule|{
        let mine=rule.split_whitespace().collect::<Vec<_>>();
        !mine.is_empty()&&mine.len()<=words.len()&&mine.iter().zip(&words).all(|(left,right)|left==right)
    })
}

fn program(command:&str)->&str { command.split_whitespace().next().unwrap_or_default() }

pub(crate) fn fetches(command:&str)->bool {
    let words=command.split_whitespace().collect::<Vec<_>>();
    matches!(words.first().copied(),Some("curl"|"wget"|"gh"))
        ||words.iter().skip(1).take(2).any(|word|matches!(*word,"install"|"i"|"add"|"ci"|"fetch"|"pull"|"push"|"clone"|"update"|"upgrade"|"download"|"sync"))
}

/// A regra do Claude para o comando: ele e o que vier depois.
pub(crate) fn claude_rule(command:&str)->Option<String> {
    let command=command.trim();
    (!command.is_empty()&&!command.contains([',','(',')'])).then(||format!("Bash({command}:*)"))
}

/// A regra de negação do Claude para uma regra da organização.
pub(crate) fn claude_deny_rule(rule:&str)->Option<String> { claude_rule(rule) }

/// A negação do Copilot para uma regra da organização: ela inteira, e não só
/// o programa e o subcomando.
pub(crate) fn copilot_deny_rule(rule:&str)->Option<String> {
    let rule=rule.trim();
    (!rule.is_empty()&&!rule.contains([',','(',')'])).then(||format!("shell({rule})"))
}

/// A ferramenta do Copilot para o comando: o programa e o subcomando.
pub(crate) fn copilot_rule(command:&str)->Option<String> {
    let prefix=command_prefix(command);
    (!prefix.is_empty()&&!prefix.contains([',','(',')'])).then(||format!("shell({prefix})"))
}

pub(crate) fn without_shell(mechanisms:&[String])->Vec<String> { mechanisms.iter().filter(|mechanism|*mechanism!=SHELL).cloned().collect() }

/// Um modelo que o agente oferece, com os números que a tela preenche sozinha
/// ao escolhê-lo.
#[derive(Debug,Clone,Serialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct KnownModel { pub id:String, pub label:String, pub context_window:usize, pub cost_class:String, pub speed:String, pub capabilities:Vec<String> }

impl KnownModel {
    /// Custo, velocidade e capacidades saem do nome: o CLI não os diz. A
    /// família pequena é barata, rápida e não conta como raciocínio; a grande,
    /// cara e lenta; o resto fica no meio. Os `o1`/`o3`/`o4` da OpenAI são de
    /// raciocínio, e só a versão `mini` deles é pequena. O mod que só responde
    /// por texto não leva `code` nem `tools`.
    pub(crate) fn named(agent:AgentId,id:&str,label:Option<String>,context_window:Option<usize>)->Self {
        Self::named_as(agent,!agent.module().edits_project(&Value::Null),id,label,context_window)
    }

    /// O mesmo, dizendo se o mod só responde por texto: o mod criado só sabe
    /// com as opções na mão.
    pub(crate) fn named_as(agent:AgentId,text_only:bool,id:&str,label:Option<String>,context_window:Option<usize>)->Self {
        let lower=id.to_ascii_lowercase();
        let has=|words:&[&str]|words.iter().any(|word|lower.contains(word));
        let (cost_class,mut speed)=if has(&["haiku","mini","flash","luna","nano","lite"]) {("low","fast")}
            else if has(&["opus","max","astra","best","-pro"])||reasoning_series(&lower) {("high","slow")}
            else if has(&["fable"]) {("high","medium")}
            else {("medium","medium")};
        if lower.ends_with("-fast") { speed="fast"; }
        let context=context_window.unwrap_or(if lower.contains("[1m]") {1_000_000} else {agent.module().default_context()});
        let capabilities=if text_only { gateway_capabilities(cost_class) } else if cost_class=="low" {strings(&["chat","code","tools"])} else {strings(&CAPABILITIES)};
        Self{id:id.into(),label:label.unwrap_or_else(||id.into()),context_window:context.clamp(CONTEXT_RANGE.0,CONTEXT_RANGE.1),cost_class:cost_class.into(),speed:speed.into(),capabilities}
    }
}

/// O que um modelo de quem só responde por texto faz: responder e, nos
/// maiores, raciocinar. Sem `code` e `tools`: ele devolve texto, não edita o
/// projeto.
fn gateway_capabilities(cost_class:&str)->Vec<String> { if cost_class=="low" { strings(&["chat"]) } else { strings(&["chat","reasoning"]) } }

/// `o1`, `o3`, `o4`… sozinhos ou com sufixo (`o3-2025`), mas não `gpt-4o`.
fn reasoning_series(lower:&str)->bool {
    let mut chars=lower.chars();
    chars.next()==Some('o') && chars.next().is_some_and(|digit|digit.is_ascii_digit()) && chars.next().is_none_or(|next|next=='-')
}

/// Os modelos que vêm de fábrica, para a máquina em que o agente ainda não
/// respondeu à descoberta (`LlmMod::starter_models`).
fn starter_models(agent:AgentId)->Vec<AgentModel> {
    agent.module().starter_models().iter().map(|id|fresh_model(agent,&KnownModel::named(agent,id,None,None))).collect()
}

fn fresh_model(agent:AgentId,known:&KnownModel)->AgentModel {
    AgentModel{agent,model:known.id.clone(),enabled:false,capabilities:known.capabilities.clone(),cost_class:known.cost_class.clone(),speed:known.speed.clone(),context_window:known.context_window}
}

/// A chave de API de cada gateway e de cada mod de API, em memória: lida do
/// banco local ao carregar e ao salvar. Fica fora do `AgentSettings`, que vai
/// para a tela e para a nuvem.
static SECRETS:std::sync::LazyLock<std::sync::RwLock<HashMap<AgentId,String>>>=std::sync::LazyLock::new(Default::default);

/// A chave guardada do mod, se houver.
pub fn secret(agent:AgentId)->Option<String> { SECRETS.read().ok()?.get(&agent).cloned().filter(|key|!key.trim().is_empty()) }

fn remember_secrets(connection:&Connection)->Result<()> {
    let mut statement=connection.prepare("SELECT agent,secret FROM llm_secrets")?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let found:HashMap<AgentId,String>=rows.into_iter().filter_map(|(agent,secret)|Some((AgentId::parse(&agent).ok()?,secret))).collect();
    if let Ok(mut cache)=SECRETS.write() { *cache=found; }
    Ok(())
}

/// Os modelos do mod de API, pelo `/models` dele. Quem só responde por texto
/// nunca leva as capacidades `code` e `tools`.
pub async fn discover_gateway(agent:&AgentSettings)->Result<Vec<KnownModel>> {
    let config=agent.module().provider(agent);
    let listed=crate::providers::discover_listing(agent.id.key(),&config).await?;
    let text_only=!agent.edits_project();
    let found:Vec<KnownModel>=unique(listed.into_iter().filter(|model|valid_id(&model.id)).map(|model|KnownModel::named_as(agent.id,text_only,&model.id,None,model.context)));
    if let Ok(mut cache)=DISCOVERED.lock() { cache.insert(agent.id,found.clone()); }
    Ok(found)
}

/// A descoberta de um agente, de qualquer tipo: o CLI pelo comando, o de API
/// pelo endereço. `None` quando não respondeu ou quando o mod não sabe listar
/// (o mod criado de linha de comando: os modelos dele são os que a pessoa
/// escreve).
pub async fn discover_agent(agent:&AgentSettings)->Option<Vec<KnownModel>> {
    if agent.is_api() {
        return match discover_gateway(agent).await { Ok(found) if !found.is_empty()=>Some(found), Ok(_)=>None, Err(error)=>{ eprintln!("[llm] {} listed no models: {error}",agent.id.key()); None } };
    }
    discover(agent.id,&agent.command).await
}

/// O que o último `/model` de cada agente devolveu nesta execução do app.
static DISCOVERED:std::sync::LazyLock<std::sync::Mutex<HashMap<AgentId,Vec<KnownModel>>>>=std::sync::LazyLock::new(Default::default);

/// O catálogo que a tela oferece: o que o CLI listou e, enquanto ele não
/// respondeu, os modelos já gravados do agente.
pub fn catalog(agent:AgentId,settings:&LlmSettings)->Vec<KnownModel> {
    if let Some(found)=DISCOVERED.lock().ok().and_then(|cache|cache.get(&agent).cloned()) { return found; }
    settings.models.iter().filter(|model|model.agent==agent).map(|model|KnownModel{
        id:model.model.clone(),label:model.model.clone(),context_window:model.context_window,cost_class:model.cost_class.clone(),speed:model.speed.clone(),capabilities:model.capabilities.clone(),
    }).collect()
}

/// Pergunta ao CLI quais modelos ele oferece, com os argumentos que o mod
/// diz (`LlmMod::listing_args`), sem gastar crédito. `None` quando ele não está
/// instalado, não respondeu, respondeu algo que esta versão não lê ou o mod não
/// sabe listar.
pub async fn discover(agent:AgentId,command:&str)->Option<Vec<KnownModel>> {
    let module=agent.module();
    if module.listing_args().is_empty() { return None; }
    let path=locate(command)?;
    let (program,lead)=launcher(&path);
    let mut process=tokio::process::Command::new(&program);
    if let Some(search)=agent_path(&path) { process.env("PATH",search); }
    let run=crate::providers::quiet(&mut process).args(lead).args(module.listing_args()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).output();
    let output=tokio::time::timeout(Duration::from_secs(60),run).await.ok()?.ok()?;
    let found=module.parse_listing(&String::from_utf8_lossy(&output.stdout));
    if found.is_empty() { eprintln!("[llm] {} listed no models",agent.key()); return None; }
    if let Ok(mut cache)=DISCOVERED.lock() { cache.insert(agent,found.clone()); }
    Some(found)
}

pub(crate) fn unique(models:impl Iterator<Item=KnownModel>)->Vec<KnownModel> {
    let mut seen=HashSet::new();
    models.filter(|model|seen.insert(model.id.clone())).collect()
}

pub(crate) fn valid_id(id:&str)->bool { !id.is_empty()&&id.chars().all(|char|char.is_ascii_alphanumeric()||"._-:/@[]".contains(char)) }

/// Troca os modelos do agente pela lista do CLI, na ordem dele. O que já
/// estava gravado guarda o que o usuário mudou (ligado, custo, capacidades);
/// o que é novo nasce ligado, para o Jev poder escolhê-lo; o que o CLI deixou
/// de oferecer sai — e sai também das opções do mod (o modelo reserva do
/// Claude).
pub fn adopt_listing(settings:&LlmSettings,agent:AgentId,found:&[KnownModel])->LlmSettings {
    let mut models:Vec<AgentModel>=settings.models.iter().filter(|model|model.agent!=agent).cloned().collect();
    models.extend(found.iter().map(|known|settings.models.iter().find(|model|model.agent==agent&&model.model==known.id).cloned().unwrap_or_else(||fresh_model(agent,known))));
    let mut agents=settings.agents.clone();
    for entry in agents.iter_mut().filter(|entry|entry.id==agent) {
        if let Some(options)=entry.module().adopted(&entry.options,found) { entry.options=options; }
    }
    LlmSettings{agents,models}
}

/// Cria as tabelas e, na primeira vez, cadastra os mods do app com os modelos
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
    // Um mod que chegou numa versão nova entra no banco que já existia com
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

/// A posição do agente nas abas: os mods do app na ordem deles, depois os
/// criados pelo nome.
fn tab_order(agent:&AgentSettings)->(usize,String,AgentId) {
    let position=AgentId::ALL.iter().position(|id|*id==agent.id).unwrap_or(AgentId::ALL.len());
    (position,if position<AgentId::ALL.len() { String::new() } else { agent.label().to_lowercase() },agent.id)
}

pub fn load(connection:&Connection)->Result<LlmSettings> {
    let mut agents=Vec::new();
    {
        let mut statement=connection.prepare("SELECT id,enabled,command,timeout,options FROM llm_agents")?;
        let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?)))?;
        for row in rows {
            let (id,enabled,command,timeout,options)=row?;
            let Ok(id)=AgentId::parse(&id) else { continue };
            agents.push(AgentSettings{id,enabled,command,timeout:timeout.max(0) as u64,options:id.module().filled(legacy_effort(serde_json::from_str(&options).unwrap_or(Value::Null)))});
        }
    }
    remember_secrets(connection)?;
    for agent in agents.iter_mut().filter(|agent|agent.is_api()) {
        if let Some(options)=agent.options.as_object_mut() { options.insert("hasKey".into(),Value::Bool(secret(agent.id).is_some())); }
    }
    // Um mod do app que falte no banco volta com o padrão: a tela sempre tem
    // uma aba por mod do app.
    for id in AgentId::ALL { if !agents.iter().any(|agent|agent.id==id) { agents.push(AgentSettings::fresh(id)); } }
    agents.sort_by_cached_key(tab_order);
    let order:HashMap<AgentId,usize>=agents.iter().enumerate().map(|(position,agent)|(agent.id,position)).collect();
    let mut models=Vec::new();
    let mut statement=connection.prepare("SELECT agent,model,enabled,capabilities,cost_class,speed,context_window FROM llm_models ORDER BY agent,position,model")?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,i64>(6)?)))?;
    for row in rows {
        let (agent,model,enabled,capabilities,cost_class,speed,context_window)=row?;
        let Ok(agent)=AgentId::parse(&agent) else { continue };
        // O modelo de um mod que não está mais aqui não tem dono.
        if !order.contains_key(&agent) { continue; }
        models.push(AgentModel{agent,model,enabled,capabilities:serde_json::from_str(&capabilities).unwrap_or_default(),cost_class,speed,context_window:context_window.max(0) as usize});
    }
    // Na ordem dos agentes do app, não na do nome: o banco ordena o texto.
    models.sort_by_key(|model|order.get(&model.agent).copied().unwrap_or(usize::MAX));
    Ok(LlmSettings{agents,models})
}

/// Confere tudo antes de gravar e devolve a versão limpa. Nada chega ao banco
/// sem passar por aqui.
pub fn validate(settings:&LlmSettings)->Result<LlmSettings> {
    let customs=settings.agents.iter().filter(|agent|agent.id.is_custom()).count();
    if customs>crate::mods::custom::MAX_MODS { bail!(Text::new("mods.tooMany").with("max",crate::mods::custom::MAX_MODS as u32)); }
    let mut ids:Vec<AgentId>=AgentId::ALL.to_vec();
    for agent in &settings.agents { if agent.id.is_custom()&&!ids.contains(&agent.id) { ids.push(agent.id); } }
    let mut agents=Vec::new();
    for id in ids {
        let agent=settings.agents.iter().find(|agent|agent.id==id).cloned().unwrap_or_else(||AgentSettings::fresh(id));
        let name=agent.label();
        // O mod de API não abre programa: o endereço dele mora nas opções.
        let api=agent.is_api();
        let command=if api { String::new() } else { agent.command.trim().to_string() };
        if command.is_empty()&&!api { bail!(Text::new("settings.commandRequired").with("agent",&name)); }
        if command.chars().any(char::is_whitespace) { bail!(Text::new("settings.commandArgs").with("agent",&name)); }
        if !(TIMEOUT_RANGE.0..=TIMEOUT_RANGE.1).contains(&agent.timeout) { bail!(Text::new("settings.timeout").with("agent",&name).with("min",TIMEOUT_RANGE.0).with("max",TIMEOUT_RANGE.1)); }
        let own:HashSet<&str>=settings.models.iter().filter(|model|model.agent==id).map(|model|model.model.trim()).collect();
        let options=agent.checked(&own)?;
        agents.push(AgentSettings{id,enabled:agent.enabled,command,timeout:agent.timeout,options});
    }
    let mut seen=HashSet::new();
    let mut models=Vec::new();
    let checked=LlmSettings{agents,models:vec![]};
    for model in &settings.models {
        // O modelo de um mod apagado sai junto com ele.
        let Some(owner)=checked.agent(model.agent) else { continue };
        let name=model.model.trim().to_string();
        if name.is_empty() { bail!(Text::new("settings.modelNoId").with("agent",owner.label())); }
        if !valid_id(&name) { bail!(Text::new("settings.modelBadId").with("name",&name)); }
        if !seen.insert((model.agent,name.clone())) { bail!(Text::new("settings.modelDuplicate").with("name",&name).with("agent",owner.label())); }
        let mut capabilities=Vec::new();
        for capability in &model.capabilities { one_of("model.capability",capability,&CAPABILITIES)?; if !capabilities.contains(capability) { capabilities.push(capability.clone()); } }
        // O mod que só responde por texto não escreve no projeto nem usa ferramentas.
        if !owner.edits_project() { capabilities.retain(|capability|capability!="code"&&capability!="tools"); if capabilities.is_empty() { capabilities.push("chat".into()); } }
        if capabilities.is_empty() { bail!(Text::new("settings.modelNoCapability").with("name",&name)); }
        one_of("model.cost",&model.cost_class,&COSTS)?;
        one_of("model.speed",&model.speed,&SPEEDS)?;
        if !(CONTEXT_RANGE.0..=CONTEXT_RANGE.1).contains(&model.context_window) { bail!(Text::new("settings.contextWindow").with("name",&name).with("min",CONTEXT_RANGE.0).with("max",CONTEXT_RANGE.1)); }
        models.push(AgentModel{model:name,capabilities,..model.clone()});
    }
    let LlmSettings{agents,..}=checked;
    if !agents.iter().any(|agent|agent.enabled) { bail!(Text::new("settings.noAgent")); }
    Ok(LlmSettings{agents,models})
}

pub fn save(connection:&mut Connection,settings:&LlmSettings)->Result<LlmSettings> {
    let incoming=settings;
    let mut settings=validate(settings)?;
    let transaction=connection.transaction()?;
    // A chave do mod de API: a que veio nas opções entra na tabela local e
    // some do que se grava (o `validate` já a tirou); `clearKey` apaga.
    for agent in incoming.agents.iter().filter(|agent|agent.is_api()) {
        if agent.options.get("clearKey").and_then(Value::as_bool)==Some(true) { transaction.execute("DELETE FROM llm_secrets WHERE agent=?1",[agent.id.key()])?; }
        if let Some(key)=agent.options.get("apiKey").and_then(Value::as_str).map(str::trim).filter(|key|!key.is_empty()) {
            if key.chars().any(char::is_whitespace) { bail!(Text::new("settings.keyInvalid").with("agent",agent.label())); }
            transaction.execute("INSERT INTO llm_secrets(agent,secret) VALUES(?1,?2) ON CONFLICT(agent) DO UPDATE SET secret=excluded.secret",params![agent.id.key(),key])?;
        }
    }
    for agent in settings.agents.iter().filter(|agent|agent.enabled&&agent.is_api()&&agent.module().requires_key(&agent.options)) {
        let stored:i64=transaction.query_row("SELECT COUNT(*) FROM llm_secrets WHERE agent=?1",[agent.id.key()],|row|row.get(0))?;
        if stored==0 { bail!(Text::new("settings.keyRequired").with("agent",agent.label())); }
    }
    write(&transaction,&settings)?;
    transaction.commit()?;
    remember_secrets(connection)?;
    for agent in settings.agents.iter_mut().filter(|agent|agent.is_api()) {
        if let Some(options)=agent.options.as_object_mut() { options.insert("hasKey".into(),Value::Bool(secret(agent.id).is_some())); }
    }
    Ok(settings)
}

/// Grava os agentes e os modelos. O mod criado que saiu da lista sai do banco
/// (e da nuvem, pela fila de sync) com os modelos e a chave dele; os mods do
/// app nunca saem.
fn write(connection:&Connection,settings:&LlmSettings)->Result<()> {
    let now=chrono::Utc::now().to_rfc3339();
    connection.execute("DELETE FROM llm_models",[])?;
    let stored:Vec<String>={
        let mut statement=connection.prepare("SELECT id FROM llm_agents")?;
        statement.query_map([],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for gone in stored.iter().filter(|id|AgentId::parse(id).is_ok_and(|id|id.is_custom()&&settings.agent(id).is_none())) {
        connection.execute("DELETE FROM llm_agents WHERE id=?1",[gone])?;
        connection.execute("DELETE FROM llm_secrets WHERE agent=?1",[gone])?;
    }
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

/// A linha do Claude com os arquivos protegidos (`privacy.deny`) fora do
/// alcance das ferramentas dele: `Read(padrão)` e `Edit(padrão)` entram no
/// `--disallowed-tools`. O padrão segue o `.gitignore` nos dois lados — `.env`
/// vale em qualquer pasta, `secrets/**` também. Padrão com vírgula, parêntese
/// ou espaço não cabe na lista e fica só com o firewall do Jev; o que começa
/// com `/` é da raiz do projeto (`./`). Os outros agentes não têm flag para
/// isto: neles a proteção é o firewall, que tira os arquivos do contexto, e a
/// portaria de saída, que segura o que o agente mexeu.
pub fn guarding(args:&[String],deny:&[String])->Vec<String> {
    let expanded:Vec<String>=deny.iter().flat_map(|pattern|if pattern.trim()==jayv_base::firewall::ENV_ANY { jayv_base::firewall::ENV_REAL.iter().map(|real|real.to_string()).collect() } else { vec![pattern.clone()] }).collect();
    let rules:Vec<String>=expanded.iter().map(|pattern|pattern.trim()).filter(|pattern|!pattern.is_empty()&&!pattern.starts_with('!')&&!pattern.contains([',','(',')',' ','\t']))
        .map(|pattern|match pattern.strip_prefix('/') { Some(rest)=>format!("./{rest}"), None=>pattern.to_string() })
        .flat_map(|pattern|[format!("Read({pattern})"),format!("Edit({pattern})")]).collect();
    if rules.is_empty() { return args.to_vec(); }
    let mut guarded=args.to_vec();
    match guarded.iter().position(|arg|arg=="--disallowed-tools") {
        Some(index) if index+1<guarded.len()=>{ let joined=format!("{},{}",guarded[index+1],rules.join(",")); guarded[index+1]=joined; }
        _=>guarded.extend(["--disallowed-tools".to_string(),rules.join(",")]),
    }
    guarded
}

/// Os provedores e modelos no formato que o orquestrador já entende: cada mod
/// diz o provedor dele (`LlmMod::provider`).
pub fn to_config(settings:&LlmSettings)->(HashMap<String,ProviderConfig>,HashMap<String,ModelConfig>) {
    let providers=settings.agents.iter().map(|agent|(agent.id.key().to_string(),agent.module().provider(agent))).collect();
    let models=settings.models.iter().map(|model|(model_key(model),ModelConfig{
        enabled:model.enabled,provider:model.agent.key().into(),model:model.model.clone(),capabilities:model.capabilities.clone(),
        cost_class:model.cost_class.clone(),speed:model.speed.clone(),context_window:model.context_window,
    })).collect();
    (providers,models)
}

/// O nome do mod sem as opções na mão: o do mod criado é o id.
fn label(agent:AgentId)->String { agent.module().label(&Value::Null) }

/// Onde o executável está, procurando como o shell faria: no PATH e, depois,
/// nas pastas onde os instaladores dos agentes os põem — o app aberto pelo menu
/// não herda o PATH do terminal. No Windows, com as extensões do `PATHEXT`.
///
/// A resposta fica guardada por `LOCATE_TTL`: cada pedido conferia os quatro
/// agentes no disco — o PATH, as pastas dos instaladores, as versões do nvm —
/// antes de começar. O caminho guardado que sumiu é procurado de novo na hora.
pub fn locate(command:&str)->Option<PathBuf> {
    let now=std::time::Instant::now();
    if let Some((at,found))=located().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(command).cloned() {
        let fresh=now.duration_since(at)<LOCATE_TTL;
        if fresh && found.as_ref().is_none_or(|path|path.is_file()) { return found; }
    }
    locate_fresh(command)
}

/// A procura sem a lembrança, para quem precisa da resposta de agora — a
/// tela que confere o agente recém-instalado. Ela também renova a lembrança.
pub fn locate_fresh(command:&str)->Option<PathBuf> {
    let mut dirs:Vec<PathBuf>=env::var_os("PATH").map(|path|env::split_paths(&path).collect()).unwrap_or_default();
    dirs.extend(install_dirs());
    let found=search(command,&dirs,&extensions());
    located().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(command.to_string(),(std::time::Instant::now(),found.clone()));
    found
}

const LOCATE_TTL:Duration=Duration::from_secs(30);
type Located=std::sync::Mutex<HashMap<String,(std::time::Instant,Option<PathBuf>)>>;
fn located()->&'static Located { static LOCATED:std::sync::OnceLock<Located>=std::sync::OnceLock::new(); LOCATED.get_or_init(Default::default) }

/// A linha de comando sem as flags que este executável não conhece
/// (`OPTIONAL_FLAGS`). Uma flag nova numa CLI antiga derrubava o pedido com
/// "unknown option"; aqui ela só sai. A ajuda do executável (do subcomando,
/// no `codex exec`) é lida uma vez por instalação.
pub async fn understood(program:&Path,args:Vec<String>)->Vec<String> {
    let used:Vec<(&str,usize)>=OPTIONAL_FLAGS.iter().copied().filter(|(flag,_)|args.iter().any(|arg|arg==flag)).collect();
    if used.is_empty() { return args; }
    let subcommands:Vec<String>=args.iter().take_while(|arg|!arg.starts_with('-')&&!arg.starts_with('{')).cloned().collect();
    let help=help_of(program,&subcommands).await;
    let unknown:Vec<(&str,usize)>=used.into_iter().filter(|(flag,_)|!help.contains(flag)).collect();
    if unknown.is_empty() { return args; }
    let mut kept=Vec::with_capacity(args.len());
    let mut given=args.into_iter();
    while let Some(arg)=given.next() {
        match unknown.iter().find(|(flag,_)|*flag==arg) {
            Some((_,values))=>{ for _ in 0..*values { given.next(); } }
            None=>kept.push(arg),
        }
    }
    kept
}

type Helps=tokio::sync::Mutex<HashMap<(PathBuf,Vec<String>),(Option<std::time::SystemTime>,String)>>;
fn helps()->&'static Helps { static HELPS:std::sync::OnceLock<Helps>=std::sync::OnceLock::new(); HELPS.get_or_init(Default::default) }

/// O texto do `--help`, guardado por caminho e data do executável. Sem
/// resposta em dez segundos, vazio: a flag opcional sai, o que é o lado seguro.
async fn help_of(program:&Path,subcommands:&[String])->String {
    let stamp=program.metadata().ok().and_then(|meta|meta.modified().ok());
    let key=(program.to_path_buf(),subcommands.to_vec());
    let mut known=helps().lock().await;
    if let Some((_,text))=known.get(&key).filter(|(at,_)|*at==stamp) { return text.clone(); }
    let (launch,lead)=launcher(program);
    let mut command=tokio::process::Command::new(&launch);
    if let Some(search)=agent_path(program) { command.env("PATH",search); }
    let run=crate::providers::quiet(&mut command).args(lead).args(subcommands).arg("--help").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output();
    let text=match tokio::time::timeout(Duration::from_secs(10),run).await {
        Ok(Ok(output))=>format!("{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr)),
        _=>String::new(),
    };
    known.insert(key,(stamp,text.clone()));
    text
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

/// Se o agente está logado, segundo ele mesmo (`claude auth status`,
/// `codex login status`). O Copilot e o Cursor não dizem: ficam sem resposta.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Login { In, Out }

/// Quanto tempo a resposta do login vale. O Codex leva seis segundos para
/// dizer que não está logado: perguntar a cada pedido era pior que o erro.
const LOGIN_TTL:Duration=Duration::from_secs(10*60);
type Logins=std::sync::Mutex<HashMap<String,(std::time::Instant,Option<Login>)>>;
fn logins()->&'static Logins { static LOGINS:std::sync::OnceLock<Logins>=std::sync::OnceLock::new(); LOGINS.get_or_init(Default::default) }

/// O agente sabidamente sem login: o roteamento o deixa de lado (enquanto
/// houver outro), em vez de abri-lo só para ouvir que falta entrar. Sem
/// resposta guardada, ou com ela vencida, não é "sem login".
pub fn logged_out(command:&str)->bool {
    logins().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(command).is_some_and(|(at,login)|*login==Some(Login::Out)&&at.elapsed()<LOGIN_TTL)
}

/// Pergunta ao agente se está logado e guarda a resposta. Só o mod que sabe
/// perguntar (`LlmMod::login_args`) é perguntado.
pub async fn check_login(agent:AgentId,command:&str)->Option<Login> {
    let module=agent.module();
    let ask=module.login_args();
    if ask.is_empty() { return None; }
    // Marca a pergunta em andamento: outro pedido não pergunta de novo.
    logins().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(command.to_string(),(std::time::Instant::now(),None));
    let path=locate(command)?;
    let (program,lead)=launcher(&path);
    let mut run=tokio::process::Command::new(&program);
    if let Some(search)=agent_path(&path) { run.env("PATH",search); }
    let output=crate::providers::quiet(&mut run).args(lead).args(ask).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output();
    let login=match tokio::time::timeout(Duration::from_secs(20),output).await {
        Ok(Ok(output))=>module.read_login(output.status.success(),&String::from_utf8_lossy(&output.stdout),&String::from_utf8_lossy(&output.stderr)),
        _=>None,
    };
    logins().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(command.to_string(),(std::time::Instant::now(),login));
    login
}


/// Lê de antemão a ajuda dos agentes ligados que levam flag opcional, para o
/// primeiro pedido não esperar o `--help` (`understood`). Sem esperar.
pub fn warm_up(settings:&LlmSettings) {
    for agent in settings.agents.iter().filter(|agent|agent.enabled) {
        let Some(subcommands)=agent.module().help_subcommands() else { continue };
        let subcommands:Vec<String>=subcommands.iter().map(|subcommand|subcommand.to_string()).collect();
        let command=agent.command.clone();
        tokio::spawn(async move { if let Some(path)=locate(&command) { help_of(&path,&subcommands).await; } });
    }
}

/// Renova, sem esperar, o login dos agentes ligados cuja resposta venceu.
pub fn refresh_logins(settings:&LlmSettings) {
    let stale:Vec<(AgentId,String)>=settings.agents.iter().filter(|agent|agent.enabled&&!agent.module().login_args().is_empty())
        .filter(|agent|logins().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&agent.command).is_none_or(|(at,_)|at.elapsed()>=LOGIN_TTL))
        .map(|agent|(agent.id,agent.command.clone())).collect();
    for (agent,command) in stale { tokio::spawn(async move { check_login(agent,&command).await; }); }
}

/// O que a tela mostra ao conferir um agente: onde ele está e qual versão
/// responde. Sem o binário, a tela avisa antes de o pedido falhar.
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Probe { pub path:Option<String>, pub version:Option<String>, #[serde(skip_serializing_if="Option::is_none")] pub logged_in:Option<bool> }

pub async fn probe(command:&str)->Probe {
    let Some(path)=locate_fresh(command) else { return Probe{path:None,version:None,logged_in:None} };
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
    Probe{path:Some(path.display().to_string()),version,logged_in:None}
}

/// A conferência da tela com o login junto, para o agente que sabe dizer.
pub async fn probe_agent(agent:AgentId,command:&str)->Probe {
    let mut found=probe(command).await;
    if found.version.is_some() { found.logged_in=check_login(agent,command).await.map(|login|login==Login::In); }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{claude::{parse_claude_listing, SYMBOL_SERVER_TOOLS}, codex::parse_codex_listing, copilot::parse_copilot_listing, cursor::parse_cursor_listing, kilo::parse_kilo_listing};
    use serde_json::json;

    fn read_login(agent:AgentId,success:bool,stdout:&str,stderr:&str)->Option<Login> { agent.module().read_login(success,stdout,stderr) }
    /// Os servidores que o próprio JayV entrega ao agente.
    fn mcp_values(agent:&AgentSettings)->Vec<crate::mcp::McpServer> { agent.module().delivered_mcp(&agent.options) }

    fn memory()->Connection { let connection=Connection::open_in_memory().expect("banco"); ensure(&connection).expect("tabelas"); connection }
    fn agent(id:AgentId,options:Value)->AgentSettings { AgentSettings{id,enabled:true,command:id.binary().into(),timeout:300,options} }
    fn settings(agents:Vec<AgentSettings>)->LlmSettings { LlmSettings{agents,models:AgentId::ALL.into_iter().flat_map(starter_models).collect()} }

    /// Antes de o CLI responder, cada agente tem os modelos de fábrica, todos
    /// desligados — os apelidos do Claude seguem a versão nova sozinhos.
    #[test] fn the_database_starts_with_the_starter_models() {
        let loaded=load(&memory()).expect("leitura");
        assert_eq!(loaded.agents.iter().map(|agent|agent.id).collect::<Vec<_>>(),AgentId::ALL);
        let of=|id:AgentId|loaded.models.iter().filter(|model|model.agent==id).map(|model|model.model.as_str()).collect::<Vec<_>>();
        assert_eq!(of(AgentId::Claude),["sonnet","opus","haiku","fable"]);
        assert_eq!(of(AgentId::Codex).len(),1);
        assert_eq!(of(AgentId::Copilot).len(),1);
        assert_eq!(of(AgentId::Cursor),["auto"]);
        assert!(loaded.models.iter().all(|model|!model.enabled),"modelos nascem desligados");
    }

    #[test] fn claude_lists_its_models_through_the_model_command() {
        let output=json!({"type":"result","result":"Current model: `Opus`\nUsage: /model <name>. Available: sonnet, opus, haiku, best, sonnet[1m], opusplan, default, or a full model ID."}).to_string();
        let found=parse_claude_listing(&output);
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["sonnet","opus","haiku","best","sonnet[1m]","opusplan"],"`default` não é um modelo");
        assert_eq!(found[4].context_window,1_000_000);
        assert_eq!((found[2].cost_class.as_str(),found[2].speed.as_str()),("low","fast"));
    }

    /// O modelo pequeno nasce sem raciocínio, para não ganhar o bônus de
    /// pedido complexo nem disputar análise com os grandes; a série `o` da
    /// OpenAI é de raciocínio, e o `gpt-4o` não é dela.
    #[test] fn new_models_get_the_capabilities_of_their_family() {
        let caps=|id:&str|KnownModel::named(AgentId::Codex,id,None,None);
        assert!(!caps("haiku").capabilities.contains(&"reasoning".to_string()));
        assert!(caps("sonnet").capabilities.contains(&"reasoning".to_string()));
        assert_eq!(caps("o3").cost_class,"high");
        assert_eq!(caps("o4-mini").cost_class,"low");
        assert_eq!(caps("gpt-4o").cost_class,"medium");
        assert_eq!(fresh_model(AgentId::Claude,&caps("haiku")).capabilities,["chat","code","tools"]);
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

    /// A lista do CLI manda: o novo nasce desligado, o gravado guarda o que o
    /// usuário mudou, e o que sumiu sai — inclusive do modelo reserva.
    #[test] fn the_cli_listing_replaces_the_agent_models() {
        let mut current=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"haiku"})),agent(AgentId::Codex,Value::Null),agent(AgentId::Copilot,Value::Null)]);
        current.models.iter_mut().filter(|model|model.model=="sonnet").for_each(|model|model.enabled=true);
        let listing=[KnownModel::named(AgentId::Claude,"opus",None,None),KnownModel::named(AgentId::Claude,"sonnet[1m]",None,None)];
        let adopted=adopt_listing(&current,AgentId::Claude,&listing);
        let claude:Vec<&AgentModel>=adopted.models.iter().filter(|model|model.agent==AgentId::Claude).collect();
        assert_eq!(claude.iter().map(|model|model.model.as_str()).collect::<Vec<_>>(),["opus","sonnet[1m]"]);
        assert!(!claude[0].enabled,"o que estava desligado segue desligado");
        assert!(!claude[1].enabled,"o modelo novo nasce desligado");
        let kept=adopt_listing(&current,AgentId::Claude,&[KnownModel::named(AgentId::Claude,"sonnet",None,None)]);
        assert!(kept.models.iter().find(|model|model.agent==AgentId::Claude&&model.model=="sonnet").is_some_and(|model|model.enabled),"a escolha do usuário fica");
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

    /// Os modelos nascem desligados: um agente ligado sem modelo ativo salva;
    /// é o pedido que avisa para ligar um modelo em Configurações.
    #[test] fn an_enabled_agent_may_have_every_model_off() {
        let mut quiet=settings(vec![agent(AgentId::Claude,Value::Null)]);
        quiet.models.iter_mut().for_each(|model|model.enabled=false);
        assert!(validate(&quiet).is_ok());
    }

    #[test] fn the_fallback_model_must_belong_to_the_same_agent() {
        let wrong=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"gpt-5"}))]);
        assert!(validate(&wrong).is_err());
        let right=settings(vec![agent(AgentId::Claude,json!({"fallbackModel":"sonnet"}))]);
        assert!(validate(&right).is_ok());
    }

    fn two_servers()->Vec<crate::mcp::McpServer> { crate::mcp::parse(r#"{"mcpServers":{"github":{"command":"npx","args":["-y","pkg"],"env":{"TOKEN":"x"}},"docs":{"url":"https://mcp.example.com/mcp"}}}"#) }

    /// O mesmo "Aprovar servidores MCP" para os sete agentes: desligado, nada
    /// chega; ligado, cada um recebe os servidores do jeito dele.
    #[test] fn every_agent_gets_the_servers_only_with_the_same_toggle_on() {
        let servers=two_servers();
        for id in AgentId::ALL {
            let off=settings(vec![agent(id,json!({"approveMcps":false}))]).with_mcp(&servers);
            assert!(mcp_values(&off.agents[0]).is_empty()&&!off.agents[0].args().iter().any(|arg|arg.contains("mcp_servers")||arg=="--additional-mcp-config"||arg=="--mcp-config"),"{id:?} desligado não leva servidor");
            let on=settings(vec![agent(id,json!({"approveMcps":true}))]).with_mcp(&servers);
            let args=on.agents[0].args();
            let carried=match id {
                AgentId::Claude=>args.windows(2).any(|pair|pair[0]=="--allowedTools"&&pair[1].contains("mcp__github"))&&args.iter().any(|arg|arg=="--mcp-config"),
                AgentId::Codex=>args.iter().any(|arg|arg.starts_with("mcp_servers.github.command")),
                AgentId::Copilot=>args.iter().any(|arg|arg=="--additional-mcp-config"),
                _=>mcp_values(&on.agents[0]).len()==2,
            };
            assert!(carried,"{id:?} ligado recebe os servidores");
        }
    }

    #[test] fn the_toggle_defaults_off_for_every_agent() {
        for id in AgentId::ALL { assert_eq!(AgentSettings::fresh(id).options.get("approveMcps"),Some(&json!(false)),"{id:?}"); }
    }

    #[test] fn the_gateway_servers_reach_the_provider_config_and_planning_drops_them() {
        let all=settings(vec![agent(AgentId::Litellm,json!({"approveMcps":true,"baseUrl":"http://localhost:4000/v1"})),agent(AgentId::Kilo,json!({"approveMcps":true})),agent(AgentId::Cursor,json!({"approveMcps":true}))]).with_mcp(&two_servers());
        let (providers,_)=to_config(&all);
        for key in ["litellm","kilo","cursor"] {
            assert_eq!(providers[key].mcp.len(),2,"{key}");
            assert!(providers[key].for_planning().mcp.is_empty(),"{key}: o plano não leva servidor");
        }
        assert!(agent(AgentId::Cursor,json!({"approveMcps":true})).plan_args().iter().all(|arg|arg!="--approve-mcps"),"o plano do Cursor não aprova MCP");
    }

    #[test] fn the_organization_can_block_the_toggle_and_safe_agents_drop_the_servers() {
        let servers=two_servers();
        for id in AgentId::ALL {
            let on=settings(vec![agent(id,json!({"approveMcps":true}))]).with_mcp(&servers);
            let blocked=on.agents[0].without_mechanisms(&[format!("{}/mcp",id.key())]);
            assert_eq!(blocked.options.get("approveMcps"),Some(&json!(false)),"{id:?} bloqueado");
            assert!(mcp_values(&blocked).is_empty()&&!blocked.args().iter().any(|arg|arg.contains("mcp_servers")||arg=="--additional-mcp-config"),"{id:?}");
            let safe=on.agents[0].without_unsafe_modes();
            assert_eq!(safe.options.get("approveMcps").cloned().unwrap_or(json!(false)),json!(false),"{id:?} seguro");
            assert!(mcp_values(&safe).is_empty(),"{id:?}");
        }
    }

    #[test] fn the_symbol_tools_are_opt_in_and_start_this_executable_as_mcp() {
        let plain=agent(AgentId::Claude,json!({})).args();
        assert!(!plain.iter().any(|arg|arg=="--mcp-config"),"desligado por padrão");
        let args=agent(AgentId::Claude,json!({"symbolTools":true})).args();
        let at=args.iter().position(|arg|arg=="--mcp-config").expect("liga o servidor");
        let config:Value=serde_json::from_str(&args[at+1]).expect("json");
        assert_eq!(config["mcpServers"]["jayv"]["args"],json!(["mcp"]));
        assert_eq!(config["mcpServers"]["jayv"]["command"],std::env::current_exe().unwrap().display().to_string());
        assert!(args.windows(2).any(|pair|pair[0]=="--allowedTools"&&pair[1].split(',').any(|tool|tool==SYMBOL_SERVER_TOOLS)));
        let plan=agent(AgentId::Claude,json!({"symbolTools":true})).plan_args();
        assert!(plan.iter().any(|arg|arg=="--mcp-config"),"só leem: valem no plano também");
    }

    #[test] fn claude_always_speaks_stream_json() {
        let args=agent(AgentId::Claude,json!({"permissionMode":"plan","maxBudgetUsd":2.5})).args();
        for fixed in ["--print","stream-json","--include-partial-messages","{model}"] { assert!(args.iter().any(|arg|arg==fixed),"falta {fixed}"); }
        assert!(args.windows(2).any(|pair|pair==[EPHEMERAL,"--no-session-persistence"]),"guardar sessões é o padrão: só a chamada de apoio não guarda");
        assert!(!args.windows(2).any(|pair|pair[0]!=EPHEMERAL&&pair[1]=="--no-session-persistence"));
        assert!(args.windows(2).any(|pair|pair==[PERMISSION_PROMPTS,"none"]),"ninguém responde à aprovação no --print");
        let forgetful=agent(AgentId::Claude,json!({"persistSessions":false})).args();
        assert!(forgetful.iter().any(|arg|arg=="--no-session-persistence"));
        assert!(args.windows(2).any(|pair|pair==["--resume",RESUME]),"com sessões guardadas, o pedido seguinte retoma a do chat");
        assert!(!forgetful.iter().any(|arg|arg=="--resume"),"sem sessão guardada não há o que retomar");
        assert!(args.windows(2).any(|pair|pair==["--permission-mode","plan"]));
        assert!(args.windows(2).any(|pair|pair==["--max-budget-usd","2.50"]));
    }

    /// `default` é o nome antigo do `manual`; os dois saem sem a flag, e no
    /// modo desenvolvimento os modos que só leem — o `dontAsk` também — sobem
    /// para o `acceptEdits`.
    #[test] fn the_manual_and_dont_ask_modes() {
        let cleaned=|options:Value|->ClaudeOptions { serde_json::from_value(validate(&settings(vec![agent(AgentId::Claude,options)])).expect("válido").agents[0].options.clone()).expect("opções") };
        assert_eq!(cleaned(json!({"permissionMode":"default"})).permission_mode,"manual");
        assert_eq!(cleaned(json!({"permissionMode":"dontAsk"})).permission_mode,"dontAsk");
        let manual=agent(AgentId::Claude,json!({"permissionMode":"manual"})).args();
        assert!(!manual.iter().any(|arg|arg=="--permission-mode"),"o padrão vai sem flag: a CLI antiga não conhece `manual`");
        let dont=agent(AgentId::Claude,json!({"permissionMode":"dontAsk"}));
        assert!(dont.args().windows(2).any(|pair|pair==["--permission-mode","dontAsk"]));
        assert!(dont.build_args().windows(2).any(|pair|pair==["--permission-mode","acceptEdits"]));
        let safe=cleaned(json!({"safeMode":true,"symbolTools":true}));
        assert!(safe.safe_mode&&!safe.symbol_tools,"o modo seguro não sobe MCP: o índice de símbolos sai");
    }

    /// Os arquivos protegidos ficam fora das ferramentas do Claude.
    #[test] fn protected_files_are_off_limits_to_claude_tools() {
        let args=agent(AgentId::Claude,Value::Null).build_args();
        let guarded=guarding(&args,&[".env".into(),"secrets/**".into(),"/config/prod.yml".into(),"bad,pattern".into(),"!keep.env".into()]);
        let denied=guarded.windows(2).find(|pair|pair[0]=="--disallowed-tools").map(|pair|pair[1].clone()).expect("lista");
        let rules:Vec<&str>=denied.split(',').collect();
        for rule in ["AskUserQuestion","Read(.env)","Edit(.env)","Read(secrets/**)","Edit(secrets/**)","Read(./config/prod.yml)"] { assert!(rules.contains(&rule),"falta {rule}: {denied}"); }
        assert!(!denied.contains("bad")&&!denied.contains("keep"),"o que não cabe na lista fica com o firewall: {denied}");
        assert_eq!(guarding(&args,&[]),args);
        let env=guarding(&args,&[".env.*".into()]);
        let denied=env.windows(2).find(|pair|pair[0]=="--disallowed-tools").map(|pair|pair[1].clone()).expect("lista");
        assert!(denied.contains("Read(.env.local)")&&denied.contains("Read(.env.production)")&&!denied.contains(".env.example")&&!denied.contains("Read(.env.*)"),"{denied}");
    }

    /// O login lido da resposta de cada agente.
    #[test] fn the_login_state_is_read_from_each_agent() {
        assert_eq!(read_login(AgentId::Claude,true,r#"{"loggedIn": true, "authMethod": "oauth_token"}"#,""),Some(Login::In));
        assert_eq!(read_login(AgentId::Claude,true,r#"{"loggedIn": false}"#,""),Some(Login::Out));
        assert_eq!(read_login(AgentId::Claude,false,"error","boom"),None,"sem resposta clara, ninguém sai do roteamento");
        assert_eq!(read_login(AgentId::Codex,false,"","Not logged in"),Some(Login::Out));
        assert_eq!(read_login(AgentId::Codex,true,"Logged in using ChatGPT",""),Some(Login::In));
        assert!(!logged_out("agente-nunca-perguntado"));
    }

    #[test] fn codex_reads_stdin_and_only_opens_the_network_when_it_can_write() {
        let args=agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true})).args();
        let cleaned=|options:Value|->CodexOptions { serde_json::from_value(validate(&settings(vec![agent(AgentId::Codex,options)])).expect("válido").agents[1].options.clone()).expect("opções") };
        // O `read-only` guarda a escolha: é ela que vale quando o modo
        // desenvolvimento o sobe para escrever.
        assert!(cleaned(json!({"sandbox":"read-only","networkAccess":true})).network_access);
        assert!(!cleaned(json!({"sandbox":"danger-full-access","networkAccess":true})).network_access);
        assert!(!args.iter().any(|arg|arg.contains("network_access")),"lendo, não há rede");
        assert_eq!(args.last().map(String::as_str),Some("-"));
        assert_eq!(args.iter().rev().nth(1).map(String::as_str),Some(RESUME_THREAD),"a sessão entra logo antes do pedido");
        assert!(args.windows(2).any(|pair|pair==["--sandbox","read-only"]));
        let writing=agent(AgentId::Codex,json!({"sandbox":"workspace-write","networkAccess":true})).args();
        assert!(writing.windows(2).any(|pair|pair==["-c","sandbox_workspace_write.network_access=true"]));
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

    /// Sem terminal ninguém aprova nada: no modo desenvolvimento o que só lia
    /// passa a escrever no projeto, sem ganhar comandos nem a rede que a configuração não deu.
    #[test] fn development_lets_every_agent_write_to_the_project() {
        for options in [Value::Null,json!({"permissionMode":"default"}),json!({"permissionMode":"plan"})] {
            let claude=agent(AgentId::Claude,options).build_args();
            assert!(claude.windows(2).any(|pair|pair==["--permission-mode","acceptEdits"]),"{claude:?}");
        }
        for mode in ["auto","bypassPermissions","acceptEdits"] {
            assert!(agent(AgentId::Claude,json!({"permissionMode":mode})).build_args().windows(2).any(|pair|pair==["--permission-mode",mode]),"{mode} fica");
        }
        let codex=agent(AgentId::Codex,json!({"sandbox":"read-only","networkAccess":true})).build_args();
        assert!(codex.windows(2).any(|pair|pair==["--sandbox","workspace-write"]));
        assert!(codex.windows(2).any(|pair|pair==["-c","sandbox_workspace_write.network_access=true"]),"a rede ligada nas configurações vale ao escrever");
        let closed=agent(AgentId::Codex,json!({"sandbox":"read-only"})).build_args();
        assert!(!closed.iter().any(|arg|arg.contains("network_access")),"subir para escrever não abre a rede sozinho");
        let copilot=agent(AgentId::Copilot,Value::Null).build_args();
        assert!(copilot.windows(2).any(|pair|pair==["--allow-tool","write"])&&!copilot.iter().any(|arg|arg=="--allow-all-tools"));
        assert!(!agent(AgentId::Cursor,Value::Null).build_args().iter().any(|arg|arg=="--force"),"o Cursor não ganha comandos sem aprovação");
        let (providers,_)=to_config(&settings(vec![agent(AgentId::Claude,Value::Null)]));
        assert!(providers["claude"].args.windows(2).any(|pair|pair==["--permission-mode","acceptEdits"]));
        assert!(providers["claude"].for_planning().args.windows(2).any(|pair|pair==["--permission-mode","plan"]));
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

        let (providers,_)=to_config(&settings(vec![agent(AgentId::Codex,json!({"sandbox":"danger-full-access"}))]));
        assert!(providers["codex"].args.windows(2).any(|pair|pair==["--sandbox","danger-full-access"]),"o build não tira o que foi dado");
        let (providers,_)=to_config(&settings(vec![agent(AgentId::Codex,json!({"sandbox":"workspace-write"}))]));
        let codex=&providers["codex"];
        assert!(codex.args.windows(2).any(|pair|pair==["--sandbox","workspace-write"]),"o build usa o que foi configurado");
        assert!(codex.for_planning().args.windows(2).any(|pair|pair==["--sandbox","read-only"]));
    }

    /// Sem terminal, o que pede aprovação é negado: o mecanismo ligado vai
    /// liberado na linha de comando de cada agente, e o desligado não.
    #[test] fn mechanisms_reach_each_command_line() {
        let allowed=|args:&[String]|args.windows(2).find(|pair|pair[0]=="--allowedTools").map(|pair|pair[1].clone()).unwrap_or_default();
        let claude=agent(AgentId::Claude,Value::Null).args();
        assert_eq!(allowed(&claude),"WebSearch","a busca na web vem ligada");
        let claude=agent(AgentId::Claude,json!({"mechanisms":["webSearch","webFetch","shell"]})).args();
        assert_eq!(allowed(&claude),"WebSearch,WebFetch,Bash");
        assert!(!allowed(&agent(AgentId::Claude,json!({"mechanisms":["shell"]})).plan_args()).contains("Bash"),"o plano não roda comandos sem pergunta");
        assert!(!agent(AgentId::Claude,json!({"mechanisms":[]})).args().iter().any(|arg|arg=="--allowedTools"));

        let codex=agent(AgentId::Codex,Value::Null).args();
        assert!(codex.windows(2).any(|pair|pair==["-c","web_search=\"live\""]));
        let codex=agent(AgentId::Codex,json!({"mechanisms":[]})).args();
        assert!(codex.windows(2).any(|pair|pair==["-c","web_search=\"disabled\""]),"desligado não cai no cached do Codex");

        let copilot=agent(AgentId::Copilot,json!({"mechanisms":["webFetch","shell","githubTools"]})).args();
        for flag in ["--allow-all-urls","--enable-all-github-mcp-tools"] { assert!(copilot.iter().any(|arg|arg==flag),"falta {flag}"); }
        assert!(copilot.windows(2).any(|pair|pair==["--allow-tool","shell"]));
        assert!(!agent(AgentId::Copilot,Value::Null).args().iter().any(|arg|arg=="--allow-all-urls"),"desligado por padrão");
        assert!(!agent(AgentId::Copilot,json!({"mechanisms":["shell"]})).plan_args().windows(2).any(|pair|pair==["--allow-tool","shell"]));
        assert!(!agent(AgentId::Copilot,json!({"mechanisms":["shell"]})).without_unsafe_modes().args().windows(2).any(|pair|pair==["--allow-tool","shell"]),"safe_agents tira os comandos sem pergunta");
    }

    #[test] fn a_blocked_tool_wins_over_its_mechanism() {
        let checked=agent(AgentId::Claude,json!({"blockedTools":["WebSearch"],"mechanisms":["webSearch","webFetch"]})).checked(&HashSet::new()).expect("válido");
        assert_eq!(checked["mechanisms"],json!(["webFetch"]));
        let checked=agent(AgentId::Copilot,json!({"blockedTools":["shell"],"mechanisms":["shell","webFetch"]})).checked(&HashSet::new()).expect("válido");
        assert_eq!(checked["mechanisms"],json!(["webFetch"]));
        assert!(agent(AgentId::Codex,json!({"mechanisms":["shell"]})).checked(&HashSet::new()).is_err(),"o Codex não sabe ligar esse");
        assert!(mechanisms_of(AgentId::Cursor).is_empty());
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

    /// O liberado para um pedido chega à linha de comando de cada agente do
    /// jeito que a CLI dele sabe, e a política com `safe_agents` ainda tira.
    #[test] fn grants_reach_each_agent_and_safe_agents_still_removes_them() {
        let grants=Grants{commands:vec!["git add -A".into()],..Default::default()};
        let claude=agent(AgentId::Claude,Value::Null).with_grants(&grants);
        let args=claude.build_args();
        let allowed=args.iter().position(|arg|arg=="--allowedTools").map(|at|args[at+1].clone()).expect("allowedTools");
        assert!(allowed.contains("Bash(git add -A:*)"),"{allowed}");
        assert!(!claude.without_unsafe_modes().build_args().join(" ").contains("Bash("),"safe_agents tira o liberado");
        assert!(!claude.plan_args().join(" ").contains("Bash("),"o planejamento não roda comando");
        let codex=agent(AgentId::Codex,Value::Null).with_grants(&grants).build_args();
        assert!(codex.windows(2).any(|pair|pair[0]=="--sandbox"&&pair[1]=="danger-full-access"),"git abre o .git no Codex");
        let network=agent(AgentId::Codex,Value::Null).with_grants(&Grants{commands:vec!["npm install".into()],..Default::default()}).build_args();
        assert!(network.iter().any(|arg|arg=="sandbox_workspace_write.network_access=true"));
        let copilot=agent(AgentId::Copilot,Value::Null).with_grants(&grants).build_args();
        assert!(copilot.windows(2).any(|pair|pair[0]=="--allow-tool"&&pair[1]=="shell(git add)"));
        let cursor=agent(AgentId::Cursor,Value::Null).with_grants(&Grants{shell:true,..Default::default()}).build_args();
        assert!(cursor.iter().any(|arg|arg=="--force"));
        // Sem nada liberado, nada muda.
        assert_eq!(agent(AgentId::Codex,Value::Null).with_grants(&Grants::default()).build_args(),agent(AgentId::Codex,Value::Null).build_args());
        assert!(!agent(AgentId::Claude,Value::Null).build_args().join(" ").contains("Bash("));
        // A política que bloqueia o shell do Claude também tira o liberado.
        assert!(!claude.without_mechanisms(&["claude/shell".into()]).build_args().join(" ").contains("Bash("));
    }

    #[test] fn the_always_prefix_keeps_the_program_and_subcommand() {
        assert_eq!(command_prefix("git add -A"),"git add");
        assert_eq!(command_prefix("npm run build"),"npm run");
        assert_eq!(command_prefix("make lint"),"make lint");
        assert_eq!(command_prefix("pytest tests/unit"),"pytest");
        assert_eq!(command_prefix("ls -la"),"ls");
    }

    #[test] fn kilo_lists_provider_slash_model_lines_only() {
        let found=parse_kilo_listing("Available models:\nanthropic/claude-sonnet-4\n  openai/gpt-4o  \nnot a model\nanthropic/claude-sonnet-4\n");
        assert_eq!(found.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),["anthropic/claude-sonnet-4","openai/gpt-4o"]);
    }

    /// O gateway só responde por texto: o modelo dele nunca leva `code` nem `tools`.
    #[test] fn gateway_models_never_get_code_or_tools() {
        for id in [AgentId::Openrouter,AgentId::Litellm] {
            let known=KnownModel::named(id,"anthropic/claude-sonnet-4",None,None);
            assert!(known.capabilities.iter().all(|capability|capability!="code"&&capability!="tools"),"{id:?}");
        }
        let mut loaded=settings(vec![agent(AgentId::Litellm,json!({}))]);
        for model in loaded.models.iter_mut().filter(|model|model.agent==AgentId::Litellm) { model.capabilities=strings(&["code","tools"]); }
        let checked=validate(&loaded).expect("validação");
        assert!(checked.models.iter().filter(|model|model.agent==AgentId::Litellm).all(|model|model.capabilities==["chat"]));
    }

    /// A chave do gateway vai para a tabela local e nunca para as opções
    /// gravadas, nem para o que volta à tela.
    #[test] fn the_gateway_key_stays_out_of_the_saved_options() {
        let mut connection=memory();
        let loaded=settings(vec![agent(AgentId::Litellm,json!({"baseUrl":"http://localhost:4000/v1","apiKey":"sk-segredo"}))]);
        let saved=save(&mut connection,&loaded).expect("gravação");
        let options=saved.agents.iter().find(|entry|entry.id==AgentId::Litellm).expect("litellm").options.to_string();
        assert!(!options.contains("sk-segredo")&&!options.contains("apiKey"),"{options}");
        assert!(options.contains("\"hasKey\":true"),"{options}");
        let stored:String=connection.query_row("SELECT options FROM llm_agents WHERE id='litellm'",[],|row|row.get(0)).expect("linha");
        assert!(!stored.contains("sk-segredo"));
        assert_eq!(secret(AgentId::Litellm).as_deref(),Some("sk-segredo"));
        let cleared=settings(vec![agent(AgentId::Litellm,json!({"baseUrl":"http://localhost:4000/v1","clearKey":true}))]);
        save(&mut connection,&cleared).expect("gravação");
        assert_eq!(secret(AgentId::Litellm),None);
    }

    #[test] fn openrouter_needs_a_key_to_be_turned_on() {
        let mut connection=memory();
        let _=connection.execute("DELETE FROM llm_secrets WHERE agent='openrouter'",[]);
        let mut loaded=settings(vec![agent(AgentId::Claude,Value::Null),agent(AgentId::Openrouter,json!({}))]);
        let error=save(&mut connection,&loaded).expect_err("sem chave");
        assert!(format!("{error:?}").contains("settings.keyRequired"),"{error:?}");
        loaded.agents.iter_mut().find(|entry|entry.id==AgentId::Openrouter).expect("openrouter").enabled=false;
        save(&mut connection,&loaded).expect("desligado não pede chave");
    }

    fn custom(slug:&str,options:Value)->AgentSettings { AgentSettings{id:AgentId::parse(&format!("mod-{slug}")).expect("id"),enabled:true,command:"local-ai".into(),timeout:300,options} }
    fn custom_model(agent:AgentId,name:&str)->AgentModel { AgentModel{agent,model:name.into(),enabled:true,capabilities:strings(&["chat","code"]),cost_class:"medium".into(),speed:"medium".into(),context_window:128_000} }

    /// Os mods criados têm o próprio prefixo: nunca colidem com um mod do app,
    /// e o id viaja como texto, como o dos outros.
    #[test] fn custom_mod_ids_have_their_own_namespace() {
        assert!(AgentId::parse("mod-local-ai").is_ok_and(|id|id.is_custom()));
        assert_eq!(AgentId::parse("claude").expect("claude"),AgentId::Claude);
        assert!(!AgentId::Claude.is_custom());
        for bad in ["mod-".to_string(),"mod--x".into(),"mod-X".into(),"mod-a_b".into(),"local".into(),"mod-x-".into(),format!("mod-{}","a".repeat(33))] { assert!(AgentId::parse(&bad).is_err(),"{bad}"); }
        assert_eq!(serde_json::to_string(&AgentId::parse("mod-a").expect("id")).expect("json"),"\"mod-a\"");
        assert!(serde_json::from_str::<AgentId>("\"nope\"").is_err());
        assert_eq!(AgentId::parse("mod-a").expect("id"),AgentId::parse("mod-a").expect("id"),"o mesmo id é o mesmo");
    }

    /// O mod criado grava e volta depois dos mods do app, pelo nome; vira
    /// provedor como os outros; tirado da lista, sai do banco com os modelos e
    /// a chave.
    #[test] fn a_custom_mod_is_saved_listed_after_the_built_ins_and_removed() {
        let mut connection=memory();
        let writer=custom("writer",json!({"name":"Zeta","kind":"cli","args":["--model","{model}","{prompt}"],"planArgs":["--read-only","{prompt}"],"edits":true}));
        let proxy=AgentSettings{command:String::new(),..custom("proxy",json!({"name":"Alpha","kind":"api","protocol":"openai","baseUrl":"http://localhost:8080/v1","apiKey":"sk-mod"}))};
        let mut wanted=settings(vec![agent(AgentId::Claude,Value::Null),writer.clone(),proxy.clone()]);
        wanted.models.extend([custom_model(writer.id,"local-1"),custom_model(proxy.id,"remote-1")]);
        save(&mut connection,&wanted).expect("gravação");
        let loaded=load(&connection).expect("leitura");
        let ids=loaded.agents.iter().map(|entry|entry.id.key()).collect::<Vec<_>>();
        assert_eq!(&ids[..7],AgentId::ALL.map(AgentId::key));
        assert_eq!(&ids[7..],["mod-proxy","mod-writer"],"pelo nome: Alpha antes de Zeta");
        let capabilities=|id:AgentId|loaded.models.iter().find(|model|model.agent==id).expect("modelo").capabilities.clone();
        assert_eq!(capabilities(proxy.id),["chat"],"o mod de API só responde por texto");
        assert_eq!(capabilities(writer.id),["chat","code"]);
        let stored:String=connection.query_row("SELECT options FROM llm_agents WHERE id='mod-proxy'",[],|row|row.get(0)).expect("linha");
        assert!(!stored.contains("sk-mod")&&!stored.contains("apiKey"),"{stored}");
        let key:String=connection.query_row("SELECT secret FROM llm_secrets WHERE agent='mod-proxy'",[],|row|row.get(0)).expect("chave");
        assert_eq!(key,"sk-mod");
        let (providers,models)=to_config(&loaded);
        assert_eq!((providers["mod-writer"].kind.as_str(),providers["mod-writer"].plan_args.clone()),("cli",strings(&["--read-only","{prompt}"])));
        assert_eq!(providers["mod-proxy"].kind,"openai-compatible");
        assert!(models.contains_key("mod-proxy/remote-1")&&models.contains_key("mod-writer/local-1"));

        let mut fewer=loaded.clone();
        fewer.agents.retain(|entry|entry.id!=writer.id);
        save(&mut connection,&fewer).expect("gravação");
        let count=|connection:&Connection,sql:&str|connection.query_row(sql,[],|row|row.get::<_,i64>(0)).expect("contagem");
        assert_eq!((count(&connection,"SELECT COUNT(*) FROM llm_agents WHERE id='mod-writer'"),count(&connection,"SELECT COUNT(*) FROM llm_models WHERE agent='mod-writer'")),(0,0),"o modelo do mod apagado sai junto");
        let mut none=load(&connection).expect("leitura");
        none.agents.retain(|entry|!entry.id.is_custom());
        save(&mut connection,&none).expect("gravação");
        assert_eq!(count(&connection,"SELECT COUNT(*) FROM llm_secrets WHERE agent='mod-proxy'"),0,"a chave sai com o mod");
        assert!(load(&connection).expect("leitura").agents.iter().all(|entry|!entry.id.is_custom()));
    }

    #[test] fn custom_mods_are_checked_before_saving() {
        let mut connection=memory();
        let claude=||agent(AgentId::Claude,Value::Null);
        let nameless=custom("nameless",json!({"name":" ","kind":"cli","args":["{prompt}"]}));
        let error=save(&mut connection,&settings(vec![claude(),nameless])).expect_err("sem nome");
        assert!(format!("{error:?}").contains("mods.nameInvalid"),"{error:?}");
        let commandless=AgentSettings{command:String::new(),..custom("commandless",json!({"name":"C","kind":"cli"}))};
        let error=save(&mut connection,&settings(vec![claude(),commandless])).expect_err("sem programa");
        assert!(format!("{error:?}").contains("settings.commandRequired"),"{error:?}");
        let keyless=AgentSettings{command:String::new(),..custom("keyless",json!({"name":"K","kind":"api","baseUrl":"https://api.example.com/v1","keyRequired":true}))};
        let error=save(&mut connection,&settings(vec![claude(),keyless])).expect_err("sem chave");
        assert!(format!("{error:?}").contains("settings.keyRequired"),"{error:?}");
        let many=(0..=crate::mods::custom::MAX_MODS).map(|n|custom(&format!("many{n}"),json!({"name":format!("M{n}"),"kind":"cli","args":["{prompt}"]})));
        let error=validate(&settings([claude()].into_iter().chain(many).collect())).expect_err("demais");
        assert!(format!("{error:?}").contains("mods.tooMany"),"{error:?}");
    }
}
