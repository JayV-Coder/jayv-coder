//! Os mods: o catálogo que desceu do repositório `jayv-coder-mods`, o que o
//! ambiente instalou e ligou e o que fica desligado no fim (o que a tela
//! esconde, como faz com o que o plano não tem). A aba Mods instala, liga,
//! desliga e desinstala pelo `save_mods`; a instalação sozinha dos mods de
//! fábrica roda no fim de cada volta do sync (`sync::round`).

use crate::desktop::SharedWorkspace;
use crate::i18n::{failure, Text};
use jayv_mods::{InstalledMods, Manifest};
use serde::Serialize;
use tauri::State;

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ModsSnapshot {
    /// O catálogo em cache; nulo, nenhum desceu ainda.
    pub catalog:Option<Vec<Manifest>>,
    /// Quando o catálogo desceu.
    pub fetched_at:Option<String>,
    /// O estado do ambiente: o gravado mais o que vem de fábrica.
    pub state:InstalledMods,
    /// Os mods que valem agora (instalados, ligados, com as dependências e
    /// que rodam nesta versão).
    pub active:Vec<String>,
    /// Os recursos, motores e agentes que os mods desligam agora.
    pub disabled:Vec<String>,
    /// Os pontos de extensão que esta versão sabe aplicar.
    pub points:Vec<&'static str>,
    pub app_version:&'static str,
}

fn snapshot(workspace:&crate::workspace::WorkspaceStore)->anyhow::Result<ModsSnapshot> {
    let view=workspace.mods_view()?;
    // O que a tela esconde é o que o núcleo usa: o mesmo `Entitlements`.
    let disabled=workspace.entitlements()?.disabled().iter().cloned().collect();
    Ok(ModsSnapshot{
        catalog:view.catalog,fetched_at:view.fetched_at,state:view.state,active:view.active.into_iter().collect(),disabled,
        points:jayv_plans::mods::REGISTRY.keys(),app_version:env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
pub(crate) async fn get_mods(workspace:State<'_,SharedWorkspace>)->Result<ModsSnapshot,Text>{
    let workspace=workspace.lock().await;
    snapshot(&workspace).map_err(failure)
}

/// Grava o estado que a aba Mods pediu: o que entrou se instala (com as
/// dependências), o que saiu se desinstala, e cada um fica ligado ou não. O
/// atendente relê o plano, os mods e os agentes no começo de cada pedido: o
/// próximo já vale com o estado novo.
#[tauri::command]
pub(crate) async fn save_mods(workspace:State<'_,SharedWorkspace>,state:InstalledMods)->Result<ModsSnapshot,Text>{crate::desktop::require_session()?;
    let mut workspace=workspace.lock().await;
    workspace.apply_mods(state).map_err(failure)?;
    snapshot(&workspace).map_err(failure)
}
