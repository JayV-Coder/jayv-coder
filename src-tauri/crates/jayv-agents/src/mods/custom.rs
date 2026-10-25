//! Os mods que a pessoa cria em Configurações › Mods — e os agentes que os mods
//! do catálogo trazem (`mod_agents`). São de três tipos:
//!
//! - **Linha de comando** (`cli`): um programa na máquina, com os argumentos
//!   que a pessoa escreve. `{model}` vira o modelo escolhido pelo Jev, em
//!   qualquer lugar de um argumento; `{prompt}`, sozinho num argumento, vira o
//!   pedido (sem ele, o pedido vai pela entrada padrão); `{prompt_file}`,
//!   sozinho, vira o caminho de um arquivo com o pedido. O texto que a CLI
//!   escreve é a resposta. Se ela escreve no projeto, a pessoa diz também a
//!   linha do planejamento, que não escreve: é a que vale no modo
//!   planejamento e sempre que a política da organização aperta.
//! - **ACP** (`acp`): um programa que fala o Agent Client Protocol pela
//!   entrada e saída padrão (`kimi acp`, `opencode acp`). O JayV conversa com
//!   ele pelo protocolo (`crate::acp`): escolhe o modelo e o modo que o agente
//!   anuncia, entrega os servidores MCP e responde a cada pedido de permissão
//!   pelo que vale no pedido — no planejamento, só leitura.
//! - **API** (`api`): um endereço compatível com a API da OpenAI (chat ou
//!   Responses), com a da Anthropic ou com a de tarefas da Manus, com chave
//!   opcional. Só responde por texto, como os gateways.
//!
//! A definição mora nas opções do próprio agente (`llm_agents.options`), com o
//! id `mod-<nome>`: sincroniza com as outras configurações, e uma versão
//! antiga do app, que não conhece o id, só pula a linha. A chave da API mora
//! na tabela local `llm_secrets`, como a dos gateways.

use super::{api_provider, cli_provider, gateway::checked_url, LlmMod, ModKind, TypedMod, CUSTOM_PREFIX};
use crate::config::{AcpConfig, ProviderConfig};
use crate::i18n::Text;
use crate::llm::{command_blocked, AgentId, AgentSettings, Grants, KnownModel};
use crate::mcp::McpServer;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{LazyLock, Mutex, PoisonError};

pub const CLI:&str="cli";
pub const API:&str="api";
/// Um programa que fala o Agent Client Protocol (`crate::acp`).
pub const ACP:&str="acp";
/// A API compatível com a da OpenAI (`/chat/completions`, `/models`).
pub const OPENAI:&str="openai";
/// A API de mensagens da Anthropic (`/messages`).
pub const ANTHROPIC:&str="anthropic";
/// A API Responses da OpenAI (`/responses`), a única da API de agentes da
/// Perplexity.
pub const RESPONSES:&str="responses";
/// A API de tarefas da Manus (`task.create`, `task.listMessages`).
pub const MANUS:&str="manus";
const PROTOCOLS:[&str;4]=[OPENAI,ANTHROPIC,RESPONSES,MANUS];

