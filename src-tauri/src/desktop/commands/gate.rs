//! O feed da portaria, recortado pelo projeto aberto.

use crate::i18n::{failure, Text};
use crate::desktop::SharedWorkspace;
use crate::gatekeeper::GateFeed;
use tauri::State;

/// Sem projeto, a portaria mostra a sessão inteira; com projeto, só os chats
/// dele.
#[tauri::command]
pub(crate) async fn gate_feed(workspace:State<'_,SharedWorkspace>,project_id:Option<String>)->Result<GateFeed,Text>{
    let workspace=workspace.lock().await;
    let Some(project_id)=project_id else {return workspace.gate_feed(None).map_err(failure);};
    let chats=workspace.chat_ids_for_project(&project_id).map_err(failure)?.into_iter().collect();
    workspace.gate_feed(Some(&chats)).map_err(failure)
}
