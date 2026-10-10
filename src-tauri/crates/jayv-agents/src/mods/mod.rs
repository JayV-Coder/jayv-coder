//! Os mods de integração com LLM. Cada integração — Claude Code, Codex,
//! Copilot, Cursor, Kilo Code, OpenRouter, LiteLLM e os mods que a pessoa cria
//! na tela — é um mod: implementa a mesma interface (`LlmMod`), mora no seu
//! próprio módulo e se registra aqui (`of`). O resto do núcleo não sabe qual
//! integração é qual: pergunta ao mod como montar a linha de comando, como
//! travá-la no planejamento, como entregar os servidores MCP, como listar os
//! modelos e como falar com o provedor.
//!
//! Um mod novo que vem com o app é um módulo nesta pasta (um `TypedMod` com as
//! opções tipadas dele) e uma linha em `of` e em `AgentId::ALL`. Um mod que a
//! pessoa cria é um `custom::CustomMod`: a definição (linha de comando ou API)
//! mora nas opções do próprio agente, e sincroniza como elas.

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod custom;
pub mod gateway;
pub mod kilo;

use crate::config::ProviderConfig;
use crate::llm::{parse, without_shell, AgentId, AgentSettings, Grants, KnownModel, Login, MCP, SHELL};
use crate::mcp::McpServer;
use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// Como o mod fala com o modelo: abrindo um programa na máquina (`Cli`) ou por
/// HTTP, com endereço e chave (`Api`).
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize)]
#[serde(rename_all="lowercase")]
pub enum ModKind { Cli, Api }

/// O que todo mod sabe fazer, com as opções gravadas do agente (o JSON da
/// tela). Os mods que vêm com o app implementam `TypedMod`, que lê e grava as
/// opções tipadas; esta é a forma que o núcleo enxerga.
pub trait LlmMod: Send + Sync {
    fn id(&self)->AgentId;
    /// O nome nas mensagens de erro e nas abas. O mod criado o lê das opções
    /// (`Value::Null` dá o id).
    fn label(&self,options:&Value)->String;
    fn kind(&self,options:&Value)->ModKind;
    /// Se o mod edita o projeto (os modelos dele podem ter `code` e `tools`)
    /// ou só responde por texto.
    fn edits_project(&self,options:&Value)->bool;
    /// Veio com o app (não se apaga) ou foi criado pela pessoa.
    fn built_in(&self)->bool { true }
    /// O executável padrão; vazio no mod de API.
    fn binary(&self)->&'static str;
    /// O recurso do plano sem o qual o mod fica desligado (`features.rs`).
    fn feature(&self)->Option<&'static str>;
    /// Os mecanismos que a CLI dele sabe ligar por flag.
    fn mechanisms(&self)->&'static [&'static str];
    /// A janela de contexto de um modelo que não diz a dele.
    fn default_context(&self)->usize;
    /// Os modelos de fábrica, antes de a descoberta responder.
    fn starter_models(&self)->&'static [&'static str];

    fn fresh_options(&self)->Value;
    /// As opções gravadas com o que faltava preenchido pelo padrão. O que não
    /// se lê fica como veio, para a validação dizer o quê.
    fn filled(&self,options:Value)->Value;
    /// As opções limpas e conferidas, ou o motivo de não serem aceitas.
    fn checked(&self,options:&Value,models:&HashSet<&str>)->Result<Value>;
    fn args(&self,options:&Value)->Vec<String>;
    /// A linha do modo desenvolvimento (o que só lê sobe para o degrau que
    /// escreve no projeto).
    fn build_args(&self,options:&Value)->Vec<String>;
    /// A linha do modo planejamento, presa em somente leitura.
    fn plan_args(&self,options:&Value)->Vec<String>;
    /// Sem os modos sem trava (a política com `safe_agents`).
    fn without_unsafe_modes(&self,options:&Value)->Value;
    /// Com os servidores MCP do pedido, se o "Aprovar servidores MCP" está
    /// ligado. `None`: nada muda.
    fn with_mcp(&self,options:&Value,servers:Vec<McpServer>)->Option<Value>;
    /// Com o que foi liberado para um pedido só. `None`: nada muda.
    fn with_grants(&self,options:&Value,grants:&Grants)->Option<Value>;
    /// Sem poder rodar os comandos bloqueados. `None`: nada muda.
    fn without_commands(&self,options:&Value,blocked:&[String])->Option<Value>;
    /// Sem os mecanismos bloqueados (`agente/mecanismo`).
    fn without_mechanisms(&self,options:&Value,blocked:&[String])->Value;
    /// Os servidores MCP que o próprio JayV entrega (arquivo, ambiente ou o
    /// cliente MCP do app): os que não os recebem pela linha de comando.
    fn delivered_mcp(&self,options:&Value)->Vec<McpServer>;
    /// O provedor como o orquestrador o entende.
    fn provider(&self,agent:&AgentSettings)->ProviderConfig;
    /// As opções depois de a descoberta trocar a lista de modelos (o modelo
    /// reserva que saiu da lista sai das opções). `None`: nada muda.
    fn adopted(&self,options:&Value,found:&[KnownModel])->Option<Value>;
    /// Os argumentos que fazem a CLI listar os modelos sem gastar crédito.
    fn listing_args(&self)->&'static [&'static str];
    fn parse_listing(&self,text:&str)->Vec<KnownModel>;
    /// Os argumentos que perguntam à CLI se ela está logada.
    fn login_args(&self)->&'static [&'static str];
    fn read_login(&self,success:bool,stdout:&str,stderr:&str)->Option<Login>;
    /// Os subcomandos cuja ajuda se lê de antemão (`llm::understood`).
    fn help_subcommands(&self)->Option<&'static [&'static str]>;
    /// A linha com os arquivos protegidos fora do alcance das ferramentas.
    fn guard(&self,args:&[String],deny:&[String])->Vec<String>;
    /// O mod de API que não roda sem chave guardada.
    fn requires_key(&self,options:&Value)->bool;
}

