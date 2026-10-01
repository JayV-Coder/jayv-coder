//! O turno: um pedido do desenvolvedor e tudo que a portaria viu nele.
//!
//! Cada pedido enviado num chat abre um turno, e o turno é o que os dois
//! portões apontam. O desenvolvedor o lê pelo código curto `XY4T9B·04` — o
//! código do chat mais a posição do pedido dentro dele —, e é esse código que
//! aparece tanto no balão do chat quanto nos cartões da Portaria.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{gatekeeper::{Criterion, EntryCheck, EntryVerdict, ExitCheck, ExitVerdict, GateFeed, Tally}, workspace::parse_time};
use std::collections::BTreeSet;

/// Em que pé está o pedido. `Queued` é o pedido já gravado esperando a vez;
/// `Flying` é o que saiu e ainda não voltou. Os dois são trabalho em aberto, e
/// a reabertura do banco devolve à fila o que tiver ficado neles.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum TurnStatus{Queued,Flying,Answered,Failed,Blocked}

impl TurnStatus {
    pub fn as_str(&self)->&'static str{match self{Self::Queued=>"queued",Self::Flying=>"flying",Self::Answered=>"answered",Self::Failed=>"failed",Self::Blocked=>"blocked"}}

    /// O pedido que ainda vai ser atendido: ou espera a vez, ou está no ar.
    pub fn is_open(&self)->bool{matches!(self,Self::Queued|Self::Flying)}

