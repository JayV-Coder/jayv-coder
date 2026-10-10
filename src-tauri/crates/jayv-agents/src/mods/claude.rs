//! O mod do Claude Code (`claude --print`). Narra em `stream-json`, retoma a
//! sessão do chat, recebe os servidores MCP e o índice de símbolos do JayV
//! numa configuração só (`--mcp-config`) e é o único que tira os arquivos
//! protegidos do alcance das ferramentas pela linha de comando (`guard`).

use super::TypedMod;
use crate::i18n::Text;
use crate::llm::{
    claude_deny_rule, claude_rule, effort_of, one_of, strings, tools_in, unique, valid_id, with_mechanism, without_shell, AgentId, Grants,
    KnownModel, Login, AUTO_EFFORT, EFFORT, EPHEMERAL, MANUAL, PERMISSION_PROMPTS, RESUME, SHELL, WEB_FETCH, WEB_SEARCH,
};
use crate::mcp::McpServer;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",default)]
pub struct ClaudeOptions {
    /// `manual`, `plan`, `acceptEdits`, `auto`, `dontAsk` ou
    /// `bypassPermissions`. `default` é o nome antigo do `manual`.
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
    /// Dá ao Claude as ferramentas do índice de símbolos do JayV (`jayv mcp`).
    pub symbol_tools:bool,
    /// Os mecanismos liberados sem pergunta (`CLAUDE_MECHANISMS`).
    pub mechanisms:Vec<String>,
    /// As regras do `--allowedTools` que o desenvolvedor liberou para um
    /// pedido só (`Grants`): `Bash(git add:*)`. Nunca gravadas.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub granted:Vec<String>,
    /// As regras do `--disallowed-tools` que a política da organização põe
    /// (`without_commands`): `Bash(git push:*)`. Nunca gravadas.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub denied:Vec<String>,
    /// "Aprovar servidores MCP": entrega ao agente os servidores da aba MCP,
    /// cujas ferramentas rodam sem pergunta. Desligado, nenhum chega.
    pub approve_mcps:bool,
    /// Os servidores MCP configurados (`mcp::McpServer`), postos na hora do
    /// pedido (`with_mcp`). Nunca gravados aqui: moram na tabela deles.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}
impl Default for ClaudeOptions { fn default()->Self { Self{permission_mode:MANUAL.into(),effort:AUTO_EFFORT.into(),fallback_model:String::new(),max_budget_usd:None,blocked_tools:vec![],append_system_prompt:String::new(),persist_sessions:true,safe_mode:false,symbol_tools:false,mechanisms:strings(&[WEB_SEARCH]),granted:vec![],denied:vec![],approve_mcps:false,mcp:vec![]} } }

const CLAUDE_PERMISSIONS:[&str;6]=[MANUAL,"plan","acceptEdits","auto","dontAsk","bypassPermissions"];
/// Os modos que não escrevem no projeto sem aprovação: no modo
/// desenvolvimento eles sobem para o `acceptEdits`.
const READ_ONLY_MODES:[&str;4]=["default",MANUAL,"plan","dontAsk"];
const CLAUDE_EFFORTS:[&str;6]=["auto","low","medium","high","xhigh","max"];
/// A ferramenta do Claude que abre um formulário no terminal interativo.
const INTERACTIVE_ONLY_TOOL:&str="AskUserQuestion";
pub const CLAUDE_TOOLS:[&str;6]=["Bash","Edit","Write","NotebookEdit","WebFetch","WebSearch"];
/// `WebSearch`, `WebFetch` e `Bash` no `--allowedTools`.
pub const CLAUDE_MECHANISMS:[&str;3]=[WEB_SEARCH,WEB_FETCH,SHELL];

/// A ferramenta do Claude atrás de cada mecanismo.
fn claude_tool(mechanism:&str)->Option<&'static str> {
    match mechanism { WEB_SEARCH=>Some("WebSearch"), WEB_FETCH=>Some("WebFetch"), SHELL=>Some("Bash"), _=>None }
}

