//! A fila de saída. Cada escrita nas tabelas que o Supabase espelha deixa uma
//! entrada aqui, gravada por gatilho na mesma transação da escrita — nenhum
//! método do `WorkspaceStore` precisa lembrar de avisar a sincronização, e um
//! método novo não tem como esquecer.
//!
//! A entrada guarda só a chave da linha: quem sobe lê a linha como ela está na
//! hora de subir. Várias escritas na mesma linha viram uma entrada só, com a
//! `version` somada, e é a `version` que impede de dar por entregue uma escrita
//! que chegou depois do envio.

use anyhow::{Context, Result};
use rusqlite::{params_from_iter, types::Value as Sql, Connection, OptionalExtension};
use serde_json::{Map, Value};

/// Uma tabela espelhada: o nome, a chave com que o Supabase a conhece e as
/// colunas que sobem. O que fica de fora — `projects.root_path`,
/// `turns.partial`, `turns.local`, `messages.id` — é da máquina.
pub struct SyncTable {
    pub name:&'static str,
    pub key:&'static [&'static str],
    pub columns:&'static [&'static str],
}

/// Em ordem de dependência: quem sobe primeiro é quem os outros apontam.
pub const TABLES:[SyncTable;10]=[
    SyncTable{name:"projects",key:&["id"],columns:&["id","name","created_at"]},
    SyncTable{name:"chats",key:&["id"],columns:&["id","code","project_id","title","named","created_at","updated_at"]},
    SyncTable{name:"turns",key:&["id"],columns:&["id","chat_id","ordinal","status","created_at"]},
    SyncTable{name:"messages",key:&["uid"],columns:&["uid","chat_id","turn_id","role","content","created_at"]},
    SyncTable{name:"entry_checks",key:&["turn_id"],columns:&["turn_id","at","prompt","score","demand","verdict","scope","criteria","source","note"]},
    SyncTable{name:"exit_checks",key:&["id"],columns:&["id","turn_id","at","kind","target","rule","verdict"]},
    SyncTable{name:"turn_events",key:&["id"],columns:&["id","turn_id","at","seq","kind","detail"]},
    SyncTable{name:"questions",key:&["turn_id"],columns:&["turn_id","at","kind","prompt","options","source","status","answered_by","settled_at"]},
    SyncTable{name:"llm_agents",key:&["id"],columns:&["id","enabled","command","timeout","options","updated_at"]},
    SyncTable{name:"llm_models",key:&["agent","model"],columns:&["agent","model","enabled","capabilities","cost_class","speed","context_window","position"]},
];

pub fn table(name:&str)->Option<&'static SyncTable> { TABLES.iter().find(|table|table.name==name) }

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Op { Upsert, Delete }

