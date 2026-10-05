//! Projetos e chats: criar, limpar e excluir. O banco é a verdade, e a memória
//! de sessão do orquestrador acompanha o que some dele.

use crate::i18n::{failure, Text};
use crate::desktop::{forget_chats, SharedDesktopState, SharedForget, SharedWorkspace};
use crate::workspace::{ChatRecord, ProjectRecord, WorkspaceData};
use tauri::State;

#[tauri::command]
pub(crate) async fn get_workspace(workspace:State<'_,SharedWorkspace>)->Result<WorkspaceData,Text>{workspace.lock().await.snapshot().map_err(failure)}

#[tauri::command]
pub(crate) async fn create_project(workspace:State<'_,SharedWorkspace>,name:String,root_path:Option<String>)->Result<ProjectRecord,Text>{crate::desktop::require_session()?;workspace.lock().await.create_project(&name,root_path).map_err(failure)}

#[tauri::command]
pub(crate) async fn organization_project(workspace:State<'_,SharedWorkspace>,org_id:String,name:String,folder:String)->Result<ProjectRecord,Text>{crate::desktop::require_session()?;workspace.lock().await.organization_project(&org_id,&name,&folder).map_err(failure)}

#[tauri::command]
pub(crate) async fn create_chat(workspace:State<'_,SharedWorkspace>,project_id:String,title:Option<String>)->Result<ChatRecord,Text>{crate::desktop::require_session()?;workspace.lock().await.create_chat(&project_id,title).map_err(failure)}

/// Limpar, apagar o chat e apagar o projeto gravam no banco na hora: com um
/// agente trabalhando, o clique não espera o pedido dele terminar. A memória do
/// orquestrador esquece os chats agora ou no começo do próximo atendimento.
#[tauri::command]
pub(crate) async fn clear_chat(state:State<'_,SharedDesktopState>,forget:State<'_,SharedForget>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    workspace.lock().await.clear_chat(&chat_id).map_err(failure)?;
    forget_chats(&state,&forget,vec![chat_id]);
    Ok(())
}

/// Fixa o modo do chat: `auto`, `plan` ou `build`. Vale a partir do próximo
/// pedido que a fila tirar, inclusive os que já estão esperando.
#[tauri::command]
pub(crate) async fn set_work_mode(workspace:State<'_,SharedWorkspace>,chat_id:String,mode:String)->Result<(),Text>{crate::desktop::require_session()?;workspace.lock().await.set_work_mode(&chat_id,&mode).map_err(failure)}

#[tauri::command]
pub(crate) async fn delete_chat(state:State<'_,SharedDesktopState>,forget:State<'_,SharedForget>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    workspace.lock().await.delete_chat(&chat_id).map_err(failure)?;
    forget_chats(&state,&forget,vec![chat_id]);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_project(state:State<'_,SharedDesktopState>,forget:State<'_,SharedForget>,workspace:State<'_,SharedWorkspace>,project_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let chat_ids=workspace.lock().await.delete_project(&project_id).map_err(failure)?;
    forget_chats(&state,&forget,chat_ids);
    Ok(())
}
