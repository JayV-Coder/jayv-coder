//! O motor da sincronização: sobe a fila de saída, baixa o que mudou no
//! Supabase e avisa a tela e o atendente quando a conexão muda. A tela nunca
//! espera por ele — quem escreve grava no SQLite e toca o sino.

use crate::cloud::remote::{Backend, RemoteError};
use crate::desktop::SharedWorkspace;
use crate::local::outbox::{self, Op, Pending, SyncTable, TABLES};
use chrono::{DateTime, Duration as Span, SecondsFormat, Utc};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::{watch, Notify};

/// Quantas entradas da fila sobem de uma vez, e quantas linhas descem por página.
pub const BATCH:usize=500;
pub const PAGE:usize=1000;
/// Uma transação no servidor pode gravar `synced_at` e só ficar visível
/// depois de outra mais nova: o cursor recua esta folga para não perdê-la.
pub const OVERLAP:Span=Span::seconds(60);
/// Quantas vezes uma volta renumera turnos antes de desistir dela.
const RENUMBER_LIMIT:usize=50;

#[derive(Debug,Clone,Copy,PartialEq,Eq,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub enum Link { SignedOut, Offline, Online, Expired }

/// O estado da conexão, para quem precisa saber dele: a tela (indicador) e o
/// atendente da fila (só chama pedido com `Online`).
#[derive(Clone)]
pub struct Connectivity(Arc<watch::Sender<Link>>);

impl Default for Connectivity { fn default()->Self { Self(Arc::new(watch::channel(Link::SignedOut).0)) } }

impl Connectivity {
    pub fn get(&self)->Link { *self.0.borrow() }
    pub fn set(&self,link:Link) { self.0.send_if_modified(|current|{let changed=*current!=link; *current=link; changed}); }
    pub fn watch(&self)->watch::Receiver<Link> { self.0.subscribe() }
}

#[derive(Debug,Default,Clone,PartialEq,Eq)]
pub struct Round { pub pushed:usize, pub failed:usize, pub pulled:usize }

/// Uma volta: sobe a fila inteira e baixa todas as tabelas.
pub async fn round(store:&SharedWorkspace,backend:&dyn Backend)->Result<Round,RemoteError> {
    let mut result=Round::default();
    upload(store,backend,&mut result).await?;
    download(store,backend,&mut result).await?;
    refresh_policies(store,backend).await;
    refresh_features(store,backend).await;
    Ok(result)
}

/// A política de LLM desce depois dos projetos, que já subiram com os
/// `repo_keys` desta volta. Falhar aqui não para a volta — o servidor sem a
/// migração da política, por exemplo —, e o cache anterior continua valendo.
async fn refresh_policies(store:&SharedWorkspace,backend:&dyn Backend) {
    match backend.project_policies().await {
        Ok(Some(rows))=>{ if let Err(error)=store.lock().await.replace_project_policies(&rows) {eprintln!("política de LLM: {error:#}");} }
        Ok(None)=>{}
        Err(error)=>eprintln!("política de LLM: {error}"),
    }
}

/// Os recursos do plano descem como a política: falhar (servidor sem a
/// migração dos planos, por exemplo) não para a volta, e o cache anterior
/// continua valendo.
async fn refresh_features(store:&SharedWorkspace,backend:&dyn Backend) {
    match backend.features().await {
        Ok(Some(remote))=>{ if let Err(error)=store.lock().await.replace_entitlements(&remote) {eprintln!("recursos do plano: {error:#}");} }
        Ok(None)=>{}
        Err(error)=>eprintln!("recursos do plano: {error}"),
    }
}

/// Erro do banco local no meio da volta. Não é culpa da rede nem da
/// entrada, e reenviar não ajuda: a volta para e o laço tenta mais tarde.
fn local(error:anyhow::Error)->RemoteError { RemoteError::Rejected{status:0,detail:format!("banco local: {error:#}")} }

fn transient(error:&RemoteError)->bool { matches!(error,RemoteError::Offline(_)|RemoteError::Server(_)|RemoteError::Unauthorized) }

