//! A mudança de ambiente de projetos que já estavam no banco pessoal: antes
//! dos ambientes, o projeto de uma organização morava no mesmo banco do
//! pessoal. O servidor já separou as linhas (`environment_id`); aqui o que é
//! da máquina — pasta do projeto, sessões dos agentes, permissões — vai junto
//! para o banco do ambiente de destino, e o pessoal fica só com o que é dele.

use crate::local::outbox::TABLES;
use crate::workspace::WorkspaceStore;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::{collections::BTreeSet, path::Path};

/// Onde o projeto se liga ao resto: tabela e o filtro que a prende aos
/// projetos que mudam. O pai vem antes do filho.
const MOVING:&str="SELECT id FROM temp.moving";
const CHATS:&str="SELECT id FROM main.chats WHERE project_id IN (SELECT id FROM temp.moving)";
const TURNS:&str="SELECT id FROM main.turns WHERE chat_id IN (SELECT id FROM main.chats WHERE project_id IN (SELECT id FROM temp.moving))";

fn plan()->Vec<(&'static str,String)> {
    vec![
        ("projects",format!("id IN ({MOVING})")),
        ("chats",format!("project_id IN ({MOVING})")),
        ("turns",format!("chat_id IN ({CHATS})")),
        ("messages",format!("chat_id IN ({CHATS})")),
        ("entry_checks",format!("turn_id IN ({TURNS})")),
        ("exit_checks",format!("turn_id IN ({TURNS})")),
        ("turn_events",format!("turn_id IN ({TURNS})")),
        ("questions",format!("turn_id IN ({TURNS})")),
        ("agent_sessions",format!("chat_id IN ({CHATS})")),
        ("chat_grants",format!("chat_id IN ({CHATS})")),
        ("allowed_commands",format!("project_id IN ({MOVING})")),
        ("project_notes",format!("project_id IN ({MOVING})")),
        ("usage_records",format!("project_id IN ({MOVING})")),
        ("jev_records",format!("project_id IN ({MOVING})")),
    ]
}

