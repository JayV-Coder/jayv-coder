//! Os arquivos ao vivo na janela: enquanto um pedido roda, olha a pasta do
//! chat a cada segundo e avisa a tela de cada arquivo que mudou. A lógica de
//! ver o que mudou e de montar o antes e o agora mora em `live_files`; aqui
//! ficam o relógio, o aviso e os comandos.

use super::events::{LiveEvent, LIVE_EVENT};
use crate::firewall::ContextFirewall;
use crate::i18n::Text;
use crate::live_files::{self, Change, FileView, Session};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::Duration};
use tauri::{AppHandle, Emitter, State};

/// A sessão de cada chat: a do último pedido, que continua à mão depois que
/// ele termina para a pessoa revisar.
///
/// O mapa só guarda a sessão atrás do cadeado dela e a lista já pronta do que
/// mudou. A olhada — o `git status`, a leitura dos arquivos — segura só a
/// sessão do chat; a lista da tela, os outros chats e o comando que a tela
/// chama na thread principal não esperam o git de ninguém.
pub type SharedLive=Arc<Mutex<HashMap<String,Watched>>>;

pub struct Watched { session:Arc<Mutex<Session>>, turn_id:String, running:bool, files:Vec<Change>, folder:String, mode:&'static str, looks:u64 }

fn hold(session:&Mutex<Session>)->std::sync::MutexGuard<'_,Session> { session.lock().unwrap_or_else(|poisoned|poisoned.into_inner()) }

/// Quantos chats guardam a última sessão: o "antes" ocupa memória.
const KEPT:usize=6;
const EVERY:Duration=Duration::from_millis(1000);

/// Enquanto existir, a pasta é olhada; ao sair de cena (o pedido terminou, por
/// qualquer caminho), a olhada final acontece e o relógio para.
pub(crate) struct LiveWatch{stop:Arc<AtomicBool>}

impl Drop for LiveWatch {
    fn drop(&mut self) {self.stop.store(true,Ordering::SeqCst);}
}

fn locked(live:&SharedLive)->std::sync::MutexGuard<'_,HashMap<String,Watched>> {live.lock().unwrap_or_else(|poisoned|poisoned.into_inner())}

/// A sessão do chat, para olhar ou ler sem segurar o mapa.
fn session_of(live:&SharedLive,chat_id:&str)->Option<Arc<Mutex<Session>>> { locked(live).get(chat_id).map(|watched|watched.session.clone()) }

/// Começa a olhar a pasta do chat para este pedido. A sessão anterior do chat
/// dá lugar à nova, e a tela recebe o aviso de recomeço (`file` nulo). Volta só
/// depois da largada registrada: o agente só sai depois disso, e o que ele
/// gravar logo no começo não se confunde com o que já estava alterado antes.
pub(crate) async fn watch(app:&AppHandle,live:&SharedLive,chat_id:&str,turn_id:&str,folder:PathBuf,firewall:ContextFirewall)->LiveWatch {
    let stop=Arc::new(AtomicBool::new(false));
    let (app,live,chat,turn_id,flag)=(app.clone(),live.clone(),chat_id.to_string(),turn_id.to_string(),stop.clone());
    let turn=turn_id.clone();
    let Ok(session)=tauri::async_runtime::spawn_blocking(move ||Session::start(&turn,&folder,firewall)).await else {return LiveWatch{stop}};
    let (folder_shown,mode)=(session.folder().display().to_string(),session.mode());
    let session=Arc::new(Mutex::new(session));
    {
        let mut sessions=locked(&live);
        sessions.insert(chat.clone(),Watched{session:session.clone(),turn_id:turn_id.clone(),running:true,files:vec![],folder:folder_shown,mode,looks:0});
        if sessions.len()>KEPT {
            let idle:Vec<String>=sessions.iter().filter(|(id,watched)|**id!=chat && !watched.running).map(|(id,_)|id.clone()).collect();
            for id in idle.into_iter().take(sessions.len()-KEPT) {sessions.remove(&id);}
        }
    }
    let _=app.emit(LIVE_EVENT,LiveEvent{chat_id:chat.clone(),turn_id:turn_id.clone(),file:None});
    tauri::async_runtime::spawn(async move {
        loop {
            let last=flag.load(Ordering::SeqCst);
            let (live2,chat2,turn2,mine)=(live.clone(),chat.clone(),turn_id.clone(),session.clone());
            let fresh=tauri::async_runtime::spawn_blocking(move ||{
                // Outro pedido do mesmo chat já tomou o lugar: nada a olhar.
                if !locked(&live2).get(&chat2).is_some_and(|watched|Arc::ptr_eq(&watched.session,&mine)) { return None; }
                let (fresh,files)={
                    let mut session=hold(&mine);
                    let fresh=session.poll();
                    if last {session.running=false;}
                    (fresh,session.changes().to_vec())
                };
                let mut sessions=locked(&live2);
                let watched=sessions.get_mut(&chat2).filter(|watched|Arc::ptr_eq(&watched.session,&mine)&&watched.turn_id==turn2)?;
                watched.files=files;
                watched.looks+=1;
                if last { watched.running=false; }
                Some(fresh)
            }).await.ok().flatten();
            // Outro pedido do mesmo chat já tomou o lugar: este para aqui.
            let Some(fresh)=fresh else {break};
            for file in fresh {let _=app.emit(LIVE_EVENT,LiveEvent{chat_id:chat.clone(),turn_id:turn_id.clone(),file:Some(file)});}
            if last {break;}
            tokio::time::sleep(EVERY).await;
        }
    });
    LiveWatch{stop}
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct LiveList{turn_id:Option<String>,running:bool,files:Vec<Change>,folder:Option<String>,mode:Option<&'static str>,looks:u64}

/// O que o último pedido do chat mudou, do mais recente para o mais antigo.
#[tauri::command]
pub(crate) fn live_files(live:State<'_,SharedLive>,chat_id:String)->LiveList {
    let sessions=locked(&live);
    match sessions.get(&chat_id) {
        Some(watched)=>LiveList{turn_id:Some(watched.turn_id.clone()),running:watched.running,files:watched.files.clone(),folder:Some(watched.folder.clone()),mode:Some(watched.mode),looks:watched.looks},
        None=>LiveList{turn_id:None,running:false,files:vec![],folder:None,mode:None,looks:0},
    }
}

/// O antes e o agora de um arquivo que mudou no último pedido do chat.
#[tauri::command]
pub(crate) async fn live_file(live:State<'_,SharedLive>,chat_id:String,path:String)->Result<Option<FileView>,Text> {
    let live=live.inner().clone();
    tauri::async_runtime::spawn_blocking(move ||session_of(&live,&chat_id).and_then(|session|hold(&session).view(&path))).await.map_err(Text::unexpected)
}

/// Os editores com linha de comando instalados (`code`, `cursor`…).
#[tauri::command]
pub(crate) async fn editors()->Vec<String> {
    tauri::async_runtime::spawn_blocking(live_files::editors).await.unwrap_or_default()
}

/// Abre no editor um arquivo que mudou no último pedido do chat, na linha
/// pedida. Só arquivos da lista: nada de fora da pasta do chat.
#[tauri::command]
pub(crate) fn open_in_editor(live:State<'_,SharedLive>,chat_id:String,path:String,line:u32,editor:String)->Result<(),Text> {
    let file=session_of(&live,&chat_id).and_then(|session|hold(&session).absolute(&path)).ok_or_else(||Text::new("live.notChanged").with("path",&path))?;
    live_files::open_in_editor(&editor,&file,line).map_err(|error|Text::new("live.editor.failed").with("editor",&editor).with("reason",error.to_string()))
}
