//! A memória do projeto na tela: as notas, as receitas, os pedidos que se
//! repetem e a busca nas conversas.

use crate::i18n::{failure, Text};
use crate::desktop::SharedWorkspace;
use crate::project_memory::{self, NoteDraft, ProjectNote, RepeatedRequest};
use crate::search::SearchHit;
use serde::Serialize;
use tauri::State;

const SEARCH_LIMIT:usize=20;

/// Tudo o que a tela da memória mostra de um projeto, com os tetos para ela
/// dizer quanto ainda cabe.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ProjectMemory {
    pub notes:Vec<ProjectNote>,
    pub repeated:Vec<RepeatedRequest>,
    pub notes_limit:usize,
    pub recipe_limit:usize,
}

fn memory(workspace:&crate::workspace::WorkspaceStore,project_id:&str)->Result<ProjectMemory,Text> {
    Ok(ProjectMemory{
        notes:workspace.project_notes(project_id).map_err(failure)?,
        repeated:workspace.repeated_requests(project_id).map_err(failure)?,
        notes_limit:project_memory::NOTES_CHARS,
        recipe_limit:project_memory::RECIPE_CHARS,
    })
}

#[tauri::command]
pub(crate) async fn project_memory(workspace:State<'_,SharedWorkspace>,project_id:String)->Result<ProjectMemory,Text>{memory(&*workspace.lock().await,&project_id)}

#[tauri::command]
pub(crate) async fn save_project_note(workspace:State<'_,SharedWorkspace>,draft:NoteDraft)->Result<ProjectMemory,Text>{crate::desktop::require_session()?;
    let mut workspace=workspace.lock().await;
    workspace.save_project_note(&draft).map_err(failure)?;
    memory(&workspace,&draft.project_id)
}

#[tauri::command]
pub(crate) async fn delete_project_note(workspace:State<'_,SharedWorkspace>,project_id:String,id:String)->Result<ProjectMemory,Text>{crate::desktop::require_session()?;
    let mut workspace=workspace.lock().await;
    workspace.delete_project_note(&id).map_err(failure)?;
    memory(&workspace,&project_id)
}

#[tauri::command]
pub(crate) async fn search_chats(workspace:State<'_,SharedWorkspace>,project_id:String,query:String)->Result<Vec<SearchHit>,Text>{workspace.lock().await.search_chats(&project_id,&query,SEARCH_LIMIT).map_err(failure)}
