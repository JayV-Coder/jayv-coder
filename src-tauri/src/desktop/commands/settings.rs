//! Agentes, modelos e as configurações do Jev e do app: ler, salvar e conferir
//! se o agente está instalado. Tudo mora no banco; o orquestrador recebe a
//! versão nova assim que ela é gravada.

use crate::core_settings::{self, CoreSettings};
use crate::i18n::{failure, Text};
use crate::desktop::{when_free, SharedDesktopState, SharedFacts, SharedWorkspace};
use crate::orchestrator::Orchestrator;
use crate::llm::{self, AgentId, KnownModel, LlmSettings, Probe};
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

/// O que a tela precisa para desenhar as abas sem inventar nada: o que está
/// gravado e os valores que cada campo aceita.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SettingsSnapshot {
    pub settings:LlmSettings,
    pub catalog:HashMap<AgentId,Vec<KnownModel>>,
    pub timeout_range:(u64,u64),
    pub context_range:(usize,usize),
}

fn snapshot(settings:LlmSettings)->SettingsSnapshot {
    let catalog=AgentId::ALL.into_iter().map(|agent|(agent,llm::catalog(agent,&settings))).collect();
    SettingsSnapshot{settings,catalog,timeout_range:llm::TIMEOUT_RANGE,context_range:llm::CONTEXT_RANGE}
}

/// Pergunta a cada agente quais modelos o `/model` dele oferece e grava a
/// lista. Os CLIs respondem sem cadeado nenhum preso; só a gravação pega os
/// dois. Devolve o que ficou gravado e os agentes que não responderam.
pub(crate) async fn rediscover(desk:&SharedDesktopState,workspace:&SharedWorkspace,agents:&[AgentId])->anyhow::Result<(LlmSettings,Vec<AgentId>)> {
    let current=workspace.lock().await.llm_settings()?;
    let mut asked=tokio::task::JoinSet::new();
    for agent in current.agents.iter().filter(|agent|agents.contains(&agent.id)) {
        let agent=agent.clone();
        asked.spawn(async move {(agent.id,llm::discover_agent(&agent).await)});
    }
    let answers=asked.join_all().await;
    let mut workspace=workspace.lock().await;
    let before=workspace.llm_settings()?;
    let mut settings=before.clone();
    let mut silent=Vec::new();
    for (agent,found) in answers {
        match found { Some(found)=>settings=llm::adopt_listing(&settings,agent,&found), None=>silent.push(agent) }
    }
    // Gravar enfileira a sincronização de todos os modelos: só quando mudou.
    if serde_json::to_value(&settings)?==serde_json::to_value(&before)? { return Ok((before,silent)); }
    let saved=workspace.save_llm_settings(&settings)?;
    when_free(desk,|orchestrator|orchestrator.use_llm(&saved));
    Ok((saved,silent))
}

/// O que a conferência de um gateway de API achou: quantos modelos ele lista,
/// ou o motivo de não ter listado (a resposta do servidor, em inglês).
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct GatewayCheck { pub models:usize, pub error:Option<String> }

/// Confere o gateway com o que está gravado (endereço e chave): pergunta a
/// lista de modelos a ele. Não grava nada.
#[tauri::command]
pub(crate) async fn check_gateway(workspace:State<'_,SharedWorkspace>,agent:AgentId)->Result<GatewayCheck,Text>{
    let settings={ let workspace=workspace.lock().await; workspace.llm_settings().map_err(failure)? };
    let Some(found)=settings.agents.iter().find(|entry|entry.id==agent&&agent.is_gateway()) else { return Ok(GatewayCheck{models:0,error:Some("not a gateway".into())}) };
    Ok(match llm::discover_gateway(found).await { Ok(models)=>GatewayCheck{models:models.len(),error:None}, Err(error)=>GatewayCheck{models:0,error:Some(error.to_string())} })
}

/// A lista de modelos de um agente, lida de novo do CLI.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ModelsRefresh { pub snapshot:SettingsSnapshot, pub listed:bool }

#[tauri::command]
pub(crate) async fn refresh_models(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,agent:AgentId)->Result<ModelsRefresh,Text>{crate::desktop::require_session()?;
    let (saved,silent)=rediscover(&desk,&workspace,&[agent]).await.map_err(failure)?;
    Ok(ModelsRefresh{snapshot:snapshot(saved),listed:silent.is_empty()})
}

#[tauri::command]
pub(crate) async fn get_settings(workspace:State<'_,SharedWorkspace>)->Result<SettingsSnapshot,Text>{
    let workspace=workspace.lock().await;
    workspace.llm_settings().map(snapshot).map_err(failure)
}

