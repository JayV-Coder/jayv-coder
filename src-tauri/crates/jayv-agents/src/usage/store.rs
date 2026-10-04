//! As três tabelas do uso e a conta que a tela lê.
//!
//! As linhas só são acrescentadas, nunca alteradas: duas máquinas gravando ao
//! mesmo tempo não disputam linha nenhuma no Supabase, e apagar um chat não
//! apaga o que ele gastou — por isso aqui não há chave estrangeira.

use super::Entry;
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const SCHEMA:&str=
    "CREATE TABLE IF NOT EXISTS usage_records (
       id TEXT PRIMARY KEY,
       project_id TEXT,
       chat_id TEXT,
       turn_id TEXT,
       source TEXT NOT NULL,
       model TEXT NOT NULL,
       input_tokens INTEGER NOT NULL DEFAULT 0,
       output_tokens INTEGER NOT NULL DEFAULT 0,
       cache_read_tokens INTEGER NOT NULL DEFAULT 0,
       cache_write_tokens INTEGER NOT NULL DEFAULT 0,
       cost_usd REAL,
       requests REAL,
       duration_ms INTEGER NOT NULL DEFAULT 0,
       success INTEGER NOT NULL DEFAULT 1,
       precision TEXT NOT NULL,
       machine_id TEXT NOT NULL DEFAULT '',
       created_at TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS usage_records_at ON usage_records(created_at);
     CREATE INDEX IF NOT EXISTS usage_records_project ON usage_records(project_id,created_at);
     CREATE INDEX IF NOT EXISTS usage_records_chat ON usage_records(chat_id,created_at);
     CREATE TABLE IF NOT EXISTS quota_snapshots (
       id TEXT PRIMARY KEY,
       agent TEXT NOT NULL,
       span TEXT NOT NULL,
       used_percent REAL,
       resets_at TEXT,
       plan TEXT,
       captured_at TEXT NOT NULL,
       machine_id TEXT NOT NULL DEFAULT ''
     );
     CREATE INDEX IF NOT EXISTS quota_snapshots_agent ON quota_snapshots(agent,span,captured_at);
     CREATE TABLE IF NOT EXISTS jev_records (
       id TEXT PRIMARY KEY,
       project_id TEXT,
       chat_id TEXT,
       turn_id TEXT,
       kind TEXT NOT NULL,
       amount REAL NOT NULL,
       precision TEXT NOT NULL,
       created_at TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS jev_records_at ON jev_records(created_at);
     CREATE INDEX IF NOT EXISTS jev_records_project ON jev_records(project_id,created_at);
     CREATE INDEX IF NOT EXISTS jev_records_chat ON jev_records(chat_id,created_at);";

pub const LEGACY_MARK:&str="usage_legacy_imported";
const MACHINE_KEY:&str="machine_id";
/// Duas leituras iguais do mesmo limite em menos que isto são uma só.
const QUOTA_REPEAT_SECONDS:i64=60;

fn now()->String { Utc::now().to_rfc3339_opts(SecondsFormat::Millis,true) }

/// As tabelas, e uma vez só a cópia dos turnos de antes desta contagem. O id
/// da cópia nasce do evento copiado: duas máquinas com o mesmo histórico
/// sincronizado chegam à mesma linha, e o Supabase não a conta duas vezes.
pub fn ensure(connection:&Connection)->Result<()> {
    connection.execute_batch(SCHEMA)?;
    let done:Option<String>=connection.query_row("SELECT value FROM app_metadata WHERE key=?1",[LEGACY_MARK],|row|row.get(0)).optional()?;
    if done.is_none() {
        connection.execute(
            "INSERT OR IGNORE INTO usage_records(id,project_id,chat_id,turn_id,source,model,input_tokens,output_tokens,duration_ms,success,precision,machine_id,created_at)
             SELECT 'legacy-'||e.id,c.project_id,t.chat_id,e.turn_id,
                    COALESCE((SELECT json_extract(r.detail,'$.provider') FROM turn_events r WHERE r.turn_id=e.turn_id AND r.kind='route' ORDER BY r.seq DESC LIMIT 1),'unknown'),
                    COALESCE((SELECT json_extract(r.detail,'$.model') FROM turn_events r WHERE r.turn_id=e.turn_id AND r.kind='route' ORDER BY r.seq DESC LIMIT 1),'unknown'),
                    COALESCE(json_extract(e.detail,'$.inputTokens'),0),COALESCE(json_extract(e.detail,'$.outputTokens'),0),
                    COALESCE(json_extract(e.detail,'$.latencyMs'),0),1,'legacy','',strftime('%Y-%m-%dT%H:%M:%fZ',e.at)
             FROM turn_events e JOIN turns t ON t.id=e.turn_id JOIN chats c ON c.id=t.chat_id
             WHERE e.kind='done' AND json_extract(e.detail,'$.metered') IS NULL AND (COALESCE(json_extract(e.detail,'$.inputTokens'),0)>0 OR COALESCE(json_extract(e.detail,'$.outputTokens'),0)>0)",
            [],
        )?;
        connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![LEGACY_MARK,now()])?;
    }
    Ok(())
}

