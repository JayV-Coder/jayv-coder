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
use std::collections::BTreeMap;

/// Uma tabela espelhada: o nome, a chave com que o Supabase a conhece e as
/// colunas que sobem. O que fica de fora — `projects.root_path`,
/// `turns.partial`, `turns.local`, `messages.id` — é da máquina.
pub struct SyncTable {
    pub name:&'static str,
    pub key:&'static [&'static str],
    pub columns:&'static [&'static str],
}

/// Em ordem de dependência: quem sobe primeiro é quem os outros apontam.
pub const TABLES:[SyncTable;15]=[
    SyncTable{name:"projects",key:&["id"],columns:&["id","name","created_at","repo_keys","org_id"]},
    SyncTable{name:"chats",key:&["id"],columns:&["id","code","project_id","title","named","created_at","updated_at"]},
    SyncTable{name:"turns",key:&["id"],columns:&["id","chat_id","ordinal","status","created_at"]},
    SyncTable{name:"messages",key:&["uid"],columns:&["uid","chat_id","turn_id","role","content","created_at"]},
    SyncTable{name:"entry_checks",key:&["turn_id"],columns:&["turn_id","at","prompt","score","demand","verdict","scope","criteria","source","note"]},
    SyncTable{name:"exit_checks",key:&["id"],columns:&["id","turn_id","at","kind","target","rule","verdict"]},
    SyncTable{name:"turn_events",key:&["id"],columns:&["id","turn_id","at","seq","kind","detail"]},
    SyncTable{name:"questions",key:&["turn_id"],columns:&["turn_id","at","kind","prompt","options","source","status","answered_by","settled_at"]},
    SyncTable{name:"llm_agents",key:&["id"],columns:&["id","enabled","command","timeout","options","updated_at"]},
    SyncTable{name:"llm_models",key:&["agent","model"],columns:&["agent","model","enabled","capabilities","cost_class","speed","context_window","position"]},
    // O uso não aponta para chat nem projeto com chave estrangeira: apagar um
    // chat não apaga o que ele gastou.
    SyncTable{name:"usage_records",key:&["id"],columns:&["id","project_id","chat_id","turn_id","source","model","input_tokens","output_tokens","cache_read_tokens","cache_write_tokens","cost_usd","requests","duration_ms","success","precision","machine_id","created_at"]},
    SyncTable{name:"quota_snapshots",key:&["id"],columns:&["id","agent","span","used_percent","resets_at","plan","captured_at","machine_id"]},
    // O que é da conta e não de um projeto: o nível do desenvolvedor.
    SyncTable{name:"account_settings",key:&["key"],columns:&["key","value","updated_at"]},
    // A memória do projeto: notas e receitas, as mesmas em toda máquina.
    SyncTable{name:"project_notes",key:&["id"],columns:&["id","project_id","kind","title","body","trigger","covers","source","created_at","updated_at"]},
    SyncTable{name:"jev_records",key:&["id"],columns:&["id","project_id","chat_id","turn_id","kind","amount","precision","created_at"]},
];

pub fn table(name:&str)->Option<&'static SyncTable> { TABLES.iter().find(|table|table.name==name) }

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Op { Upsert, Delete }