impl Op {
    fn parse(value:&str)->Result<Self> {
        match value {"upsert"=>Ok(Op::Upsert),"delete"=>Ok(Op::Delete),other=>anyhow::bail!("operação desconhecida na fila: `{other}`")}
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct Pending {
    pub seq:i64,
    pub table:String,
    pub key:Value,
    pub op:Op,
    /// O instante da última escrita local: sobe como `row_updated_at`, e é com
    /// ele que o servidor decide quem ganha.
    pub at:String,
    pub version:i64,
}

/// O instante com milissegundos e em UTC, no formato que o Postgres lê como
/// `timestamptz`.
const NOW:&str="strftime('%Y-%m-%dT%H:%M:%fZ','now')";

/// Cria a fila, os cursores, a marca de aplicação e os gatilhos. Roda depois
/// dos padrões do banco novo (os agentes de fábrica): o que nasce em toda
/// máquina não é escrita do desenvolvedor e não pode subir por cima do que ele
/// já tem no Supabase.
pub fn install(connection:&Connection)->Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS outbox (
           seq INTEGER PRIMARY KEY AUTOINCREMENT,
           tbl TEXT NOT NULL,
           row_key TEXT NOT NULL,
           op TEXT NOT NULL,
           at TEXT NOT NULL,
           version INTEGER NOT NULL DEFAULT 1,
           status TEXT NOT NULL DEFAULT 'pending',
           error TEXT,
           UNIQUE(tbl,row_key)
         );
         CREATE TABLE IF NOT EXISTS sync_state (tbl TEXT PRIMARY KEY, cursor TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS sync_flag (applying INTEGER NOT NULL);
         INSERT INTO sync_flag(applying) SELECT 0 WHERE NOT EXISTS(SELECT 1 FROM sync_flag);
         UPDATE sync_flag SET applying=0;",
    )?;
    for table in &TABLES {connection.execute_batch(&triggers(table))?;}
    Ok(())
}

/// Os três gatilhos de uma tabela. O de update só olha as colunas que sobem:
/// é assim que o rascunho do streaming fica fora da fila.
fn triggers(table:&SyncTable)->String {
    let name=table.name;
    let key=|row:&str|format!("json_array({})",table.key.iter().map(|column|format!("{row}.{column}")).collect::<Vec<_>>().join(","));
    let enqueue=|row:&str,op:&str|format!(
        "INSERT INTO outbox(tbl,row_key,op,at) VALUES('{name}',{},'{op}',{NOW})
         ON CONFLICT(tbl,row_key) DO UPDATE SET op=excluded.op,at=excluded.at,version=outbox.version+1,status='pending',error=NULL;",
        key(row),
    );
    let quiet="WHEN (SELECT applying FROM sync_flag)=0";
    format!(
        "CREATE TRIGGER IF NOT EXISTS outbox_{name}_insert AFTER INSERT ON {name} {quiet} BEGIN {} END;
         CREATE TRIGGER IF NOT EXISTS outbox_{name}_update AFTER UPDATE OF {} ON {name} {quiet} BEGIN {} END;
         CREATE TRIGGER IF NOT EXISTS outbox_{name}_delete AFTER DELETE ON {name} {quiet} BEGIN {} END;",
        enqueue("NEW","upsert"),table.columns.join(","),enqueue("NEW","upsert"),enqueue("OLD","delete"),
    )
}

/// O que falta subir, na ordem em que foi escrito pela primeira vez. As que
/// falharam ficam de fora até a próxima escrita na mesma linha.
pub fn pending(connection:&Connection,limit:usize)->Result<Vec<Pending>> {
    let mut statement=connection.prepare("SELECT seq,tbl,row_key,op,at,version FROM outbox WHERE status='pending' ORDER BY seq LIMIT ?1")?;
    let rows=statement.query_map([limit as i64],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?)))?;
    rows.map(|row|{
        let (seq,table,key,op,at,version)=row?;
        Ok(Pending{seq,table,key:serde_json::from_str(&key).with_context(||format!("chave inválida na fila: {key}"))?,op:Op::parse(&op)?,at,version})
    }).collect()
}

/// A linha como está agora, só com as colunas que sobem. `None` quando ela
/// já não existe — a entrada, então, é de exclusão.
pub fn row(connection:&Connection,table:&SyncTable,key:&Value)->Result<Option<Value>> {
    let sql=format!("SELECT {} FROM {} WHERE {}",table.columns.join(","),table.name,matching(table));
    connection.query_row(&sql,params_from_iter(key_values(table,key)?),|row|{
        let mut object=Map::new();
        for (index,column) in table.columns.iter().enumerate() {object.insert((*column).to_owned(),to_json(row.get::<_,Sql>(index)?));}
        Ok(Value::Object(object))
    }).optional().map_err(Into::into)
}

/// Dá a entrada por entregue — se nada foi escrito na linha depois do envio.
pub fn settle(connection:&Connection,seq:i64,version:i64)->Result<()> {
    connection.execute("DELETE FROM outbox WHERE seq=?1 AND version=?2",[seq,version])?;
    Ok(())
}

/// O servidor recusou a entrada por um motivo que reenviar não resolve. Ela
/// sai da fila com o motivo, e volta se a linha for escrita de novo.
pub fn fail(connection:&Connection,seq:i64,error:&str)->Result<()> {
    connection.execute("UPDATE outbox SET status='failed',error=?2 WHERE seq=?1",rusqlite::params![seq,error])?;
    Ok(())
}