/// O nome desta máquina no uso: um id que nasce no primeiro registro e fica.
pub fn machine_id(connection:&Connection)->Result<String> {
    if let Some(id)=connection.query_row("SELECT value FROM app_metadata WHERE key=?1",[MACHINE_KEY],|row|row.get::<_,String>(0)).optional()? { return Ok(id); }
    let id=Uuid::new_v4().to_string();
    connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2)",params![MACHINE_KEY,id])?;
    Ok(id)
}

/// Grava um registro. Devolve `false` quando ele não entrou: a leitura de
/// limite igual à anterior em menos de um minuto.
pub fn write(connection:&Connection,entry:&Entry)->Result<bool> {
    let machine=machine_id(connection)?;
    let at=now();
    match entry {
        Entry::Spend(scope,spend)=>{
            connection.execute(
                "INSERT INTO usage_records(id,project_id,chat_id,turn_id,source,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,cost_usd,requests,duration_ms,success,precision,machine_id,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![Uuid::new_v4().to_string(),scope.project_id,scope.chat_id,scope.turn_id,spend.source,spend.model,spend.input_tokens as i64,spend.output_tokens as i64,spend.cache_read_tokens as i64,spend.cache_write_tokens as i64,spend.cost_usd,spend.requests,spend.duration_ms as i64,spend.success,spend.precision.as_str(),machine,at],
            )?;
        }
        Entry::Jev(scope,mark)=>{
            connection.execute(
                "INSERT INTO jev_records(id,project_id,chat_id,turn_id,kind,amount,precision,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![Uuid::new_v4().to_string(),scope.project_id,scope.chat_id,scope.turn_id,mark.kind,mark.amount,mark.precision.as_str(),at],
            )?;
        }
        Entry::Quota(quota)=>{
            let repeated:bool=connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM (SELECT used_percent,resets_at,plan,captured_at FROM quota_snapshots WHERE agent=?1 AND span=?2 ORDER BY captured_at DESC LIMIT 1)
                   WHERE used_percent IS ?3 AND resets_at IS ?4 AND plan IS ?5 AND (julianday(?6)-julianday(captured_at))*86400<?7)",
                params![quota.agent,quota.window,quota.used_percent,quota.resets_at,quota.plan,at,QUOTA_REPEAT_SECONDS],|row|row.get(0),
            )?;
            if repeated { return Ok(false); }
            connection.execute(
                "INSERT INTO quota_snapshots(id,agent,span,used_percent,resets_at,plan,captured_at,machine_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![Uuid::new_v4().to_string(),quota.agent,quota.window,quota.used_percent,quota.resets_at,quota.plan,at,machine],
            )?;
        }
    }
    Ok(true)
}

/// De quem é a conta. `Projects` é um punhado de projetos (os de uma
/// organização); vazio, não conta nada.
#[derive(Debug,Clone,PartialEq,Eq,serde::Deserialize)]
#[serde(tag="kind",content="id",rename_all="camelCase")]
pub enum ReportScope { Global, Project(String), Projects(Vec<String>), Chat(String) }

/// O pedido da tela: o escopo, o intervalo (RFC 3339, aberto nas pontas que
/// vierem vazias) e o fuso de quem lê, para que "hoje" seja o hoje dele.
#[derive(Debug,Clone,serde::Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Query { pub scope:ReportScope, pub from:Option<String>, pub to:Option<String>, #[serde(default)] pub utc_offset_minutes:i32 }

#[derive(Debug,Clone,Default,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Totals {
    pub input_tokens:u64,
    pub output_tokens:u64,
    pub cache_read_tokens:u64,
    pub cache_write_tokens:u64,
    /// Só a soma do que as ferramentas informaram; vazio quando nenhuma
    /// informou.
    pub cost_usd:Option<f64>,
    /// Pedidos cobrados pelo plano, só onde a ferramenta os contou.
    pub requests:Option<f64>,
    pub calls:u64,
    pub failures:u64,
    pub turns:u64,
    pub duration_ms:u64,
    /// Tokens (entrada + saída) que não foram informados pela ferramenta.
    pub estimated_tokens:u64,
}

