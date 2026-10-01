//! Projetos e chats: criar, limpar e excluir. O banco é a verdade, e a memória
//! de sessão do orquestrador acompanha o que some dele.

use crate::i18n::{failure, Text};
use crate::desktop::{both, SharedDesktopState, SharedWorkspace};
use crate::workspace::{ChatRecord, ProjectRecord, WorkspaceData};
use tauri::State;

#[tauri::command]
pub(crate) async fn get_workspace(workspace:State<'_,SharedWorkspace>)->Result<WorkspaceData,Text>{workspace.lock().await.snapshot().map_err(failure)}

#[tauri::command]
pub(crate) async fn create_project(workspace:State<'_,SharedWorkspace>,name:String,root_path:Option<String>)->Result<ProjectRecord,Text>{crate::desktop::require_session()?;workspace.lock().await.create_project(&name,root_path).map_err(failure)}

#[tauri::command]
pub(crate) async fn create_chat(workspace:State<'_,SharedWorkspace>,project_id:String,title:Option<String>)->Result<ChatRecord,Text>{crate::desktop::require_session()?;workspace.lock().await.create_chat(&project_id,title).map_err(failure)}

#[tauri::command]
pub(crate) async fn clear_chat(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let (mut state,mut workspace)=both(&state,&workspace).await;
    workspace.clear_chat(&chat_id).map_err(failure)?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_chat(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let (mut state,mut workspace)=both(&state,&workspace).await;
    workspace.delete_chat(&chat_id).map_err(failure)?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_project(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,project_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let (mut state,mut workspace)=both(&state,&workspace).await;
    let chat_ids=workspace.delete_project(&project_id).map_err(failure)?;
    for chat_id in chat_ids {state.orchestrator.memory.clear_session(&chat_id);}
    Ok(())
}