/// Grava primeiro e só então troca o orquestrador: uma configuração recusada
/// não deixa o orquestrador pela metade.
#[tauri::command]
pub(crate) async fn save_settings(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,settings:LlmSettings)->Result<SettingsSnapshot,Text>{crate::desktop::require_session()?;
    let saved={
        let mut workspace=workspace.lock().await;
        workspace.save_llm_settings(&settings).map_err(failure)?
    };
    when_free(&desk,|orchestrator|orchestrator.use_llm(&saved));
    Ok(snapshot(saved))
}

/// O idioma escolhido na tela, para o modelo responder nele.
#[tauri::command]
pub(crate) fn set_reply_language(language:Option<crate::i18n::ReplyLanguage>) { crate::i18n::set_reply_language(language); }

#[tauri::command]
/// Com o agente, a conferência pergunta também se ele está logado — e a
/// resposta passa a valer para o roteamento.
pub(crate) async fn check_agent(command:String,agent:Option<llm::AgentId>)->Result<Probe,Text>{
    Ok(match agent { Some(agent)=>llm::probe_agent(agent,&command).await, None=>llm::probe(&command).await })
}

/// As configurações do Jev e do app como a tela as desenha: o que está
/// gravado, os valores de partida, os limites de cada campo e, só para ler,
/// os números da portaria que chegam do Supabase.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CoreSnapshot {
    pub settings:CoreSettings,
    pub defaults:CoreSettings,
    pub gate:crate::gatekeeper::JevParameters,
    /// O nível da conta e os números da portaria e do Jev em cada nível, para
    /// a tela mostrar o que a escolha muda antes de ela ser feita.
    pub expertise:crate::expertise::Expertise,
    pub levels:Vec<LevelView>,
    /// O nível que o histórico da portaria sugere, com o histórico que o
    /// justifica. Só sugestão: a troca é de quem usa.
    pub suggestion:Option<LevelSuggestion>,
    /// A regra de código enxuto no modo build, ligada ou não na conta.
    pub lean_code:bool,
    pub confidence_range:(f64,f64),
    pub budget_range:(usize,usize),
    pub cache_ttl_range:(u64,u64),
    pub version:&'static str,
}

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct LevelSuggestion { pub level:crate::expertise::Expertise, pub history:crate::expertise::GateHistory, pub days:i64 }

fn level_suggestion(workspace:&crate::workspace::WorkspaceStore,expertise:crate::expertise::Expertise)->Option<LevelSuggestion> {
    let days=crate::expertise::SUGGESTION_DAYS;
    let history=workspace.gate_history(days).map_err(|error|eprintln!("nível: histórico da portaria ilegível ({error:#})")).ok()?;
    crate::expertise::suggestion(expertise,&history).map(|level|LevelSuggestion{level,history,days})
}

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct LevelView {
    pub id:crate::expertise::Expertise,
    pub scope_demand:[f64;3],
    pub block_margin:f64,
    pub confidence:f64,
    pub build_ceiling:&'static str,
    pub destructive_threshold:f64,
}

fn core_snapshot(settings:CoreSettings,defaults:CoreSettings,expertise:crate::expertise::Expertise,suggestion:Option<LevelSuggestion>,lean_code:bool)->CoreSnapshot {
    let gate=crate::gatekeeper::current_parameters();
    let levels=crate::expertise::Expertise::ALL.into_iter().map(|level|{
        let adjusted=level.gate(&gate);
        LevelView{id:level,scope_demand:adjusted.scope_demand,block_margin:adjusted.block_margin,confidence:level.confidence(settings.confidence_threshold),build_ceiling:level.build_ceiling(),destructive_threshold:level.destructive_threshold()}
    }).collect();
    CoreSnapshot{settings,defaults,gate:expertise.gate(&gate),expertise,levels,suggestion,lean_code,confidence_range:core_settings::CONFIDENCE_RANGE,budget_range:core_settings::BUDGET_RANGE,cache_ttl_range:core_settings::CACHE_TTL_RANGE,version:env!("CARGO_PKG_VERSION")}
}