/// Entradas seguidas da mesma tabela e operação, com a linha como está
/// agora. Agrupar só as seguidas mantém a ordem de `seq`: o pai sobe antes do
/// filho.
fn next_batch(store:&crate::workspace::WorkspaceStore,limit:usize)->anyhow::Result<Vec<(Pending,Option<Value>)>> {
    let entries=outbox::pending(store.connection(),limit)?;
    let Some(first)=entries.first().cloned() else {return Ok(vec![])};
    let mut batch=Vec::new();
    for entry in entries.into_iter().take_while(|entry|entry.table==first.table && entry.op==first.op) {
        let row=match (outbox::table(&entry.table),entry.op) {
            (Some(table),Op::Upsert)=>outbox::row(store.connection(),table,&entry.key)?.map(|mut row|{
                row["row_updated_at"]=Value::String(entry.at.clone());
                row["row_deleted_at"]=Value::Null;
                row
            }),
            _=>None,
        };
        batch.push((entry,row));
    }
    Ok(batch)
}

async fn send(backend:&dyn Backend,table:&SyncTable,op:Op,batch:&[(Pending,Option<Value>)])->Result<(),RemoteError> {
    match op {
        Op::Upsert=>backend.push(table,batch.iter().filter_map(|(_,row)|row.clone()).collect()).await,
        // A hora de cada exclusão é a da entrada; o lote vai com a mais
        // recente, que é a que decide contra uma edição em outra máquina.
        Op::Delete=>{
            let keys:Vec<Value>=batch.iter().map(|(entry,_)|entry.key.clone()).collect();
            let at=batch.iter().map(|(entry,_)|entry.at.as_str()).max().unwrap_or_default().to_string();
            backend.soft_delete(table,&keys,&at).await
        }
    }
}

async fn upload(store:&SharedWorkspace,backend:&dyn Backend,result:&mut Round)->Result<(),RemoteError> {
    let mut renumbered=0;
    loop {
        let batch=next_batch(&*store.lock().await,BATCH).map_err(local)?;
        let Some((first,_))=batch.first() else {return Ok(())};
        let Some(table)=outbox::table(&first.table) else {
            // Uma tabela que esta versão não conhece: a entrada não tem para
            // onde ir.
            outbox::fail(store.lock().await.connection(),first.seq,"tabela desconhecida").map_err(local)?;
            result.failed+=1;
            continue;
        };
        let op=first.op;
        match send(backend,table,op,&batch).await {
            Ok(())=>{settle(store,&batch).await?; result.pushed+=batch.len();}
            Err(error) if transient(&error)=>return Err(error),
            Err(_) if batch.len()>1=>{
                // Uma linha do lote foi recusada e o servidor não diz qual:
                // cada uma sobe sozinha, e só a culpada fica para trás.
                for single in batch.chunks(1) {
                    match send(backend,table,op,single).await {
                        Ok(())=>{settle(store,single).await?; result.pushed+=1;}
                        Err(error)=>{
                            if transient(&error) {return Err(error);}
                            refuse(store,table,&single[0].0,error,result,&mut renumbered).await?;
                        }
                    }
                }
            }
            Err(error)=>refuse(store,table,first,error,result,&mut renumbered).await?,
        }
    }
}

async fn settle(store:&SharedWorkspace,batch:&[(Pending,Option<Value>)])->Result<(),RemoteError> {
    let guard=store.lock().await;
    for (entry,_) in batch {outbox::settle(guard.connection(),entry.seq,entry.version).map_err(local)?;}
    Ok(())
}

/// O que fazer com uma entrada recusada. Turno com número repetido no chat —
/// outra máquina abriu o mesmo número sem rede — ganha o próximo número livre
/// e volta à fila; o resto sai da fila com o motivo.
async fn refuse(store:&SharedWorkspace,table:&SyncTable,entry:&Pending,error:RemoteError,result:&mut Round,renumbered:&mut usize)->Result<(),RemoteError> {
    let guard=store.lock().await;
    if table.name=="turns" && matches!(&error,RemoteError::Conflict{code,..} if code=="23505") && *renumbered<RENUMBER_LIMIT {
        let id=entry.key.get(0).and_then(Value::as_str).unwrap_or_default();
        guard.connection().execute(
            "UPDATE turns SET ordinal=(SELECT MAX(ordinal)+1 FROM turns WHERE chat_id=(SELECT chat_id FROM turns WHERE id=?1)) WHERE id=?1",[id],
        ).map_err(|error|local(error.into()))?;
        *renumbered+=1;
        return Ok(());
    }
    outbox::fail(guard.connection(),entry.seq,&error.to_string()).map_err(local)?;
    result.failed+=1;
    Ok(())
}

