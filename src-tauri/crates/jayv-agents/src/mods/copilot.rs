//! O mod do GitHub Copilot (`copilot -p`). Recebe o pedido no argumento,
//! libera e nega ferramenta por ferramenta (`--allow-tool`/`--deny-tool`) e
//! grava a conta do fim num arquivo (`--usage-output-file`).

use super::TypedMod;
use crate::llm::{copilot_deny_rule, copilot_rule, one_of, strings, tools_in, unique, valid_id, with_mechanism, without_shell, AgentId, Grants, KnownModel, GITHUB_TOOLS, SHELL, WEB_FETCH};
use crate::mcp::McpServer;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct CopilotOptions {
    /// `read`, `edits` ou `all`.
    pub tool_access:String,
    pub blocked_tools:Vec<String>,
    pub silent:bool,
    /// Os mecanismos liberados sem pergunta (`COPILOT_MECHANISMS`).
    pub mechanisms:Vec<String>,
    /// As ferramentas do `--allow-tool` liberadas para um pedido só
    /// (`Grants`): `shell(git add)`. Nunca gravadas.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub granted:Vec<String>,
    /// As ferramentas do `--deny-tool` que a política da organização põe
    /// (`without_commands`): `shell(git push)`. Nunca gravadas.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub denied:Vec<String>,
    /// "Aprovar servidores MCP": entrega ao agente os servidores da aba MCP,
    /// cujas ferramentas rodam sem pergunta. Desligado, nenhum chega.
    pub approve_mcps:bool,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}
impl Default for CopilotOptions { fn default()->Self { Self{tool_access:"read".into(),blocked_tools:vec![],silent:true,mechanisms:vec![],granted:vec![],denied:vec![],approve_mcps:false,mcp:vec![]} } }

const COPILOT_ACCESS:[&str;3]=["read","edits","all"];
pub const COPILOT_TOOLS:[&str;4]=["shell","write","shell(git push)","shell(rm)"];
/// `--allow-all-urls`, `--allow-tool shell` e `--enable-all-github-mcp-tools`.
pub const COPILOT_MECHANISMS:[&str;3]=[WEB_FETCH,SHELL,GITHUB_TOOLS];

impl CopilotOptions {
    fn checked(mut self)->Result<Self> {
        one_of("copilot.toolAccess",&self.tool_access,&COPILOT_ACCESS)?;
        self.blocked_tools=tools_in("copilot.blockedTools",&self.blocked_tools,&COPILOT_TOOLS)?;
        let shell_blocked=self.blocked_tools.iter().any(|tool|tool==SHELL);
        self.mechanisms=tools_in("copilot.mechanisms",&self.mechanisms,&COPILOT_MECHANISMS)?.into_iter()
            .filter(|mechanism|!(shell_blocked&&mechanism==SHELL)).collect();
        Ok(self)
    }

    pub fn args(&self)->Vec<String> {
        // O Copilot não lê o pedido da entrada padrão: ele vai no `-p`.
        let mut args=strings(&["-p","{prompt}","--model","{model}"]);
        match self.tool_access.as_str() { "edits"=>args.extend(strings(&["--allow-tool","write"])), "all"=>args.push("--allow-all-tools".into()), _=>{} }
        for mechanism in &self.mechanisms {
            match mechanism.as_str() {
                WEB_FETCH=>args.push("--allow-all-urls".into()),
                SHELL=>args.extend(strings(&["--allow-tool","shell"])),
                GITHUB_TOOLS=>args.push("--enable-all-github-mcp-tools".into()),
                _=>{}
            }
        }
        for tool in &self.granted { args.extend(["--allow-tool".to_string(),tool.clone()]); }
        args.extend(crate::mcp::args_for(&self.mcp,AgentId::Copilot));
        // A negação vence a liberação no Copilot, então o bloqueio continua
        // valendo mesmo com o mecanismo ligado.
        for tool in self.blocked_tools.iter().chain(&self.denied) { args.extend(["--deny-tool".to_string(),tool.clone()]); }
        // Sem o `--silent` o resumo de uso entra na saída — e a saída é a
        // resposta. A opção antiga (`silent`) não desliga mais isto.
        args.push("--silent".into());
        // A conta do fim vai para um arquivo, que o provedor lê e apaga: o
        // `--silent` esconde o resumo da saída, e a saída é a resposta.
        args.extend(strings(&["--usage-output-file","{usage_file}"]));
        args
    }
}