impl Op {
    fn parse(value:&str)->Result<Self> {
        match value {"upsert"=>Ok(Op::Upsert),"delete"=>Ok(Op::Delete),other=>anyhow::bail!("unknown queue operation: `{other}`")}
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
    for table in &TABLES {
        // O gatilho de update de um banco antigo olha as colunas de antes: uma
        // coluna nova que sobe não entraria na fila. Recria os três.
        let update:Option<String>=connection.query_row("SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",[format!("outbox_{}_update",table.name)],|row|row.get(0)).optional()?;
        if update.is_some_and(|sql|!sql.contains(&format!("UPDATE OF {} ON",table.columns.join(",")))) {
            connection.execute_batch(&format!("DROP TRIGGER outbox_{0}_insert; DROP TRIGGER outbox_{0}_update; DROP TRIGGER outbox_{0}_delete;",table.name))?;
        }
        connection.execute_batch(&triggers(table))?;
    }
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
        Ok(Pending{seq,table,key:serde_json::from_str(&key).with_context(||format!("invalid queue key: {key}"))?,op:Op::parse(&op)?,at,version})
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

/// As entradas recusadas, contadas por dono: cada chat, cada projeto (o dele
/// e as dos chats dele, somadas) e o resto — configurações dos agentes e
/// exclusões, cuja linha já não existe para dizer de quem era.
#[derive(Debug,Default,Clone,PartialEq,Eq,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct Refusals { pub by_chat:BTreeMap<String,i64>, pub by_project:BTreeMap<String,i64>, pub unplaced:i64 }

pub fn refusals(connection:&Connection)->Result<Refusals> {
    let mut statement=connection.prepare(
        "WITH failed AS (SELECT tbl,json_extract(row_key,'$[0]') AS k FROM outbox WHERE status='failed'),
         owned AS (
           SELECT CASE tbl
               WHEN 'chats' THEN (SELECT id FROM chats WHERE id=k)
               WHEN 'turns' THEN (SELECT chat_id FROM turns WHERE id=k)
               WHEN 'messages' THEN (SELECT chat_id FROM messages WHERE uid=k)
               WHEN 'entry_checks' THEN (SELECT chat_id FROM turns WHERE id=k)
               WHEN 'questions' THEN (SELECT chat_id FROM turns WHERE id=k)
               WHEN 'exit_checks' THEN (SELECT t.chat_id FROM exit_checks e JOIN turns t ON t.id=e.turn_id WHERE e.id=k)
               WHEN 'turn_events' THEN (SELECT t.chat_id FROM turn_events e JOIN turns t ON t.id=e.turn_id WHERE e.id=k)
             END AS chat,
             CASE tbl
               WHEN 'projects' THEN (SELECT id FROM projects WHERE id=k)
               WHEN 'project_notes' THEN (SELECT project_id FROM project_notes WHERE id=k)
             END AS project
           FROM failed)
         SELECT chat,COALESCE((SELECT project_id FROM chats WHERE id=chat),project),COUNT(*) FROM owned GROUP BY 1,2",
    )?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,Option<String>>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,i64>(2)?)))?;
    let mut found=Refusals::default();
    for row in rows {
        let (chat,project,count)=row?;
        if let Some(chat)=chat { *found.by_chat.entry(chat).or_default()+=count; }
        match project { Some(project)=>*found.by_project.entry(project).or_default()+=count, None=>found.unplaced+=count }
    }
    Ok(found)
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
        let written=if deleted {
            transaction.execute(&format!("DELETE FROM {} WHERE {}",table.name,matching(table)),params_from_iter(key_values(table,&key)?))
        } else {
            transaction.execute(&upsert(table),params_from_iter(table.columns.iter().map(|column|to_sql(remote.get(*column).unwrap_or(&absent(table,column))))))
        };
        // No SQLite a restrição desfaz só o comando, não a transação: a linha
        // órfã fica de fora e as outras seguem.
        match written {
            Ok(_)=>applied+=1,
            Err(rusqlite::Error::SqliteFailure(failure,_)) if failure.code==rusqlite::ErrorCode::ConstraintViolation=>{}
            Err(error)=>return Err(error.into()),
        }
    }
    transaction.execute("UPDATE sync_flag SET applying=0",[])?;
    transaction.commit()?;
    Ok(applied)
}

/// O valor de uma coluna que a linha baixada não trouxe — o servidor ainda sem
/// a coluna nova — : o padrão dela aqui, para a linha não cair no `NOT NULL`.
fn absent(table:&SyncTable,column:&str)->Value {
    match (table.name,column) {
        ("projects","repo_keys")=>Value::String("[]".into()),
        _=>Value::Null,
    }
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
    let parts=key.as_array().filter(|parts|parts.len()==table.key.len()).with_context(||format!("malformed key for `{}`: {key}",table.name))?;
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
