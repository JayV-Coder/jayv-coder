//! O retrato do sistema: provedores, modelos, índice, cache e banco.

use crate::desktop::{both, SharedDesktopState, SharedWorkspace};
use crate::workspace;
use serde::Serialize;
use tauri::State;

#[derive(Debug,Serialize)]
pub struct SystemStatus { pub version:&'static str,pub config_path:String,pub database_path:String,pub database_name:String,pub tables:Vec<workspace::TableCount>,pub providers:usize,pub models:usize,pub indexed_files:usize,pub cache_entries:usize,pub session_messages:usize,pub performance_records:usize }

#[tauri::command]
pub(crate) async fn system_status(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>)->Result<SystemStatus,String>{
    let (state,workspace)=both(&state,&workspace).await;
    let orchestrator=&state.orchestrator;
    Ok(SystemStatus{version:env!("CARGO_PKG_VERSION"),config_path:orchestrator.config_path.display().to_string(),database_path:workspace.database_path().display().to_string(),database_name:workspace.database_name(),tables:workspace.table_counts().unwrap_or_default(),providers:orchestrator.executable_provider_count(),models:orchestrator.executable_model_count(),indexed_files:orchestrator.rag.len(),cache_entries:orchestrator.cache.len(),session_messages:orchestrator.memory.session_messages(),performance_records:orchestrator.performance.len()})
}
