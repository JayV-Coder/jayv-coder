//! O feed da portaria, recortado pelo projeto aberto.

use crate::i18n::{failure, Text};
use crate::desktop::SharedWorkspace;
use crate::gatekeeper::GateFeed;
use crate::turns::TurnEvidence;
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

/// A portaria de uma organização: só os chats dos projetos dela, ou só um
/// desses chats. Sem projeto nenhum, o feed vem vazio (e não o da sessão
/// inteira).
#[tauri::command]
pub(crate) async fn scoped_gate_feed(workspace:State<'_,SharedWorkspace>,project_ids:Vec<String>,chat_id:Option<String>)->Result<GateFeed,Text>{
    let workspace=workspace.lock().await;
    let chats=workspace.chat_ids_for_projects(&project_ids,chat_id.as_deref()).map_err(failure)?;
    workspace.gate_feed(Some(&chats)).map_err(failure)
}

/// O que foi conferido num turno, quando o desenvolvedor abre o balão para
/// ver: o que o JayV observou, o que um modelo disse e o que ficou sem
/// conferir.
#[tauri::command]
pub(crate) async fn turn_evidence(workspace:State<'_,SharedWorkspace>,turn_id:String)->Result<TurnEvidence,Text>{
    workspace.lock().await.turn_evidence(&turn_id).map_err(failure)
}
