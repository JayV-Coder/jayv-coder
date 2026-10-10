//! O mod do Kilo Code (`kilo run`). Recebe o pedido no argumento e responde em
//! texto; o único degrau que escreve é o `--auto`, e os servidores MCP chegam
//! na configuração em linha (`KILO_CONFIG_CONTENT`).

use super::{cli_provider, TypedMod};
use crate::config::ProviderConfig;
use crate::llm::{strings, unique, valid_id, AgentId, AgentSettings, Grants, KnownModel};
use crate::mcp::McpServer;
use serde::{Deserialize, Serialize};

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Default)]
#[serde(rename_all="camelCase",default)]
pub struct KiloOptions {
    /// Aprova sozinho o que o agente pedir (`kilo run --auto`). Sem isto, o que
    /// pediria aprovação é recusado, já que ninguém responde no terminal.
    /// O modo desenvolvimento liga; o planejamento desliga.
    pub auto:bool,
    /// "Aprovar servidores MCP": entrega ao agente os servidores da aba MCP,
    /// cujas ferramentas rodam sem pergunta. Desligado, nenhum chega.
    pub approve_mcps:bool,
    /// Os servidores da aba MCP deste pedido (`with_mcp`), levados na
    /// configuração em linha. Nunca gravados aqui.
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub mcp:Vec<McpServer>,
}

impl KiloOptions {
    pub fn args(&self)->Vec<String> {
        // `kilo run` recebe o pedido como argumento e responde em texto.
        let mut args=strings(&["run","--model","{model}"]);
        if self.auto { args.push("--auto".into()); }
        args.push("{prompt}".into());
        args
    }
}

/// Uma linha `provedor/modelo` por modelo; o resto (cabeçalho, aviso) não passa
/// pelo `valid_id` ou não tem a barra.
pub(crate) fn parse_kilo_listing(text:&str)->Vec<KnownModel> {
    let ids=text.lines().map(str::trim).filter(|line|line.contains('/')&&valid_id(line));
    unique(ids.map(|id|KnownModel::named(AgentId::Kilo,id,None,None)))
}

/// A variável de ambiente que o Kilo Code soma à configuração dele.
const KILO_CONFIG:&str="KILO_CONFIG_CONTENT";

pub struct Kilo;

impl TypedMod for Kilo {
    type Options=KiloOptions;
    const DELIVERS_MCP:bool=true;
    fn id(&self)->AgentId { AgentId::Kilo }
    fn label(&self,_options:&KiloOptions)->String { "Kilo Code".into() }
    fn feature(&self)->Option<&'static str> { Some("kiloCode") }
    fn starter_models(&self)->&'static [&'static str] { &["anthropic/claude-sonnet-4"] }
    fn line(&self,options:&KiloOptions)->Vec<String> { options.args() }
    /// O desenvolvimento aprova sozinho; o planejamento recusa o que pediria
    /// aprovação.
    fn build_line(&self,_options:&KiloOptions)->Vec<String> { KiloOptions{auto:true,..Default::default()}.args() }
    fn plan_line(&self,_options:&KiloOptions)->Vec<String> { KiloOptions{auto:false,..Default::default()}.args() }
    fn lock(&self,options:&mut KiloOptions) { options.auto=false; }
    fn mcp<'a>(&self,options:&'a mut KiloOptions)->(&'a mut bool,&'a mut Vec<McpServer>) { (&mut options.approve_mcps,&mut options.mcp) }
    fn grant(&self,mut options:KiloOptions,_grants:&Grants)->Option<KiloOptions> { options.auto=true; Some(options) }
    fn deny(&self,mut options:KiloOptions,_blocked:&[String])->Option<KiloOptions> { options.auto=false; Some(options) }
    /// Os servidores vão na configuração em linha. O planejamento não leva
    /// servidor nenhum (`ProviderConfig::for_planning` tira o ambiente junto).
    fn provider_of(&self,agent:&AgentSettings,options:&KiloOptions)->ProviderConfig {
        let mut config=cli_provider(agent,&options.mcp);
        if let Some(inline)=crate::mcp::kilo_config(&options.mcp) { config.env.push((KILO_CONFIG.into(),inline)); }
        config
    }
    fn listing_args(&self)->&'static [&'static str] { &["models"] }
    fn parse_listing(&self,text:&str)->Vec<KnownModel> { parse_kilo_listing(text) }
}
