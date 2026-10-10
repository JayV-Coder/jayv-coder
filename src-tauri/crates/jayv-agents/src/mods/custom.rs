//! Os mods que a pessoa cria em Configurações › Mods. São de dois tipos:
//!
//! - **Linha de comando** (`cli`): um programa na máquina, com os argumentos
//!   que a pessoa escreve. `{model}` vira o modelo escolhido pelo Jev, em
//!   qualquer lugar de um argumento; `{prompt}`, sozinho num argumento, vira o
//!   pedido (sem ele, o pedido vai pela entrada padrão). O texto que a CLI
//!   escreve é a resposta. Se ela escreve no projeto, a pessoa diz também a
//!   linha do planejamento, que não escreve: é a que vale no modo
//!   planejamento e sempre que a política da organização aperta.
//! - **API** (`api`): um endereço compatível com a API da OpenAI ou com a da
//!   Anthropic, com chave opcional. Só responde por texto, como os gateways.
//!
//! A definição mora nas opções do próprio agente (`llm_agents.options`), com o
//! id `mod-<nome>`: sincroniza com as outras configurações, e uma versão
//! antiga do app, que não conhece o id, só pula a linha. A chave da API mora
//! na tabela local `llm_secrets`, como a dos gateways.

use super::{api_provider, cli_provider, gateway::checked_url, LlmMod, ModKind, TypedMod, CUSTOM_PREFIX};
use crate::config::ProviderConfig;
use crate::i18n::Text;
use crate::llm::{AgentId, AgentSettings, KnownModel};
use crate::mcp::McpServer;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, PoisonError};

pub const CLI:&str="cli";
pub const API:&str="api";
/// A API compatível com a da OpenAI (`/chat/completions`, `/models`).
pub const OPENAI:&str="openai";
/// A API de mensagens da Anthropic (`/messages`).
pub const ANTHROPIC:&str="anthropic";

/// Quantos mods criados um ambiente aceita.
pub const MAX_MODS:usize=20;
/// O tamanho do nome na tela.
pub const MAX_NAME:usize=40;
/// Quantos argumentos uma linha aceita, e o tamanho de cada um.
const MAX_ARGS:usize=64;
const MAX_ARG:usize=1_000;
const MODEL:&str="{model}";
const PROMPT:&str="{prompt}";

/// A definição de um mod criado, nas opções do agente.
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CustomOptions {
    /// O nome na tela.
    pub name:String,
    /// `cli` ou `api`; não muda depois de criado.
    pub kind:String,
    /// Linha de comando: os argumentos do modo desenvolvimento.
    pub args:Vec<String>,
    /// Linha de comando: os argumentos do modo planejamento, que não escrevem
    /// no projeto. Vazio, o planejamento usa a mesma linha (só vale para o mod
    /// que não escreve no projeto).
    pub plan_args:Vec<String>,
    /// Linha de comando: o mod escreve no projeto (os modelos podem ter
    /// `code` e `tools`). Desligado, só responde por texto.
    pub edits:bool,
    /// API: `openai` ou `anthropic`.
    pub protocol:String,
    /// API: o endereço base (`…/v1`).
    pub base_url:String,
    /// API: a chave nova, só da tela para o salvar.
    #[serde(skip_serializing)]
    pub api_key:Option<String>,
    /// API: apaga a chave guardada.
    #[serde(skip_serializing)]
    pub clear_key:bool,
    /// API: há chave guardada (só a tela lê).
    pub has_key:bool,
    /// API: sem chave guardada, o mod não roda.
    pub key_required:bool,
    /// "Aprovar servidores MCP" (só nos mods de API com o protocolo da
    /// OpenAI, que o JayV chama como cliente MCP).
    pub approve_mcps:bool,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}

