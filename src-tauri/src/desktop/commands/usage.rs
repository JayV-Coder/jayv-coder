//! As estatísticas de uso: a conta por escopo e período, o gasto de cada
//! turno de um chat e a releitura do limite dos planos.

use crate::desktop::SharedWorkspace;
use crate::i18n::{failure, Text};
use crate::usage::{quota::{self, QuotaStatus}, store::{Query, Report, TurnUsage}};
use tauri::State;

#[tauri::command]
pub(crate) async fn usage_report(workspace:State<'_,SharedWorkspace>,query:Query)->Result<Report,Text>{
    workspace.lock().await.usage_report(&query).map_err(failure)
}

#[tauri::command]
pub(crate) async fn chat_usage(workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<Vec<TurnUsage>,Text>{
    workspace.lock().await.chat_usage(&chat_id).map_err(failure)
}

/// Relê o limite de cada agente ligado. O banco fica livre durante a
/// leitura: abrir o CLI leva segundos, e a fila não pode esperar por isso.
#[tauri::command]
pub(crate) async fn refresh_quotas(workspace:State<'_,SharedWorkspace>)->Result<Vec<QuotaStatus>,Text>{
    let settings=workspace.lock().await.llm_settings().map_err(failure)?;
    let agents=settings.agents.iter().filter(|agent|agent.enabled).map(|agent|(agent.id.key().to_string(),agent.command.clone())).collect::<Vec<_>>();
    Ok(quota::read_all(&agents).await)
}