async fn download(store:&SharedWorkspace,backend:&dyn Backend,result:&mut Round)->Result<(),RemoteError> {
    for table in &TABLES {
        let mut since=outbox::cursor(store.lock().await.connection(),table).map_err(local)?.unwrap_or_default();
        loop {
            // Tabela que o servidor recusa (a migração dela ainda não rodou,
            // uma coluna nova) fica para a próxima volta; as outras descem e a
            // fila de pedidos não fica parada como se faltasse rede.
            let rows=match backend.pull(table,&since,PAGE).await {
                Ok(rows)=>rows,
                Err(RemoteError::Rejected{status,detail})=>{eprintln!("sincronização: `{}` recusada ({status}): {detail}",table.name); break;}
                Err(error)=>return Err(error),
            };
            let Some(last)=rows.last().and_then(|row|row["synced_at"].as_str()).map(str::to_string) else {break};
            let mut guard=store.lock().await;
            result.pulled+=outbox::apply_remote(guard.connection_mut(),table,&rows).map_err(local)?;
            outbox::set_cursor(guard.connection(),table,&behind(&last)).map_err(local)?;
            drop(guard);
            // Página cheia: pode haver mais. A próxima começa na hora da
            // última linha — se a página inteira caiu no mesmo instante, para
            // aqui em vez de pedir a mesma página para sempre.
            if rows.len()<PAGE || last==since {break;}
            since=last;
        }
    }
    Ok(())
}

/// O cursor guardado: a hora da última linha baixada menos a folga.
fn behind(synced_at:&str)->String {
    DateTime::parse_from_rfc3339(synced_at).map(|moment|(moment.with_timezone(&Utc)-OVERLAP).to_rfc3339_opts(SecondsFormat::Micros,true)).unwrap_or_default()
}

/// Espera até `limit`, voltando antes se alguém escrever. Olhar a fila a cada
/// dois segundos agrupa as escritas de um momento numa volta só, sem que cada
/// comando precise lembrar de acordar a sincronização.
async fn until_written(store:&SharedWorkspace,limit:Duration) {
    let step=Duration::from_secs(2);
    let mut waited=Duration::ZERO;
    while waited<limit {
        tokio::time::sleep(step.min(limit-waited)).await;
        waited+=step;
        if outbox::pending(store.lock().await.connection(),1).is_ok_and(|pending|!pending.is_empty()) {return;}
    }
}

/// A espera depois de uma falha de rede: dobra de 5 s até 60 s.
pub fn backoff(previous:Option<Duration>)->Duration {
    previous.map(|pause|(pause*2).min(Duration::from_secs(60))).unwrap_or(Duration::from_secs(5))
}

