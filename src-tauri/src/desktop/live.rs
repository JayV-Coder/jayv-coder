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
pub type SharedLive=Arc<Mutex<HashMap<String,Session>>>;

/// Quantos chats guardam a última sessão: o "antes" ocupa memória.
const KEPT:usize=6;
const EVERY:Duration=Duration::from_millis(1000);

/// Enquanto existir, a pasta é olhada; ao sair de cena (o pedido terminou, por
/// qualquer caminho), a olhada final acontece e o relógio para.
pub(crate) struct LiveWatch{stop:Arc<AtomicBool>}

impl Drop for LiveWatch {
    fn drop(&mut self) {self.stop.store(true,Ordering::SeqCst);}
}

fn locked(live:&SharedLive)->std::sync::MutexGuard<'_,HashMap<String,Session>> {live.lock().unwrap_or_else(|poisoned|poisoned.into_inner())}

/// Começa a olhar a pasta do chat para este pedido. A sessão anterior do chat
/// dá lugar à nova, e a tela recebe o aviso de recomeço (`file` nulo).
pub(crate) fn watch(app:&AppHandle,live:&SharedLive,chat_id:&str,turn_id:&str,folder:PathBuf,firewall:ContextFirewall)->LiveWatch {
    let stop=Arc::new(AtomicBool::new(false));
    let (app,live,chat_id,turn_id,flag)=(app.clone(),live.clone(),chat_id.to_string(),turn_id.to_string(),stop.clone());
    tauri::async_runtime::spawn(async move {
        let started={
            let (chat,turn,folder)=(chat_id.clone(),turn_id.clone(),folder.clone());
            tauri::async_runtime::spawn_blocking(move ||Session::start(&turn,&folder,firewall)).await.ok().map(|session|(chat,session))
        };
        let Some((chat,session))=started else {return};
        {
            let mut sessions=locked(&live);
            sessions.insert(chat.clone(),session);
            if sessions.len()>KEPT {
                let idle:Vec<String>=sessions.iter().filter(|(id,session)|**id!=chat && !session.running).map(|(id,_)|id.clone()).collect();
                for id in idle.into_iter().take(sessions.len()-KEPT) {sessions.remove(&id);}
            }
        }
        let _=app.emit(LIVE_EVENT,LiveEvent{chat_id:chat.clone(),turn_id:turn_id.clone(),file:None});
        loop {
            let last=flag.load(Ordering::SeqCst);
            let (live2,chat2,turn2)=(live.clone(),chat.clone(),turn_id.clone());
            let fresh=tauri::async_runtime::spawn_blocking(move ||{
                let mut sessions=locked(&live2);
                match sessions.get_mut(&chat2) {
                    Some(session) if session.turn_id==turn2=>{
                        let fresh=session.poll();
                        if last {session.running=false;}
                        Some(fresh)
                    }
                    _=>None,
                }
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
pub(crate) struct LiveList{turn_id:Option<String>,running:bool,files:Vec<Change>}

/// O que o último pedido do chat mudou, do mais recente para o mais antigo.
#[tauri::command]
pub(crate) fn live_files(live:State<'_,SharedLive>,chat_id:String)->LiveList {
    let sessions=locked(&live);
    match sessions.get(&chat_id) {
        Some(session)=>LiveList{turn_id:Some(session.turn_id.clone()),running:session.running,files:session.changes().to_vec()},
        None=>LiveList{turn_id:None,running:false,files:vec![]},
    }
}

/// O antes e o agora de um arquivo que mudou no último pedido do chat.
#[tauri::command]
pub(crate) async fn live_file(live:State<'_,SharedLive>,chat_id:String,path:String)->Result<Option<FileView>,Text> {
    let live=live.inner().clone();
    tauri::async_runtime::spawn_blocking(move ||locked(&live).get(&chat_id).and_then(|session|session.view(&path))).await.map_err(Text::unexpected)
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
    let file=locked(&live).get(&chat_id).and_then(|session|session.absolute(&path)).ok_or_else(||Text::new("live.notChanged").with("path",&path))?;
    live_files::open_in_editor(&editor,&file,line).map_err(|error|Text::new("live.editor.failed").with("editor",&editor).with("reason",error.to_string()))
}