/// Um mod com as opções tipadas. Quase tudo tem padrão: o mod diz só o que é
/// dele — a linha de comando, como ela trava, onde moram os mecanismos e os
/// servidores MCP nas opções.
pub trait TypedMod: Send + Sync + 'static {
    type Options: Serialize + DeserializeOwned + Default + Clone;
    /// O mod recebe os servidores MCP pelo JayV (arquivo, ambiente ou cliente
    /// MCP do app), e não pela linha de comando.
    const DELIVERS_MCP:bool=false;

    fn id(&self)->AgentId;
    fn label(&self,_options:&Self::Options)->String;
    fn kind(&self,_options:&Self::Options)->ModKind { ModKind::Cli }
    fn edits_project(&self,options:&Self::Options)->bool { self.kind(options)==ModKind::Cli }
    fn built_in(&self)->bool { true }
    fn binary(&self)->&'static str { self.id().key() }
    fn feature(&self)->Option<&'static str> { None }
    fn mechanisms(&self)->&'static [&'static str] { &[] }
    fn default_context(&self)->usize { 200_000 }
    fn starter_models(&self)->&'static [&'static str];

    fn fresh(&self)->Self::Options { Self::Options::default() }
    fn fill(&self,options:Self::Options)->Self::Options { options }
    fn check(&self,options:Self::Options,_models:&HashSet<&str>)->Result<Self::Options> { Ok(options) }
    fn line(&self,options:&Self::Options)->Vec<String>;
    fn build_line(&self,options:&Self::Options)->Vec<String> { self.line(options) }
    fn plan_line(&self,options:&Self::Options)->Vec<String> { self.line(options) }
    /// O que é só deste mod ao tirar os modos sem trava. Os mecanismos perdem
    /// o `shell`, as liberações do pedido e os servidores MCP saem no padrão.
    fn lock(&self,_options:&mut Self::Options) {}
    /// O "Aprovar servidores MCP" e os servidores do pedido, nas opções.
    fn mcp<'a>(&self,options:&'a mut Self::Options)->(&'a mut bool,&'a mut Vec<McpServer>);
    /// Os mecanismos ligados, se o mod os tem.
    fn mechanisms_mut<'a>(&self,_options:&'a mut Self::Options)->Option<&'a mut Vec<String>> { None }
    /// As liberações de um pedido só, se o mod as tem.
    fn granted_mut<'a>(&self,_options:&'a mut Self::Options)->Option<&'a mut Vec<String>> { None }
    fn grant(&self,_options:Self::Options,_grants:&Grants)->Option<Self::Options> { None }
    fn deny(&self,_options:Self::Options,_blocked:&[String])->Option<Self::Options> { None }
    fn provider_of(&self,agent:&AgentSettings,options:&Self::Options)->ProviderConfig {
        let mut options=options.clone();
        let delivered=if Self::DELIVERS_MCP { self.mcp(&mut options).1.clone() } else { vec![] };
        cli_provider(agent,&delivered)
    }
    fn adopt(&self,_options:&mut Self::Options,_found:&[KnownModel])->bool { false }
    fn listing_args(&self)->&'static [&'static str] { &[] }
    fn parse_listing(&self,_text:&str)->Vec<KnownModel> { vec![] }
    fn login_args(&self)->&'static [&'static str] { &[] }
    fn read_login(&self,_success:bool,_stdout:&str,_stderr:&str)->Option<Login> { None }
    fn help_subcommands(&self)->Option<&'static [&'static str]> { None }
    fn guard(&self,args:&[String],_deny:&[String])->Vec<String> { args.to_vec() }
    fn requires_key(&self,_options:&Self::Options)->bool { false }
}

