//! A validação do build: os comandos que a pessoa escreve para o projeto e a
//! volta ao ponto gravado antes do build. Nenhum chama modelo.

use crate::desktop::events::*;
use crate::desktop::SharedWorkspace;
use crate::i18n::{failure, Text};
use crate::{snapshot, validator};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// O que a tela precisa para editar os comandos de validação de um projeto:
/// o que a pessoa escreveu e o que o JayV detecta na pasta.
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ValidationSetup {
    pub manual:validator::Manual,
    pub detected:validator::Plan,
}

async fn setup_of(workspace:&SharedWorkspace,chat_id:&str)->Result<ValidationSetup,Text> {
    let (root,stored)={
        let store=workspace.lock().await;
        let root=store.chat_root(chat_id).map_err(failure)?.ok_or_else(||Text::new("validation.noFolder"))?;
        let stored=store.validation_commands(&root.to_string_lossy()).map_err(failure)?;
        (root,stored)
    };
    let manual=serde_json::from_str::<validator::Manual>(&stored).unwrap_or_default();
    let detected=tokio::task::spawn_blocking(move||validator::detect(&root)).await.map_err(|error|failure(anyhow::anyhow!("{error}")))?;
    Ok(ValidationSetup{manual,detected})
}

/// Os comandos de validação do projeto do chat: os escritos e os detectados.
#[tauri::command]
pub(crate) async fn validation_setup(workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<ValidationSetup,Text>{crate::desktop::require_session()?;
    setup_of(&workspace,&chat_id).await
}

/// Grava os comandos que a pessoa escreveu para o projeto do chat. Tudo vazio
/// apaga o que havia: o JayV volta a detectar.
#[tauri::command]
pub(crate) async fn save_validation_commands(workspace:State<'_,SharedWorkspace>,chat_id:String,manual:validator::Manual)->Result<ValidationSetup,Text>{crate::desktop::require_session()?;
    let manual=manual.cleaned();
    {
        let mut store=workspace.lock().await;
        let root=store.chat_root(&chat_id).map_err(failure)?.ok_or_else(||Text::new("validation.noFolder"))?;
        let json=if manual.is_empty() { String::new() } else { serde_json::to_string(&manual).map_err(|error|failure(anyhow::anyhow!("{error}")))? };
        store.save_validation_commands(&root.to_string_lossy(),&json).map_err(failure)?;
    }
    setup_of(&workspace,&chat_id).await
}

/// Devolve a pasta do projeto ao que era antes do build do turno. Só o último
/// build do chat volta (uma volta mais antiga desfaria também o que veio
/// depois), e só com o chat parado: um agente escrevendo na pasta ao mesmo
/// tempo perderia o que escreve. A pasta de antes da volta fica guardada.
#[tauri::command]
pub(crate) async fn rollback_checkpoint(app:AppHandle,workspace:State<'_,SharedWorkspace>,turn_id:String)->Result<snapshot::Rollback,Text>{crate::desktop::require_session()?;
    let (chat_id,root,point)={
        let store=workspace.lock().await;
        let turn=store.turn(&turn_id).map_err(failure)?.ok_or_else(||Text::new("validation.noCheckpoint"))?;
        let chat_id=turn.chat_id;
        if store.has_open_turn(&chat_id).map_err(failure)? { return Err(Text::new("validation.busy")); }
        if store.latest_checkpoint_turn(&chat_id).map_err(failure)?.as_deref()!=Some(turn_id.as_str()) { return Err(Text::new("validation.notLatest")); }
        let (detail,back)=store.checkpoint_of(&turn_id).map_err(failure)?.ok_or_else(||Text::new("validation.noCheckpoint"))?;
        if back { return Err(Text::new("validation.alreadyBack")); }
        let text=|key:&str|detail.get(key).and_then(|value|value.as_str()).map(str::to_string);
        let (Some(id),Some(commit))=(text("id"),text("commit")) else { return Err(Text::new("validation.noCheckpoint")) };
        let root=store.chat_root(&chat_id).map_err(failure)?.ok_or_else(||Text::new("validation.noFolder"))?;
        (chat_id,root,snapshot::Checkpoint{id,commit,head:text("head")})
    };
    let safety=format!("{}-before-rollback",point.id);
    let done=tokio::task::spawn_blocking(move||snapshot::rollback(&root,&point,&safety)).await
        .map_err(|error|failure(anyhow::anyhow!("{error}")))?
        .map_err(|error|{ eprintln!("ponto de volta: {error:#}"); Text::new("validation.rollbackFailed").with("reason",format!("{error:#}")) })?;
    {
        let mut store=workspace.lock().await;
        let _=store.record_beat(&turn_id,"rolled_back",&serde_json::json!({"files":done.files,"headMoved":done.head_moved,"safety":done.safety}));
    }
    let _=app.emit(TURN_EVENT,TurnEvent{chat_id,turn_id});
    Ok(done)
}
