//! Quem está usando o app e o que vale para a máquina toda: a sessão que o
//! React entrega, o banco do usuário que ela abre, o estado da conexão e o
//! conteúdo global (idiomas, traduções, parâmetros do Jev).

use crate::i18n::{failure, Text};
use crate::cloud::{remote::Remote, session::{self, fetch_jwks, validate_offline, Identity, SessionError}};
use crate::cloud::remote::Backend;
use crate::desktop::events::{EnvironmentEvent, LinkEvent, ENVIRONMENT_EVENT, LINK_EVENT, MODELS_EVENT, TRANSLATIONS_EVENT};
use crate::environment::Environment;
use crate::desktop::{Lanes, QueueBell, SharedDesktopState, SharedWorkspace, SyncBell};
use crate::local::global::{GlobalCache, LocaleRow};
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
    /// O ambiente do banco aberto: o pessoal ou o de uma organização.
    pub environment:Environment,
    /// O usuário cujos projetos de organização já saíram do banco pessoal
    /// nesta execução.
    pub relocated_for:Option<String>,
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

/// Troca o banco do orquestrador e da fila. Pega os cadeados na ordem de
/// sempre — os atendentes, depois o banco —, então espera os pedidos que estão
/// no ar terminarem. Os atendentes extras vão embora com o usuário antigo.
async fn adopt(lanes:&Lanes,workspace:&SharedWorkspace,store:WorkspaceStore)->anyhow::Result<()> {
    // A fila para de chamar antes: o pedido chamado do banco antigo não sai
    // para um atendente que já vai servir o novo.
    let _switching=lanes.switching().await;
    let first=lanes.first();
    let mut desk=first.lock().await;
    let extras=lanes.hold_extras().await;
    let mut workspace=workspace.lock().await;
    desk.orchestrator.use_llm(&store.llm_settings()?);
    let defaults=desk.orchestrator.core_defaults();
    desk.orchestrator.use_core(&store.core_settings(&defaults)?);
    desk.orchestrator.memory=MemoryManager::default();
    for chat in store.snapshot()?.chats {
        let conversation=store.conversation(&chat.id)?;
        desk.orchestrator.memory.set_conversation(chat.id,conversation);
    }
    *workspace=store;
    drop(extras);
    lanes.retire_extras();
    Ok(())
}

/// O React chama isto a cada mudança de sessão — login, renovação do token,
/// volta ao app. Trocar de usuário troca o banco; o mesmo usuário com token
/// novo só destrava a sincronização.
#[tauri::command]
pub(crate) async fn set_session(app:AppHandle,lanes:State<'_,Lanes>,workspace:State<'_,SharedWorkspace>,session:State<'_,SharedSession>,sync:State<'_,SyncBell>,queue:State<'_,QueueBell>,token:String)->Result<SessionView,Text> {
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
        // O app volta ao ambiente em que o usuário estava; o que não se lê
        // ou não existe mais cai no pessoal.
        let environment={
            let state=session.lock().await;
            state.cache.last_environment(&identity.user_id).ok().flatten().and_then(|text|Environment::parse(&text)).unwrap_or_default()
        };
        let store=WorkspaceStore::for_environment(&dir,&identity.user_id,&environment).map_err(failure)?;
        adopt(&lanes,&workspace,store).await.map_err(failure)?;
        session.lock().await.environment=environment;
        tauri::async_runtime::spawn(refresh_models(app.clone(),lanes.first(),workspace.inner().clone()));
    }
    tauri::async_runtime::spawn(relocate_moved_projects(app.clone(),session.inner().clone(),workspace.inner().clone()));
    sync.0.notify_one();
    queue.notify_one();
    tauri::async_runtime::spawn(refresh_jev_parameters(session.inner().clone()));
    Ok(SessionView::from(&identity))
}