/// Lê sem o orquestrador: os valores de partida vêm do `config.yaml`, e o
/// resto do banco. Com um agente trabalhando, a tela abre na hora.
#[tauri::command]
pub(crate) async fn get_core_settings(known:State<'_,SharedFacts>,workspace:State<'_,SharedWorkspace>)->Result<CoreSnapshot,Text>{
    let defaults=defaults(&known);
    let workspace=workspace.lock().await;
    let settings=workspace.core_settings(&seeded(&workspace,&defaults)).map_err(failure)?;
    let expertise=workspace.expertise().map_err(failure)?;
    Ok(core_snapshot(settings,defaults,expertise,level_suggestion(&workspace,expertise),workspace.lean_code().map_err(failure)?))
}

fn defaults(known:&SharedFacts)->CoreSettings {
    let path=known.lock().unwrap_or_else(std::sync::PoisonError::into_inner).config_path.clone();
    Orchestrator::core_defaults_at(&path)
}

/// Os valores de partida com os do plano por cima: quem nunca gravou as
/// configurações do Jev começa com o que o plano diz.
fn seeded(workspace:&crate::workspace::WorkspaceStore,defaults:&CoreSettings)->CoreSettings {
    workspace.entitlements().unwrap_or_default().seed_core(defaults)
}

/// Grava primeiro e só então troca o orquestrador, como as dos agentes. Com
/// um pedido no ar, a troca fica para o próximo: o atendente relê tudo do
/// banco no começo de cada pedido, já com a política e o plano por cima.
#[tauri::command]
pub(crate) async fn save_core_settings(desk:State<'_,SharedDesktopState>,known:State<'_,SharedFacts>,workspace:State<'_,SharedWorkspace>,settings:CoreSettings)->Result<CoreSnapshot,Text>{crate::desktop::require_session()?;
    let defaults=defaults(&known);
    let mut workspace=workspace.lock().await;
    let saved=workspace.save_core_settings(&settings).map_err(failure)?;
    // O orquestrador recebe o que vale de fato: sem o que o plano não tem e
    // com o que ele trava. A tela continua vendo o que quem usa escolheu.
    let effective=workspace.entitlements().unwrap_or_default().apply_core(&saved);
    when_free(&desk,|orchestrator|orchestrator.use_core(&effective));
    let expertise=workspace.expertise().map_err(failure)?;
    Ok(core_snapshot(saved,defaults,expertise,level_suggestion(&workspace,expertise),workspace.lean_code().map_err(failure)?))
}

/// Grava o nível na conta. A sincronização o leva para os outros computadores,
/// e o próximo pedido já é julgado por ele.
#[tauri::command]
pub(crate) async fn save_expertise(desk:State<'_,SharedDesktopState>,known:State<'_,SharedFacts>,workspace:State<'_,SharedWorkspace>,level:String)->Result<CoreSnapshot,Text>{crate::desktop::require_session()?;
    let defaults=defaults(&known);
    let mut workspace=workspace.lock().await;
    let expertise=workspace.save_expertise(&level).map_err(failure)?;
    when_free(&desk,|orchestrator|orchestrator.expertise=expertise);
    let settings=workspace.core_settings(&seeded(&workspace,&defaults)).map_err(failure)?;
    Ok(core_snapshot(settings,defaults,expertise,level_suggestion(&workspace,expertise),workspace.lean_code().map_err(failure)?))
}

/// Liga ou desliga a regra de código enxuto. Vale na hora, como o nível, e
/// anda com a conta pela sincronização.
#[tauri::command]
pub(crate) async fn save_lean_code(desk:State<'_,SharedDesktopState>,known:State<'_,SharedFacts>,workspace:State<'_,SharedWorkspace>,enabled:bool)->Result<CoreSnapshot,Text>{crate::desktop::require_session()?;
    let defaults=defaults(&known);
    let mut workspace=workspace.lock().await;
    let lean_code=workspace.save_lean_code(enabled).map_err(failure)?;
    when_free(&desk,|orchestrator|orchestrator.lean_code=lean_code);
    let expertise=workspace.expertise().map_err(failure)?;
    let settings=workspace.core_settings(&seeded(&workspace,&defaults)).map_err(failure)?;
    Ok(core_snapshot(settings,defaults,expertise,level_suggestion(&workspace,expertise),lean_code))
}

/// Os servidores MCP configurados neste computador.
#[tauri::command]
pub(crate) async fn get_mcp_servers(workspace:State<'_,SharedWorkspace>)->Result<Vec<jayv_agents::mcp::McpServer>,Text>{
    workspace.lock().await.mcp_servers().map_err(failure)
}