#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Breakdown { pub key:String, pub label:Option<String>, pub parent:Option<String>, pub totals:Totals }

#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct DayRow { pub day:String, pub source:String, pub input_tokens:u64, pub output_tokens:u64 }

#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuotaView { pub agent:String, pub window:String, pub used_percent:Option<f64>, pub resets_at:Option<String>, pub plan:Option<String>, pub captured_at:String }

#[derive(Debug,Clone,Default,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct JevSummary {
    /// Contagens exatas: vereditos, cache, firewall.
    pub work:BTreeMap<String,f64>,
    /// Tokens poupados por causa (`blocked`, `cache`, `context`), sempre
    /// estimados.
    pub saved:BTreeMap<String,f64>,
}

#[derive(Debug,Clone,Default,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Report {
    pub totals:Totals,
    pub daily:Vec<DayRow>,
    pub by_source:Vec<Breakdown>,
    pub by_model:Vec<Breakdown>,
    pub by_project:Vec<Breakdown>,
    pub by_chat:Vec<Breakdown>,
    pub quotas:Vec<QuotaView>,
    pub jev:JevSummary,
}

/// O filtro comum às duas tabelas com escopo, e os valores dele em ordem.
fn filter(query:&Query)->(String,Vec<Box<dyn ToSql>>) {
    let mut clauses:Vec<String>=Vec::new();
    let mut values:Vec<Box<dyn ToSql>>=Vec::new();
    match &query.scope {
        ReportScope::Global=>{}
        ReportScope::Project(id)=>{ clauses.push("project_id=?".into()); values.push(Box::new(id.clone())); }
        ReportScope::Projects(ids) if ids.is_empty()=>clauses.push("0=1".into()),
        ReportScope::Projects(ids)=>{
            clauses.push(format!("project_id IN ({})",vec!["?"; ids.len()].join(",")));
            values.extend(ids.iter().map(|id|Box::new(id.clone()) as Box<dyn ToSql>));
        }
        ReportScope::Chat(id)=>{ clauses.push("chat_id=?".into()); values.push(Box::new(id.clone())); }
    }
    if let Some(from)=&query.from { clauses.push("julianday(created_at)>=julianday(?)".into()); values.push(Box::new(from.clone())); }
    if let Some(to)=&query.to { clauses.push("julianday(created_at)<julianday(?)".into()); values.push(Box::new(to.clone())); }
    let sql=if clauses.is_empty() { "1=1".to_string() } else { clauses.join(" AND ") };
    (sql,values)
}

const SUMS:&str="COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0),COALESCE(SUM(cache_read_tokens),0),COALESCE(SUM(cache_write_tokens),0),
    SUM(cost_usd),SUM(requests),COUNT(*),COALESCE(SUM(1-success),0),COUNT(DISTINCT turn_id),COALESCE(SUM(duration_ms),0),
    COALESCE(SUM(CASE WHEN precision<>'reported' THEN input_tokens+output_tokens ELSE 0 END),0)";

fn totals_at(row:&rusqlite::Row,first:usize)->rusqlite::Result<Totals> {
    let int=|index:usize|row.get::<_,i64>(first+index).map(|value|value.max(0) as u64);
    Ok(Totals{input_tokens:int(0)?,output_tokens:int(1)?,cache_read_tokens:int(2)?,cache_write_tokens:int(3)?,cost_usd:row.get(first+4)?,requests:row.get(first+5)?,calls:int(6)?,failures:int(7)?,turns:int(8)?,duration_ms:int(9)?,estimated_tokens:int(10)?})
}

