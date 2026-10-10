//! O mod do Codex (`codex exec --json`). Roda num sandbox: a escrita, a rede e
//! o `.git` são degraus do sandbox, não listas de comandos. Retoma a sessão
//! pelo subcomando `resume` e recebe os servidores MCP como chaves `-c`.

use super::TypedMod;
use crate::llm::{effort_of, fetches, one_of, strings, tools_in, unique, valid_id, AgentId, Grants, KnownModel, Login, AUTO_EFFORT, EFFORT, EPHEMERAL, RESUME_THREAD, WEB_SEARCH};
use crate::mcp::McpServer;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CodexOptions {
    /// `read-only`, `workspace-write` ou `danger-full-access`.
    pub sandbox:String,
    /// `auto` (o Jev escolhe por pedido), `low`, `medium` ou `high`.
    pub reasoning_effort:String,
    /// A rede do sandbox quando o Codex escreve no projeto: no
    /// `workspace-write` e no `read-only`, que o modo desenvolvimento sobe para
    /// `workspace-write`. O `danger-full-access` já tem rede, e o modo
    /// planejamento nunca tem.
    pub network_access:bool,
    pub skip_git_repo_check:bool,
    /// Os mecanismos ligados (`CODEX_MECHANISMS`).
    pub mechanisms:Vec<String>,
    /// "Aprovar servidores MCP": entrega ao agente os servidores da aba MCP,
    /// cujas ferramentas rodam sem pergunta. Desligado, nenhum chega.
    pub approve_mcps:bool,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}
impl Default for CodexOptions { fn default()->Self { Self{sandbox:"read-only".into(),reasoning_effort:AUTO_EFFORT.into(),network_access:false,skip_git_repo_check:true,mechanisms:strings(&[WEB_SEARCH]),approve_mcps:false,mcp:vec![]} } }

const CODEX_SANDBOXES:[&str;3]=["read-only","workspace-write","danger-full-access"];
const CODEX_EFFORTS:[&str;4]=["auto","low","medium","high"];
/// `web_search = "live"`; desligado é `"disabled"`.
pub const CODEX_MECHANISMS:[&str;1]=[WEB_SEARCH];

impl CodexOptions {
    fn checked(mut self)->Result<Self> {
        one_of("codex.sandbox",&self.sandbox,&CODEX_SANDBOXES)?;
        self.reasoning_effort=effort_of(&self.reasoning_effort);
        one_of("codex.reasoning",&self.reasoning_effort,&CODEX_EFFORTS)?;
        if self.sandbox=="danger-full-access" { self.network_access=false; }
        self.mechanisms=tools_in("codex.mechanisms",&self.mechanisms,&CODEX_MECHANISMS)?;
        Ok(self)
    }

    pub fn args(&self)->Vec<String> {
        // `--json` narra em eventos: a fala do agente, os passos e a conta
        // dos tokens chegam separados, e é dela que sai o uso informado.
        let mut args=strings(&["exec","--json","--model","{model}","--sandbox",&self.sandbox]);
        // Fora de um repositório git o Codex recusa o pedido: a pasta do
        // projeto nem sempre é um. A opção antiga (`skip_git_repo_check`) não
        // desliga mais isto.
        args.push("--skip-git-repo-check".into());
        let effort=effort_of(&self.reasoning_effort);
        args.extend(["-c".to_string(),format!("model_reasoning_effort=\"{}\"",if effort==AUTO_EFFORT {EFFORT} else {&effort})]);
        // A chave só existe no `workspace-write`: no `read-only` não há rede,
        // e o `danger-full-access` não tem sandbox.
        if self.network_access&&self.sandbox=="workspace-write" { args.extend(strings(&["-c","sandbox_workspace_write.network_access=true"])); }
        // `live` busca na hora; desligado é desligado mesmo, e não o `cached`
        // que o Codex usa quando ninguém diz nada.
        let search=if self.mechanisms.iter().any(|mechanism|mechanism==WEB_SEARCH) {"live"} else {"disabled"};
        args.extend(["-c".to_string(),format!("web_search=\"{search}\"")]);
        args.extend(crate::mcp::args_for(&self.mcp,AgentId::Codex));
        // A chamada de apoio não grava sessão.
        args.extend(strings(&[EPHEMERAL,"--ephemeral"]));
        // A sessão do chat, quando há uma para retomar: o agente não relê o
        // projeto do zero.
        args.push(RESUME_THREAD.into());
        // O pedido chega pela entrada padrão.
        args.push("-".into());
        args
    }
}