/// Os valores da chave `model` na ajuda de configuração: uma linha
/// `- "id"` por modelo, até a linha em branco.
pub(crate) fn parse_copilot_listing(text:&str)->Vec<KnownModel> {
    let mut lines=text.lines().skip_while(|line|!line.trim_start().starts_with("`model`"));
    lines.next();
    let ids=lines.map(str::trim).take_while(|line|!line.is_empty()).filter_map(|line|line.strip_prefix("- \"").and_then(|rest|rest.strip_suffix('"'))).filter(|id|valid_id(id));
    unique(ids.map(|id|KnownModel::named(AgentId::Copilot,id,None,None)))
}

pub struct Copilot;

impl TypedMod for Copilot {
    type Options=CopilotOptions;
    fn id(&self)->AgentId { AgentId::Copilot }
    fn label(&self,_options:&CopilotOptions)->String { "Copilot".into() }
    fn mechanisms(&self)->&'static [&'static str] { &COPILOT_MECHANISMS }
    fn default_context(&self)->usize { 128_000 }
    fn starter_models(&self)->&'static [&'static str] { &["claude-sonnet-4.5"] }
    fn check(&self,options:CopilotOptions,_models:&HashSet<&str>)->Result<CopilotOptions> { options.checked() }
    fn line(&self,options:&CopilotOptions)->Vec<String> { options.args() }
    /// O `read` sobe para o `edits`.
    fn build_line(&self,options:&CopilotOptions)->Vec<String> {
        let mut options=options.clone();
        if options.tool_access=="read" { options.tool_access="edits".into(); }
        options.args()
    }
    /// Só lê, sem rodar comandos sem pergunta e sem os servidores MCP.
    fn plan_line(&self,options:&CopilotOptions)->Vec<String> {
        CopilotOptions{tool_access:"read".into(),mechanisms:without_shell(&options.mechanisms),granted:vec![],mcp:vec![],..options.clone()}.args()
    }
    fn lock(&self,options:&mut CopilotOptions) { if options.tool_access=="all" { options.tool_access="edits".into(); } }
    fn mcp<'a>(&self,options:&'a mut CopilotOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn mechanisms_mut<'a>(&self,options:&'a mut CopilotOptions)->Option<&'a mut Vec<String>> { Some(&mut options.mechanisms) }
    fn granted_mut<'a>(&self,options:&'a mut CopilotOptions)->Option<&'a mut Vec<String>> { Some(&mut options.granted) }
    /// O Copilot libera comando por comando (`shell(git add)`).
    fn grant(&self,mut options:CopilotOptions,grants:&Grants)->Option<CopilotOptions> {
        if grants.shell { options.mechanisms=with_mechanism(&options.mechanisms,SHELL); }
        if grants.network { options.mechanisms=with_mechanism(&options.mechanisms,WEB_FETCH); }
        if options.tool_access=="read" { options.tool_access="edits".into(); }
        let mut tools=grants.commands.iter().filter_map(|command|copilot_rule(command)).collect::<Vec<_>>();
        if grants.git { tools.push("shell(git)".into()); }
        if grants.commands.iter().any(|command|copilot_rule(command).is_none()) { options.mechanisms=with_mechanism(&options.mechanisms,SHELL); }
        for tool in tools { if !options.granted.contains(&tool) { options.granted.push(tool); } }
        Some(options)
    }
    /// Nega cada regra por comando; a negação vence a liberação.
    fn deny(&self,mut options:CopilotOptions,blocked:&[String])->Option<CopilotOptions> {
        options.denied=blocked.iter().filter_map(|rule|copilot_deny_rule(rule)).collect();
        options.granted.retain(|tool|!blocked.iter().any(|blocked|copilot_deny_rule(blocked).as_deref()==Some(tool.as_str())));
        Some(options)
    }
    /// O Copilot lista os valores aceitos de `model` na ajuda de configuração.
    fn listing_args(&self)->&'static [&'static str] { &["help","config"] }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { parse_copilot_listing(text) }
}