/// Aplica as linhas baixadas sem acordar os gatilhos. A linha com escrita
/// local pendente é pulada: o servidor decide entre as duas quando ela subir.
/// Devolve quantas linhas foram aplicadas.
pub fn apply_remote(connection:&mut Connection,table:&SyncTable,rows:&[Value])->Result<usize> {
    let transaction=connection.transaction()?;
    transaction.execute("UPDATE sync_flag SET applying=1",[])?;
    let mut applied=0;
    for remote in rows {
        let key=Value::Array(table.key.iter().map(|column|remote.get(*column).cloned().unwrap_or(Value::Null)).collect());
        let waiting:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE tbl=?1 AND row_key=json(?2) AND status='pending')",[table.name,&key.to_string()],|row|row.get(0))?;
        if waiting {continue;}
        let deleted=remote.get("row_deleted_at").is_some_and(|value|!value.is_null());
        if deleted {
            transaction.execute(&format!("DELETE FROM {} WHERE {}",table.name,matching(table)),params_from_iter(key_values(table,&key)?))?;
        } else {
            transaction.execute(&upsert(table),params_from_iter(table.columns.iter().map(|column|to_sql(remote.get(*column).unwrap_or(&Value::Null)))))?;
        }
        applied+=1;
    }
    transaction.execute("UPDATE sync_flag SET applying=0",[])?;
    transaction.commit()?;
    Ok(applied)
}

/// O último `synced_at` baixado desta tabela.
pub fn cursor(connection:&Connection,table:&SyncTable)->Result<Option<String>> {
    Ok(connection.query_row("SELECT cursor FROM sync_state WHERE tbl=?1",[table.name],|row|row.get(0)).optional()?)
}

pub fn set_cursor(connection:&Connection,table:&SyncTable,cursor:&str)->Result<()> {
    connection.execute("INSERT INTO sync_state(tbl,cursor) VALUES(?1,?2) ON CONFLICT(tbl) DO UPDATE SET cursor=excluded.cursor",[table.name,cursor])?;
    Ok(())
}

/// Um turno baixado é de outra máquina: nasce com `local = 0`, e a fila daqui
/// nunca o chama.
fn upsert(table:&SyncTable)->String {
    let arrived=if table.name=="turns" {",local"} else {""};
    let marks=(1..=table.columns.len()).map(|index|format!("?{index}")).collect::<Vec<_>>().join(",");
    let arrived_value=if table.name=="turns" {",0"} else {""};
    let updates=table.columns.iter().filter(|column|!table.key.contains(column)).map(|column|format!("{column}=excluded.{column}")).collect::<Vec<_>>().join(",");
    format!("INSERT INTO {}({}{arrived}) VALUES({marks}{arrived_value}) ON CONFLICT({}) DO UPDATE SET {updates}",table.name,table.columns.join(","),table.key.join(","))
}

fn matching(table:&SyncTable)->String {
    table.key.iter().enumerate().map(|(index,column)|format!("{column}=?{}",index+1)).collect::<Vec<_>>().join(" AND ")
}

fn key_values(table:&SyncTable,key:&Value)->Result<Vec<Sql>> {
    let parts=key.as_array().filter(|parts|parts.len()==table.key.len()).with_context(||format!("chave de `{}` fora do formato: {key}",table.name))?;
    Ok(parts.iter().map(to_sql).collect())
}

fn to_sql(value:&Value)->Sql {
    match value {
        Value::Null=>Sql::Null,
        Value::Bool(flag)=>Sql::Integer(*flag as i64),
        Value::Number(number)=>number.as_i64().map(Sql::Integer).unwrap_or_else(||Sql::Real(number.as_f64().unwrap_or_default())),
        Value::String(text)=>Sql::Text(text.clone()),
        other=>Sql::Text(other.to_string()),
    }
}

