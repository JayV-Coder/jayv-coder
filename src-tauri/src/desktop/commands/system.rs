//! O retrato do sistema: provedores, modelos, índice, cache e banco.

use crate::i18n::Text;
use crate::desktop::{facts, SharedDesktopState, SharedFacts, SharedWorkspace};
use crate::workspace;
use serde::Serialize;
use tauri::State;

#[derive(Debug,Serialize)]
pub struct SystemStatus { pub version:&'static str,pub config_path:String,pub database_path:String,pub database_name:String,pub tables:Vec<workspace::TableCount>,pub providers:usize,pub models:usize,pub indexed_files:usize,pub cache_entries:usize,pub session_messages:usize,pub performance_records:usize }

/// Não espera o pedido que está no ar: os números do orquestrador são os da
/// última vez que ele esteve livre.
#[tauri::command]
pub(crate) async fn system_status(state:State<'_,SharedDesktopState>,known:State<'_,SharedFacts>,workspace:State<'_,SharedWorkspace>)->Result<SystemStatus,Text>{
    let orchestrator=facts(&state,&known);
    let workspace=workspace.lock().await;
    Ok(SystemStatus{version:env!("CARGO_PKG_VERSION"),config_path:orchestrator.config_path.display().to_string(),database_path:workspace.database_path().display().to_string(),database_name:workspace.database_name(),tables:workspace.table_counts().unwrap_or_default(),providers:orchestrator.providers,models:orchestrator.models,indexed_files:orchestrator.indexed_files,cache_entries:orchestrator.cache_entries,session_messages:orchestrator.session_messages,performance_records:orchestrator.performance_records})
}