    /// Um estado que não se reconhece é erro, não `Flying`: cair no padrão
    /// faria um pedido já respondido voltar a girar a ampulheta para sempre.
    fn parse(value:&str)->Result<Self> {
        Ok(match value{"queued"=>Self::Queued,"flying"=>Self::Flying,"answered"=>Self::Answered,"failed"=>Self::Failed,"blocked"=>Self::Blocked,other=>anyhow::bail!("estado de turno desconhecido: `{other}`")})
    }
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Turn {
    pub id:String,
    pub chat_id:String,
    /// `XY4T9B·04`, sem a cerquilha: quem desenha decide se mostra `#`.
    pub code:String,
    pub ordinal:u32,
    pub status:TurnStatus,
    pub created_at:DateTime<Utc>,
}

pub const SCHEMA:&str=
    "CREATE TABLE IF NOT EXISTS turns (
       id TEXT PRIMARY KEY,
       chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
       ordinal INTEGER NOT NULL,
       status TEXT NOT NULL,
       created_at TEXT NOT NULL,
       partial TEXT,
       local INTEGER NOT NULL DEFAULT 1
     );
     CREATE UNIQUE INDEX IF NOT EXISTS turns_chat_ordinal ON turns(chat_id,ordinal);
     CREATE TABLE IF NOT EXISTS entry_checks (
       turn_id TEXT PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
       at TEXT NOT NULL,
       prompt TEXT NOT NULL,
       score INTEGER NOT NULL,
       demand INTEGER NOT NULL,
       verdict TEXT NOT NULL,
       scope TEXT NOT NULL,
       criteria TEXT NOT NULL,
       source TEXT NOT NULL,
       note TEXT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS exit_checks (
       id TEXT PRIMARY KEY,
       turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
       at TEXT NOT NULL,
       kind TEXT NOT NULL,
       target TEXT NOT NULL,
       rule TEXT,
       verdict TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS exit_checks_turn ON exit_checks(turn_id);
     CREATE TABLE IF NOT EXISTS turn_events (
       id TEXT PRIMARY KEY,
       turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
       at TEXT NOT NULL,
       seq INTEGER NOT NULL,
       kind TEXT NOT NULL,
       detail TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS turn_events_turn ON turn_events(turn_id,seq);
     CREATE TABLE IF NOT EXISTS questions (
       turn_id TEXT PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
       at TEXT NOT NULL,
       kind TEXT NOT NULL,
       prompt TEXT NOT NULL,
       options TEXT NOT NULL,
       source TEXT NOT NULL,
       status TEXT NOT NULL,
       answered_by TEXT REFERENCES turns(id) ON DELETE SET NULL,
       settled_at TEXT
     );
     CREATE INDEX IF NOT EXISTS questions_status ON questions(status);";

/// Quantos itens de cada portão a tela recebe. O placar não usa esta janela:
/// ele conta no banco, então continua certo depois que os antigos saem de
/// vista — que era exatamente o que a fila em memória não conseguia fazer.
pub const FEED_WINDOW:usize=200;

/// Abre o turno do pedido que está sendo enviado agora neste chat. Ele nasce
/// na fila, não no ar: o que o desenvolvedor mandou fica gravado e numerado
/// antes de existir qualquer processo para atendê-lo, e é daí que a tela o lê.
pub fn open_turn(connection:&Connection,chat_id:&str)->Result<Turn> {
    let code:String=connection.query_row("SELECT code FROM chats WHERE id=?1",[chat_id],|row|row.get(0))?;
    let ordinal:u32=connection.query_row("SELECT COALESCE(MAX(ordinal),0)+1 FROM turns WHERE chat_id=?1",[chat_id],|row|row.get(0))?;
    let turn=Turn{id:Uuid::new_v4().to_string(),chat_id:chat_id.into(),code:turn_code(&code,ordinal),ordinal,status:TurnStatus::Queued,created_at:Utc::now()};
    connection.execute(
        "INSERT INTO turns(id,chat_id,ordinal,status,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![turn.id,turn.chat_id,turn.ordinal,turn.status.as_str(),turn.created_at.to_rfc3339()],
    )?;
    Ok(turn)
}

/// O pedido que está sendo enviado: um turno novo, ou o mesmo turno que falhou
/// e está voltando. Retentar não gasta número novo — o balão já mostra o código
/// antigo, e ele tem de continuar valendo. Um turno pedido que não existe, ou
/// que é de outro chat, é erro: seguir abrindo um turno novo em silêncio daria
/// ao desenvolvedor um código diferente do que ele viu no balão.
pub fn open_or_reopen(connection:&Connection,chat_id:&str,requested:Option<&str>)->Result<Turn> {
    let Some(turn_id)=requested else {return open_turn(connection,chat_id)};
    let existing=turn(connection,turn_id)?.ok_or_else(||anyhow::anyhow!("o turno `{turn_id}` não existe"))?;
    anyhow::ensure!(existing.chat_id==chat_id,"o turno `{turn_id}` é de outro chat");
    reopen_turn(connection,turn_id)
}

/// O código que o desenvolvedor lê: o do chat, o ponto medial, e a posição do
/// pedido com dois dígitos — `XY4T9B·04`. Passando de noventa e nove pedidos o
/// número simplesmente cresce, porque truncá-lo faria dois turnos do mesmo
/// chat dividirem um código.
fn turn_code(chat_code:&str,ordinal:u32)->String{format!("{chat_code}·{ordinal:02}")}

/// O turno, como está gravado agora.
pub fn turn(connection:&Connection,turn_id:&str)->Result<Option<Turn>> {
    connection.query_row(
        "SELECT t.id,t.chat_id,c.code,t.ordinal,t.status,t.created_at FROM turns t JOIN chats c ON c.id=t.chat_id WHERE t.id=?1",
        [turn_id],read_turn,
    ).optional()?.transpose()
}

/// Move o turno de estado: respondido, falho ou barrado.
pub fn set_status(connection:&Connection,turn_id:&str,status:TurnStatus)->Result<()> {
    connection.execute("UPDATE turns SET status=?1 WHERE id=?2",params![status.as_str(),turn_id])?;
    Ok(())
}

/// Retentar não é pedir de novo: o turno volta para a fila com o mesmo número.
pub fn reopen_turn(connection:&Connection,turn_id:&str)->Result<Turn> {
    set_status(connection,turn_id,TurnStatus::Queued)?;
    turn(connection,turn_id)?.ok_or_else(||anyhow::anyhow!("o turno `{turn_id}` não existe mais"))
}

/// Na abertura do banco, todo pedido que ficou pela metade volta para a fila.
/// Ele já está gravado com o texto do desenvolvedor: descartá-lo seria perder
/// um pedido que o aplicativo aceitou, e é exatamente o que a fila existe para
/// impedir. Quem retoma é o atendente, na ordem de sempre.
pub fn requeue_interrupted_turns(connection:&Connection)->Result<usize> {
    Ok(connection.execute("UPDATE turns SET status=?1 WHERE status=?2 AND local=1",params![TurnStatus::Queued.as_str(),TurnStatus::Flying.as_str()])?)
}

/// De quem é a vez: o mais antigo dos que esperam. Ordenar por `created_at` e
/// desempatar pelo número do turno mantém a ordem estável quando dois pedidos
/// caem no mesmo instante do relógio. Só entram os turnos desta máquina: o
/// pedido que outro computador do mesmo usuário enfileirou é atendido lá.
pub fn next_queued(connection:&Connection)->Result<Option<Turn>> {
    connection.query_row(
        "SELECT t.id,t.chat_id,c.code,t.ordinal,t.status,t.created_at FROM turns t JOIN chats c ON c.id=t.chat_id
         WHERE t.status=?1 AND t.local=1 ORDER BY t.created_at,t.ordinal LIMIT 1",
        [TurnStatus::Queued.as_str()],read_turn,
    ).optional()?.transpose()
}

/// Se há um pedido desta máquina no ar agora — o de outro computador não
/// segura a fila daqui. Enquanto houver, a fila não chama o seguinte:
/// um pedido de cada vez é o que mantém o histórico do chat numa ordem que o
/// desenvolvedor consegue ler.
pub fn is_flying(connection:&Connection)->Result<bool> {
    Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE status=?1 AND local=1)",[TurnStatus::Flying.as_str()],|row|row.get(0))?)
}

/// Quantos pedidos deste chat ainda não foram atendidos, contando o que está
/// no ar. É o número que a tela mostra junto do balão que espera.
pub fn queue_depth(connection:&Connection,chat_id:&str)->Result<u32> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM turns WHERE chat_id=?1 AND status IN (?2,?3)",
        params![chat_id,TurnStatus::Queued.as_str(),TurnStatus::Flying.as_str()],|row|row.get(0),
    )?)
}

/// Em que lugar da fila do seu chat este pedido está: 1 é o que está sendo
/// atendido. Um turno já fechado não está em fila nenhuma.
pub fn place_in_queue(connection:&Connection,turn_id:&str)->Result<Option<u32>> {
    let Some(turn)=turn(connection,turn_id)? else {return Ok(None)};
    if !turn.status.is_open() {return Ok(None);}
    Ok(Some(connection.query_row(
        "SELECT COUNT(*) FROM turns WHERE chat_id=?1 AND status IN (?2,?3) AND (created_at,ordinal)<=(?4,?5)",
        params![turn.chat_id,TurnStatus::Queued.as_str(),TurnStatus::Flying.as_str(),turn.created_at.to_rfc3339(),turn.ordinal],
        |row|row.get(0),
    )?))
}

