//! A memória do projeto: o que o JayV aprende com quem usa e leva ao agente
//! sem custar chamada nenhuma a modelo.
//!
//! - **Notas** (`kind = note`): fatos curtos do projeto — como conferir uma
//!   mudança, onde as coisas ficam, o que não fazer. Têm teto somado
//!   ([`NOTES_CHARS`]) e entram no prompt só quando uma sessão do agente
//!   começa: a sessão retomada já as tem, e o prefixo não muda no meio dela.
//!   Nascem à mão ou da portaria ([`SOURCE_GATE`]): quando ela cobra onde fica
//!   ou como conferir e o desenvolvedor responde, a resposta vira nota e a
//!   portaria não cobra de novo.
//! - **Receitas** (`kind = recipe`): um pedido que se repete, com o passo a
//!   passo que o desenvolvedor escreveu uma vez. Só o pedido parecido com o
//!   gatilho dela a leva junto — o prompt não carrega receita à toa.

use crate::{firewall::ContextFirewall, i18n::Text, rag};
use anyhow::{bail, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

pub const SCHEMA:&str="CREATE TABLE IF NOT EXISTS project_notes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'note',
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL,
    trigger TEXT NOT NULL DEFAULT '',
    covers TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS project_notes_project ON project_notes(project_id, kind, created_at);";

pub const KIND_NOTE:&str="note";
pub const KIND_RECIPE:&str="recipe";
pub const SOURCE_MANUAL:&str="manual";
pub const SOURCE_GATE:&str="gate";
pub const SOURCE_REPEAT:&str="repeat";
/// O teto das notas de um projeto, somadas: cerca de 550 tokens. Pequeno de
/// propósito — a nota vai em toda sessão nova, e nota demais vira ruído.
pub const NOTES_CHARS:usize=2_200;
/// O teto de uma receita.
pub const RECIPE_CHARS:usize=2_000;
const TITLE_CHARS:usize=80;
/// O quanto um pedido tem de parecer com o gatilho para a receita ir junto.
const RECIPE_MATCH:f64=0.5;
/// Pedidos parecidos a partir de quantos viram sugestão de receita.
const REPEAT_COUNT:usize=3;
const REPEAT_MATCH:f64=0.5;
const REPEAT_WINDOW:usize=300;
const REPEAT_MIN_TERMS:usize=3;

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProjectNote {
    pub id:String,
    pub project_id:String,
    pub kind:String,
    pub title:String,
    pub body:String,
    pub trigger:String,
    pub covers:String,
    pub source:String,
    pub created_at:String,
    pub updated_at:String,
}

/// O que a tela manda para gravar: sem `id`, é nota nova.
#[derive(Debug,Clone,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct NoteDraft {
    pub id:Option<String>,
    pub project_id:String,
    pub kind:String,
    #[serde(default)] pub title:String,
    pub body:String,
    #[serde(default)] pub trigger:String,
}

/// Um pedido que o desenvolvedor já fez várias vezes no projeto e que ainda
/// não é receita.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct RepeatedRequest { pub prompt:String, pub count:usize }

const COLUMNS:&str="id,project_id,kind,title,body,trigger,covers,source,created_at,updated_at";

fn read(row:&rusqlite::Row)->rusqlite::Result<ProjectNote> {
    Ok(ProjectNote{id:row.get(0)?,project_id:row.get(1)?,kind:row.get(2)?,title:row.get(3)?,body:row.get(4)?,trigger:row.get(5)?,covers:row.get(6)?,source:row.get(7)?,created_at:row.get(8)?,updated_at:row.get(9)?})
}