/// As ferramentas do servidor `jayv` no Claude, liberadas sem pergunta: só leem.
pub(crate) const SYMBOL_SERVER_TOOLS:&str="mcp__jayv";

/// A configuração MCP que sobe este mesmo executável como `jayv mcp`. O agente
/// roda na pasta do projeto, e o servidor indexa a pasta onde nasce.
fn symbol_server_config()->Option<String> {
    let executable=std::env::current_exe().ok()?;
    Some(serde_json::json!({"mcpServers":{"jayv":{"command":executable.display().to_string(),"args":["mcp"]}}}).to_string())
}

impl ClaudeOptions {
    fn checked(mut self,models:&HashSet<&str>)->Result<Self> {
        if self.permission_mode=="default" { self.permission_mode=MANUAL.into(); }
        one_of("claude.permissionMode",&self.permission_mode,&CLAUDE_PERMISSIONS)?;
        // O `--safe-mode` não sobe servidor MCP nenhum: o índice de símbolos
        // ligado junto seria uma promessa que o agente não cumpre.
        if self.safe_mode { self.symbol_tools=false; }
        self.effort=effort_of(&self.effort);
        one_of("claude.effort",&self.effort,&CLAUDE_EFFORTS)?;
        self.fallback_model=self.fallback_model.trim().to_string();
        if !self.fallback_model.is_empty()&&!models.contains(self.fallback_model.as_str()) { bail!(Text::new("settings.claude.fallback")); }
        if let Some(budget)=self.max_budget_usd { if !(budget.is_finite()&&budget>0.0&&budget<=1_000.0) { bail!(Text::new("settings.claude.budget")); } }
        self.blocked_tools=tools_in("claude.blockedTools",&self.blocked_tools,&CLAUDE_TOOLS)?;
        // Ferramenta bloqueada não se libera: o bloqueio vence.
        let blocked=self.blocked_tools.clone();
        self.mechanisms=tools_in("claude.mechanisms",&self.mechanisms,&CLAUDE_MECHANISMS)?.into_iter()
            .filter(|mechanism|claude_tool(mechanism).is_none_or(|tool|!blocked.iter().any(|item|item==tool))).collect();
        self.append_system_prompt=self.append_system_prompt.trim().to_string();
        if self.append_system_prompt.chars().count()>4_000 { bail!(Text::new("settings.claude.instructions").with("max",4_000u32)); }
        Ok(self)
    }