/// O catálogo do Codex, só com os que o `/model` mostra (`visibility: list`).
pub(crate) fn parse_codex_listing(text:&str)->Vec<KnownModel> {
    let Ok(value)=serde_json::from_str::<Value>(text) else { return vec![] };
    let mut models:Vec<&Value>=value["models"].as_array().map(|models|models.iter().filter(|model|model["visibility"].as_str()==Some("list")).collect()).unwrap_or_default();
    models.sort_by_key(|model|model["priority"].as_i64().unwrap_or(i64::MAX));
    unique(models.into_iter().filter_map(|model|{
        let id=model["slug"].as_str().filter(|id|valid_id(id))?;
        Some(KnownModel::named(AgentId::Codex,id,model["display_name"].as_str().map(str::to_string),model["context_window"].as_u64().map(|window|window as usize)))
    }))
}

pub struct Codex;

impl TypedMod for Codex {
    type Options=CodexOptions;
    fn id(&self)->AgentId { AgentId::Codex }
    fn label(&self,_options:&CodexOptions)->String { "Codex".into() }
    fn mechanisms(&self)->&'static [&'static str] { &CODEX_MECHANISMS }
    fn default_context(&self)->usize { 272_000 }
    fn starter_models(&self)->&'static [&'static str] { &["gpt-5.5"] }
    fn check(&self,options:CodexOptions,_models:&HashSet<&str>)->Result<CodexOptions> { options.checked() }
    fn line(&self,options:&CodexOptions)->Vec<String> { options.args() }
    /// O `read-only` sobe para o `workspace-write`, com a rede que a
    /// configuração deu.
    fn build_line(&self,options:&CodexOptions)->Vec<String> {
        let mut options=options.clone();
        if options.sandbox=="read-only" { options.sandbox="workspace-write".into(); }
        options.args()
    }
    /// O sandbox `read-only`, sem rede e sem os servidores MCP.
    fn plan_line(&self,options:&CodexOptions)->Vec<String> { CodexOptions{sandbox:"read-only".into(),network_access:false,mcp:vec![],..options.clone()}.args() }
    fn lock(&self,options:&mut CodexOptions) {
        if options.sandbox=="danger-full-access" { options.sandbox="workspace-write".into(); options.network_access=false; }
    }
    fn mcp<'a>(&self,options:&'a mut CodexOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn mechanisms_mut<'a>(&self,options:&'a mut CodexOptions)->Option<&'a mut Vec<String>> { Some(&mut options.mechanisms) }
    /// O Codex não tem lista de comandos: ele já roda comandos no sandbox, e o
    /// que o sandbox nega é a rede (`network_access`) e o `.git`, que só o
    /// `danger-full-access` abre.
    fn grant(&self,mut options:CodexOptions,grants:&Grants)->Option<CodexOptions> {
        if grants.needs_git() { options.sandbox="danger-full-access".into(); }
        if grants.needs_network()&&options.sandbox!="danger-full-access" { options.network_access=true; }
        Some(options)
    }
    /// Com qualquer comando bloqueado, o sandbox fecha; a rede sai quando o
    /// que a política bloqueia é dela (instalar, publicar, empurrar).
    fn deny(&self,mut options:CodexOptions,blocked:&[String])->Option<CodexOptions> {
        if options.sandbox=="danger-full-access" { options.sandbox="workspace-write".into(); }
        if blocked.iter().any(|rule|fetches(rule)) { options.network_access=false; }
        Some(options)
    }
    fn listing_args(&self)->&'static [&'static str] { &["debug","models"] }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { parse_codex_listing(text) }
    fn login_args(&self)->&'static [&'static str] { &["login","status"] }
    fn read_login(&self,success:bool,stdout:&str,stderr:&str)->Option<Login> {
        let said=format!("{stdout}\n{stderr}").to_lowercase();
        if said.contains("not logged in") { Some(Login::Out) } else if success&&said.contains("logged in") { Some(Login::In) } else { None }
    }
    fn help_subcommands(&self)->Option<&'static [&'static str]> { Some(&["exec"]) }
}
