//! Quem está usando o app e o que vale para a máquina toda: a sessão que o
//! React entrega, o banco do usuário que ela abre, o estado da conexão e o
//! conteúdo global (idiomas, traduções, parâmetros do Jev).

use crate::i18n::{failure, Text};
use crate::cloud::{remote::Remote, session::{self, fetch_jwks, validate_offline, Identity, SessionError}};
use crate::desktop::events::{LinkEvent, LINK_EVENT, TRANSLATIONS_EVENT};
use crate::desktop::{both, QueueBell, SharedDesktopState, SharedWorkspace, SyncBell};
use crate::local::global::{self, GlobalCache, LocaleRow};
use crate::memory::MemoryManager;
use crate::sync::{Connectivity, Link};
use crate::workspace::WorkspaceStore;
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

pub struct SessionState {
    pub cache:GlobalCache,
    /// Onde moram `cache.sqlite3` e os `workspace-<usuário>.sqlite3`.
    pub data_dir:PathBuf,
    pub identity:Option<Identity>,
    pub http:reqwest::Client,
}

pub type SharedSession=Arc<Mutex<SessionState>>;

impl SessionState {
    async fn refresh_jwks(&mut self)->anyhow::Result<jsonwebtoken::jwk::JwkSet> {
        let keys=fetch_jwks(&self.http).await?;
        self.cache.save_jwks(&keys)?;
        Ok(keys)
    }

    /// Valida contra o JWKS guardado e só o baixa de novo quando não há
    /// nenhum ou quando o token foi assinado por uma chave que ele não tem —
    /// a rotação de chaves do projeto.
    async fn validate(&mut self,token:&str)->Result<Identity,Text> {
        let now=chrono::Utc::now().timestamp();
        let last=self.cache.last_user().map_err(failure)?;
        let keys=match self.cache.jwks().map_err(failure)? {
            Some(keys)=>keys,
            None=>self.refresh_jwks().await.map_err(|error|Text::new("session.offlineFirst").with("reason",format!("{error:#}")))?,
        };
        match validate_offline(token,&keys,last.as_deref(),now) {
            Err(SessionError::UnknownKey)=>{
                let keys=self.refresh_jwks().await.map_err(|error|Text::new("session.keyOffline").with("reason",format!("{error:#}")))?;
                validate_offline(token,&keys,last.as_deref(),now).map_err(Text::from)
            }
            other=>other.map_err(Text::from),
        }
    }
}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SessionView { pub user_id:String, pub email:Option<String>, pub expires_at:i64 }

impl From<&Identity> for SessionView {
    fn from(identity:&Identity)->Self { Self{user_id:identity.user_id.clone(),email:identity.email.clone(),expires_at:identity.expires_at} }
}

/// Troca o banco do orquestrador e da fila. Pega os dois cadeados na ordem
/// de sempre, então espera o pedido que está no ar terminar.
async fn adopt(desk:&SharedDesktopState,workspace:&SharedWorkspace,store:WorkspaceStore)->anyhow::Result<()> {
    let (mut desk,mut workspace)=both(desk,workspace).await;
    desk.orchestrator.use_llm(&store.llm_settings()?);
    desk.orchestrator.memory=MemoryManager::default();
    for chat in store.snapshot()?.chats {
        let conversation=store.conversation(&chat.id)?;
        desk.orchestrator.memory.set_conversation(chat.id,conversation);
    }
    *workspace=store;
    Ok(())
}

/// O React chama isto a cada mudança de sessão — login, renovação do token,
/// volta ao app. Trocar de usuário troca o banco; o mesmo usuário com token
/// novo só destrava a sincronização.
#[tauri::command]
pub(crate) async fn set_session(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,session:State<'_,SharedSession>,sync:State<'_,SyncBell>,queue:State<'_,QueueBell>,token:String)->Result<SessionView,Text> {
    let (identity,changed,dir)={
        let mut state=session.lock().await;
        let identity=state.validate(&token).await?;
        let changed=state.identity.as_ref().map(|current|current.user_id.as_str())!=Some(identity.user_id.as_str());
        state.cache.set_last_user(Some(&identity.user_id)).map_err(failure)?;
        state.identity=Some(identity.clone());
        (identity,changed,state.data_dir.clone())
    };
    session::set_current(Some(token));
    if changed {
        let store=WorkspaceStore::for_user(&dir,&identity.user_id).map_err(failure)?;
        adopt(&desk,&workspace,store).await.map_err(failure)?;
    }
    sync.0.notify_one();
    queue.notify_one();
    tauri::async_runtime::spawn(refresh_jev_parameters(session.inner().clone()));
    Ok(SessionView::from(&identity))
}