impl Default for CustomOptions {
    fn default()->Self { Self{name:String::new(),kind:CLI.into(),args:vec![],plan_args:vec![],edits:false,protocol:OPENAI.into(),base_url:String::new(),api_key:None,clear_key:false,has_key:false,key_required:false,approve_mcps:false,mcp:vec![]} }
}

/// Um mod por id, criado uma vez. Os ids já são internados (`AgentId::parse`),
/// com teto: o número de mods guardados aqui tem o mesmo teto.
static MODS:LazyLock<Mutex<HashMap<AgentId,&'static CustomMod>>>=LazyLock::new(Default::default);

pub fn of(id:AgentId)->&'static dyn LlmMod {
    let mut mods=MODS.lock().unwrap_or_else(PoisonError::into_inner);
    *mods.entry(id).or_insert_with(||Box::leak(Box::new(CustomMod{id})))
}

/// Separa a linha que a pessoa escreveu em argumentos, como o shell faria com
/// aspas simples e duplas — sem expandir nada.
pub fn split_line(line:&str)->Vec<String> {
    let mut args=Vec::new();
    let mut current=String::new();
    let mut quote:Option<char>=None;
    let mut started=false;
    for char in line.chars() {
        match quote {
            Some(open) if char==open=>quote=None,
            Some(_)=>current.push(char),
            None if char=='"'||char=='\''=>{ quote=Some(char); started=true; }
            None if char.is_whitespace()=>{ if started { args.push(std::mem::take(&mut current)); started=false; } }
            None=>{ current.push(char); started=true; }
        }
    }
    if started { args.push(current); }
    args
}

/// Cada `{nome}` de um argumento (letras, dígitos e `_` entre chaves).
fn placeholders(arg:&str)->impl Iterator<Item=&str> {
    arg.match_indices('{').filter_map(move |(start,_)|{
        let length=arg[start..].find('}')?;
        let token=&arg[start..=start+length];
        (token.len()>2&&token[1..token.len()-1].chars().all(|char|char.is_ascii_alphanumeric()||char=='_')).then_some(token)
    })
}

/// Os argumentos limpos: sem vazio, sem quebra de linha, e só com os lugares
/// `{model}` (em qualquer parte) e `{prompt}` (sozinho). Os outros lugares
/// (`{resume}`, `{effort}`…) são da montagem dos mods do app e mudariam a
/// linha sem a pessoa saber.
fn checked_args(args:&[String],name:&str)->Result<Vec<String>> {
    if args.len()>MAX_ARGS { bail!(Text::new("mods.argsTooMany").with("name",name).with("max",MAX_ARGS as u32)); }
    let mut clean=Vec::with_capacity(args.len());
    for arg in args {
        if arg.is_empty()||arg.chars().count()>MAX_ARG||arg.chars().any(char::is_control) { bail!(Text::new("mods.argInvalid").with("name",name)); }
        let rest=if arg==PROMPT { String::new() } else { arg.replace(MODEL,"") };
        if let Some(token)=placeholders(&rest).next() { bail!(Text::new("mods.placeholder").with("name",name).with("token",token)); }
        clean.push(arg.clone());
    }
    Ok(clean)
}

impl CustomOptions {
    fn checked(mut self)->Result<Self> {
        self.name=self.name.trim().to_string();
        if self.name.is_empty()||self.name.chars().count()>MAX_NAME||self.name.chars().any(char::is_control) { bail!(Text::new("mods.nameInvalid").with("max",MAX_NAME as u32)); }
        let name=self.name.clone();
        match self.kind.as_str() {
            CLI=>{
                self.args=checked_args(&self.args,&name)?;
                self.plan_args=checked_args(&self.plan_args,&name)?;
                // O mod que escreve precisa de uma linha que não escreva: é a
                // do planejamento e a que vale quando a política aperta.
                if self.edits&&self.plan_args.is_empty() { bail!(Text::new("mods.planRequired").with("name",&name)); }
                self.protocol=OPENAI.into();
                self.base_url.clear();
                self.key_required=false;
                self.has_key=false;
                self.approve_mcps=false;
            }
            API=>{
                if ![OPENAI,ANTHROPIC].contains(&self.protocol.as_str()) { bail!(Text::new("mods.protocolInvalid").with("name",&name)); }
                if self.base_url.trim().is_empty() { bail!(Text::new("mods.urlRequired").with("name",&name)); }
                self.base_url=checked_url(&self.base_url,"",&name)?;
                self.args.clear();
                self.plan_args.clear();
                self.edits=false;
                // As ferramentas MCP só passam pelo protocolo da OpenAI.
                if self.protocol!=OPENAI { self.approve_mcps=false; }
            }
            _=>bail!(Text::new("mods.kindInvalid").with("name",&name)),
        }
        Ok(self)
    }