    pub fn args(&self)->Vec<String> {
        let mut args=strings(&["--print","--output-format","stream-json","--verbose","--include-partial-messages","--model","{model}"]);
        if !matches!(self.permission_mode.as_str(),"default"|MANUAL) { args.extend(strings(&["--permission-mode",&self.permission_mode])); }
        args.extend(strings(&[PERMISSION_PROMPTS,"none"]));
        let effort=effort_of(&self.effort);
        args.extend(strings(&["--effort",if effort==AUTO_EFFORT {EFFORT} else {&effort}]));
        if !self.fallback_model.is_empty() { args.extend(strings(&["--fallback-model",&self.fallback_model])); }
        if let Some(budget)=self.max_budget_usd { args.extend(["--max-budget-usd".to_string(),format!("{budget:.2}")]); }
        // O `AskUserQuestion` só existe no terminal interativo: no `--print` ele
        // falha, e o Claude despejava as perguntas em texto avisando que "não
        // conseguiu abrir o formulário". Sem ele, o Claude pergunta no fim da
        // resposta, e é de lá que o JayV monta o formulário.
        let blocked=[INTERACTIVE_ONLY_TOOL.to_string()].into_iter().chain(self.blocked_tools.iter().cloned()).chain(self.denied.iter().cloned()).collect::<Vec<_>>();
        args.extend(["--disallowed-tools".to_string(),blocked.join(",")]);
        // O que roda sem pergunta. Sem terminal, o resto do que pede
        // aprovação — a busca na web inclusive — é negado.
        let mut allowed=self.mechanisms.iter().filter_map(|mechanism|claude_tool(mechanism))
            .filter(|tool|!self.blocked_tools.iter().any(|item|item==tool)).map(str::to_string).collect::<Vec<_>>();
        let symbols=if self.symbol_tools&&!self.safe_mode { symbol_server_config() } else { None };
        // O que o desenvolvedor liberou para este pedido; o `Bash` bloqueado
        // continua bloqueado.
        if !self.blocked_tools.iter().any(|item|item=="Bash") { allowed.extend(self.granted.iter().cloned()); }
        if symbols.is_some() { allowed.push(SYMBOL_SERVER_TOOLS.to_string()); }
        // As ferramentas dos servidores MCP configurados rodam sem pergunta.
        if !self.safe_mode { allowed.extend(crate::mcp::claude_tools(&self.mcp)); }
        // O índice de símbolos e os servidores configurados vão numa
        // configuração só.
        let base=symbols.and_then(|config|serde_json::from_str::<Value>(&config).ok()).and_then(|config|config.get("mcpServers").and_then(Value::as_object).cloned()).unwrap_or_default();
        let symbols=if self.safe_mode { None } else { crate::mcp::config_json(&self.mcp,AgentId::Claude,base) };
        if !allowed.is_empty() { args.extend(["--allowedTools".to_string(),allowed.join(",")]); }
        if !self.append_system_prompt.is_empty() { args.extend(["--append-system-prompt".to_string(),self.append_system_prompt.clone()]); }
        // Com as sessões guardadas, o pedido seguinte do mesmo chat retoma a
        // sessão do anterior: o agente já leu o que leu e não explora tudo de
        // novo. Sem elas não há o que retomar.
        if self.persist_sessions { args.extend(["--resume".to_string(),RESUME.to_string(),EPHEMERAL.to_string(),"--no-session-persistence".to_string()]); } else { args.push("--no-session-persistence".into()); }
        if self.safe_mode { args.push("--safe-mode".into()); }
        // O índice de símbolos do próprio JayV, como servidor MCP só de
        // leitura: o Claude pergunta onde algo mora em vez de varrer a pasta.
        // Desligado por padrão — as definições das ferramentas custam tokens em
        // toda sessão, e o `jayv bench` diz se se pagam no projeto.
        if let Some(config)=symbols { args.extend(["--mcp-config".to_string(),config]); }
        args
    }
}

/// `Available: sonnet, opus, …, default, or a full model ID.` — `default` é
/// "o que a conta escolher", não um modelo.
pub(crate) fn parse_claude_listing(text:&str)->Vec<KnownModel> {
    let result=serde_json::from_str::<Value>(text).ok().and_then(|value|value["result"].as_str().map(str::to_string)).unwrap_or_else(||text.to_string());
    let Some(rest)=result.split("Available:").nth(1) else { return vec![] };
    let list=rest.split(" or a full model ID").next().unwrap_or(rest);
    let ids=list.split(',').map(|item|item.trim().trim_end_matches('.').trim()).filter(|id|!id.is_empty()&&*id!="default"&&valid_id(id));
    unique(ids.map(|id|KnownModel::named(AgentId::Claude,id,None,None)))
}

pub struct Claude;