/// A linha do banco virando turno. O código legível não é guardado: ele nasce
/// do código do chat toda vez, para que renomear nunca deixe os dois em
/// desacordo.
fn read_turn(row:&rusqlite::Row)->rusqlite::Result<Result<Turn>> {
    let (id,chat_id,chat_code,ordinal,status,created_at)=(row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,u32>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?);
    Ok((||Ok(Turn{id,chat_id,code:turn_code(&chat_code,ordinal),ordinal,status:TurnStatus::parse(&status)?,created_at:parse_time(&created_at)?}))())
}

/// Grava o veredito de entrada do turno. Retentar reescreve a linha: o pedido
/// foi pontuado uma vez, e a pontuação não muda por ter faltado rede.
pub fn record_entry(connection:&Connection,check:&EntryCheck)->Result<()> {
    connection.execute(
        "INSERT INTO entry_checks(turn_id,at,prompt,score,demand,verdict,scope,criteria,source,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(turn_id) DO UPDATE SET at=excluded.at,prompt=excluded.prompt,score=excluded.score,demand=excluded.demand,
           verdict=excluded.verdict,scope=excluded.scope,criteria=excluded.criteria,source=excluded.source,note=excluded.note",
        params![check.id,check.at.to_rfc3339(),check.prompt,check.score,check.demand,check.verdict.as_str(),check.scope,serde_json::to_string(&check.criteria)?,check.source,check.note],
    )?;
    Ok(())
}

