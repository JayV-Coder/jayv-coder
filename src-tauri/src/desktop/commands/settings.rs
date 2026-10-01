//! Agentes, modelos e as configurações do Jev e do app: ler, salvar e conferir
//! se o agente está instalado. Tudo mora no banco; o orquestrador recebe a
//! versão nova assim que ela é gravada.

use crate::core_settings::{self, CoreSettings};
use crate::i18n::{failure, Text};
use crate::desktop::{SharedDesktopState, SharedWorkspace};
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
        let (id,command)=(agent.id,agent.command.clone());
        asked.spawn(async move {(id,llm::discover(id,&command).await)});
    }
    let answers=asked.join_all().await;
    let (mut desk,mut workspace)=crate::desktop::both(desk,workspace).await;
    let before=workspace.llm_settings()?;
    let mut settings=before.clone();
    let mut silent=Vec::new();
    for (agent,found) in answers {
        match found { Some(found)=>settings=llm::adopt_listing(&settings,agent,&found), None=>silent.push(agent) }
    }
    // Gravar enfileira a sincronização de todos os modelos: só quando mudou.
    if serde_json::to_value(&settings)?==serde_json::to_value(&before)? { return Ok((before,silent)); }
    let saved=workspace.save_llm_settings(&settings)?;
    desk.orchestrator.use_llm(&saved);
    Ok((saved,silent))
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
    desk.lock().await.orchestrator.use_llm(&saved);
    Ok(snapshot(saved))
}

/// O idioma escolhido na tela, para o modelo responder nele.
#[tauri::command]
pub(crate) fn set_reply_language(language:Option<crate::i18n::ReplyLanguage>) { crate::i18n::set_reply_language(language); }

#[tauri::command]
pub(crate) async fn check_agent(command:String)->Result<Probe,Text>{
    Ok(llm::probe(&command).await)
}

/// As configurações do Jev e do app como a tela as desenha: o que está
/// gravado, os valores de partida, os limites de cada campo e, só para ler,
/// os números da portaria que chegam do Supabase.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CoreSnapshot {
    pub settings:CoreSettings,
    pub defaults:CoreSettings,
    pub gate:crate::local::global::JevParameters,
    pub confidence_range:(f64,f64),
    pub budget_range:(usize,usize),
    pub cache_ttl_range:(u64,u64),
    pub version:&'static str,
}

fn core_snapshot(settings:CoreSettings,defaults:CoreSettings)->CoreSnapshot {
    CoreSnapshot{settings,defaults,gate:crate::local::global::current_parameters(),confidence_range:core_settings::CONFIDENCE_RANGE,budget_range:core_settings::BUDGET_RANGE,cache_ttl_range:core_settings::CACHE_TTL_RANGE,version:env!("CARGO_PKG_VERSION")}
}

#[tauri::command]
pub(crate) async fn get_core_settings(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>)->Result<CoreSnapshot,Text>{
    let (desk,workspace)=crate::desktop::both(&desk,&workspace).await;
    let defaults=desk.orchestrator.core_defaults();
    let settings=workspace.core_settings(&defaults).map_err(failure)?;
    Ok(core_snapshot(settings,defaults))
}

/// Grava primeiro e só então troca o orquestrador, como as dos agentes.
#[tauri::command]
pub(crate) async fn save_core_settings(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,settings:CoreSettings)->Result<CoreSnapshot,Text>{crate::desktop::require_session()?;
    let (mut desk,mut workspace)=crate::desktop::both(&desk,&workspace).await;
    let saved=workspace.save_core_settings(&settings).map_err(failure)?;
    desk.orchestrator.use_core(&saved);
    Ok(core_snapshot(saved,desk.orchestrator.core_defaults()))
}
