//! Os mods: o catálogo que desceu do repositório `jayv-coder-mods`, o que quem
//! usa ligou e desligou na aba Mods e o que fica desligado no fim (o que a tela
//! esconde, como faz com o que o plano não tem).

use crate::desktop::SharedWorkspace;
use crate::i18n::{failure, Text};
use jayv_plans::mods::Manifest;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ModsSnapshot {
    /// O catálogo em cache; nulo, nenhum desceu ainda.
    pub catalog:Option<Vec<Manifest>>,
    /// Quando o catálogo desceu.
    pub fetched_at:Option<String>,
    /// O que quem usa escolheu (só os recursos que viraram mods).
    pub states:BTreeMap<String,bool>,
    /// Os recursos e motores de LLM que os mods desligam agora.
    pub disabled:Vec<String>,
    pub app_version:&'static str,
}

fn snapshot(workspace:&crate::workspace::WorkspaceStore)->anyhow::Result<ModsSnapshot> {
    let cached=workspace.mod_catalog()?;
    let disabled=workspace.entitlements()?.disabled().iter().cloned().collect();
    let (catalog,fetched_at)=match cached { Some((mods,at))=>(Some(mods),Some(at)), None=>(None,None) };
    Ok(ModsSnapshot{catalog,fetched_at,states:workspace.mod_states()?,disabled,app_version:env!("CARGO_PKG_VERSION")})
}

#[tauri::command]
pub(crate) async fn get_mods(workspace:State<'_,SharedWorkspace>)->Result<ModsSnapshot,Text>{
    let workspace=workspace.lock().await;
    snapshot(&workspace).map_err(failure)
}

/// Grava o que quem usa ligou e desligou. O atendente relê o plano e os mods no
/// começo de cada pedido: o próximo já vale com a escolha nova.
#[tauri::command]
pub(crate) async fn save_mod_states(workspace:State<'_,SharedWorkspace>,states:BTreeMap<String,bool>)->Result<ModsSnapshot,Text>{crate::desktop::require_session()?;
    let mut workspace=workspace.lock().await;
    workspace.save_mod_states(&states).map_err(failure)?;
    snapshot(&workspace).map_err(failure)
}
