//! A busca nas conversas do projeto, num índice FTS5 do próprio SQLite.
//!
//! Serve a dois fins. A tela acha um chat antigo pelo que se disse nele. E o
//! atendimento acha um pedido quase igual já respondido noutro chat do mesmo
//! projeto: a resposta de lá vai junto, curta, e o agente parte dela em vez de
//! explorar e responder tudo de novo.
//!
//! O índice é da máquina: não sobe. As mensagens que a sincronização baixa
//! entram nele pelos mesmos gatilhos das escritas locais.

use crate::rag;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const SCHEMA:&str="CREATE VIRTUAL TABLE IF NOT EXISTS message_search USING fts5(content, content='messages', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER IF NOT EXISTS message_search_insert AFTER INSERT ON messages BEGIN
  INSERT INTO message_search(rowid,content) VALUES (new.id,new.content);
END;
CREATE TRIGGER IF NOT EXISTS message_search_delete AFTER DELETE ON messages BEGIN
  INSERT INTO message_search(message_search,rowid,content) VALUES('delete',old.id,old.content);
END;
CREATE TRIGGER IF NOT EXISTS message_search_update AFTER UPDATE OF content ON messages BEGIN
  INSERT INTO message_search(message_search,rowid,content) VALUES('delete',old.id,old.content);
  INSERT INTO message_search(rowid,content) VALUES (new.id,new.content);
END;";

/// O pedido de antes tem de ser quase o mesmo para a resposta dele valer.
const RECALL_MATCH:f64=0.75;
const RECALL_MIN_TERMS:usize=4;
const RECALL_CANDIDATES:i64=20;
const RECALL_CHARS:usize=1_500;
const HIT_CANDIDATES:i64=60;
/// Os marcadores do trecho achado: a tela troca por destaque.
pub const MARK_START:&str="\u{2}";
pub const MARK_END:&str="\u{3}";

/// Cria o índice. No banco que ainda não o tinha, indexa o que já existe.
pub fn ensure(connection:&Connection)->Result<()> {
    let existed:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='message_search')",[],|row|row.get(0))?;
    connection.execute_batch(SCHEMA)?;
    if !existed { connection.execute("INSERT INTO message_search(message_search) VALUES('rebuild')",[])?; }
    Ok(())
}

/// Um chat achado pela busca, com o trecho que casou.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SearchHit { pub chat_id:String, pub title:String, pub role:String, pub snippet:String, pub at:String }

/// A consulta FTS5 com as palavras do texto, cada uma entre aspas (nada do
/// que a pessoa digita vira operador). `all`: todas precisam estar.
fn query(text:&str,all:bool,prefix:bool)->Option<String> {
    let mut words=rag::terms(text).into_iter().collect::<Vec<_>>();
    words.sort();
    let quoted=words.iter().map(|word|format!("\"{}\"{}",word.replace('"',""),if prefix {"*"} else {""})).collect::<Vec<_>>();
    (!quoted.is_empty()).then(||quoted.join(if all {" AND "} else {" OR "}))
}