fn value<T:Serialize>(options:T)->Value { serde_json::to_value(options).unwrap_or_default() }

impl<T:TypedMod> LlmMod for T {
    fn id(&self)->AgentId { TypedMod::id(self) }
    fn label(&self,options:&Value)->String { TypedMod::label(self,&parse::<T::Options>(options).unwrap_or_default()) }
    fn kind(&self,options:&Value)->ModKind { TypedMod::kind(self,&parse::<T::Options>(options).unwrap_or_default()) }
    fn edits_project(&self,options:&Value)->bool { TypedMod::edits_project(self,&parse::<T::Options>(options).unwrap_or_default()) }
    fn built_in(&self)->bool { TypedMod::built_in(self) }
    fn binary(&self)->&'static str { TypedMod::binary(self) }
    fn feature(&self)->Option<&'static str> { TypedMod::feature(self) }
    fn mechanisms(&self)->&'static [&'static str] { TypedMod::mechanisms(self) }
    fn default_context(&self)->usize { TypedMod::default_context(self) }
    fn starter_models(&self)->&'static [&'static str] { TypedMod::starter_models(self) }

    fn fresh_options(&self)->Value { value(self.fresh()) }
    fn filled(&self,options:Value)->Value { parse::<T::Options>(&options).map(|typed|value(self.fill(typed))).unwrap_or(options) }
    fn checked(&self,options:&Value,models:&HashSet<&str>)->Result<Value> { Ok(serde_json::to_value(self.check(parse::<T::Options>(options)?,models)?)?) }
    fn args(&self,options:&Value)->Vec<String> { self.line(&parse::<T::Options>(options).unwrap_or_default()) }
    fn build_args(&self,options:&Value)->Vec<String> { self.build_line(&parse::<T::Options>(options).unwrap_or_default()) }
    fn plan_args(&self,options:&Value)->Vec<String> { self.plan_line(&parse::<T::Options>(options).unwrap_or_default()) }

    fn without_unsafe_modes(&self,options:&Value)->Value {
        let mut options=parse::<T::Options>(options).unwrap_or_default();
        self.lock(&mut options);
        if let Some(mechanisms)=self.mechanisms_mut(&mut options) { *mechanisms=without_shell(mechanisms); }
        if let Some(granted)=self.granted_mut(&mut options) { granted.clear(); }
        let (approve,servers)=self.mcp(&mut options);
        *approve=false;
        servers.clear();
        value(options)
    }

    fn with_mcp(&self,options:&Value,servers:Vec<McpServer>)->Option<Value> {
        let mut options=parse::<T::Options>(options).unwrap_or_default();
        let (approve,mine)=self.mcp(&mut options);
        if !*approve { return None; }
        *mine=servers;
        Some(value(options))
    }

    fn with_grants(&self,options:&Value,grants:&Grants)->Option<Value> { self.grant(parse::<T::Options>(options).unwrap_or_default(),grants).map(value) }
    fn without_commands(&self,options:&Value,blocked:&[String])->Option<Value> { self.deny(parse::<T::Options>(options).unwrap_or_default(),blocked).map(value) }

    fn without_mechanisms(&self,options:&Value,blocked:&[String])->Value {
        let key=TypedMod::id(self).key();
        let gone=|mechanism:&str|blocked.contains(&format!("{key}/{mechanism}"));
        let mut options=parse::<T::Options>(options).unwrap_or_default();
        if let Some(mechanisms)=self.mechanisms_mut(&mut options) { mechanisms.retain(|mechanism|!gone(mechanism)); }
        // Os comandos liberados para um pedido também são o mecanismo `shell`.
        if gone(SHELL) { if let Some(granted)=self.granted_mut(&mut options) { granted.clear(); } }
        // `mcp` é o "Aprovar servidores MCP": bloqueado, nenhum servidor chega.
        if gone(MCP) { let (approve,servers)=self.mcp(&mut options); *approve=false; servers.clear(); }
        value(options)
    }

    fn delivered_mcp(&self,options:&Value)->Vec<McpServer> {
        if !T::DELIVERS_MCP { return vec![]; }
        let mut options=parse::<T::Options>(options).unwrap_or_default();
        self.mcp(&mut options).1.clone()
    }

    fn provider(&self,agent:&AgentSettings)->ProviderConfig { self.provider_of(agent,&parse::<T::Options>(&agent.options).unwrap_or_default()) }

    fn adopted(&self,options:&Value,found:&[KnownModel])->Option<Value> {
        let mut options=parse::<T::Options>(options).unwrap_or_default();
        self.adopt(&mut options,found).then(||value(options))
    }

    fn listing_args(&self)->&'static [&'static str] { TypedMod::listing_args(self) }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { TypedMod::parse_listing(self,text) }
    fn login_args(&self)->&'static [&'static str] { TypedMod::login_args(self) }
    fn read_login(&self,success:bool,stdout:&str,stderr:&str)->Option<Login> { TypedMod::read_login(self,success,stdout,stderr) }
    fn help_subcommands(&self)->Option<&'static [&'static str]> { TypedMod::help_subcommands(self) }
    fn guard(&self,args:&[String],deny:&[String])->Vec<String> { TypedMod::guard(self,args,deny) }
    fn requires_key(&self,options:&Value)->bool { TypedMod::requires_key(self,&parse::<T::Options>(options).unwrap_or_default()) }
}