/// Quantos mods criados um ambiente aceita.
pub const MAX_MODS:usize=20;
/// O tamanho do nome na tela.
pub const MAX_NAME:usize=40;
/// Quantos argumentos uma linha aceita, e o tamanho de cada um.
const MAX_ARGS:usize=64;
const MAX_ARG:usize=1_000;
const MODEL:&str="{model}";
const PROMPT:&str="{prompt}";
const PROMPT_FILE:&str=crate::llm::PROMPT_FILE;
/// Quantas variáveis um agente ACP leva, e o tamanho de cada valor.
const MAX_ENV:usize=16;
const MAX_ENV_VALUE:usize=4_000;
/// As variáveis que mudam como qualquer programa abre, e não só o agente: o
/// mod não as troca.
const RESERVED_ENV:[&str;8]=["PATH","HOME","USERPROFILE","LD_PRELOAD","LD_LIBRARY_PATH","DYLD_INSERT_LIBRARIES","DYLD_LIBRARY_PATH","NODE_OPTIONS"];

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
    /// "Aprovar servidores MCP": nos mods de API com o protocolo da OpenAI, o
    /// JayV chama as ferramentas como cliente MCP; no ACP, os servidores vão
    /// ao agente pelo protocolo.
    pub approve_mcps:bool,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
    /// API: o campo do pedido que leva o modelo (`preset` na Perplexity).
    /// Vazio é `model`. Só no protocolo Responses.
    #[serde(skip_serializing_if="String::is_empty")]
    pub model_field:String,
    /// ACP: as variáveis que o agente recebe (deixar as permissões dele em
    /// "perguntar", por exemplo).
    #[serde(skip_serializing_if="BTreeMap::is_empty")]
    pub env:BTreeMap<String,String>,
    /// ACP: o modo do desenvolvimento, com o id que o agente anuncia. Vazio,
    /// fica o dele.
    #[serde(skip_serializing_if="String::is_empty")]
    pub build_mode:String,
    /// ACP: o modo só leitura do agente, usado no planejamento. Vazio, o JayV
    /// só recusa o que escreve.
    #[serde(skip_serializing_if="String::is_empty")]
    pub plan_mode:String,
    /// ACP: travado pela política (agentes seguros): só os comandos liberados
    /// no pedido rodam. Nunca gravado.
    #[serde(skip_serializing_if="std::ops::Not::not")]
    pub locked:bool,
    /// ACP: os comandos liberados para um pedido só (`Grants`). Nunca gravados.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub granted:Vec<String>,
    /// ACP: os comandos que a política da organização bloqueia
    /// (`without_commands`). Nunca gravados.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub denied:Vec<String>,
}