/// O laço da sincronização. Roda uma volta quando o sino toca (escrita,
/// login, sessão renovada), a cada 60 s com conexão, e no fim do backoff sem
/// ela. Sem usuário, ou com a sessão vencida, só o sino o acorda.
pub async fn run<F>(store:SharedWorkspace,backend:F,connectivity:Connectivity,bell:Arc<Notify>,queue:Arc<Notify>)
where F:Fn()->Option<Arc<dyn Backend>>+Send+Sync+'static {
    let mut pause:Option<Duration>=None;
    loop {
        // Com um pedido no ar a volta espera: baixar turnos e mensagens no
        // meio dele disputaria o banco com a narração e poderia mexer no
        // turno que está sendo atendido. A fila é olhada de novo a cada dois
        // segundos.
        if backend().is_some() && store.lock().await.turn_in_flight().unwrap_or(false) {
            tokio::select!{_=bell.notified()=>{},_=tokio::time::sleep(Duration::from_secs(2))=>{}}
            continue;
        }
        match backend() {
            None=>connectivity.set(Link::SignedOut),
            Some(backend)=>match round(&store,backend.as_ref()).await {
                Ok(_)=>{
                    pause=None;
                    connectivity.set(Link::Online);
                    // Com rede, o atendente pode chamar o que esperava por ela.
                    queue.notify_one();
                }
                Err(RemoteError::Unauthorized)=>{pause=None; connectivity.set(Link::Expired);}
                Err(error)=>{
                    if !matches!(error,RemoteError::Offline(_)|RemoteError::Server(_)) {eprintln!("sincronização: {error}");}
                    pause=Some(backoff(pause));
                    connectivity.set(Link::Offline);
                    // Sem rede, mas com sessão: a fila anda pela heurística.
                    queue.notify_one();
                }
            },
        }
        let wait=match connectivity.get() {
            Link::Online=>Some(Duration::from_secs(60)),
            Link::Offline=>pause,
            Link::SignedOut|Link::Expired=>None,
        };
        match wait {
            Some(wait)=>{tokio::select!{_=bell.notified()=>{},_=until_written(&store,wait)=>{}}}
            None=>bell.notified().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::outbox::table;
    use crate::workspace::WorkspaceStore;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex as Plain;
    use tokio::sync::Mutex;

    /// O servidor de mentira: guarda as linhas por tabela e chave, aplica a
    /// regra da edição mais recente, carimba `synced_at` com um relógio que
    /// só anda e recusa ordinal repetido no mesmo chat, como o Postgres.
    #[derive(Default)]
    struct FakeBackend {
        rows:Plain<BTreeMap<(String,String),Value>>,
        clock:Plain<i64>,
        calls:Plain<Vec<String>>,
        failures:Plain<VecDeque<RemoteError>>,
        /// O que `my_project_policies` devolve; `None` é o backend que não
        /// fala da política.
        policies:Plain<Option<Result<Vec<crate::policy::RemotePolicy>,RemoteError>>>,
    }

    impl FakeBackend {
        fn stamp(&self)->String { let mut clock=self.clock.lock().unwrap(); *clock+=1; (DateTime::<Utc>::from_timestamp(1_900_000_000,0).unwrap()+Span::seconds(*clock)).to_rfc3339_opts(SecondsFormat::Micros,true) }
        fn key(table:&SyncTable,row:&Value)->String { Value::Array(table.key.iter().map(|column|row[*column].clone()).collect()).to_string() }
        fn fail_next(&self,error:RemoteError) { self.failures.lock().unwrap().push_back(error); }
        fn row(&self,table:&str,key:Value)->Option<Value> { self.rows.lock().unwrap().get(&(table.to_string(),key.to_string())).cloned() }
        fn seed(&self,table:&SyncTable,mut row:Value) { row["synced_at"]=json!(self.stamp()); self.rows.lock().unwrap().insert((table.name.to_string(),Self::key(table,&row)),row); }
        fn calls(&self)->Vec<String> { self.calls.lock().unwrap().clone() }
    }

    #[async_trait]
    impl Backend for FakeBackend {
        async fn push(&self,table:&SyncTable,rows:Vec<Value>)->Result<(),RemoteError> {
            self.calls.lock().unwrap().push(format!("push {} {}",table.name,rows.len()));
            if let Some(error)=self.failures.lock().unwrap().pop_front() {return Err(error);}
            let stored=self.rows.lock().unwrap();
            for row in &rows {
                if table.name=="turns" {
                    let taken=stored.iter().any(|((name,key),other)|name=="turns" && *key!=Self::key(table,row) && other["chat_id"]==row["chat_id"] && other["ordinal"]==row["ordinal"]);
                    if taken {return Err(RemoteError::Conflict{code:"23505".into(),detail:"turns_chat_id_ordinal_key".into()});}
                }
            }
            drop(stored);
            for row in rows {
                let key=Self::key(table,&row);
                let newer=self.rows.lock().unwrap().get(&(table.name.to_string(),key.clone())).is_none_or(|old|old["row_updated_at"].as_str()<=row["row_updated_at"].as_str());
                if newer {self.seed(table,row);}
            }
            Ok(())
        }

        async fn soft_delete(&self,table:&SyncTable,keys:&[Value],at:&str)->Result<(),RemoteError> {
            self.calls.lock().unwrap().push(format!("delete {} {}",table.name,keys.len()));
            if let Some(error)=self.failures.lock().unwrap().pop_front() {return Err(error);}
            for key in keys {
                let found=self.rows.lock().unwrap().get(&(table.name.to_string(),key.to_string())).cloned();
                if let Some(mut row)=found {row["row_deleted_at"]=json!(at); row["row_updated_at"]=json!(at); self.seed(table,row);}
            }
            Ok(())
        }

        async fn pull(&self,table:&SyncTable,since:&str,limit:usize)->Result<Vec<Value>,RemoteError> {
            self.calls.lock().unwrap().push(format!("pull {} {since}",table.name));
            let mut rows:Vec<Value>=self.rows.lock().unwrap().iter().filter(|((name,_),row)|name==table.name && row["synced_at"].as_str().unwrap_or("")>=since).map(|(_,row)|row.clone()).collect();
            rows.sort_by(|a,b|a["synced_at"].as_str().cmp(&b["synced_at"].as_str()));
            rows.truncate(limit);
            Ok(rows)
        }

        async fn project_policies(&self)->Result<Option<Vec<crate::policy::RemotePolicy>>,RemoteError> {
            self.policies.lock().unwrap().clone().transpose()
        }
    }

    fn shared(store:WorkspaceStore)->SharedWorkspace { Arc::new(Mutex::new(store)) }

    async fn queue(store:&SharedWorkspace)->Vec<Pending> { outbox::pending(store.lock().await.connection(),10_000).unwrap() }

    async fn a_chat(store:&SharedWorkspace)->(String,String) {
        let mut guard=store.lock().await;
        let project=guard.create_project("Loja",None).unwrap();
        let chat=guard.create_chat(&project.id,None).unwrap();
        (project.id,chat.id)
    }

    #[test] fn the_backoff_doubles_up_to_a_minute() {
        let steps:Vec<u64>=std::iter::successors(Some(backoff(None)),|pause|Some(backoff(Some(*pause)))).take(6).map(|pause|pause.as_secs()).collect();
        assert_eq!(steps,[5,10,20,40,60,60]);
    }

    #[tokio::test] async fn the_queue_uploads_in_order_grouped_by_table() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let (project,chat)=a_chat(&store).await;
        store.lock().await.enqueue_prompt(&chat,"oi",None).unwrap();
        let backend=FakeBackend::default();
        let result=round(&store,&backend).await.expect("volta");
        let pushes:Vec<String>=backend.calls().into_iter().filter(|call|call.starts_with("push")).collect();
        assert_eq!(pushes,["push projects 1","push chats 1","push turns 1","push messages 1"]);
        assert_eq!(result.pushed,4);
        assert!(queue(&store).await.is_empty());
        let sent=backend.row("projects",json!([project])).expect("projeto no remoto");
        assert_eq!(sent["name"],"Loja");
        assert!(sent["row_updated_at"].is_string(),"a hora da escrita local sobe junto");
        assert!(sent["row_deleted_at"].is_null(),"recriar desfaz uma exclusão anterior");
    }

    #[tokio::test] async fn the_round_brings_the_llm_policies_and_survives_their_failure() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let (project,chat)=a_chat(&store).await;
        let backend=FakeBackend::default();
        *backend.policies.lock().unwrap()=Some(Ok(vec![crate::policy::RemotePolicy{project_id:project.clone(),org_slug:"acme".into(),policy:json!({"agents":["codex"]})}]));
        round(&store,&backend).await.expect("volta");
        let cached=store.lock().await.chat_policy(&chat).unwrap().expect("o chat ganhou a política do projeto");
        assert_eq!(cached.org_slug,"acme");

        // Servidor sem a migração: a volta segue e o cache anterior fica.
        *backend.policies.lock().unwrap()=Some(Err(RemoteError::Rejected{status:404,detail:"function not found".into()}));
        round(&store,&backend).await.expect("a falha da política não derruba a volta");
        assert!(store.lock().await.chat_policy(&chat).unwrap().is_some());

        *backend.policies.lock().unwrap()=Some(Ok(vec![]));
        round(&store,&backend).await.expect("volta");
        assert!(store.lock().await.chat_policy(&chat).unwrap().is_none(),"lista vazia tira a política");
    }

    #[tokio::test] async fn offline_the_queue_stays_whole() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        a_chat(&store).await;
        let backend=FakeBackend::default();
        backend.fail_next(RemoteError::Offline("dns".into()));
        assert!(matches!(round(&store,&backend).await,Err(RemoteError::Offline(_))));
        assert_eq!(queue(&store).await.len(),2);
    }

    #[tokio::test] async fn an_expired_session_stops_without_losing_anything() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        a_chat(&store).await;
        let backend=FakeBackend::default();
        backend.fail_next(RemoteError::Unauthorized);
        assert_eq!(round(&store,&backend).await,Err(RemoteError::Unauthorized));
        assert_eq!(queue(&store).await.len(),2);
        assert_eq!(round(&store,&backend).await.expect("com sessão nova").pushed,2);
    }

    /// Duas máquinas abriram o turno 1 do mesmo chat sem rede. A segunda a
    /// subir renumera o seu e sobe de novo — os dois pedidos ficam.
    #[tokio::test] async fn a_repeated_ordinal_renumbers_the_turn_and_uploads_again() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let (_,chat)=a_chat(&store).await;
        let mine=store.lock().await.enqueue_prompt(&chat,"daqui",None).unwrap();
        let backend=FakeBackend::default();
        backend.seed(table("turns").unwrap(),json!({"id":"de-la","chat_id":chat,"ordinal":mine.ordinal,"status":"answered","created_at":"2026-09-30T11:00:00+00:00","row_updated_at":"2026-09-30T11:00:00Z","row_deleted_at":null}));
        round(&store,&backend).await.expect("volta");
        let renumbered=store.lock().await.turn(&mine.id).unwrap().expect("turno");
        assert_ne!(renumbered.ordinal,mine.ordinal);
        assert_eq!(backend.row("turns",json!([mine.id])).expect("subiu")["ordinal"],json!(renumbered.ordinal));
        assert!(queue(&store).await.is_empty());
        assert!(store.lock().await.turn("de-la").unwrap().is_some(),"o turno da outra máquina desceu");
    }

    #[tokio::test] async fn a_refusal_marks_the_entry_and_the_queue_moves_on() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let (project,_)=a_chat(&store).await;
        let backend=FakeBackend::default();
        backend.fail_next(RemoteError::Rejected{status:400,detail:"coluna desconhecida".into()});
        let result=round(&store,&backend).await.expect("volta");
        assert_eq!((result.failed,result.pushed),(1,1));
        assert!(backend.row("projects",json!([project])).is_none());
        let failed:(String,String)=store.lock().await.connection().query_row("SELECT tbl,error FROM outbox WHERE status='failed'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(failed.0,"projects");
        assert!(failed.1.contains("coluna desconhecida"));
    }

    #[tokio::test] async fn deleting_uploads_as_a_marked_deletion() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let (_,chat)=a_chat(&store).await;
        let backend=FakeBackend::default();
        round(&store,&backend).await.unwrap();
        store.lock().await.delete_chat(&chat).unwrap();
        round(&store,&backend).await.unwrap();
        assert!(backend.row("chats",json!([chat])).expect("continua no remoto")["row_deleted_at"].is_string());
        assert!(queue(&store).await.is_empty());
    }

    #[tokio::test] async fn the_download_follows_table_order_and_rewinds_the_cursor() {
        let store=shared(WorkspaceStore::in_memory().unwrap());
        let backend=FakeBackend::default();
        backend.seed(table("chats").unwrap(),json!({"id":"c1","code":"ABC123","project_id":"p1","title":"Remoto","named":1,"created_at":"x","updated_at":"x","row_updated_at":"2026-09-30T11:00:00Z","row_deleted_at":null}));
        backend.seed(table("projects").unwrap(),json!({"id":"p1","name":"Remoto","created_at":"x","row_updated_at":"2026-09-30T11:00:00Z","row_deleted_at":null}));
        let result=round(&store,&backend).await.expect("volta");
        assert_eq!(result.pulled,2);
        assert!(store.lock().await.contains_chat("c1").unwrap(),"o chat desceu depois do projeto, que chegou ao servidor depois dele");
        let pulls:Vec<String>=backend.calls().into_iter().filter(|call|call.starts_with("pull")).map(|call|call.split(' ').nth(1).unwrap().to_string()).collect();
        assert_eq!(pulls,TABLES.iter().map(|table|table.name.to_string()).collect::<Vec<_>>());
        let guard=store.lock().await;
        let cursor=outbox::cursor(guard.connection(),table("chats").unwrap()).unwrap().expect("cursor");
        let last=backend.row("chats",json!(["c1"])).unwrap()["synced_at"].as_str().unwrap().to_string();
        assert_eq!(DateTime::parse_from_rfc3339(&last).unwrap()-DateTime::parse_from_rfc3339(&cursor).unwrap(),OVERLAP);
        assert!(outbox::pending(guard.connection(),10).unwrap().is_empty());
    }
}
