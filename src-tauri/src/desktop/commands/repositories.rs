//! Os repositórios da organização neste computador: achar os clones numa
//! pasta, clonar um que falta e conferir de que repositório é uma pasta.

use crate::checkout::{self, FoundRepository};
use crate::desktop::SharedWorkspace;
use crate::i18n::{failure, Text};
use crate::workspace::ProjectRecord;
use std::path::PathBuf;
use tauri::State;

/// Os clones de `keys` dentro de `folder`, até as netas da pasta.
#[tauri::command]
pub(crate) async fn scan_repositories(folder:String,keys:Vec<String>)->Result<Vec<FoundRepository>,Text>{
    let folder=PathBuf::from(folder.trim());
    if !folder.is_dir() {return Err(Text::new("repos.folder.missing").with("path",folder.display().to_string()));}
    tauri::async_runtime::spawn_blocking(move ||checkout::scan(&folder,&keys)).await.map_err(Text::unexpected)
}

/// Clona o repositório dentro de `folder` e já cria o projeto na pasta nova,
/// com o nome do repositório.
#[tauri::command]
pub(crate) async fn clone_repository(workspace:State<'_,SharedWorkspace>,key:String,folder:String)->Result<ProjectRecord,Text>{
    crate::desktop::require_session()?;
    let target=checkout::clone(&key,&PathBuf::from(folder.trim())).await.map_err(failure)?;
    let name=checkout::folder_name(key.trim()).to_string();
    workspace.lock().await.create_project(&name,Some(target.display().to_string())).map_err(failure)
}

/// As chaves dos remotes de uma pasta: confere, antes de criar o projeto, se a
/// pasta escolhida é mesmo um clone do repositório.
#[tauri::command]
pub(crate) async fn folder_repo_keys(path:String)->Result<Vec<String>,Text>{
    tauri::async_runtime::spawn_blocking(move ||crate::repo_keys::of_folder(&path)).await.map_err(Text::unexpected)
}