impl TypedMod for Claude {
    type Options=ClaudeOptions;
    fn id(&self)->AgentId { AgentId::Claude }
    fn label(&self,_options:&ClaudeOptions)->String { "Claude Code".into() }
    fn mechanisms(&self)->&'static [&'static str] { &CLAUDE_MECHANISMS }
    /// Os apelidos do Claude Code seguem sozinhos a versão mais nova de cada
    /// família.
    fn starter_models(&self)->&'static [&'static str] { &["sonnet","opus","haiku","fable"] }
    fn check(&self,options:ClaudeOptions,models:&HashSet<&str>)->Result<ClaudeOptions> { options.checked(models) }
    fn line(&self,options:&ClaudeOptions)->Vec<String> { options.args() }
    /// O que na configuração só lê sobe para o `acceptEdits`, e nada além.
    fn build_line(&self,options:&ClaudeOptions)->Vec<String> {
        let mut options=options.clone();
        if READ_ONLY_MODES.contains(&options.permission_mode.as_str()) { options.permission_mode="acceptEdits".into(); }
        options.args()
    }
    /// `--permission-mode plan`, sem rodar comandos sem pergunta e sem os
    /// servidores MCP.
    fn plan_line(&self,options:&ClaudeOptions)->Vec<String> {
        ClaudeOptions{permission_mode:"plan".into(),mechanisms:without_shell(&options.mechanisms),granted:vec![],mcp:vec![],..options.clone()}.args()
    }
    fn lock(&self,options:&mut ClaudeOptions) { if options.permission_mode=="bypassPermissions" { options.permission_mode=MANUAL.into(); } }
    fn mcp<'a>(&self,options:&'a mut ClaudeOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn mechanisms_mut<'a>(&self,options:&'a mut ClaudeOptions)->Option<&'a mut Vec<String>> { Some(&mut options.mechanisms) }
    fn granted_mut<'a>(&self,options:&'a mut ClaudeOptions)->Option<&'a mut Vec<String>> { Some(&mut options.granted) }
    /// O Claude libera comando por comando (`Bash(git add:*)`).
    fn grant(&self,mut options:ClaudeOptions,grants:&Grants)->Option<ClaudeOptions> {
        if grants.shell { options.mechanisms=with_mechanism(&options.mechanisms,SHELL); }
        if grants.network { options.mechanisms=with_mechanism(&options.mechanisms,WEB_FETCH); }
        let mut rules=grants.commands.iter().filter_map(|command|claude_rule(command)).collect::<Vec<_>>();
        if grants.git { rules.push("Bash(git:*)".into()); }
        // O comando que não cabe numa regra (vírgula, parêntese) libera o
        // `Bash` inteiro: foi o que o desenvolvedor aprovou.
        if grants.commands.iter().any(|command|claude_rule(command).is_none()) { options.mechanisms=with_mechanism(&options.mechanisms,SHELL); }
        for rule in rules { if !options.granted.contains(&rule) { options.granted.push(rule); } }
        Some(options)
    }
    /// Nega cada regra por comando; a negação vence a liberação.
    fn deny(&self,mut options:ClaudeOptions,blocked:&[String])->Option<ClaudeOptions> {
        options.denied=blocked.iter().filter_map(|rule|claude_deny_rule(rule)).collect();
        options.granted.retain(|rule|!blocked.iter().any(|blocked|claude_deny_rule(blocked).as_deref()==Some(rule.as_str())));
        Some(options)
    }
    /// O modelo reserva que a CLI deixou de oferecer sai das opções.
    fn adopt(&self,options:&mut ClaudeOptions,found:&[KnownModel])->bool {
        if options.fallback_model.is_empty()||found.iter().any(|known|known.id==options.fallback_model) { return false; }
        options.fallback_model.clear();
        true
    }
    /// O Claude Code responde ao `/model` no `--print` sem chamar o modelo.
    fn listing_args(&self)->&'static [&'static str] { &["--print","/model","--output-format","json","--no-session-persistence"] }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { parse_claude_listing(text) }
    fn login_args(&self)->&'static [&'static str] { &["auth","status","--json"] }
    fn read_login(&self,_success:bool,stdout:&str,_stderr:&str)->Option<Login> {
        match serde_json::from_str::<Value>(stdout.trim()).ok().and_then(|status|status.get("loggedIn").and_then(Value::as_bool)) {
            Some(true)=>Some(Login::In),
            Some(false)=>Some(Login::Out),
            None=>None,
        }
    }
    fn help_subcommands(&self)->Option<&'static [&'static str]> { Some(&[]) }
    fn guard(&self,args:&[String],deny:&[String])->Vec<String> { crate::llm::guarding(args,deny) }
}