/// Os chats do projeto onde aparece o que foi buscado, o mais relevante
/// primeiro, um trecho por chat.
pub fn search(connection:&Connection,project_id:&str,text:&str,limit:usize)->Result<Vec<SearchHit>> {
    let Some(query)=query(text,true,true) else { return Ok(vec![]) };
    let mut statement=connection.prepare(
        "SELECT m.chat_id,c.title,m.role,snippet(message_search,0,?3,?4,'…',12),m.created_at
           FROM message_search JOIN messages m ON m.id=message_search.rowid JOIN chats c ON c.id=m.chat_id
          WHERE message_search MATCH ?1 AND c.project_id=?2
          ORDER BY bm25(message_search) LIMIT ?5")?;
    let rows=statement.query_map(params![query,project_id,MARK_START,MARK_END,HIT_CANDIDATES],|row|Ok(SearchHit{chat_id:row.get(0)?,title:row.get(1)?,role:row.get(2)?,snippet:row.get(3)?,at:row.get(4)?}))?;
    let mut hits:Vec<SearchHit>=vec![];
    for row in rows {
        let hit=row?;
        if !hits.iter().any(|seen|seen.chat_id==hit.chat_id) { hits.push(hit); }
        if hits.len()>=limit { break; }
    }
    Ok(hits)
}

/// A resposta que o projeto já tem para um pedido quase igual, feito noutro
/// chat e respondido.
#[derive(Debug,Clone,PartialEq)]
pub struct Recall { pub chat_id:String, pub title:String, pub request:String, pub answer:String }

pub fn recall(connection:&Connection,project_id:&str,chat_id:&str,request:&str)->Result<Option<Recall>> {
    let asked=rag::terms(request);
    if asked.len()<RECALL_MIN_TERMS { return Ok(None); }
    let Some(query)=query(request,false,false) else { return Ok(None) };
    let candidates={
        let mut statement=connection.prepare(
            "SELECT m.chat_id,c.title,m.content,m.turn_id
               FROM message_search JOIN messages m ON m.id=message_search.rowid JOIN chats c ON c.id=m.chat_id
              WHERE message_search MATCH ?1 AND c.project_id=?2 AND m.chat_id<>?3 AND m.role='user' AND m.turn_id IS NOT NULL
              ORDER BY bm25(message_search) LIMIT ?4")?;
        statement.query_map(params![query,project_id,chat_id,RECALL_CANDIDATES],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let best=candidates.into_iter()
        .map(|candidate|{let score=rag::similarity(&asked,&rag::terms(&candidate.2));(candidate,score)})
        .filter(|(_,score)|*score>=RECALL_MATCH)
        .max_by(|left,right|left.1.total_cmp(&right.1));
    let Some(((chat_id,title,request,turn_id),_))=best else { return Ok(None) };
    let answer=connection.query_row(
        "SELECT m.content FROM messages m JOIN turns t ON t.id=m.turn_id
          WHERE m.turn_id=?1 AND m.role='assistant' AND t.status='answered' ORDER BY m.id DESC LIMIT 1",
        [&turn_id],|row|row.get::<_,String>(0));
    match answer {
        Ok(answer)=>Ok(Some(Recall{chat_id,title,request,answer:answer.chars().take(RECALL_CHARS).collect()})),
        Err(rusqlite::Error::QueryReturnedNoRows)=>Ok(None),
        Err(error)=>Err(error.into()),
    }
}

/// O que vai junto do pedido para o agente partir da resposta de antes.
pub fn recall_prompt(recall:&Recall)->String {
    let cut=if recall.answer.chars().count()>=RECALL_CHARS {"\n[EARLIER ANSWER SHORTENED]"} else {""};
    format!("EARLIER ANSWER: a near-identical request was already answered in another chat of this project. Reuse it when it still holds and check only what may have changed.\nEARLIER REQUEST: {}\nEARLIER ANSWER:\n{}{cut}",recall.request,recall.answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store()->Connection {
        let connection=Connection::open_in_memory().expect("database");
        connection.execute_batch("
            CREATE TABLE chats(id TEXT PRIMARY KEY,project_id TEXT NOT NULL,title TEXT NOT NULL);
            CREATE TABLE turns(id TEXT PRIMARY KEY,chat_id TEXT NOT NULL,status TEXT NOT NULL);
            CREATE TABLE messages(id INTEGER PRIMARY KEY AUTOINCREMENT,chat_id TEXT NOT NULL,turn_id TEXT,role TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL);
            INSERT INTO chats VALUES('old','p1','Cálculo do frete'),('new','p1','Outro'),('elsewhere','p2','Frete de outro projeto');
            INSERT INTO turns VALUES('t1','old','answered');
            INSERT INTO messages(chat_id,turn_id,role,content,created_at) VALUES
              ('old','t1','user','how is the shipping rate computed for international orders','2026-10-01T00:00:00Z'),
              ('old','t1','assistant','The shipping rate comes from compute_shipping_rate in src/lib.rs.','2026-10-01T00:00:01Z');
        ").expect("schema");
        ensure(&connection).expect("index");
        connection
    }

    #[test] fn the_index_covers_what_existed_and_what_arrives() {
        let connection=store();
        assert_eq!(search(&connection,"p1","shipping",5).unwrap().len(),1,"o que já existia entra no índice");
        connection.execute("INSERT INTO messages(chat_id,turn_id,role,content,created_at) VALUES('new',NULL,'user','the invoice totals are wrong','2026-10-02T00:00:00Z')",[]).unwrap();
        let hits=search(&connection,"p1","invoice",5).unwrap();
        assert_eq!(hits[0].chat_id,"new");
        assert!(hits[0].snippet.contains(&format!("{MARK_START}invoice{MARK_END}")),"{}",hits[0].snippet);
        assert!(search(&connection,"p1","\" OR 1",5).unwrap().is_empty(),"o texto digitado nunca vira operador");
        connection.execute("DELETE FROM messages WHERE chat_id='new'",[]).unwrap();
        assert!(search(&connection,"p1","invoice",5).unwrap().is_empty(),"apagou, sai do índice");
    }

    #[test] fn a_near_identical_request_brings_the_earlier_answer() {
        let connection=store();
        let found=recall(&connection,"p1","new","How is the shipping rate computed for international orders?").unwrap().expect("quase igual");
        assert_eq!(found.chat_id,"old");
        assert!(recall_prompt(&found).contains("compute_shipping_rate"));
        assert!(recall(&connection,"p1","new","how is the invoice total computed").unwrap().is_none(),"parecido não basta");
        assert!(recall(&connection,"p1","old","how is the shipping rate computed for international orders").unwrap().is_none(),"o mesmo chat já tem a conversa");
        assert!(recall(&connection,"p2","x","how is the shipping rate computed for international orders").unwrap().is_none(),"outro projeto, outra memória");
    }
}