/// As notas e as receitas do projeto, as notas primeiro, da mais antiga à
/// mais nova.
pub fn notes(connection:&Connection,project_id:&str)->Result<Vec<ProjectNote>> {
    let mut statement=connection.prepare(&format!("SELECT {COLUMNS} FROM project_notes WHERE project_id=?1 ORDER BY kind,created_at,id"))?;
    let rows=statement.query_map([project_id],read)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn note(connection:&Connection,id:&str)->Result<Option<ProjectNote>> {
    Ok(connection.query_row(&format!("SELECT {COLUMNS} FROM project_notes WHERE id=?1"),[id],read).optional()?)
}

fn notes_chars(connection:&Connection,project_id:&str,except:Option<&str>)->Result<usize> {
    Ok(notes(connection,project_id)?.iter().filter(|note|note.kind==KIND_NOTE&&Some(note.id.as_str())!=except).map(|note|note.body.chars().count()).sum())
}

/// Grava a nota ou a receita da tela. Segredo não entra: a nota vai ao agente
/// em toda sessão.
pub fn save(connection:&Connection,draft:&NoteDraft)->Result<ProjectNote> {
    let body=draft.body.trim();
    let title:String=draft.title.trim().chars().take(TITLE_CHARS).collect();
    if body.is_empty() { bail!(Text::new("memory.empty")); }
    if !matches!(draft.kind.as_str(),KIND_NOTE|KIND_RECIPE) { bail!("unknown project memory kind: `{}`",draft.kind); }
    if holds_secret(body)||holds_secret(&title)||holds_secret(&draft.trigger) { bail!(Text::new("memory.secret")); }
    if draft.kind==KIND_RECIPE && title.is_empty() { bail!(Text::new("memory.recipe.untitled")); }
    if draft.kind==KIND_RECIPE && body.chars().count()>RECIPE_CHARS { bail!(Text::new("memory.recipe.tooLong").with("limit",RECIPE_CHARS)); }
    if draft.kind==KIND_NOTE && notes_chars(connection,&draft.project_id,draft.id.as_deref())?+body.chars().count()>NOTES_CHARS { bail!(Text::new("memory.full").with("limit",NOTES_CHARS)); }
    let now=Utc::now().to_rfc3339();
    let trigger=if draft.kind==KIND_RECIPE && draft.trigger.trim().is_empty() { title.clone() } else { draft.trigger.trim().to_string() };
    let id=match &draft.id {
        Some(id)=>{
            let changed=connection.execute("UPDATE project_notes SET title=?1,body=?2,trigger=?3,updated_at=?4 WHERE id=?5 AND project_id=?6",params![title,body,trigger,now,id,draft.project_id])?;
            if changed==0 { bail!(Text::new("memory.missing")); }
            id.clone()
        }
        None=>{
            let id=Uuid::new_v4().to_string();
            let source=if draft.kind==KIND_RECIPE && !draft.trigger.trim().is_empty() {SOURCE_REPEAT} else {SOURCE_MANUAL};
            connection.execute(&format!("INSERT INTO project_notes({COLUMNS}) VALUES(?1,?2,?3,?4,?5,?6,'',?7,?8,?8)"),params![id,draft.project_id,draft.kind,title,body,trigger,source,now])?;
            id
        }
    };
    note(connection,&id)?.ok_or_else(||anyhow::anyhow!("project note `{id}` vanished after writing"))
}

pub fn delete(connection:&Connection,id:&str)->Result<()> {
    connection.execute("DELETE FROM project_notes WHERE id=?1",[id])?;
    Ok(())
}

fn holds_secret(text:&str)->bool {
    let firewall=ContextFirewall::new(Default::default());
    firewall.redact_secrets(text)!=text
}

/// Aprende com a portaria: a frase que respondeu a um critério cobrado vira
/// a nota daquele critério no projeto (uma por critério; a resposta nova toma
/// o lugar da antiga). Sem espaço sob o teto, não aprende — a nota escrita à
/// mão vale mais que a aprendida.
pub fn learn(connection:&Connection,project_id:&str,criterion:&str,sentence:&str)->Result<Option<ProjectNote>> {
    let sentence=sentence.trim();
    if sentence.is_empty()||holds_secret(sentence) { return Ok(None); }
    let existing=connection.query_row("SELECT id FROM project_notes WHERE project_id=?1 AND kind='note' AND covers=?2 AND source=?3",params![project_id,criterion,SOURCE_GATE],|row|row.get::<_,String>(0)).optional()?;
    if notes_chars(connection,project_id,existing.as_deref())?+sentence.chars().count()>NOTES_CHARS { return Ok(None); }
    let now=Utc::now().to_rfc3339();
    let id=match existing {
        Some(id)=>{ connection.execute("UPDATE project_notes SET body=?1,updated_at=?2 WHERE id=?3",params![sentence,now,id])?; id }
        None=>{
            let id=Uuid::new_v4().to_string();
            connection.execute(&format!("INSERT INTO project_notes({COLUMNS}) VALUES(?1,?2,'note','',?3,'',?4,?5,?6,?6)"),params![id,project_id,sentence,criterion,SOURCE_GATE,now])?;
            id
        }
    };
    note(connection,&id)
}

/// Os critérios da portaria que alguma nota do projeto já responde.
pub fn covered(notes:&[ProjectNote])->Vec<String> {
    let mut covered=notes.iter().filter(|note|note.kind==KIND_NOTE&&!note.covers.is_empty()).map(|note|note.covers.clone()).collect::<Vec<_>>();
    covered.sort(); covered.dedup(); covered
}

/// As notas como o agente as lê, no começo de uma sessão.
pub fn notes_prompt(notes:&[ProjectNote])->Option<String> {
    let lines=notes.iter().filter(|note|note.kind==KIND_NOTE).map(|note|format!("- {}",note.body.replace('\n'," "))).collect::<Vec<_>>();
    (!lines.is_empty()).then(||format!("PROJECT NOTES (facts the developer keeps about this project; prefer them over exploring or guessing):\n{}",lines.join("\n")))
}

/// A receita cujo gatilho mais se parece com o pedido, se alguma parece o
/// bastante.
pub fn recipe_for<'a>(notes:&'a [ProjectNote],request:&str)->Option<&'a ProjectNote> {
    let asked=rag::terms(request);
    notes.iter().filter(|note|note.kind==KIND_RECIPE)
        .map(|note|(note,rag::similarity(&asked,&rag::terms(&format!("{} {}",note.title,note.trigger)))))
        .filter(|(_,score)|*score>=RECIPE_MATCH)
        .max_by(|left,right|left.1.total_cmp(&right.1))
        .map(|(note,_)|note)
}