/// Logout: o banco do usuário fecha e o app volta ao banco em memória.
#[tauri::command]
pub(crate) async fn clear_session(desk:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,session:State<'_,SharedSession>,sync:State<'_,SyncBell>)->Result<(),Text> {
    session::set_current(None);
    {
        let mut state=session.lock().await;
        state.identity=None;
        state.cache.set_last_user(None).map_err(failure)?;
    }
    adopt(&desk,&workspace,WorkspaceStore::in_memory().map_err(failure)?).await.map_err(failure)?;
    sync.0.notify_one();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ConnectionStatus { pub link:Link, pub pending:i64, pub failed:i64 }

#[tauri::command]
pub(crate) async fn connection_status(workspace:State<'_,SharedWorkspace>,connectivity:State<'_,Connectivity>)->Result<ConnectionStatus,Text> {
    let workspace=workspace.lock().await;
    let (pending,failed)=workspace.connection().query_row(
        "SELECT COUNT(*) FILTER (WHERE status='pending'),COUNT(*) FILTER (WHERE status='failed') FROM outbox",[],|row|Ok((row.get(0)?,row.get(1)?)),
    ).map_err(failure)?;
    Ok(ConnectionStatus{link:connectivity.get(),pending,failed})
}

/// Os idiomas do cache, na hora; a lista nova chega depois pelo evento
/// `translations-updated`, se mudou.
#[tauri::command]
pub(crate) async fn get_locales(app:AppHandle,session:State<'_,SharedSession>)->Result<Vec<LocaleRow>,Text> {
    let locales=session.lock().await.cache.locales().map_err(failure)?;
    tauri::async_runtime::spawn(refresh_locales(app,session.inner().clone()));
    Ok(locales)
}

#[tauri::command]
pub(crate) async fn get_translations(app:AppHandle,session:State<'_,SharedSession>,locale:String)->Result<BTreeMap<String,Value>,Text> {
    let messages=session.lock().await.cache.translations(&locale).map_err(failure)?;
    tauri::async_runtime::spawn(refresh_translations(app,session.inner().clone(),locale));
    Ok(messages)
}

fn remote(http:reqwest::Client)->Remote { Remote::new(http,session::current()) }

/// O evento só sai quando algo mudou: a tela pede as traduções de novo ao
/// recebê-lo, e um aviso a cada pedido viraria um laço.
async fn refresh_locales(app:AppHandle,session:SharedSession) {
    let http=session.lock().await.http.clone();
    let Ok(fresh)=remote(http).locales().await else {return};
    let mut state=session.lock().await;
    if state.cache.locales().ok().as_ref()==Some(&fresh) || fresh.is_empty() {return;}
    if state.cache.save_locales(&fresh).is_ok() {let _=app.emit(TRANSLATIONS_EVENT,());}
}

async fn refresh_translations(app:AppHandle,session:SharedSession,locale:String) {
    let http=session.lock().await.http.clone();
    let Ok(fresh)=remote(http).translations(&locale).await else {return};
    let mut state=session.lock().await;
    if state.cache.translations(&locale).ok().as_ref()==Some(&fresh) || fresh.is_empty() {return;}
    if state.cache.save_translations(&locale,&fresh).is_ok() {let _=app.emit(TRANSLATIONS_EVENT,());}
}

async fn refresh_jev_parameters(session:SharedSession) {
    let http=session.lock().await.http.clone();
    let Ok(fresh)=remote(http).jev_parameters().await else {return};
    if fresh.is_empty() {return;}
    let mut state=session.lock().await;
    if state.cache.save_jev_parameters(&fresh).is_ok() {
        if let Ok(parameters)=state.cache.jev_parameters() {global::set_current_parameters(parameters);}
    }
}

/// Repassa cada mudança da conexão à tela.
pub(crate) async fn announce_links(app:AppHandle,connectivity:Connectivity) {
    let mut changes=connectivity.watch();
    while changes.changed().await.is_ok() {
        let link=*changes.borrow_and_update();
        let _=app.emit(LINK_EVENT,LinkEvent{link});
    }
}
