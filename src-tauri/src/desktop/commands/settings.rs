//! Agentes e modelos: ler, salvar e conferir se o agente está instalado. Tudo
//! mora no banco; o orquestrador recebe a versão nova assim que ela é gravada.

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
    SettingsSnapshot{settings,catalog:AgentId::ALL.into_iter().map(|agent|(agent,llm::catalog(agent))).collect(),timeout_range:llm::TIMEOUT_RANGE,context_range:llm::CONTEXT_RANGE}
}

#[tauri::command]
pub(crate) async fn get_settings(workspace:State<'_,SharedWorkspace>)->Result<SettingsSnapshot,String>{
    let workspace=workspace.lock().await;
    workspace.llm_settings().map(snapshot).map_err(|error|error.to_string())
}

/// Grava primeiro e só então troca o orquestrador: uma configuração recusada
/// não deixa o orquestrador pela metade.
#[tauri::command]
pub(crate) async fn save_settings(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,settings:LlmSettings)->Result<SettingsSnapshot,String>{
    let saved={
        let mut workspace=workspace.lock().await;
        workspace.save_llm_settings(&settings).map_err(|error|error.to_string())?
    };
    desk.lock().await.orchestrator.use_llm(&saved);
    Ok(snapshot(saved))
}

#[tauri::command]
pub(crate) async fn check_agent(command:String)->Result<Probe,String>{
    Ok(llm::probe(&command).await)
}