/// Logout: o banco do usuário fecha e o app volta ao banco em memória.
#[tauri::command]
pub(crate) async fn clear_session(lanes:State<'_,Lanes>,workspace:State<'_,SharedWorkspace>,session:State<'_,SharedSession>,sync:State<'_,SyncBell>)->Result<(),Text> {
    session::set_current(None);
    {
        let mut state=session.lock().await;
        state.identity=None;
        state.environment=Environment::Personal;
        state.relocated_for=None;
        state.cache.set_last_user(None).map_err(failure)?;
    }
    adopt(&lanes,&workspace,WorkspaceStore::in_memory().map_err(failure)?).await.map_err(failure)?;
    sync.0.notify_one();
    Ok(())
}

/// `refusals` são as entradas que o servidor recusou, por dono: a tela mostra
/// cada uma no chat ou no projeto dela, e só o resto fica no rodapé.
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ConnectionStatus { pub link:Link, pub pending:i64, pub refusals:crate::local::outbox::Refusals }

#[tauri::command]
pub(crate) async fn connection_status(workspace:State<'_,SharedWorkspace>,connectivity:State<'_,Connectivity>)->Result<ConnectionStatus,Text> {
    let workspace=workspace.lock().await;
    let pending=workspace.connection().query_row("SELECT COUNT(*) FROM outbox WHERE status='pending'",[],|row|row.get(0)).map_err(failure)?;
    let refusals=crate::local::outbox::refusals(workspace.connection()).map_err(failure)?;
    Ok(ConnectionStatus{link:connectivity.get(),pending,refusals})
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

/// Ao abrir o banco de um usuário, a lista de modelos de cada agente vem do
/// `/model` do CLI dele, e o que é novo nasce ligado para o Jev escolher.
async fn refresh_models(app:AppHandle,desk:SharedDesktopState,workspace:SharedWorkspace) {
    match super::settings::rediscover(&desk,&workspace,&crate::llm::AgentId::ALL).await {
        Ok(_)=>{let _=app.emit(MODELS_EVENT,());}
        Err(error)=>eprintln!("[llm] model discovery not saved: {error:#}"),
    }
}

async fn refresh_jev_parameters(session:SharedSession) {
    let http=session.lock().await.http.clone();
    let Ok(fresh)=remote(http).jev_parameters().await else {return};
    if fresh.is_empty() {return;}
    let mut state=session.lock().await;
    if state.cache.save_jev_parameters(&fresh).is_ok() {
        if let Ok(values)=state.cache.jev_parameter_values() {crate::gatekeeper::set_current_parameters(crate::gatekeeper::JevParameters::from_values(&values));}
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

/// O ambiente do banco aberto.
#[tauri::command]
pub(crate) async fn current_environment(session:State<'_,SharedSession>)->Result<String,Text> {
    Ok(session.lock().await.environment.id())
}

/// Troca o banco aberto pelo de outro ambiente do mesmo usuário: cada um tem
/// as suas configurações e os seus dados. Uma organização que ainda não tem
/// banco neste computador só abre se o servidor confirma que a pessoa é membro.
#[tauri::command]
pub(crate) async fn set_environment(app:AppHandle,lanes:State<'_,Lanes>,workspace:State<'_,SharedWorkspace>,session:State<'_,SharedSession>,sync:State<'_,SyncBell>,queue:State<'_,QueueBell>,environment:String)->Result<String,Text> {
    let wanted=Environment::parse(&environment).ok_or_else(||Text::new("environment.unknown").with("environment",environment.clone()))?;
    let (user,dir,http)={
        let state=session.lock().await;
        let identity=state.identity.as_ref().ok_or_else(||Text::new("session.required"))?;
        if state.environment==wanted {return Ok(wanted.id());}
        (identity.user_id.clone(),state.data_dir.clone(),state.http.clone())
    };
    if !wanted.is_personal() {
        let known=WorkspaceStore::environment_path(&dir,&user,&wanted).map_err(failure)?.exists();
        if !known {
            let listed=remote(http).environments().await.map_err(|error|Text::new("environment.listFailed").with("reason",error.to_string()))?;
            if !listed.iter().any(|item|item.id==wanted.id()) {return Err(Text::new("environment.notMember"));}
        }
    }
    let store=WorkspaceStore::for_environment(&dir,&user,&wanted).map_err(failure)?;
    adopt(&lanes,&workspace,store).await.map_err(failure)?;
    {
        let mut state=session.lock().await;
        state.environment=wanted.clone();
        state.cache.set_last_environment(&user,&wanted.id()).map_err(failure)?;
    }
    tauri::async_runtime::spawn(refresh_models(app.clone(),lanes.first(),workspace.inner().clone()));
    let _=app.emit(ENVIRONMENT_EVENT,EnvironmentEvent{environment:wanted.id()});
    sync.0.notify_one();
    queue.notify_one();
    Ok(wanted.id())
}

/// Antes dos ambientes, os projetos de uma organização moravam no banco
/// pessoal. O servidor já os guarda no ambiente de cada organização; aqui o
/// que é da máquina (pasta, sessões dos agentes, permissões) vai para o banco
/// dessa organização e o pessoal fica só com o que é pessoal. Roda uma vez por
/// execução e usuário; sem rede ou com um pedido no ar, tenta de novo na
/// próxima sessão.
async fn relocate_moved_projects(app:AppHandle,session:SharedSession,workspace:SharedWorkspace) {
    let (user,dir,http)={
        let state=session.lock().await;
        let Some(identity)=state.identity.as_ref() else {return};
        if !state.environment.is_personal() || state.relocated_for.as_deref()==Some(identity.user_id.as_str()) {return;}
        (identity.user_id.clone(),state.data_dir.clone(),state.http.clone())
    };
    let moved=match remote(http).moved_projects().await {
        Ok(Some(moved))=>moved,
        Ok(None)=>return,
        Err(error)=>{eprintln!("ambientes: não consegui ler os projetos das organizações ({error})"); return;}
    };
    let mut groups:BTreeMap<String,Vec<String>>=BTreeMap::new();
    for project in moved {
        if Environment::parse(&project.environment_id).is_some_and(|environment|!environment.is_personal()) {
            groups.entry(project.environment_id).or_default().push(project.id);
        }
    }
    let mut state_changed=false;
    {
        let store=workspace.lock().await;
        if !store.environment().is_personal() || store.turn_in_flight().unwrap_or(true) {return;}
        for (environment,ids) in &groups {
            let Some(environment)=Environment::parse(environment) else {continue};
            let Ok(path)=WorkspaceStore::environment_path(&dir,&user,&environment) else {continue};
            let Ok(personal)=WorkspaceStore::environment_path(&dir,&user,&Environment::Personal) else {continue};
            // Uma cópia do banco de antes: a mudança só tira do pessoal, e isto
            // deixa voltar atrás se algo der errado.
            let backup=personal.with_extension("sqlite3.before-environments");
            if !backup.exists() && store.connection().query_row("SELECT COUNT(*) FROM projects WHERE id IN (SELECT value FROM json_each(?1))",[serde_json::to_string(ids).unwrap_or_default()],|row|row.get::<_,i64>(0)).unwrap_or(0)>0 {
                if let Err(error)=std::fs::copy(&personal,&backup) {eprintln!("ambientes: sem cópia de segurança, nada foi movido ({error})"); return;}
            }
            match crate::environments::relocate(store.connection(),&path,ids) {
                Ok(0)=>{}
                Ok(moved)=>{eprintln!("ambientes: {moved} projeto(s) foram para o ambiente {environment}"); state_changed=true;}
                Err(error)=>{eprintln!("ambientes: não consegui mover projetos para {environment}: {error:#}"); return;}
            }
        }
    }
    session.lock().await.relocated_for=Some(user);
    if state_changed {let _=app.emit(ENVIRONMENT_EVENT,EnvironmentEvent{environment:Environment::Personal.id()});}
}