    fn is_api(&self)->bool { self.kind==API }

    /// O nome na tela, ou o id sem o prefixo enquanto não há nome.
    fn shown(&self,id:AgentId)->String {
        let name=self.name.trim();
        if name.is_empty() { id.key().trim_start_matches(CUSTOM_PREFIX).to_string() } else { name.to_string() }
    }
}

/// Um mod criado pela pessoa. Tudo o que ele é — o nome, o tipo, a linha de
/// comando ou o endereço — está nas opções do agente.
pub struct CustomMod { id:AgentId }

impl TypedMod for CustomMod {
    type Options=CustomOptions;
    const DELIVERS_MCP:bool=true;
    fn id(&self)->AgentId { self.id }
    fn label(&self,options:&CustomOptions)->String { options.shown(self.id) }
    fn kind(&self,options:&CustomOptions)->ModKind { if options.is_api() { ModKind::Api } else { ModKind::Cli } }
    fn edits_project(&self,options:&CustomOptions)->bool { !options.is_api()&&options.edits }
    fn built_in(&self)->bool { false }
    /// Não há executável padrão: é a pessoa quem diz.
    fn binary(&self)->&'static str { "" }
    fn feature(&self)->Option<&'static str> { Some("customMods") }
    fn default_context(&self)->usize { 128_000 }
    fn starter_models(&self)->&'static [&'static str] { &[] }
    fn check(&self,options:CustomOptions,_models:&HashSet<&str>)->Result<CustomOptions> { options.checked() }
    fn line(&self,options:&CustomOptions)->Vec<String> { if options.is_api() { vec![] } else { options.args.clone() } }
    /// Sem linha própria, o planejamento usa a mesma: só o mod que não escreve
    /// no projeto chega aqui sem ela (`checked`).
    fn plan_line(&self,options:&CustomOptions)->Vec<String> {
        if options.is_api() { return vec![]; }
        if options.plan_args.is_empty() { options.args.clone() } else { options.plan_args.clone() }
    }
    /// O modo travado de um mod criado é a linha que não escreve.
    fn lock(&self,options:&mut CustomOptions) { if !options.plan_args.is_empty() { options.args=options.plan_args.clone(); } }
    fn mcp<'a>(&self,options:&'a mut CustomOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    /// Sem lista de comandos: com qualquer comando bloqueado, o mod roda com a
    /// linha que não escreve.
    fn deny(&self,mut options:CustomOptions,_blocked:&[String])->Option<CustomOptions> {
        if options.is_api()||options.plan_args.is_empty() { return None; }
        options.args=options.plan_args.clone();
        Some(options)
    }
    fn provider_of(&self,agent:&AgentSettings,options:&CustomOptions)->ProviderConfig {
        if !options.is_api() { return cli_provider(agent,&[]); }
        let protocol=if options.protocol==ANTHROPIC { "anthropic" } else { "openai-compatible" };
        api_provider(agent,protocol,options.base_url.clone(),options.key_required,&options.mcp)
    }
    fn requires_key(&self,options:&CustomOptions)->bool { options.is_api()&&options.key_required }
    fn adopt(&self,_options:&mut CustomOptions,_found:&[KnownModel])->bool { false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn custom(options:serde_json::Value)->AgentSettings { AgentSettings{id:AgentId::parse("mod-local-ai").expect("id"),enabled:true,command:"local-ai".into(),timeout:300,options} }

    #[test] fn a_line_splits_like_a_shell_without_expanding() {
        assert_eq!(split_line(r#"run --model {model} --system "be brief" '{prompt}'"#),["run","--model","{model}","--system","be brief","{prompt}"]);
        assert_eq!(split_line("  "),Vec::<String>::new());
        assert_eq!(split_line(r#"--flag "" x"#),["--flag","","x"]);
    }

    /// A linha de comando só leva os dois lugares conhecidos; os da montagem
    /// dos mods do app ficam de fora.
    #[test] fn only_the_model_and_prompt_placeholders_are_accepted() {
        let line=|args:&[&str]|CustomOptions{name:"Local".into(),args:args.iter().map(|arg|arg.to_string()).collect(),..Default::default()}.checked();
        assert!(line(&["--model={model}","{prompt}"]).is_ok());
        assert!(line(&["--resume","{resume}"]).is_err());
        assert!(line(&["{effort}"]).is_err());
        assert!(line(&["--prompt={prompt}"]).is_err(),"o pedido vai sozinho no argumento");
        assert!(line(&["--json","{\"a\":1}"]).is_ok(),"chave com aspas não é lugar");
        assert!(line(&["a\nb"]).is_err());
    }

    #[test] fn a_mod_that_writes_needs_a_planning_line() {
        let options=CustomOptions{name:"Writer".into(),args:vec!["{prompt}".into()],edits:true,..Default::default()};
        assert!(options.clone().checked().is_err());
        assert!(CustomOptions{plan_args:vec!["--dry-run".into(),"{prompt}".into()],..options}.checked().is_ok());
    }

    #[test] fn an_api_mod_needs_a_valid_address() {
        let api=|url:&str|CustomOptions{name:"Proxy".into(),kind:API.into(),base_url:url.into(),..Default::default()}.checked();
        assert!(api("").is_err());
        assert!(api("ftp://host/v1").is_err());
        assert!(api("https://user:pass@host/v1").is_err());
        assert_eq!(api("http://localhost:8080/v1/").expect("ok").base_url,"http://localhost:8080/v1");
    }

    /// O mod de linha de comando vira um provedor CLI com as duas linhas; o de
    /// API, um provedor HTTP do protocolo escolhido, desligado sem a chave que
    /// exige.
    #[test] fn custom_mods_become_providers() {
        let cli=custom(json!({"name":"Local","kind":"cli","args":["--model","{model}"],"planArgs":["--read-only","--model","{model}"],"edits":true}));
        let config=of(cli.id).provider(&cli);
        assert_eq!((config.kind.as_str(),config.args.clone(),config.plan_args.clone()),("cli",vec!["--model".to_string(),"{model}".into()],vec!["--read-only".to_string(),"--model".into(),"{model}".into()]));
        assert!(of(cli.id).edits_project(&cli.options));
        assert_eq!(of(cli.id).label(&cli.options),"Local");
        assert_eq!(of(cli.id).label(&serde_json::Value::Null),"local-ai","sem nome, o id");
        let locked=cli.without_commands(&["git push".into()]);
        assert_eq!(locked.build_args(),["--read-only","--model","{model}"],"a política aperta para a linha que não escreve");

        let api=custom(json!({"name":"Proxy","kind":"api","protocol":"anthropic","baseUrl":"http://localhost:9000/v1","keyRequired":true}));
        let config=of(api.id).provider(&api);
        assert_eq!((config.kind.as_str(),config.base_url.as_deref(),config.enabled),("anthropic",Some("http://localhost:9000/v1"),false));
        assert!(!of(api.id).edits_project(&api.options));
    }
}