/// O agente de linha de comando como provedor: o programa, o prazo, as duas
/// linhas (desenvolvimento e planejamento) e os servidores MCP que o JayV
/// entrega a ele.
pub fn cli_provider(agent:&AgentSettings,delivered:&[McpServer])->ProviderConfig {
    ProviderConfig{
        enabled:agent.enabled,kind:"cli".into(),command:Some(agent.command.clone()),timeout:agent.timeout,
        args:agent.build_args(),plan_args:agent.plan_args(),
        mcp:delivered.iter().filter_map(|server|serde_json::to_value(server).ok()).collect(),
        ..ProviderConfig::default()
    }
}

/// O mod de API como provedor HTTP: o protocolo (`openai-compatible` ou
/// `anthropic`), o endereço e a chave guardada (`llm::secret`). Sem a chave
/// que ele exige, fica desligado.
pub fn api_provider(agent:&AgentSettings,protocol:&str,base_url:String,key_required:bool,delivered:&[McpServer])->ProviderConfig {
    let key=crate::llm::secret(agent.id);
    ProviderConfig{
        enabled:agent.enabled&&(!key_required||key.is_some()),
        kind:protocol.into(),base_url:Some(base_url),api_key:key,timeout:agent.timeout,
        mcp:delivered.iter().filter_map(|server|serde_json::to_value(server).ok()).collect(),
        ..ProviderConfig::default()
    }
}

/// O prefixo dos mods criados pela pessoa: nenhum mod do app começa assim, e
/// um mod novo do app nunca colide com um criado.
pub const CUSTOM_PREFIX:&str="mod-";

static CLAUDE:claude::Claude=claude::Claude;
static CODEX:codex::Codex=codex::Codex;
static COPILOT:copilot::Copilot=copilot::Copilot;
static CURSOR:cursor::Cursor=cursor::Cursor;
static KILO:kilo::Kilo=kilo::Kilo;
static OPENROUTER:gateway::Gateway=gateway::Gateway::OPENROUTER;
static LITELLM:gateway::Gateway=gateway::Gateway::LITELLM;

/// O mod de um agente. Os que vêm com o app são fixos; um mod criado
/// (`mod-…`) é o `custom::CustomMod`, que lê a definição das opções.
pub fn of(id:AgentId)->&'static dyn LlmMod {
    match id {
        AgentId::Claude=>&CLAUDE,
        AgentId::Codex=>&CODEX,
        AgentId::Copilot=>&COPILOT,
        AgentId::Cursor=>&CURSOR,
        AgentId::Kilo=>&KILO,
        AgentId::Openrouter=>&OPENROUTER,
        AgentId::Litellm=>&LITELLM,
        _=>custom::of(id),
    }
}