fn to_json(value:Sql)->Value {
    match value {
        Sql::Null=>Value::Null,
        Sql::Integer(number)=>Value::from(number),
        Sql::Real(number)=>Value::from(number),
        Sql::Text(text)=>Value::String(text),
        Sql::Blob(bytes)=>Value::String(String::from_utf8_lossy(&bytes).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceStore;
    use crate::turns::TurnStatus;
    use serde_json::json;

    fn settle_all(store:&WorkspaceStore) {
        for entry in pending(store.connection(),10_000).expect("fila") {settle(store.connection(),entry.seq,entry.version).expect("settle");}
    }

    fn entries(store:&WorkspaceStore,table:&str)->Vec<Pending> {
        pending(store.connection(),10_000).expect("fila").into_iter().filter(|entry|entry.table==table).collect()
    }

    fn project_name(store:&WorkspaceStore,id:&str)->Option<String> {
        store.connection().query_row("SELECT name FROM projects WHERE id=?1",[id],|row|row.get(0)).optional().expect("projeto")
    }

    /// Os padrões dos agentes nascem em toda máquina nova. Se subissem, a
    /// primeira abertura num computador novo apagaria a configuração que o
    /// desenvolvedor já tem no Supabase.
    #[test] fn um_banco_novo_nasce_com_a_fila_vazia() {
        let store=WorkspaceStore::in_memory().expect("store");
        assert!(pending(store.connection(),100).expect("fila").is_empty());
        assert!(!store.llm_settings().expect("llm").agents.is_empty());
    }

    #[test] fn criar_um_projeto_poe_uma_entrada_na_fila() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let queue=entries(&store,"projects");
        assert_eq!(queue.len(),1);
        assert_eq!(queue[0].key,json!([project.id]));
        assert_eq!(queue[0].op,Op::Upsert);
        assert_eq!(queue[0].version,1);
        assert_eq!(row(store.connection(),table("projects").unwrap(),&queue[0].key).expect("linha"),Some(json!({"id":project.id,"name":"Loja","created_at":project.created_at.to_rfc3339()})));
    }

    #[test] fn escritas_no_mesmo_turno_viram_uma_entrada_so() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        store.set_turn_status(&turn.id,TurnStatus::Flying).expect("no ar");
        store.set_turn_status(&turn.id,TurnStatus::Answered).expect("respondido");
        let queue=entries(&store,"turns");
        assert_eq!(queue.len(),1);
        assert_eq!(queue[0].version,3);
    }

    /// O rascunho do streaming muda a cada pedaço que chega. Se ele subisse,
    /// cada resposta viraria centenas de envios.
    #[test] fn o_rascunho_do_streaming_nao_entra_na_fila() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        settle_all(&store);
        store.set_turn_partial(&turn.id,"meia resp").expect("rascunho");
        store.clear_turn_partial(&turn.id).expect("limpa");
        assert!(pending(store.connection(),100).expect("fila").is_empty());
    }

    #[test] fn apagar_um_chat_apaga_tambem_os_filhos_no_remoto() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        settle_all(&store);
        store.delete_chat(&chat.id).expect("apaga");
        let queue=pending(store.connection(),100).expect("fila");
        let deleted:Vec<(&str,&Value)>=queue.iter().filter(|entry|entry.op==Op::Delete).map(|entry|(entry.table.as_str(),&entry.key)).collect();
        assert!(deleted.contains(&("chats",&json!([chat.id]))),"{deleted:?}");
        assert!(deleted.contains(&("turns",&json!([turn.id]))),"{deleted:?}");
        assert!(deleted.iter().any(|(table,_)|*table=="messages"),"{deleted:?}");
        assert!(!queue.iter().any(|entry|entry.table=="projects"),"o projeto não foi tocado");
    }

    #[test] fn o_que_vem_do_remoto_nao_volta_para_a_fila() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let applied=apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":"p1","name":"Remoto","created_at":"2026-09-30T12:00:00+00:00","row_updated_at":"2026-09-30T12:00:00Z","row_deleted_at":null,"synced_at":"2026-09-30T12:00:00Z"})]).expect("aplica");
        assert_eq!(applied,1);
        assert_eq!(project_name(&store,"p1").as_deref(),Some("Remoto"));
        assert!(pending(store.connection(),100).expect("fila").is_empty());
    }

    /// A escrita local ainda não subiu: quem decide entre as duas é o
    /// servidor, na subida. Aplicar a remota aqui apagaria a local antes disso.
    #[test] fn a_linha_com_escrita_pendente_nao_e_sobrescrita() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Local",None).expect("projeto");
        let applied=apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":project.id,"name":"Remoto","created_at":"2026-09-30T12:00:00+00:00","row_deleted_at":null})]).expect("aplica");
        assert_eq!(applied,0);
        assert_eq!(project_name(&store,&project.id).as_deref(),Some("Local"));
    }

    #[test] fn a_linha_apagada_no_remoto_sai_do_cache() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        settle_all(&store);
        apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":project.id,"name":"Loja","created_at":"2026-09-30T12:00:00+00:00","row_deleted_at":"2026-09-30T13:00:00Z"})]).expect("aplica");
        assert_eq!(project_name(&store,&project.id),None);
        assert!(!store.contains_chat(&chat.id).expect("chat"));
        assert!(pending(store.connection(),100).expect("fila").is_empty(),"apagar o que veio do remoto não sobe de volta");
    }

    /// A escrita chegou enquanto a anterior subia. Dar a entrada por entregue
    /// perderia a segunda.
    #[test] fn entregar_uma_versao_antiga_nao_tira_a_entrada() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let sent=entries(&store,"chats").remove(0);
        store.rename_chat(&chat.id,"Outro nome").expect("renomeia");
        settle(store.connection(),sent.seq,sent.version).expect("settle");
        let left=entries(&store,"chats");
        assert_eq!(left.len(),1);
        assert_eq!(left[0].version,sent.version+1);
        settle(store.connection(),left[0].seq,left[0].version).expect("settle");
        assert!(entries(&store,"chats").is_empty());
    }

    #[test] fn a_entrada_que_falhou_sai_da_fila_ate_a_proxima_escrita() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let sent=entries(&store,"chats").remove(0);
        fail(store.connection(),sent.seq,"recusado").expect("falha");
        assert!(entries(&store,"chats").is_empty());
        store.rename_chat(&chat.id,"Outro nome").expect("renomeia");
        assert_eq!(entries(&store,"chats").len(),1);
    }

    fn remote_chat(store:&mut WorkspaceStore)->(String,String) {
        apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":"p1","name":"Remoto","created_at":"2026-09-30T12:00:00+00:00"})]).expect("projeto");
        apply_remote(store.connection_mut(),table("chats").unwrap(),&[json!({"id":"c1","code":"ABC123","project_id":"p1","title":"Remoto","named":1,"created_at":"2026-09-30T12:00:00+00:00","updated_at":"2026-09-30T12:00:00+00:00"})]).expect("chat");
        apply_remote(store.connection_mut(),table("turns").unwrap(),&[json!({"id":"t1","chat_id":"c1","ordinal":1,"status":"queued","created_at":"2026-09-30T12:00:00+00:00"})]).expect("turno");
        ("c1".into(),"t1".into())
    }

    /// Dois computadores com o mesmo usuário: o pedido que outro computador
    /// pôs na fila é dele, e atender aqui também faria o trabalho duas vezes.
    #[test] fn a_fila_so_atende_pedidos_desta_maquina() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let (chat,turn)=remote_chat(&mut store);
        apply_remote(store.connection_mut(),table("messages").unwrap(),&[json!({"uid":"m1","chat_id":chat,"turn_id":turn,"role":"user","content":"oi","created_at":"2026-09-30T12:00:00+00:00"})]).expect("mensagem");
        assert!(store.claim_next_turn().expect("fila").is_none());
        apply_remote(store.connection_mut(),table("turns").unwrap(),&[json!({"id":"t2","chat_id":chat,"ordinal":2,"status":"flying","created_at":"2026-09-30T12:01:00+00:00"})]).expect("turno no ar");
        let mine=store.enqueue_prompt(&chat,"daqui",None).expect("turno");
        let (claimed,prompt)=store.claim_next_turn().expect("fila").expect("o pedido daqui é atendido mesmo com outro no ar lá");
        assert_eq!((claimed.id,prompt.as_str()),(mine.id,"daqui"));
    }

    #[test] fn mensagens_baixadas_fora_de_ordem_aparecem_pela_hora() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let (chat,turn)=remote_chat(&mut store);
        apply_remote(store.connection_mut(),table("messages").unwrap(),&[
            json!({"uid":"m2","chat_id":chat,"turn_id":turn,"role":"assistant","content":"resposta","created_at":"2026-09-30T12:00:05+00:00"}),
            json!({"uid":"m1","chat_id":chat,"turn_id":turn,"role":"user","content":"pedido","created_at":"2026-09-30T12:00:00+00:00"}),
        ]).expect("mensagens");
        let contents:Vec<String>=store.conversation(&chat).expect("conversa").into_iter().map(|message|message.content).collect();
        assert_eq!(contents,["pedido","resposta"]);
    }

    #[test] fn o_cursor_de_cada_tabela_fica_guardado() {
        let store=WorkspaceStore::in_memory().expect("store");
        let chats=table("chats").unwrap();
        assert_eq!(cursor(store.connection(),chats).expect("cursor"),None);
        set_cursor(store.connection(),chats,"2026-09-30T12:00:00Z").expect("grava");
        assert_eq!(cursor(store.connection(),chats).expect("cursor").as_deref(),Some("2026-09-30T12:00:00Z"));
        assert_eq!(cursor(store.connection(),table("projects").unwrap()).expect("cursor"),None);
    }
}