fn columns(connection:&Connection,schema:&str,table:&str)->Result<Vec<String>> {
    let mut statement=connection.prepare(&format!("SELECT name FROM pragma_table_info('{table}','{schema}')"))?;
    let names=statement.query_map([],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// Leva os projetos `ids` que existem neste banco, com tudo que depende deles,
/// para o banco `destination` (criado se faltar) e os tira daqui. Nada disso
/// entra na fila de saída: o servidor já tem as linhas no ambiente certo. As
/// subidas que ainda estavam na fila vão junto, para o ambiente de destino
/// terminá-las. Tudo numa transação só; devolve quantos projetos mudaram.
pub fn relocate(connection:&Connection,destination:&Path,ids:&[String])->Result<usize> {
    if ids.is_empty() {return Ok(0);}
    // O destino ganha as tabelas, os gatilhos e os padrões do app.
    drop(WorkspaceStore::open(destination.to_path_buf())?);
    connection.execute("ATTACH DATABASE ?1 AS dest",[destination.to_string_lossy().as_ref()]).context("could not attach the destination database")?;
    let result=relocate_attached(connection,ids);
    let detached=connection.execute_batch("DETACH DATABASE dest");
    let moved=result?;
    detached?;
    Ok(moved)
}

fn relocate_attached(connection:&Connection,ids:&[String])->Result<usize> {
    connection.execute_batch("DROP TABLE IF EXISTS temp.moving; CREATE TEMP TABLE moving(id TEXT PRIMARY KEY)")?;
    for id in ids {connection.execute("INSERT OR IGNORE INTO temp.moving(id) SELECT id FROM main.projects WHERE id=?1",[id])?;}
    let count:usize=connection.query_row("SELECT COUNT(*) FROM temp.moving",[],|row|row.get(0))?;
    if count==0 {return Ok(0);}
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let done=(|| -> Result<()> {
        connection.execute_batch("UPDATE main.sync_flag SET applying=1; UPDATE dest.sync_flag SET applying=1;")?;
        let tables:BTreeSet<&str>=plan().iter().map(|(table,_)|*table).collect();
        for (table,filter) in plan() {
            let wanted=columns(connection,"dest",table)?;
            let have=columns(connection,"main",table)?;
            // `messages.id` é do banco: o `uid` identifica a mensagem.
            let shared:Vec<&String>=wanted.iter().filter(|name|have.contains(name) && !(table=="messages" && *name=="id")).collect();
            if shared.is_empty() {continue;}
            let list=shared.iter().map(|name|format!("\"{name}\"")).collect::<Vec<_>>().join(",");
            connection.execute_batch(&format!("INSERT OR IGNORE INTO dest.{table}({list}) SELECT {list} FROM main.{table} WHERE {filter}"))?;
        }
        // A fila: as entradas das linhas que mudam passam para o destino.
        for sync in TABLES.iter().filter(|sync|tables.contains(sync.name)) {
            let Some((_,filter))=plan().into_iter().find(|(table,_)|*table==sync.name) else {continue};
            let key=format!("json_array({})",sync.key.iter().map(|column|format!("\"{column}\"")).collect::<Vec<_>>().join(","));
            connection.execute_batch(&format!(
                "INSERT OR REPLACE INTO dest.outbox(tbl,row_key,op,at,version,status,error)
                   SELECT tbl,row_key,op,at,version,status,error FROM main.outbox
                    WHERE tbl='{name}' AND row_key IN (SELECT {key} FROM main.{name} WHERE {filter});
                 DELETE FROM main.outbox WHERE tbl='{name}' AND row_key IN (SELECT {key} FROM main.{name} WHERE {filter});",
                name=sync.name))?;
        }
        // Sem chave estrangeira entre si, o uso e as notas saem explicitamente; o
        // resto vai em cascata com o projeto.
        for table in ["usage_records","jev_records"] {
            connection.execute_batch(&format!("DELETE FROM main.{table} WHERE project_id IN ({MOVING})"))?;
        }
        connection.execute_batch(&format!("DELETE FROM main.projects WHERE id IN ({MOVING})"))?;
        connection.execute_batch("UPDATE main.sync_flag SET applying=0; UPDATE dest.sync_flag SET applying=0;")?;
        Ok(())
    })();
    match done {
        Ok(())=>{connection.execute_batch("COMMIT")?; Ok(count)}
        Err(error)=>{let _=connection.execute_batch("ROLLBACK"); Err(error)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::outbox;
    use crate::workspace::WorkspaceStore;

    fn count(connection:&Connection,sql:&str)->i64 { connection.query_row(sql,[],|row|row.get(0)).expect("count") }

    /// Dois projetos com uma pasta cada: o `moved` muda de ambiente, o `kept` fica.
    fn seeded(dir:&Path)->(WorkspaceStore,String,String) {
        let root=dir.join("moved-folder"); std::fs::create_dir_all(&root).unwrap();
        let other=dir.join("kept-folder"); std::fs::create_dir_all(&other).unwrap();
        let mut store=WorkspaceStore::open(dir.join("personal.sqlite3")).expect("store");
        let moved=store.create_project("Moved",Some(root.display().to_string())).expect("project");
        let kept=store.create_project("Kept",Some(other.display().to_string())).expect("project");
        for project in [&moved,&kept] {
            let chat=store.create_chat(&project.id,Some("Chat".into())).expect("chat");
            store.connection().execute("INSERT INTO messages(chat_id,role,content,created_at) VALUES(?1,'user','unicorn question','2026-10-08T00:00:00Z')",[&chat.id]).unwrap();
            store.connection().execute("INSERT INTO allowed_commands(project_id,command,created_at) VALUES(?1,'cargo test','2026-10-08T00:00:00Z')",[&project.id]).unwrap();
            store.connection().execute("INSERT INTO usage_records(id,project_id,chat_id,source,model,precision,created_at) VALUES(?1,?2,?3,'claude','m','reported','2026-10-08T00:00:00Z')",rusqlite::params![format!("u-{}",project.id),project.id,chat.id]).unwrap();
        }
        (store,moved.id,kept.id)
    }

    #[test] fn a_project_leaves_with_everything_that_depends_on_it() {
        let dir=tempfile::tempdir().unwrap();
        let (store,moved,kept)=seeded(dir.path());
        let destination=dir.path().join("org.sqlite3");
        assert_eq!(relocate(store.connection(),&destination,&[moved.clone(),"unknown".into()]).expect("relocate"),1);
        // Aqui só sobra o projeto que ficou, com tudo dele.
        assert_eq!(count(store.connection(),"SELECT COUNT(*) FROM projects"),1);
        assert_eq!(count(store.connection(),&format!("SELECT COUNT(*) FROM projects WHERE id='{kept}'")),1);
        for table in ["chats","messages","allowed_commands","usage_records"] {
            assert_eq!(count(store.connection(),&format!("SELECT COUNT(*) FROM {table}")),1,"{table} keeps one");
        }
        // No destino está o outro, com a pasta, que é da máquina e não sobe.
        let target=WorkspaceStore::open(destination).expect("destination");
        assert_eq!(count(target.connection(),&format!("SELECT COUNT(*) FROM projects WHERE id='{moved}' AND root_path LIKE '%moved-folder'")),1);
        for table in ["chats","messages","allowed_commands","usage_records"] {
            assert_eq!(count(target.connection(),&format!("SELECT COUNT(*) FROM {table}")),1,"{table} arrives");
        }
        // A busca de conversas funciona no destino sem reindexar.
        assert_eq!(crate::search::search(target.connection(),&moved,"unicorn",5).expect("search").len(),1);
        assert!(crate::search::search(store.connection(),&moved,"unicorn",5).expect("search").is_empty());
    }

    #[test] fn the_move_does_not_queue_uploads_but_carries_the_pending_ones() {
        let dir=tempfile::tempdir().unwrap();
        let (store,moved,kept)=seeded(dir.path());
        // Tudo que existe até aqui já foi entregue ao servidor, menos o projeto que muda.
        store.connection().execute_batch("DELETE FROM outbox").unwrap();
        store.connection().execute("UPDATE projects SET name='Moved again' WHERE id=?1",[&moved]).unwrap();
        store.connection().execute("UPDATE projects SET name='Kept again' WHERE id=?1",[&kept]).unwrap();
        let destination=dir.path().join("org.sqlite3");
        relocate(store.connection(),&destination,&[moved.clone()]).unwrap();
        let here=outbox::pending(store.connection(),100).unwrap();
        assert_eq!(here.iter().map(|entry|entry.key.to_string()).collect::<Vec<_>>(),[format!("[\"{kept}\"]")],"only the kept project still waits here");
        let target=WorkspaceStore::open(destination).unwrap();
        let there=outbox::pending(target.connection(),100).unwrap();
        assert_eq!(there.iter().map(|entry|entry.key.to_string()).collect::<Vec<_>>(),[format!("[\"{moved}\"]")],"the move queued nothing new, it only carried the waiting upload");
        assert_eq!(count(target.connection(),"SELECT applying FROM sync_flag"),0,"the quiet flag is off again");
        assert_eq!(count(store.connection(),"SELECT applying FROM sync_flag"),0);
    }

    #[test] fn nothing_to_move_changes_nothing() {
        let dir=tempfile::tempdir().unwrap();
        let (store,moved,_)=seeded(dir.path());
        let destination=dir.path().join("org.sqlite3");
        assert_eq!(relocate(store.connection(),&destination,&[]).unwrap(),0);
        assert!(!destination.exists(),"no destination database for an empty move");
        assert_eq!(relocate(store.connection(),&destination,&[moved.clone()]).unwrap(),1);
        assert_eq!(relocate(store.connection(),&destination,&[moved]).unwrap(),0,"a second run finds nothing here");
    }
}
