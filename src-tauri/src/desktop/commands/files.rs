//! Os arquivos que a resposta cita: um clique no caminho abre o arquivo no
//! programa que o sistema usa para aquele tipo — editor, visualizador de
//! imagens, leitor de PDF.

use crate::desktop::SharedWorkspace;
use crate::i18n::Text;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use walkdir::WalkDir;

#[tauri::command]
pub(crate) async fn open_file(app:AppHandle,workspace:State<'_,SharedWorkspace>,chat_id:String,path:String)->Result<(),Text>{
    let failed=|reason:String|Text::new("file.open.failed").with("path",&path).with("reason",reason);
    let root=workspace.lock().await.chat_root(&chat_id).map_err(|error|failed(error.to_string()))?.ok_or_else(||Text::new("file.open.noFolder"))?;
    let cited=path.clone();
    let file=tauri::async_runtime::spawn_blocking(move ||locate_file(&root,&cited)).await.map_err(|error|failed(error.to_string()))??;
    app.opener().open_path(file.to_string_lossy(),None::<&str>).map_err(|error|failed(error.to_string()))
}

/// Onde está, dentro do projeto, o arquivo que a resposta citou. O modelo nem
/// sempre escreve o caminho a partir da raiz (`util.py`,
/// `src/main.ts` dentro de um subprojeto), então quem não está na raiz é
/// procurado pelo fim do caminho — e o mais curto ganha. Nada fora da pasta do
/// projeto é aberto.
pub(crate) fn locate_file(root:&Path,cited:&str)->Result<PathBuf,Text> {
    let cited=cited.trim().trim_start_matches("./");
    let wanted=Path::new(cited);
    let outside=||Text::new("file.open.outside").with("path",cited);
    if cited.is_empty()||wanted.components().any(|component|component==Component::ParentDir) {return Err(outside());}
    let root=root.canonicalize().map_err(|error|Text::new("file.open.failed").with("path",cited).with("reason",error.to_string()))?;
    let inside=|candidate:&Path|candidate.canonicalize().ok().filter(|real|real.starts_with(&root)&&real.is_file());
    if let Some(found)=inside(&root.join(wanted)) {return Ok(found);}
    if wanted.is_absolute() {return Err(outside());}
    WalkDir::new(&root).follow_links(false).max_depth(16).into_iter()
        .filter_entry(crate::rag::allowed_entry)
        .filter_map(Result::ok)
        .filter(|entry|entry.file_type().is_file()&&entry.path().ends_with(wanted))
        .map(|entry|entry.into_path())
        .min_by_key(|found|found.components().count())
        .ok_or_else(||Text::new("file.open.notFound").with("path",cited))
}

#[cfg(test)] mod tests {
    use super::*;
    use std::fs;

    fn project()->tempfile::TempDir {
        let root=tempfile::tempdir().expect("root");
        for file in ["src/lib.rs","web/src/main.ts","pkg/core/util.py","pkg/core/deep/util.py","node_modules/x/util.py","docs/foto.png"] {
            let path=root.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).expect("pasta");
            fs::write(path,"x").expect("arquivo");
        }
        root
    }

    #[test] fn the_cited_path_is_found_from_the_root_or_by_its_tail() {
        let root=project();
        let real=root.path().canonicalize().unwrap();
        assert_eq!(locate_file(root.path(),"./src/lib.rs").unwrap(),real.join("src/lib.rs"));
        assert_eq!(locate_file(root.path(),"docs/foto.png").unwrap(),real.join("docs/foto.png"));
        assert_eq!(locate_file(root.path(),"src/main.ts").unwrap(),real.join("web/src/main.ts"));
        assert_eq!(locate_file(root.path(),"util.py").unwrap(),real.join("pkg/core/util.py"));
        assert_eq!(locate_file(root.path(),&real.join("src/lib.rs").to_string_lossy()).unwrap(),real.join("src/lib.rs"));
    }

    #[test] fn nothing_outside_the_project_opens() {
        let root=project();
        assert_eq!(locate_file(root.path(),"../etc/passwd").unwrap_err().key,"file.open.outside");
        assert_eq!(locate_file(root.path(),"/etc/hosts").unwrap_err().key,"file.open.outside");
        assert_eq!(locate_file(root.path(),"src").unwrap_err().key,"file.open.notFound");
    }

    /// O erro sai do núcleo como chave e valores; o texto é da tela.
    #[test] fn a_failure_reaches_the_screen_as_an_i18n_key() {
        let root=project();
        let failure=locate_file(root.path(),"nao/existe.rs").unwrap_err();
        assert_eq!(serde_json::to_value(&failure).unwrap(),serde_json::json!({"key":"file.open.notFound","params":{"path":"nao/existe.rs"}}));
    }
}