/// Troca as saídas do turno pelas desta tentativa. O que a tentativa anterior
/// pediu deixou de valer, e deixá-lo no banco faria o placar contar duas vezes
/// uma regra tocada uma vez só.
pub fn record_exits(connection:&Connection,turn:&Turn,checks:&[ExitCheck])->Result<()> {
    let transaction=connection.unchecked_transaction()?;
    transaction.execute("DELETE FROM exit_checks WHERE turn_id=?1",[&turn.id])?;
    for check in checks {
        transaction.execute(
            "INSERT INTO exit_checks(id,turn_id,at,kind,target,rule,verdict) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![check.id,turn.id,check.at.to_rfc3339(),check.kind,check.target,check.rule,check.verdict.as_str()],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

/// O turno como o balão do chat precisa dele: o código que o desenvolvedor lê,
/// em que pé está, e o que cada portão disse. Os dois vereditos são opcionais
/// porque um pedido em voo ainda não tem nenhum.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct TurnView {
    pub id:String,
    pub code:String,
    pub status:TurnStatus,
    pub entry:Option<EntryVerdict>,
    pub exit:Option<ExitVerdict>,
    /// A resposta que já chegou, num turno que ainda não fechou. Sai do banco,
    /// e não de um registro da tela: é por isso que o texto continua na tela
    /// depois de sair do chat, recarregar a janela ou reabrir o aplicativo.
    pub partial:Option<String>,
    /// O que o Jev fez neste pedido, na ordem. Só vem preenchida nos turnos em
    /// aberto: a narração de um pedido já respondido fica no banco e é buscada
    /// sob demanda, em vez de engordar todo retrato da área de trabalho.
    pub activity:Vec<Activity>,
}

/// Uma linha da narração do turno, como a tela a lê.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Activity {
    pub at:DateTime<Utc>,
    pub seq:u32,
    pub kind:String,
    /// O corpo do evento. O formato varia por `kind`, e a tela desenha a linha
    /// a partir dele sem um caso por tipo do lado do Rust.
    pub detail:serde_json::Value,
}

/// Os turnos de um chat, na ordem em que foram pedidos.
pub fn views_for_chat(connection:&Connection,chat_id:&str)->Result<Vec<TurnView>> {
    let mut statement=connection.prepare(
        "SELECT t.id,c.code,t.ordinal,t.status,
           (SELECT e.verdict FROM entry_checks e WHERE e.turn_id=t.id),
           (SELECT CASE WHEN COUNT(*)=0 THEN NULL WHEN SUM(x.verdict='held')>0 THEN 'held' ELSE 'cleared' END FROM exit_checks x WHERE x.turn_id=t.id),
           t.partial
         FROM turns t JOIN chats c ON c.id=t.chat_id WHERE t.chat_id=?1 ORDER BY t.ordinal")?;
    let rows=statement.query_map([chat_id],|row|Ok((
        row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,u32>(2)?,row.get::<_,String>(3)?,
        row.get::<_,Option<String>>(4)?,row.get::<_,Option<String>>(5)?,row.get::<_,Option<String>>(6)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,chat_code,ordinal,status,entry,exit,partial)|{
        let status=TurnStatus::parse(&status)?;
        let activity=if status.is_open(){activity(connection,&id)?}else{vec![]};
        Ok(TurnView{
            id,code:turn_code(&chat_code,ordinal),status,
            entry:entry.as_deref().map(EntryVerdict::parse).transpose()?,
            exit:exit.as_deref().map(ExitVerdict::parse).transpose()?,
            partial:partial.filter(|text|!text.is_empty()),activity,
        })
    }).collect()
}

/// Grava uma etapa da narração. O número de ordem sai do banco e não do relógio:
/// dois eventos no mesmo milissegundo continuam distinguíveis, e a tela os
/// desenha na ordem em que aconteceram.
pub fn record_beat(connection:&Connection,turn_id:&str,kind:&str,detail:&serde_json::Value)->Result<u32> {
    let seq:u32=connection.query_row("SELECT COALESCE(MAX(seq),0)+1 FROM turn_events WHERE turn_id=?1",[turn_id],|row|row.get(0))?;
    connection.execute(
        "INSERT INTO turn_events(id,turn_id,at,seq,kind,detail) VALUES(?1,?2,?3,?4,?5,?6)",
        params![Uuid::new_v4().to_string(),turn_id,Utc::now().to_rfc3339(),seq,kind,detail.to_string()],
    )?;
    Ok(seq)
}

/// A narração de um turno, na ordem.
pub fn activity(connection:&Connection,turn_id:&str)->Result<Vec<Activity>> {
    let rows={
        let mut statement=connection.prepare("SELECT at,seq,kind,detail FROM turn_events WHERE turn_id=?1 ORDER BY seq")?;
        statement.query_map([turn_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u32>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    rows.into_iter().map(|(at,seq,kind,detail)|Ok(Activity{
        at:parse_time(&at)?,seq,kind,
        // Um detalhe corrompido não pode derrubar a leitura do chat: a linha
        // aparece sem corpo, e o resto da narração continua legível.
        detail:serde_json::from_str(&detail).unwrap_or(serde_json::Value::Null),
    })).collect()
}

/// A resposta que já chegou. Escrita com folga pela consumidora do barramento,
/// nunca a cada pedaço.
pub fn set_partial(connection:&Connection,turn_id:&str,text:&str)->Result<()> {
    connection.execute("UPDATE turns SET partial=?1 WHERE id=?2",params![text,turn_id])?;
    Ok(())
}

/// Apaga o rascunho. Chamado quando o turno fecha: a resposta definitiva já
/// está em `messages`, e deixar as duas cópias faria a tela escolher entre
/// elas.
pub fn clear_partial(connection:&Connection,turn_id:&str)->Result<()> {
    connection.execute("UPDATE turns SET partial=NULL WHERE id=?1",[turn_id])?;
    Ok(())
}

/// A pergunta como o box precisa dela: de que tipo é, o que ela pergunta, o que
/// ela oferece, e de qual pedido ela veio — é pelo código que o desenvolvedor vê
/// a qual resposta ele está respondendo.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct QuestionView {
    pub turn_id:String,
    pub code:String,
    pub kind:String,
    pub prompt:String,
    pub options:Vec<String>,
    pub source:String,
    pub status:String,
}

pub const QUESTION_PENDING:&str="pending";
pub const QUESTION_ANSWERED:&str="answered";
pub const QUESTION_DISMISSED:&str="dismissed";

/// Habilita a interação de um turno. Uma pergunta por turno: se o mesmo turno
/// for atendido de novo — um reenvio —, a pergunta antiga é substituída, não
/// duplicada.
pub fn ask(connection:&Connection,turn_id:&str,kind:&str,prompt:&str,options:&[String],source:&str)->Result<()> {
    connection.execute(
        "INSERT INTO questions(turn_id,at,kind,prompt,options,source,status,answered_by,settled_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,NULL)
         ON CONFLICT(turn_id) DO UPDATE SET at=excluded.at,kind=excluded.kind,prompt=excluded.prompt,
           options=excluded.options,source=excluded.source,status=excluded.status,answered_by=NULL,settled_at=NULL",
        params![turn_id,Utc::now().to_rfc3339(),kind,prompt,serde_json::to_string(options)?,source,QUESTION_PENDING],
    )?;
    Ok(())
}

/// A pergunta em aberto deste chat, se houver. Só a mais recente trava o box:
/// uma pergunta antiga que ficou sem resposta não pode prender a conversa de
/// hoje.
pub fn pending_question(connection:&Connection,chat_id:&str)->Result<Option<QuestionView>> {
    let mut statement=connection.prepare(
        "SELECT q.turn_id,c.code,t.ordinal,q.kind,q.prompt,q.options,q.source,q.status
         FROM questions q JOIN turns t ON t.id=q.turn_id JOIN chats c ON c.id=t.chat_id
         WHERE t.chat_id=?1 AND q.status=?2 ORDER BY t.ordinal DESC LIMIT 1")?;
    let mut rows=statement.query(params![chat_id,QUESTION_PENDING])?;
    rows.next()?.map(read_question).transpose()
}

/// A pergunta de um turno, em qualquer estado. É por aqui que o turno-resposta
/// confere se ainda há o que responder.
pub fn question_of(connection:&Connection,turn_id:&str)->Result<Option<QuestionView>> {
    let mut statement=connection.prepare(
        "SELECT q.turn_id,c.code,t.ordinal,q.kind,q.prompt,q.options,q.source,q.status
         FROM questions q JOIN turns t ON t.id=q.turn_id JOIN chats c ON c.id=t.chat_id
         WHERE q.turn_id=?1")?;
    let mut rows=statement.query([turn_id])?;
    rows.next()?.map(read_question).transpose()
}

fn read_question(row:&rusqlite::Row)->Result<QuestionView> {
    let options:String=row.get(5)?;
    Ok(QuestionView{
        turn_id:row.get(0)?,
        code:turn_code(&row.get::<_,String>(1)?,row.get::<_,u32>(2)?),
        kind:row.get(3)?,
        prompt:row.get(4)?,
        // Uma lista corrompida não pode travar o chat: a pergunta aparece sem
        // alternativas, e o `RESPONDER` continua de pé.
        options:serde_json::from_str(&options).unwrap_or_default(),
        source:row.get(6)?,
        status:row.get(7)?,
    })
}

/// O chat de um turno.
pub fn chat_of(connection:&Connection,turn_id:&str)->Result<Option<String>> {
    Ok(connection.query_row("SELECT chat_id FROM turns WHERE id=?1",[turn_id],|row|row.get(0)).optional()?)
}

/// O pedido que fez a pergunta nascer, quando este turno é a resposta dela.
/// `None` quando o turno é pedido comum — e é esse `None` que mantém o caminho
/// de sempre intacto.
pub fn question_origin(connection:&Connection,turn_id:&str)->Result<Option<String>> {
    Ok(connection.query_row(
        "SELECT (SELECT m.content FROM messages m WHERE m.turn_id=q.turn_id AND m.role='user' ORDER BY m.created_at,m.id LIMIT 1)
         FROM questions q WHERE q.answered_by=?1",
        [turn_id],|row|row.get::<_,Option<String>>(0),
    ).optional()?.flatten())
}

/// Fecha a pergunta. Devolve `false` quando não havia nada pendente para fechar
/// — responder duas vezes a mesma pergunta não é acidente de tela, é dois
/// turnos em cima de um só pedido, e o segundo tem de ser recusado.
pub fn settle_question(connection:&Connection,turn_id:&str,status:&str,answered_by:Option<&str>)->Result<bool> {
    let changed=connection.execute(
        "UPDATE questions SET status=?1,answered_by=?2,settled_at=?3 WHERE turn_id=?4 AND status=?5",
        params![status,answered_by,Utc::now().to_rfc3339(),turn_id,QUESTION_PENDING],
    )?;
    Ok(changed>0)
}

/// O que as duas colunas mostram. `chats` vazio é a área de trabalho inteira;
/// com chats, só o que passou pelos daquele projeto.
pub fn feed(connection:&Connection,chats:Option<&BTreeSet<String>>)->Result<GateFeed> {
    if chats.is_some_and(BTreeSet::is_empty){return Ok(GateFeed::default());}
    Ok(GateFeed{entries:entries(connection,chats)?,exits:exits(connection,chats)?,tally:tally(connection,chats)?})
}

/// Um projeto é um punhado de chats: a cláusula que recorta a portaria para
/// eles, ou nada, quando a portaria é a da área de trabalho inteira.
fn scope(chats:Option<&BTreeSet<String>>)->String {
    chats.map_or(String::new(),|set|format!(" AND t.chat_id IN ({})",vec!["?"; set.len()].join(",")))
}

fn bound(chats:Option<&BTreeSet<String>>)->Vec<&String>{chats.map_or_else(Vec::new,|set|set.iter().collect())}

fn entries(connection:&Connection,chats:Option<&BTreeSet<String>>)->Result<Vec<EntryCheck>> {
    let sql=format!(
        "SELECT e.turn_id,e.at,t.chat_id,c.code,t.ordinal,e.prompt,e.score,e.demand,e.verdict,e.scope,e.criteria,e.source,e.note
         FROM entry_checks e JOIN turns t ON t.id=e.turn_id JOIN chats c ON c.id=t.chat_id
         WHERE 1=1{} ORDER BY e.at DESC,t.ordinal DESC LIMIT {FEED_WINDOW}",scope(chats));
    let mut statement=connection.prepare(&sql)?;
    let rows=statement.query_map(rusqlite::params_from_iter(bound(chats)),|row|Ok((
        row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,u32>(4)?,
        row.get::<_,String>(5)?,row.get::<_,u8>(6)?,row.get::<_,u8>(7)?,row.get::<_,String>(8)?,row.get::<_,String>(9)?,
        row.get::<_,String>(10)?,row.get::<_,String>(11)?,row.get::<_,String>(12)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,at,chat_id,chat_code,ordinal,prompt,score,demand,verdict,scope_,criteria,source,note)|Ok(EntryCheck{
        id,at:parse_time(&at)?,chat_id,turn:turn_code(&chat_code,ordinal),prompt,score,demand,
        verdict:EntryVerdict::parse(&verdict)?,scope:scope_,criteria:serde_json::from_str::<Vec<Criterion>>(&criteria)?,source,note,
    })).collect()
}

fn exits(connection:&Connection,chats:Option<&BTreeSet<String>>)->Result<Vec<ExitCheck>> {
    let sql=format!(
        "SELECT x.id,x.at,t.chat_id,x.turn_id,c.code,t.ordinal,x.kind,x.target,x.rule,x.verdict
         FROM exit_checks x JOIN turns t ON t.id=x.turn_id JOIN chats c ON c.id=t.chat_id
         WHERE 1=1{} ORDER BY x.at DESC,t.ordinal DESC LIMIT {FEED_WINDOW}",scope(chats));
    let mut statement=connection.prepare(&sql)?;
    let rows=statement.query_map(rusqlite::params_from_iter(bound(chats)),|row|Ok((
        row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,
        row.get::<_,u32>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?,row.get::<_,Option<String>>(8)?,row.get::<_,String>(9)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,at,chat_id,turn_id,chat_code,ordinal,kind,target,rule,verdict)|Ok(ExitCheck{
        id,at:parse_time(&at)?,chat_id,turn_id,turn:turn_code(&chat_code,ordinal),kind,target,rule,verdict:ExitVerdict::parse(&verdict)?,
    })).collect()
}

/// O placar conta no banco, não na janela: é por isso que ele não encolhe
/// quando os cartões antigos saem da tela.
fn tally(connection:&Connection,chats:Option<&BTreeSet<String>>)->Result<Tally> {
    let verdicts=format!(
        "SELECT COALESCE(SUM(e.verdict='pass'),0),COALESCE(SUM(e.verdict='ask'),0),COALESCE(SUM(e.verdict='block'),0)
         FROM entry_checks e JOIN turns t ON t.id=e.turn_id WHERE 1=1{}",scope(chats));
    let (passed,asked,blocked)=connection.query_row(&verdicts,rusqlite::params_from_iter(bound(chats)),|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
    let held_sql=format!("SELECT COUNT(*) FROM exit_checks x JOIN turns t ON t.id=x.turn_id WHERE x.verdict='held'{}",scope(chats));
    let held=connection.query_row(&held_sql,rusqlite::params_from_iter(bound(chats)),|row|row.get(0))?;
    Ok(Tally{passed,asked,blocked,held})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_primeiro_pedido_de_um_chat_vira_o_turno_01() {
        let connection=bench("XY4T9B");
        let turn=open_turn(&connection,"chat-1").expect("turno");

        assert_eq!(turn.ordinal,1);
        assert_eq!(turn.code,"XY4T9B·01","o código do chat carrega a posição do pedido");
        assert_eq!(turn.status,TurnStatus::Queued,"o pedido acabou de ser aceito e espera a vez");
    }

    #[test]
    fn cada_pedido_novo_ganha_o_numero_seguinte_do_seu_chat() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");

        let primeiro=open_turn(&connection,"chat-1").expect("turno");
        let segundo=open_turn(&connection,"chat-1").expect("turno");
        let outro=open_turn(&connection,"chat-2").expect("turno");

        assert_eq!([primeiro.ordinal,segundo.ordinal],[1,2]);
        assert_eq!(segundo.code,"XY4T9B·02");
        assert_eq!(outro.code,"K7M2QX·01","a contagem é por chat, não global");
        assert_ne!(primeiro.id,segundo.id,"dois turnos nunca compartilham o id");
    }

    #[test]
    fn retentar_pede_o_turno_pelo_nome_e_nao_gasta_numero_novo() {
        let connection=bench("XY4T9B");
        let primeiro=open_or_reopen(&connection,"chat-1",None).expect("turno");
        set_status(&connection,&primeiro.id,TurnStatus::Failed).expect("falhou");

        let de_novo=open_or_reopen(&connection,"chat-1",Some(&primeiro.id)).expect("retentativa");
        assert_eq!(de_novo.id,primeiro.id,"a retentativa é o mesmo pedido");
        assert_eq!(de_novo.code,"XY4T9B·01","o código do balão não muda ao retentar");
        assert_eq!(de_novo.status,TurnStatus::Queued,"o pedido voltou para a fila");

        let seguinte=open_or_reopen(&connection,"chat-1",None).expect("turno");
        assert_eq!(seguinte.ordinal,2,"a retentativa não queimou o número 2");
    }

    #[test]
    fn um_turno_de_outro_chat_nunca_e_reaberto_aqui() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        let alheio=open_turn(&connection,"chat-2").expect("turno");

        assert!(open_or_reopen(&connection,"chat-1",Some(&alheio.id)).is_err(),"o pedido de outro chat não se reabre aqui");
        assert!(open_or_reopen(&connection,"chat-1",Some("inexistente")).is_err(),"turno que não existe não vira turno novo em silêncio");
    }

    #[test]
    fn um_pedido_em_voo_volta_para_a_fila_quando_o_app_reabre() {
        let connection=bench("XY4T9B");
        let voando=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&voando.id,TurnStatus::Flying).expect("estado");
        let respondido=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&respondido.id,TurnStatus::Answered).expect("estado");

        let retomados=requeue_interrupted_turns(&connection).expect("varredura");

        assert_eq!(retomados,1,"só o que ficou pela metade é retomado");
        assert_eq!(status_of(&connection,&voando.id),TurnStatus::Queued,"o pedido aceito volta para a fila em vez de ser descartado");
        assert_eq!(status_of(&connection,&respondido.id),TurnStatus::Answered,"quem já tinha resposta não é tocado");
    }

    #[test]
    fn retentar_reabre_o_mesmo_turno_sem_gastar_numero_novo() {
        let connection=bench("XY4T9B");
        let primeiro=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&primeiro.id,TurnStatus::Failed).expect("estado");

        let retentado=reopen_turn(&connection,&primeiro.id).expect("retentativa");

        assert_eq!(retentado.id,primeiro.id,"a retentativa é o mesmo pedido");
        assert_eq!(retentado.code,"XY4T9B·01");
        assert_eq!(retentado.status,TurnStatus::Queued,"retentar põe o pedido de volta na fila, não direto no ar");
        assert_eq!(open_turn(&connection,"chat-1").expect("seguinte").ordinal,2,"a numeração continua de onde parou");
    }

    #[test]
    fn o_veredito_de_entrada_volta_inteiro_do_banco() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        let turno=open_turn(&connection,"chat-1").expect("turno");
        let pedido="Corrigir o cálculo do frete em src/checkout.rs; pronto quando o teste de frete passar";
        let check=judge(&turno,pedido,&heuristic_entry(pedido),"heurística local");

        record_entry(&connection,&check).expect("gravar");
        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.len(),1);
        assert_eq!(feed.entries[0].turn,"XY4T9B·01","o cartão da portaria cita o turno");
        assert_eq!(feed.entries[0],check,"o check volta do banco igual ao que entrou, medidores inclusive");
    }

    #[test]
    fn retentar_troca_as_saidas_em_vez_de_empilhar() {
        let connection=bench("XY4T9B");
        let turno=open_turn(&connection,"chat-1").expect("turno");
        record_exits(&connection,&turno,&[ExitCheck::new(&turno,"comando","rm -rf build",Some("permissions.shell · deny".into()))]).expect("primeira tentativa");

        record_exits(&connection,&turno,&[ExitCheck::new(&turno,"comando","cargo test",None)]).expect("retentativa");

        let feed=feed(&connection,None).expect("feed");
        assert_eq!(feed.exits.len(),1,"o que a tentativa anterior pediu deixou de valer");
        assert_eq!(feed.exits[0].target,"cargo test");
        assert_eq!(feed.tally.held,0,"o placar não carrega uma regra que deixou de ser tocada");
    }

    #[test]
    fn o_placar_conta_tudo_o_que_ja_passou_nao_so_o_que_cabe_na_tela() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        for _ in 0..FEED_WINDOW+40 {
            let turno=open_turn(&connection,"chat-1").expect("turno");
            record_entry(&connection,&judge(&turno,"x",&heuristic_entry("x"),"heurística local")).expect("gravar");
        }

        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.len(),FEED_WINDOW,"a tela recebe só uma janela");
        assert_eq!(u32::from(feed.tally.passed+feed.tally.asked+feed.tally.blocked),(FEED_WINDOW+40) as u32,"o placar conta os que já saíram de vista");
    }

    #[test]
    fn a_visao_do_turno_traz_o_codigo_o_estado_e_os_dois_vereditos() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        let turno=open_turn(&connection,"chat-1").expect("turno");
        let check=judge(&turno,"x",&heuristic_entry("x"),"heurística local");
        record_entry(&connection,&check).expect("entrada");
        record_exits(&connection,&turno,&[
            ExitCheck::new(&turno,"comando","cargo test",None),
            ExitCheck::new(&turno,"comando","rm -rf build",Some("permissions.shell · deny".into())),
        ]).expect("saídas");
        set_status(&connection,&turno.id,TurnStatus::Answered).expect("estado");

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views.len(),1);
        assert_eq!(views[0].code,"XY4T9B·01");
        assert_eq!(views[0].status,TurnStatus::Answered);
        assert_eq!(views[0].entry,Some(check.verdict));
        assert_eq!(views[0].exit,Some(ExitVerdict::Held),"uma regra tocada pinta a resposta inteira, mesmo com outra saída limpa");
    }

    #[test]
    fn um_turno_que_nao_passou_por_portao_nenhum_nao_tem_o_que_pintar() {
        let connection=bench("XY4T9B");
        open_turn(&connection,"chat-1").expect("turno");

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views[0].status,TurnStatus::Queued);
        assert_eq!(views[0].entry,None);
        assert_eq!(views[0].exit,None,"nada saiu, nada a colorir");
    }

    #[test]
    fn os_turnos_do_chat_vem_na_ordem_em_que_foram_pedidos() {
        let connection=bench("XY4T9B");
        for _ in 0..3 {open_turn(&connection,"chat-1").expect("turno");}

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views.iter().map(|view|view.code.as_str()).collect::<Vec<_>>(),vec!["XY4T9B·01","XY4T9B·02","XY4T9B·03"]);
    }

    /// A coluna é uma chegada: o que acabou de acontecer fica em cima.
    #[test]
    fn o_feed_mostra_o_mais_novo_primeiro() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        for pedido in ["primeiro","segundo","terceiro"] {
            let turno=open_turn(&connection,"chat-1").expect("turno");
            record_entry(&connection,&judge(&turno,pedido,&heuristic_entry(pedido),"heurística local")).expect("gravar");
            record_exits(&connection,&turno,&[ExitCheck::new(&turno,"comando",pedido,None)]).expect("saídas");
        }

        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.iter().map(|check|check.prompt.as_str()).collect::<Vec<_>>(),vec!["terceiro","segundo","primeiro"]);
        assert_eq!(feed.exits[0].target,"terceiro");
    }

    #[test]
    fn a_portaria_de_um_projeto_so_ve_os_chats_dele() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        for chat in ["chat-1","chat-2"] {
            let turno=open_turn(&connection,chat).expect("turno");
            record_entry(&connection,&judge(&turno,"x",&heuristic_entry("x"),"heurística local")).expect("gravar");
            record_exits(&connection,&turno,&[ExitCheck::new(&turno,"arquivo",".env",Some("privacy.deny · .env".into()))]).expect("saídas");
        }

        let feed=feed(&connection,Some(&BTreeSet::from(["chat-1".to_string()]))).expect("feed");

        assert_eq!(feed.entries.len(),1);
        assert_eq!(feed.entries[0].chat_id,"chat-1");
        assert_eq!(feed.exits.len(),1);
        assert_eq!(feed.tally.held,1,"o placar do projeto soma só os chats dele");
    }

    fn status_of(connection:&Connection,turn_id:&str)->TurnStatus {
        turn(connection,turn_id).expect("leitura").expect("o turno existe").status
    }

    /// Um banco de teste com o bastante para um turno existir: um projeto e um
    /// chat com o código que o turno vai herdar.

    #[test]
    fn o_pedido_nasce_na_fila_e_so_voa_quando_chega_a_vez_dele() {
        let connection=bench("XY4T9B");
        let turn=open_turn(&connection,"chat-1").expect("turno");

        assert_eq!(turn.status,TurnStatus::Queued,"o pedido gravado espera a vez, ele não sai voando");
    }

    #[test]
    fn a_fila_entrega_os_pedidos_na_ordem_em_que_foram_enviados() {
        let connection=bench("XY4T9B");
        let primeiro=open_turn(&connection,"chat-1").expect("primeiro");
        let segundo=open_turn(&connection,"chat-1").expect("segundo");

        let vez=next_queued(&connection).expect("consulta").expect("há fila");
        assert_eq!(vez.id,primeiro.id,"quem chegou antes é atendido antes");
        set_status(&connection,&primeiro.id,TurnStatus::Answered).expect("respondido");
        let vez=next_queued(&connection).expect("consulta").expect("ainda há fila");
        assert_eq!(vez.id,segundo.id,"o seguinte só é chamado depois que o anterior sai");
        set_status(&connection,&segundo.id,TurnStatus::Answered).expect("respondido");
        assert!(next_queued(&connection).expect("consulta").is_none(),"fila vazia não chama ninguém");
    }

    #[test]
    fn enquanto_um_pedido_esta_no_ar_a_fila_nao_chama_o_seguinte() {
        let connection=bench("XY4T9B");
        let voando=open_turn(&connection,"chat-1").expect("primeiro");
        let esperando=open_turn(&connection,"chat-1").expect("segundo");
        set_status(&connection,&voando.id,TurnStatus::Flying).expect("saiu");

        assert!(is_flying(&connection).expect("consulta"),"há um pedido sendo atendido");
        assert_eq!(next_queued(&connection).expect("consulta").map(|turn|turn.id),Some(esperando.id),"o seguinte está lá, esperando a vez");
    }

    #[test]
    fn a_fila_de_um_chat_nao_atropela_a_de_outro() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        let aqui=open_turn(&connection,"chat-1").expect("aqui");
        let ali=open_turn(&connection,"chat-2").expect("ali");

        assert_eq!(queue_depth(&connection,"chat-1").expect("fila"),1);
        assert_eq!(queue_depth(&connection,"chat-2").expect("fila"),1);
        assert_eq!(place_in_queue(&connection,&aqui.id).expect("posição"),Some(1));
        assert_eq!(place_in_queue(&connection,&ali.id).expect("posição"),Some(1),"cada chat tem a sua própria fila");
    }

    #[test]
    fn o_pedido_que_o_fechamento_pegou_no_ar_volta_para_a_fila_em_vez_de_morrer() {
        let connection=bench("XY4T9B");
        let turn=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&turn.id,TurnStatus::Flying).expect("saiu");

        requeue_interrupted_turns(&connection).expect("reabertura");

        assert_eq!(super::turn(&connection,&turn.id).expect("turno").expect("existe").status,TurnStatus::Queued,"o que ficou pela metade é retomado, não descartado");
    }

    /// O ciclo da pergunta: nasce pendente com o turno que a fez, é encerrada
    /// pelo turno que a responde e não volta. O `pending_question` é o que veste
    /// a caixa de enviar mensagem — depois do desfecho ela não veste mais nada.
    #[test]
    fn a_pergunta_nasce_pendente_e_o_turno_resposta_a_encerra() {
        let connection=bench("XY4T9B");
        let pergunta=open_turn(&connection,"chat-1").expect("turno");
        ask(&connection,&pergunta.id,"single","Qual provedor?",&["Anthropic".into(),"OpenAI".into()],"jev").expect("pergunta");

        let aberta=pending_question(&connection,"chat-1").expect("consulta").expect("pergunta em aberto");
        assert_eq!(aberta.turn_id,pergunta.id);
        assert_eq!(aberta.status,QUESTION_PENDING);
        assert_eq!(aberta.options,vec!["Anthropic".to_string(),"OpenAI".to_string()]);
        assert_eq!(aberta.code,pergunta.code,"a caixa mostra de qual pedido veio a pergunta");

        let resposta=open_turn(&connection,"chat-1").expect("turno-resposta");
        assert!(settle_question(&connection,&pergunta.id,QUESTION_ANSWERED,Some(&resposta.id)).expect("encerrar"));
        assert!(pending_question(&connection,"chat-1").expect("consulta").is_none(),"respondida não trava mais a caixa");
        assert_eq!(question_of(&connection,&pergunta.id).expect("consulta").expect("pergunta").status,QUESTION_ANSWERED);
    }

    /// Duas mãos na mesma pergunta — o clique repetido, a janela aberta duas
    /// vezes — e só a primeira encerra. A segunda descobre pelo retorno que
    /// chegou tarde, em vez de mandar um pedido a mais para o modelo.
    #[test]
    fn a_mesma_pergunta_nao_e_encerrada_duas_vezes() {
        let connection=bench("XY4T9B");
        let pergunta=open_turn(&connection,"chat-1").expect("turno");
        ask(&connection,&pergunta.id,"noul","Devo seguir?",&[],"jev").expect("pergunta");

        assert!(settle_question(&connection,&pergunta.id,QUESTION_DISMISSED,None).expect("ignorar"));
        assert!(!settle_question(&connection,&pergunta.id,QUESTION_ANSWERED,None).expect("segunda tentativa"),"o segundo desfecho não vale");
        assert_eq!(question_of(&connection,&pergunta.id).expect("consulta").expect("pergunta").status,QUESTION_DISMISSED,"quem ignorou primeiro decidiu");
    }

    pub(super) fn bench(chat_code:&str)->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL,root_path TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL);
             CREATE TABLE chats(id TEXT PRIMARY KEY,code TEXT NOT NULL DEFAULT '',project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,title TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
             INSERT INTO projects(id,name,created_at) VALUES('p','Produto','');"
        ).expect("esquema base");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-1',?1,'p','Chat','','')",[chat_code]).expect("chat");
        connection.execute_batch(SCHEMA).expect("esquema dos turnos");
        connection
    }
}