impl Default for CustomOptions {
    fn default()->Self {
        Self{name:String::new(),kind:CLI.into(),args:vec![],plan_args:vec![],edits:false,protocol:OPENAI.into(),base_url:String::new(),api_key:None,clear_key:false,has_key:false,key_required:false,approve_mcps:false,mcp:vec![],
            model_field:String::new(),env:BTreeMap::new(),build_mode:String::new(),plan_mode:String::new(),locked:false,granted:vec![],denied:vec![]}
    }
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
/// `{model}` (em qualquer parte) e `{prompt}` ou `{prompt_file}` (sozinhos).
/// Os outros lugares (`{resume}`, `{effort}`…) são da montagem dos mods do app
/// e mudariam a linha sem a pessoa saber. O agente ACP recebe o pedido pelo
/// protocolo: na linha dele, só `{model}`.
fn checked_args(args:&[String],name:&str,acp:bool)->Result<Vec<String>> {
    if args.len()>MAX_ARGS { bail!(Text::new("mods.argsTooMany").with("name",name).with("max",MAX_ARGS as u32)); }
    let mut clean=Vec::with_capacity(args.len());
    for arg in args {
        if arg.is_empty()||arg.chars().count()>MAX_ARG||arg.chars().any(char::is_control) { bail!(Text::new("mods.argInvalid").with("name",name)); }
        let rest=if !acp&&(arg==PROMPT||arg==PROMPT_FILE) { String::new() } else { arg.replace(MODEL,"") };
        if let Some(token)=placeholders(&rest).next() { bail!(Text::new("mods.placeholder").with("name",name).with("token",token)); }
        clean.push(arg.clone());
    }
    Ok(clean)
}

/// As variáveis de um agente ACP: nomes de variável de verdade, valores sem
/// quebra de linha, e nenhuma das que mudam como qualquer programa abre.
fn checked_env(env:BTreeMap<String,String>,name:&str)->Result<BTreeMap<String,String>> {
    if env.len()>MAX_ENV { bail!(Text::new("mods.envInvalid").with("name",name)); }
    for (key,value) in &env {
        let valid=key.len()<=64&&key.chars().next().is_some_and(|first|first.is_ascii_alphabetic()||first=='_')&&key.chars().all(|char|char.is_ascii_alphanumeric()||char=='_');
        let reserved=RESERVED_ENV.iter().any(|reserved|reserved.eq_ignore_ascii_case(key))||key.to_ascii_uppercase().starts_with("DYLD_");
        if !valid||reserved||value.chars().count()>MAX_ENV_VALUE||value.chars().any(char::is_control) { bail!(Text::new("mods.envInvalid").with("name",name)); }
    }
    Ok(env)
}

/// O id de um modo do agente ACP (`plan`, `accept-edits`), ou vazio.
fn checked_mode(mode:&str,name:&str)->Result<String> {
    let mode=mode.trim();
    if mode.len()>64||!mode.chars().all(|char|char.is_ascii_alphanumeric()||"_.:-".contains(char)) { bail!(Text::new("mods.modeInvalid").with("name",name)); }
    Ok(mode.to_string())
}

impl CustomOptions {
    pub(crate) fn checked(mut self)->Result<Self> {
        self.name=self.name.trim().to_string();
        if self.name.is_empty()||self.name.chars().count()>MAX_NAME||self.name.chars().any(char::is_control) { bail!(Text::new("mods.nameInvalid").with("max",MAX_NAME as u32)); }
        let name=self.name.clone();
        // O que vale só num pedido nunca chega gravado.
        self.locked=false;
        self.granted.clear();
        self.denied.clear();
        match self.kind.as_str() {
            CLI=>{
                self.args=checked_args(&self.args,&name,false)?;
                self.plan_args=checked_args(&self.plan_args,&name,false)?;
                // O mod que escreve precisa de uma linha que não escreva: é a
                // do planejamento e a que vale quando a política aperta.
                if self.edits&&self.plan_args.is_empty() { bail!(Text::new("mods.planRequired").with("name",&name)); }
                self.clear_api();
                self.clear_acp();
                self.approve_mcps=false;
            }
            ACP=>{
                // Uma linha só: o modo só leitura é o agente quem tem, e o
                // JayV recusa o que escreve no planejamento.
                self.args=checked_args(&self.args,&name,true)?;
                self.plan_args.clear();
                self.edits=true;
                self.env=checked_env(std::mem::take(&mut self.env),&name)?;
                self.build_mode=checked_mode(&self.build_mode,&name)?;
                self.plan_mode=checked_mode(&self.plan_mode,&name)?;
                self.clear_api();
            }
            API=>{
                if !PROTOCOLS.contains(&self.protocol.as_str()) { bail!(Text::new("mods.protocolInvalid").with("name",&name)); }
                if self.base_url.trim().is_empty() { bail!(Text::new("mods.urlRequired").with("name",&name)); }
                self.base_url=checked_url(&self.base_url,"",&name)?;
                self.args.clear();
                self.plan_args.clear();
                self.edits=false;
                self.clear_acp();
                // As ferramentas MCP só passam pelo protocolo de chat da OpenAI.
                if self.protocol!=OPENAI { self.approve_mcps=false; }
                // O campo do modelo só existe na API Responses.
                let field=self.model_field.trim();
                if self.protocol!=RESPONSES||field=="model" { self.model_field.clear(); }
                else if !field.is_empty()&&(field.len()>32||!field.chars().all(|char|char.is_ascii_lowercase()||char=='_')) { bail!(Text::new("mods.modelFieldInvalid").with("name",&name)); }
                else { self.model_field=field.to_string(); }
            }
            _=>bail!(Text::new("mods.kindInvalid").with("name",&name)),
        }
        Ok(self)
    }

    fn clear_api(&mut self) {
        self.protocol=OPENAI.into();
        self.base_url.clear();
        self.key_required=false;
        self.has_key=false;
        self.model_field.clear();
    }

    fn clear_acp(&mut self) {
        self.env.clear();
        self.build_mode.clear();
        self.plan_mode.clear();
    }