/// A receita como vai junto do pedido.
pub fn recipe_prompt(recipe:&ProjectNote)->String {
    format!("RECIPE \"{}\" (the developer's own procedure for this kind of request; follow it):\n{}",recipe.title,recipe.body)
}

/// Os pedidos que se repetem no projeto e ainda não são receita: grupos de
/// pedidos parecidos com pelo menos [`REPEAT_COUNT`] membros, do maior para
/// o menor. Conta só o que o desenvolvedor escreveu, nunca a resposta.
pub fn repeated_requests(connection:&Connection,project_id:&str)->Result<Vec<RepeatedRequest>> {
    let prompts={
        let mut statement=connection.prepare(
            "SELECT m.content FROM messages m JOIN chats c ON c.id=m.chat_id
             WHERE c.project_id=?1 AND m.role='user' ORDER BY m.created_at DESC,m.id DESC LIMIT ?2")?;
        statement.query_map(params![project_id,REPEAT_WINDOW as i64],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let recipes=notes(connection,project_id)?.into_iter().filter(|note|note.kind==KIND_RECIPE).map(|note|rag::terms(&format!("{} {}",note.title,note.trigger))).collect::<Vec<_>>();
    let mut groups:Vec<(String,HashSet<String>,usize)>=vec![];
    for prompt in prompts {
        // Aviso gravado (pedido barrado, resposta a pergunta) não é pedido.
        if prompt.trim_start().starts_with('{') { continue; }
        let terms=rag::terms(&prompt);
        if terms.len()<REPEAT_MIN_TERMS { continue; }
        match groups.iter_mut().find(|(_,anchor,_)|rag::similarity(anchor,&terms)>=REPEAT_MATCH) {
            Some(group)=>group.2+=1,
            None=>groups.push((prompt.trim().to_string(),terms,1)),
        }
    }
    let mut found=groups.into_iter()
        .filter(|(_,terms,count)|*count>=REPEAT_COUNT&&!recipes.iter().any(|recipe|rag::similarity(recipe,terms)>=RECIPE_MATCH))
        .map(|(prompt,_,count)|RepeatedRequest{prompt,count}).collect::<Vec<_>>();
    found.sort_by(|left,right|right.count.cmp(&left.count));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store()->Connection {
        let connection=Connection::open_in_memory().expect("database");
        connection.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE projects(id TEXT PRIMARY KEY);
            CREATE TABLE chats(id TEXT PRIMARY KEY,project_id TEXT NOT NULL);
            CREATE TABLE messages(id INTEGER PRIMARY KEY AUTOINCREMENT,chat_id TEXT NOT NULL,role TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL);
            INSERT INTO projects VALUES('p1');").expect("schema");
        connection.execute_batch(SCHEMA).expect("notes");
        connection
    }
    fn draft(kind:&str,title:&str,body:&str)->NoteDraft { NoteDraft{project_id:"p1".into(),kind:kind.into(),title:title.into(),body:body.into(),..Default::default()} }

    #[test] fn notes_stay_under_their_ceiling() {
        let connection=store();
        save(&connection,&draft(KIND_NOTE,"","Run the test suite with `make check`.")).expect("first");
        let error=save(&connection,&draft(KIND_NOTE,"",&"x".repeat(NOTES_CHARS))).expect_err("over the ceiling").to_string();
        assert!(error.contains("memory.full"),"{error}");
        let recipe=save(&connection,&draft(KIND_RECIPE,"New endpoint","1. Add the route\n2. Add a test")).expect("recipes have their own ceiling");
        assert_eq!(recipe.trigger,"New endpoint","sem gatilho, o nome é o gatilho");
        assert_eq!(notes(&connection,"p1").unwrap().len(),2);
        let prompt=notes_prompt(&notes(&connection,"p1").unwrap()).expect("prompt");
        assert!(prompt.contains("- Run the test suite with `make check`.")&&!prompt.contains("Add the route"),"receita não vai em toda sessão: {prompt}");
    }

    #[test] fn a_secret_never_becomes_a_note() {
        let connection=store();
        let error=save(&connection,&draft(KIND_NOTE,"","deploy key: AKIAABCDEFGHIJKLMNOP")).expect_err("segredo").to_string();
        assert!(error.contains("memory.secret"),"{error}");
        assert!(learn(&connection,"p1","says_when_done","use the token ghp_abcdefghijklmnopqrstuvwxyz0123456789").unwrap().is_none());
    }

    #[test] fn the_gate_learns_one_note_per_criterion_and_the_newest_answer_wins() {
        let connection=store();
        learn(&connection,"p1","says_when_done","Done when npm test passes").unwrap().expect("learned");
        learn(&connection,"p1","says_when_done","Done when cargo test passes").unwrap().expect("relearned");
        let notes=notes(&connection,"p1").unwrap();
        assert_eq!(notes.len(),1);
        assert_eq!((notes[0].body.as_str(),notes[0].source.as_str()),("Done when cargo test passes",SOURCE_GATE));
        assert_eq!(covered(&notes),vec!["says_when_done".to_string()]);
    }

    #[test] fn a_request_that_repeats_is_offered_as_a_recipe_until_it_becomes_one() {
        let connection=store();
        connection.execute("INSERT INTO chats VALUES('c1','p1')",[]).unwrap();
        for (index,text) in ["add a migration for the orders table","add a migration for the invoices table","add a migration for the users table","explain the router"].iter().enumerate() {
            connection.execute("INSERT INTO messages(chat_id,role,content,created_at) VALUES('c1','user',?1,?2)",params![text,format!("2026-10-03T00:00:0{index}Z")]).unwrap();
        }
        let repeated=repeated_requests(&connection,"p1").unwrap();
        assert_eq!(repeated.len(),1,"{repeated:?}");
        assert_eq!(repeated[0].count,3);
        let recipe=NoteDraft{trigger:repeated[0].prompt.clone(),..draft(KIND_RECIPE,"Migration","1. Create the file under migrations\n2. Run the migration tests")};
        let recipe=save(&connection,&recipe).unwrap();
        assert_eq!(recipe.source,SOURCE_REPEAT);
        assert!(repeated_requests(&connection,"p1").unwrap().is_empty(),"virou receita, sai das sugestões");
        let notes=notes(&connection,"p1").unwrap();
        assert_eq!(recipe_for(&notes,"add a migration for the payments table").map(|recipe|recipe.title.as_str()),Some("Migration"));
        assert!(recipe_for(&notes,"explain the router").is_none());
    }
}