/// Grava a lista inteira. O pedido seguinte já leva os servidores novos: a
/// fila os lê a cada pedido.
#[tauri::command]
pub(crate) async fn save_mcp_servers(workspace:State<'_,SharedWorkspace>,servers:Vec<jayv_agents::mcp::McpServer>)->Result<Vec<jayv_agents::mcp::McpServer>,Text>{crate::desktop::require_session()?;
    workspace.lock().await.save_mcp_servers(servers).map_err(failure)
}

/// O rascunho do `/mcp` do chat: o texto colado é lido aqui (configuração,
/// `claude mcp add`, comando `npx`); o que não é configuração vai ao agente
/// mais barato, em somente leitura, que devolve a configuração. Nada é
/// gravado: a tela abre o rascunho para o desenvolvedor conferir.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct McpDraft { pub servers:Vec<jayv_agents::mcp::McpServer>, pub from_model:bool }

#[tauri::command]
pub(crate) async fn draft_mcp(desk:State<'_,SharedDesktopState>,text:String)->Result<McpDraft,Text>{crate::desktop::require_session()?;
    let parsed=jayv_agents::mcp::parse(&text);
    if !parsed.is_empty() { return Ok(McpDraft{servers:parsed,from_model:false}); }
    let request=desk.lock().await.orchestrator.mcp_draft_request(&text).ok_or_else(||Text::new("mcp.draft.noAgent"))?;
    let answer=request.run().await.ok_or_else(||Text::new("mcp.draft.failed"))?;
    let servers=jayv_agents::mcp::from_model(&answer);
    if servers.is_empty() { return Err(Text::new("mcp.draft.unknown")); }
    Ok(McpDraft{servers,from_model:true})
}

/// O que as organizações de quem usa dão: servidores MCP e skills, que descem
/// pela sincronização e valem nos projetos delas. Só leitura, e sem segredos.
#[tauri::command]
pub(crate) async fn get_org_extensions(workspace:State<'_,SharedWorkspace>)->Result<jayv_agents::org_extensions::OrgExtensions,Text>{
    workspace.lock().await.org_extensions().map_err(failure)
}

/// Onde as skills instaladas ficam: dentro da pasta de dados do app.
fn skills_root()->Result<std::path::PathBuf,Text> {
    crate::workspace::app_data_dir().map(|dir|jayv_agents::skills::root(&dir)).ok_or_else(||Text::new("skills.noFolder"))
}

/// As skills instaladas neste computador.
#[tauri::command]
pub(crate) async fn get_skills(workspace:State<'_,SharedWorkspace>)->Result<Vec<jayv_agents::skills::Skill>,Text>{
    workspace.lock().await.skills().map_err(failure)
}

/// Instala a skill de uma pasta com `SKILL.md` (ou as de uma pasta com várias).
/// Devolve a lista inteira.
#[tauri::command]
pub(crate) async fn install_skill_folder(workspace:State<'_,SharedWorkspace>,path:String)->Result<Vec<jayv_agents::skills::Skill>,Text>{crate::desktop::require_session()?;
    let root=skills_root()?;
    let mut workspace=workspace.lock().await;
    workspace.install_skill_folder(&root,std::path::Path::new(path.trim())).map_err(failure)?;
    workspace.skills().map_err(failure)
}

/// Instala a skill a partir do texto do `SKILL.md` colado na tela.
#[tauri::command]
pub(crate) async fn install_skill_text(workspace:State<'_,SharedWorkspace>,text:String)->Result<Vec<jayv_agents::skills::Skill>,Text>{crate::desktop::require_session()?;
    let root=skills_root()?;
    let mut workspace=workspace.lock().await;
    workspace.install_skill_text(&root,&text).map_err(failure)?;
    workspace.skills().map_err(failure)
}

/// Liga ou desliga uma skill: desligada, o Jev nem a considera.
#[tauri::command]
pub(crate) async fn set_skill_enabled(workspace:State<'_,SharedWorkspace>,name:String,enabled:bool)->Result<Vec<jayv_agents::skills::Skill>,Text>{crate::desktop::require_session()?;
    let mut workspace=workspace.lock().await;
    workspace.set_skill_enabled(&name,enabled).map_err(failure)?;
    workspace.skills().map_err(failure)
}

/// Remove a skill e a cópia dela na pasta de dados.
#[tauri::command]
pub(crate) async fn remove_skill(workspace:State<'_,SharedWorkspace>,name:String)->Result<Vec<jayv_agents::skills::Skill>,Text>{crate::desktop::require_session()?;
    let root=skills_root()?;
    let mut workspace=workspace.lock().await;
    workspace.remove_skill(&root,&name).map_err(failure)?;
    workspace.skills().map_err(failure)
}