    fn is_api(&self)->bool { self.kind==API }
    fn is_acp(&self)->bool { self.kind==ACP }

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
    /// O agente ACP também é um programa na máquina.
    fn kind(&self,options:&CustomOptions)->ModKind { if options.is_api() { ModKind::Api } else { ModKind::Cli } }
    fn edits_project(&self,options:&CustomOptions)->bool { options.is_acp()||(!options.is_api()&&options.edits) }
    fn built_in(&self)->bool { false }
    /// Não há executável padrão: é a pessoa quem diz.
    fn binary(&self)->&'static str { "" }
    fn feature(&self)->Option<&'static str> { Some("customMods") }
    fn default_context(&self)->usize { 128_000 }
    fn starter_models(&self)->&'static [&'static str] { &[] }
    fn check(&self,options:CustomOptions,_models:&HashSet<&str>)->Result<CustomOptions> { options.checked() }
    fn line(&self,options:&CustomOptions)->Vec<String> { if options.is_api() { vec![] } else { options.args.clone() } }
    /// Sem linha própria, o planejamento usa a mesma: só o mod que não escreve
    /// no projeto chega aqui sem ela (`checked`). O agente ACP abre igual nos
    /// dois modos; o modo só leitura vai pelo protocolo.
    fn plan_line(&self,options:&CustomOptions)->Vec<String> {
        if options.is_api() { return vec![]; }
        if options.plan_args.is_empty() { options.args.clone() } else { options.plan_args.clone() }
    }
    /// O modo travado de um mod criado é a linha que não escreve; no ACP, os
    /// comandos que ninguém liberou são recusados.
    fn lock(&self,options:&mut CustomOptions) {
        if options.is_acp() { options.locked=true; return; }
        if !options.plan_args.is_empty() { options.args=options.plan_args.clone(); }
    }
    fn mcp<'a>(&self,options:&'a mut CustomOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn granted_mut<'a>(&self,options:&'a mut CustomOptions)->Option<&'a mut Vec<String>> { options.is_acp().then_some(&mut options.granted) }
    /// O ACP libera comando por comando, na resposta à permissão que o agente
    /// pede; todos os comandos liberados destravam o agente.
    fn grant(&self,mut options:CustomOptions,grants:&Grants)->Option<CustomOptions> {
        if !options.is_acp() { return None; }
        if grants.shell { options.locked=false; }
        let mut rules=grants.commands.clone();
        if grants.git { rules.push("git".into()); }
        for rule in rules { if !options.granted.contains(&rule) { options.granted.push(rule); } }
        Some(options)
    }
    /// Sem lista de comandos: com qualquer comando bloqueado, o mod de linha de
    /// comando roda com a linha que não escreve. O ACP recusa só os
    /// bloqueados, na permissão que o agente pede para rodá-los.
    fn deny(&self,mut options:CustomOptions,blocked:&[String])->Option<CustomOptions> {
        if options.is_acp() {
            options.denied=blocked.to_vec();
            options.granted.retain(|rule|!command_blocked(blocked,rule));
            return Some(options);
        }
        if options.is_api()||options.plan_args.is_empty() { return None; }
        options.args=options.plan_args.clone();
        Some(options)
    }
    fn provider_of(&self,agent:&AgentSettings,options:&CustomOptions)->ProviderConfig {
        if options.is_acp() {
            let mut config=cli_provider(agent,&[]);
            config.kind=ACP.into();
            config.mcp=options.mcp.iter().filter_map(|server|serde_json::to_value(server).ok()).collect();
            config.acp=Some(AcpConfig{
                build_mode:options.build_mode.clone(),plan_mode:options.plan_mode.clone(),
                env:options.env.iter().map(|(key,value)|(key.clone(),value.clone())).collect(),
                auto:!options.locked,planning:false,granted:options.granted.clone(),denied:options.denied.clone(),
            });
            return config;
        }
        if !options.is_api() { return cli_provider(agent,&[]); }
        let protocol=match options.protocol.as_str() { ANTHROPIC=>"anthropic", RESPONSES=>RESPONSES, MANUS=>MANUS, _=>"openai-compatible" };
        let mut config=api_provider(agent,protocol,options.base_url.clone(),options.key_required,&options.mcp);
        config.model_field=Some(options.model_field.clone()).filter(|field|!field.is_empty());
        config
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

    #[test] fn the_request_can_go_in_a_file() {
        let line=|args:&[&str]|CustomOptions{name:"Aider".into(),args:args.iter().map(|arg|arg.to_string()).collect(),..Default::default()}.checked();
        assert!(line(&["--message-file","{prompt_file}"]).is_ok());
        assert!(line(&["--message-file={prompt_file}"]).is_err(),"o arquivo vai sozinho no argumento");
    }

    /// O agente ACP: uma linha só (sem o pedido, que vai pelo protocolo), as
    /// variáveis dele e os ids dos modos. Escreve no projeto.
    #[test] fn an_acp_agent_keeps_its_line_env_and_modes() {
        let acp=|patch:serde_json::Value|{
            let mut value=json!({"name":"Kimi Code","kind":"acp","args":["acp"],"env":{"OPENCODE_PERMISSION":"{\"bash\":\"ask\"}"},"buildMode":"default","planMode":"plan","planArgs":["x"],"baseUrl":"http://x"});
            for (key,field) in patch.as_object().expect("objeto") { value[key]=field.clone(); }
            serde_json::from_value::<CustomOptions>(value).expect("opções").checked()
        };
        let checked=acp(json!({})).expect("aceita");
        assert!(checked.edits&&checked.plan_args.is_empty()&&checked.base_url.is_empty(),"{checked:?}");
        assert_eq!((checked.build_mode.as_str(),checked.plan_mode.as_str()),("default","plan"));
        assert!(acp(json!({"args":["acp","{prompt}"]})).is_err(),"o pedido vai pelo protocolo");
        assert!(acp(json!({"args":["acp","--model","{model}"]})).is_ok());
        assert!(acp(json!({"env":{"PATH":"/evil"}})).is_err(),"o PATH é do app");
        assert!(acp(json!({"env":{"LD_PRELOAD":"x.so"}})).is_err());
        assert!(acp(json!({"env":{"1BAD":"x"}})).is_err());
        assert!(acp(json!({"planMode":"plan mode"})).is_err());
    }

    #[test] fn the_responses_api_can_name_the_model_field() {
        let api=|protocol:&str,field:&str|CustomOptions{name:"Perplexity".into(),kind:API.into(),protocol:protocol.into(),base_url:"https://api.perplexity.ai/v1".into(),model_field:field.into(),..Default::default()}.checked();
        assert_eq!(api(RESPONSES,"preset").expect("aceita").model_field,"preset");
        assert_eq!(api(RESPONSES,"model").expect("aceita").model_field,"","`model` é o padrão");
        assert_eq!(api(OPENAI,"preset").expect("aceita").model_field,"","só a API Responses tem o campo");
        assert!(api(RESPONSES,"Preset!").is_err());
        assert!(api(MANUS,"").is_ok());
        assert!(api("grpc","").is_err());
    }

    /// O que vale só num pedido (travado, liberado, bloqueado) nunca chega
    /// gravado, e vira as respostas do JayV às permissões do agente.
    #[test] fn an_acp_agent_answers_by_the_policy_of_the_request() {
        let agent=custom(json!({"name":"Kimi Code","kind":"acp","args":["acp"],"edits":true,"buildMode":"default","planMode":"plan","env":{"KIMI_X":"1"}}));
        let module=of(agent.id);
        let config=module.provider(&agent);
        let acp=config.acp.clone().expect("acp");
        assert_eq!((config.kind.as_str(),config.args.clone(),acp.auto,acp.env.clone()),("acp",vec!["acp".to_string()],true,vec![("KIMI_X".to_string(),"1".to_string())]));
        assert!(module.edits_project(&agent.options));
        let locked=agent.without_unsafe_modes();
        assert!(!module.provider(&locked).acp.expect("acp").auto,"travado, só o liberado roda");
        let granted=locked.with_grants(&crate::llm::Grants{commands:vec!["npm test".into()],git:true,..Default::default()});
        assert_eq!(module.provider(&granted).acp.expect("acp").granted,["npm test","git"]);
        let blocked=granted.without_commands(&["git".into()]);
        let acp=module.provider(&blocked).acp.expect("acp");
        assert_eq!((acp.denied.clone(),acp.granted.clone()),(vec!["git".to_string()],vec!["npm test".to_string()]),"o bloqueado sai do liberado");
        let saved=module.checked(&blocked.options,&HashSet::new()).expect("grava");
        assert!(saved.get("denied").is_none()&&saved.get("granted").is_none()&&saved.get("locked").is_none(),"{saved}");
        assert!(config.for_planning().acp.expect("acp").planning);
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
