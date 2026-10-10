//! O mod do Cursor (`cursor-agent --print`). Narra em `stream-json` como o
//! Claude; o único degrau que escreve é o `--force`, e os servidores MCP só
//! chegam pelo `~/.cursor/mcp.json` (`mcp::sync_cursor`).

use super::TypedMod;
use crate::llm::{one_of, strings, unique, valid_id, AgentId, Grants, KnownModel};
use crate::mcp::McpServer;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CursorOptions {
    /// `default`, `enabled` ou `disabled`. `default` segue a configuração
    /// do próprio Cursor.
    pub sandbox:String,
    /// Aplica as edições e roda os comandos sem pedir aprovação.
    pub force:bool,
    pub approve_mcps:bool,
    /// Os servidores da aba MCP deste pedido (`with_mcp`), postos no
    /// `~/.cursor/mcp.json` antes de abrir o agente. Nunca gravados aqui.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}
impl Default for CursorOptions { fn default()->Self { Self{sandbox:"default".into(),force:false,approve_mcps:false,mcp:vec![]} } }

const CURSOR_SANDBOXES:[&str;3]=["default","enabled","disabled"];

impl CursorOptions {
    pub fn args(&self)->Vec<String> {
        // Em `stream-json` o Cursor narra como o Claude: a fala do assistente
        // num evento, as ferramentas em outro e a conta no `result`. O pedido
        // chega pela entrada padrão.
        let mut args=strings(&["--print","--output-format","stream-json","--model","{model}"]);
        if self.sandbox!="default" { args.extend(strings(&["--sandbox",&self.sandbox])); }
        if self.force { args.push("--force".into()); }
        if self.approve_mcps { args.push("--approve-mcps".into()); }
        args
    }
}

/// Uma linha `id - Nome` por modelo, depois do `Available models`. O
/// `(default)` e o `(current)` do fim do nome são da conta, não do modelo.
pub(crate) fn parse_cursor_listing(text:&str)->Vec<KnownModel> {
    let found=text.lines().map(str::trim).filter_map(|line|line.split_once(" - ")).filter(|(id,_)|valid_id(id)).map(|(id,name)|{
        let name=name.trim().trim_end_matches("(default)").trim_end_matches("(current)").trim();
        KnownModel::named(AgentId::Cursor,id,Some(name.to_string()).filter(|name|!name.is_empty()),None)
    });
    unique(found)
}

pub struct Cursor;

impl TypedMod for Cursor {
    type Options=CursorOptions;
    const DELIVERS_MCP:bool=true;
    fn id(&self)->AgentId { AgentId::Cursor }
    fn label(&self,_options:&CursorOptions)->String { "Cursor".into() }
    /// O instalador do Cursor cria `agent` e `cursor-agent`; `agent` sozinho
    /// é genérico demais para achar no PATH.
    fn binary(&self)->&'static str { "cursor-agent" }
    fn starter_models(&self)->&'static [&'static str] { &["auto"] }
    fn check(&self,options:CursorOptions,_models:&HashSet<&str>)->Result<CursorOptions> {
        one_of("cursor.sandbox",&options.sandbox,&CURSOR_SANDBOXES)?;
        Ok(options)
    }
    fn line(&self,options:&CursorOptions)->Vec<String> { options.args() }
    /// O planejamento só lê: entra no `--mode plan`, sem `--force` e sem
    /// aprovar as ferramentas MCP, que rodam sem pergunta.
    fn plan_line(&self,options:&CursorOptions)->Vec<String> {
        let mut args=CursorOptions{force:false,approve_mcps:false,..options.clone()}.args();
        args.extend(strings(&["--mode","plan"]));
        args
    }
    fn lock(&self,options:&mut CursorOptions) {
        options.force=false;
        if options.sandbox=="disabled" { options.sandbox="enabled".into(); }
    }
    fn mcp<'a>(&self,options:&'a mut CursorOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    /// O Cursor só tem o `--force`.
    fn grant(&self,mut options:CursorOptions,grants:&Grants)->Option<CursorOptions> {
        if grants.shell||grants.git||!grants.commands.is_empty() { options.force=true; }
        Some(options)
    }
    /// Sem lista de comandos: com qualquer comando bloqueado, sem `--force` e
    /// sem o sandbox desligado.
    fn deny(&self,mut options:CursorOptions,_blocked:&[String])->Option<CursorOptions> {
        options.force=false;
        if options.sandbox=="disabled" { options.sandbox="enabled".into(); }
        Some(options)
    }
    fn listing_args(&self)->&'static [&'static str] { &["models"] }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { parse_cursor_listing(text) }
}
