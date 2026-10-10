//! Os mods dos gateways de API (OpenRouter e LiteLLM): falam por HTTP com a
//! API compatível com a da OpenAI, com endereço e chave, e só respondem por
//! texto — não editam o projeto. Os servidores MCP são chamados pelo próprio
//! JayV, como cliente MCP (`mcp_client`).

use super::{api_provider, ModKind, TypedMod};
use crate::config::ProviderConfig;
use crate::i18n::Text;
use crate::llm::{AgentId, AgentSettings};
use crate::mcp::McpServer;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// As opções de um gateway. A chave nunca volta para a tela nem para o banco
/// de opções (que sobe para a nuvem): ela mora na tabela local `llm_secrets`.
/// `api_key` só viaja da tela para o salvar; `has_key` é o que a tela lê.
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Default)]
#[serde(rename_all="camelCase",default)]
pub struct GatewayOptions {
    /// O endereço base compatível com a API da OpenAI (`…/v1`).
    pub base_url:String,
    #[serde(skip_serializing)]
    pub api_key:Option<String>,
    /// Apaga a chave guardada.
    #[serde(skip_serializing)]
    pub clear_key:bool,
    pub has_key:bool,
    /// "Aprovar servidores MCP": entrega ao agente os servidores da aba MCP,
    /// cujas ferramentas rodam sem pergunta. Desligado, nenhum chega.
    pub approve_mcps:bool,
    /// Os servidores da aba MCP deste pedido (`with_mcp`): o JayV os chama
    /// como cliente MCP. Nunca gravados aqui.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}

/// O endereço HTTP(S) com host e sem usuário nem senha embutidos, sem a barra
/// do fim. Vazio, o padrão do mod.
pub(crate) fn checked_url(url:&str,fallback:&str,label:&str)->Result<String> {
    let mut url=url.trim().trim_end_matches('/').to_string();
    if url.is_empty() { url=fallback.to_string(); }
    let valid=reqwest::Url::parse(&url).is_ok_and(|parsed|matches!(parsed.scheme(),"http"|"https")&&parsed.host_str().is_some()&&parsed.username().is_empty()&&parsed.password().is_none());
    if !valid { bail!(Text::new("settings.baseUrlInvalid").with("agent",label)); }
    Ok(url)
}

/// Um gateway: qual é, o endereço padrão e se roda sem chave.
pub struct Gateway { id:AgentId, label:&'static str, base_url:&'static str, key_required:bool }

impl Gateway {
    pub const OPENROUTER:Gateway=Gateway{id:AgentId::Openrouter,label:"OpenRouter",base_url:"https://openrouter.ai/api/v1",key_required:true};
    /// O LiteLLM é o servidor do próprio usuário: o endereço padrão é o do
    /// proxy local, na porta que a documentação dele usa, e ele pode rodar sem
    /// chave.
    pub const LITELLM:Gateway=Gateway{id:AgentId::Litellm,label:"LiteLLM",base_url:"http://localhost:4000/v1",key_required:false};
}

impl TypedMod for Gateway {
    type Options=GatewayOptions;
    const DELIVERS_MCP:bool=true;
    fn id(&self)->AgentId { self.id }
    fn label(&self,_options:&GatewayOptions)->String { self.label.into() }
    fn kind(&self,_options:&GatewayOptions)->ModKind { ModKind::Api }
    fn binary(&self)->&'static str { "" }
    fn feature(&self)->Option<&'static str> { Some("gatewayProviders") }
    fn default_context(&self)->usize { 128_000 }
    fn starter_models(&self)->&'static [&'static str] { if self.id==AgentId::Openrouter { &["anthropic/claude-sonnet-4"] } else { &["gpt-4o-mini"] } }
    fn fresh(&self)->GatewayOptions { GatewayOptions{base_url:self.base_url.into(),..Default::default()} }
    fn fill(&self,mut options:GatewayOptions)->GatewayOptions { if options.base_url.trim().is_empty() { options.base_url=self.base_url.into(); } options }
    fn check(&self,mut options:GatewayOptions,_models:&HashSet<&str>)->Result<GatewayOptions> {
        options.base_url=checked_url(&options.base_url,self.base_url,self.label)?;
        Ok(options)
    }
    fn line(&self,_options:&GatewayOptions)->Vec<String> { vec![] }
    fn mcp<'a>(&self,options:&'a mut GatewayOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn provider_of(&self,agent:&AgentSettings,options:&GatewayOptions)->ProviderConfig {
        let base_url=if options.base_url.trim().is_empty() { self.base_url.to_string() } else { options.base_url.clone() };
        api_provider(agent,"openai-compatible",base_url,self.key_required,&options.mcp)
    }
    fn requires_key(&self,_options:&GatewayOptions)->bool { self.key_required }
}