fn grouped(connection:&Connection,query:&Query,group:&str,label:&str,parent:&str)->Result<Vec<Breakdown>> {
    let (filter,values)=filter(query);
    let sql=format!("SELECT {group},{label},{parent},{SUMS} FROM usage_records u WHERE {filter} AND {group} IS NOT NULL GROUP BY {group} ORDER BY SUM(input_tokens+output_tokens) DESC");
    let mut statement=connection.prepare(&sql)?;
    let rows=statement.query_map(rusqlite::params_from_iter(values.iter().map(|value|value.as_ref())),|row|Ok(Breakdown{key:row.get(0)?,label:row.get(1)?,parent:row.get(2)?,totals:totals_at(row,3)?}))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn report(connection:&Connection,query:&Query)->Result<Report> {
    let (where_sql,values)=filter(query);
    let bind=||rusqlite::params_from_iter(values.iter().map(|value|value.as_ref()));
    let totals=connection.query_row(&format!("SELECT {SUMS} FROM usage_records WHERE {where_sql}"),bind(),|row|totals_at(row,0))?;

    let offset=format!("{:+} minutes",query.utc_offset_minutes);
    let daily={
        let mut statement=connection.prepare(&format!(
            "SELECT date(created_at,'{offset}') AS day,source,COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0) FROM usage_records WHERE {where_sql} GROUP BY day,source ORDER BY day,source"))?;
        let rows=statement.query_map(bind(),|row|Ok(DayRow{day:row.get(0)?,source:row.get(1)?,input_tokens:row.get::<_,i64>(2)?.max(0) as u64,output_tokens:row.get::<_,i64>(3)?.max(0) as u64}))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let by_source=grouped(connection,query,"source","NULL","NULL")?;
    let by_model=grouped(connection,query,"source||'/'||model","model","source")?;
    let by_project=grouped(connection,query,"project_id","(SELECT name FROM projects p WHERE p.id=u.project_id)","NULL")?;
    let by_chat=grouped(connection,query,"chat_id","(SELECT title FROM chats c WHERE c.id=u.chat_id)","project_id")?;

    // O limite do plano é da conta, não do chat: a última leitura de cada
    // janela, qualquer que seja o escopo.
    let quotas={
        let mut statement=connection.prepare(
            "SELECT agent,span,used_percent,resets_at,plan,captured_at FROM (
               SELECT *,ROW_NUMBER() OVER (PARTITION BY agent,span ORDER BY captured_at DESC,rowid DESC) AS position FROM quota_snapshots)
             WHERE position=1 ORDER BY agent,span")?;
        let rows=statement.query_map([],|row|Ok(QuotaView{agent:row.get(0)?,window:row.get(1)?,used_percent:row.get(2)?,resets_at:row.get(3)?,plan:row.get(4)?,captured_at:row.get(5)?}))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut jev=JevSummary::default();
    {
        let mut statement=connection.prepare(&format!("SELECT kind,SUM(amount) FROM jev_records WHERE {where_sql} GROUP BY kind"))?;
        let rows=statement.query_map(bind(),|row|Ok((row.get::<_,String>(0)?,row.get::<_,f64>(1)?)))?;
        for row in rows {
            let (kind,amount)=row?;
            match kind.strip_prefix("saved_tokens:") { Some(cause)=>{ jev.saved.insert(cause.to_string(),amount); } None=>{ jev.work.insert(kind,amount); } }
        }
    }
    Ok(Report{totals,daily,by_source,by_model,by_project,by_chat,quotas,jev})
}

/// A saída média de uma resposta de modelo, no que foi informado. É a base
/// da economia estimada de um pedido barrado: o que ele teria custado de volta.
pub fn average_output(connection:&Connection)->Result<u64> {
    let average:Option<f64>=connection.query_row(
        "SELECT AVG(output_tokens) FROM (SELECT SUM(output_tokens) AS output_tokens FROM usage_records WHERE precision='reported' AND source NOT LIKE 'jev:%' AND turn_id IS NOT NULL GROUP BY turn_id ORDER BY MAX(created_at) DESC LIMIT 200)",
        [],|row|row.get(0))?;
    Ok(average.unwrap_or(0.0).round() as u64)
}

/// O que um chat e um turno gastaram, para os rodapés da conversa.
#[derive(Debug,Clone,Default,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct TurnUsage { pub turn_id:String, pub input_tokens:u64, pub output_tokens:u64, pub cost_usd:Option<f64>, pub duration_ms:u64, pub estimated:bool }

/// O gasto de cada turno de um chat, só dos modelos que responderam (o Jev
/// tem a conta dele no painel).
pub fn chat_turns(connection:&Connection,chat_id:&str)->Result<Vec<TurnUsage>> {
    let mut statement=connection.prepare(
        "SELECT turn_id,COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0),SUM(cost_usd),COALESCE(MAX(duration_ms),0),MAX(precision<>'reported')
         FROM usage_records WHERE chat_id=?1 AND turn_id IS NOT NULL AND source NOT LIKE 'jev:%' GROUP BY turn_id")?;
    let rows=statement.query_map([chat_id],|row|Ok(TurnUsage{turn_id:row.get(0)?,input_tokens:row.get::<_,i64>(1)?.max(0) as u64,output_tokens:row.get::<_,i64>(2)?.max(0) as u64,cost_usd:row.get(3)?,duration_ms:row.get::<_,i64>(4)?.max(0) as u64,estimated:row.get(5)?}))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
