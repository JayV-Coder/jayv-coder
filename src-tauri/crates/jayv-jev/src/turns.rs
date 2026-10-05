//! O turno: um pedido do desenvolvedor e tudo que a portaria viu nele.
//!
//! Cada pedido enviado num chat abre um turno, e o turno é o que os dois
//! portões apontam. O desenvolvedor o lê pelo código curto `XY4T9B·04` — o
//! código do chat mais a posição do pedido dentro dele —, e é esse código que
//! aparece tanto no balão do chat quanto nos cartões da Portaria.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::i18n::Text;
use crate::{gatekeeper::{Criterion, EntryCheck, EntryVerdict, ExitCheck, ExitVerdict, GateFeed, Tally}};
use std::collections::BTreeSet;

/// Um instante gravado no banco (RFC 3339).
pub fn parse_time(value:&str)->Result<DateTime<Utc>>{Ok(DateTime::parse_from_rfc3339(value).with_context(||format!("invalid timestamp `{value}`"))?.with_timezone(&Utc))}

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
    /// Na leitura, um estado que esta versão não conhece (gravado por uma
    /// versão mais nova em outra máquina) vale como falho: some da fila, e o
    /// chat continua abrindo.
    fn read(value:&str)->Self { Self::parse(value).unwrap_or(Self::Failed) }
    fn parse(value:&str)->Result<Self> {
        Ok(match value{"queued"=>Self::Queued,"flying"=>Self::Flying,"answered"=>Self::Answered,"failed"=>Self::Failed,"blocked"=>Self::Blocked,other=>anyhow::bail!("unknown turn status: `{other}`")})
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
    let existing=turn(connection,turn_id)?.ok_or_else(||Text::new("turn.notFound").with("turn",turn_id))?;
    anyhow::ensure!(existing.chat_id==chat_id,Text::new("turn.otherChat").with("turn",turn_id));
    // Só o que fechou sem resposta volta: reabrir um turno no ar ou respondido
    // apagaria a resposta dele. Um turno de outra máquina passa a ser daqui,
    // senão a fila (que só chama os locais) nunca o atenderia.
    anyhow::ensure!(matches!(existing.status,TurnStatus::Failed|TurnStatus::Blocked),Text::new("turn.notRetryable").with("turn",&existing.code));
    connection.execute("UPDATE turns SET local=1 WHERE id=?1",[turn_id])?;
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
    turn(connection,turn_id)?.ok_or_else(||Text::new("turn.notFound").with("turn",turn_id).into())
}

/// Na abertura do banco, o pedido que estava no ar quando o app fechou vira
/// falho. Ele não volta sozinho: um build pela metade, recomeçado do zero sem
/// ninguém olhando, mexeria de novo numa árvore que o agente já tinha mexido.
/// O reenvio fica a um clique, no balão. O que só esperava a vez continua na
/// fila — esse nunca começou. Devolve os turnos e os chats deles.
pub fn fail_interrupted_turns(connection:&Connection)->Result<Vec<(String,String)>> {
    let mut statement=connection.prepare("SELECT id,chat_id FROM turns WHERE status=?1 AND local=1 ORDER BY created_at,ordinal")?;
    let interrupted=statement.query_map([TurnStatus::Flying.as_str()],|row|Ok((row.get(0)?,row.get(1)?)))?.collect::<rusqlite::Result<Vec<(String,String)>>>()?;
    for (turn,_) in &interrupted { connection.execute("UPDATE turns SET status=?1,partial=NULL WHERE id=?2",params![TurnStatus::Failed.as_str(),turn])?; }
    Ok(interrupted)
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
    Ok((||Ok(Turn{id,chat_id,code:turn_code(&chat_code,ordinal),ordinal,status:TurnStatus::read(&status),created_at:parse_time(&created_at)?}))())
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
    /// Quem atendeu o pedido, como o Jev decidiu: o último `route` da narração.
    pub route:Option<TurnRoute>,
}

/// O agente (CLI), o modelo, o modo e o papel de um turno, para o balão.
/// Turno narrado antes do modo existir chega sem modo nem papel.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct TurnRoute {
    pub provider:String,
    pub model:String,
    #[serde(default)] pub mode:Option<String>,
    #[serde(default)] pub agent:Option<String>,
    /// O Jev trocou o modo do chat neste pedido.
    #[serde(default,skip_serializing_if="Option::is_none")] pub switched:Option<crate::progress::ModeSwitch>,
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
           t.partial,
           (SELECT v.detail FROM turn_events v WHERE v.turn_id=t.id AND v.kind='route' ORDER BY v.seq DESC LIMIT 1)
         FROM turns t JOIN chats c ON c.id=t.chat_id WHERE t.chat_id=?1 ORDER BY t.ordinal")?;
    let rows=statement.query_map([chat_id],|row|Ok((
        row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,u32>(2)?,row.get::<_,String>(3)?,
        row.get::<_,Option<String>>(4)?,row.get::<_,Option<String>>(5)?,row.get::<_,Option<String>>(6)?,
        row.get::<_,Option<String>>(7)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,chat_code,ordinal,status,entry,exit,partial,route)|{
        let status=TurnStatus::read(&status);
        let activity=if status.is_open(){activity(connection,&id)?}else{vec![]};
        Ok(TurnView{
            id,code:turn_code(&chat_code,ordinal),status,
            // Veredito desconhecido só tira o selo do balão.
            entry:entry.as_deref().and_then(|value|EntryVerdict::parse(value).ok()),
            exit:exit.as_deref().and_then(|value|ExitVerdict::parse(value).ok()),
            partial:partial.filter(|text|!text.is_empty()),activity,
            // Um detalhe ilegível só tira a linha do balão, não o chat.
            route:route.and_then(|detail|serde_json::from_str::<TurnRoute>(&detail).ok()).filter(|route|!route.provider.is_empty()),
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
/// Os critérios que ficaram fora da faixa no pedido anterior do mesmo chat:
/// o que a portaria cobrou por último ali.
pub fn previous_failing_criteria(connection:&Connection,turn_id:&str)->Result<Vec<String>> {
    let criteria:Option<String>=connection.query_row(
        "SELECT e.criteria FROM turns current JOIN turns previous ON previous.chat_id=current.chat_id AND previous.ordinal<current.ordinal
           JOIN entry_checks e ON e.turn_id=previous.id WHERE current.id=?1 ORDER BY previous.ordinal DESC LIMIT 1",
        [turn_id],|row|row.get(0)).optional()?;
    let criteria:Vec<crate::gatekeeper::Criterion>=criteria.map(|text|serde_json::from_str(&text).unwrap_or_default()).unwrap_or_default();
    Ok(criteria.into_iter().filter(|criterion|criterion.band.is_some_and(|[from,to]|!(from..=to).contains(&criterion.percent))).map(|criterion|criterion.id).collect())
}

/// Até onde a corrente de perguntas é seguida de volta. Um plano costuma
/// perguntar duas ou três coisas seguidas; o limite só existe para um laço no
/// banco não virar laço aqui.
pub const ORIGIN_DEPTH:usize=8;

/// O que o desenvolvedor disse até chegar a esta resposta, do pedido de origem
/// à resposta anterior — vazio quando o turno não responde pergunta nenhuma.
///
/// A corrente é seguida até o começo: quando o agente pergunta de novo depois
/// de uma resposta, a pergunta nova nasceu num turno-resposta, e parar ali
/// deixaria a portaria julgando "Resposta à pergunta…" sem o pedido que deu
/// assunto à conversa.
pub fn question_origin(connection:&Connection,turn_id:&str)->Result<Vec<String>> {
    let mut said=Vec::new();
    let mut current=turn_id.to_string();
    for _ in 0..ORIGIN_DEPTH {
        let step:Option<(String,Option<String>)>=connection.query_row(
            "SELECT q.turn_id,(SELECT m.content FROM messages m WHERE m.turn_id=q.turn_id AND m.role='user' ORDER BY m.created_at,m.id LIMIT 1)
             FROM questions q WHERE q.answered_by=?1",
            [&current],|row|Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let Some((question_turn,content))=step else {break};
        said.extend(content);
        current=question_turn;
    }
    said.reverse();
    Ok(said)
}

/// O veredito que a portaria deu ao turno que fez a pergunta respondida por
/// `turn_id`. Se a pergunta existe, o modelo respondeu aquele turno — ele
/// nunca foi barrado.
pub fn question_verdict(connection:&Connection,turn_id:&str)->Result<Option<EntryVerdict>> {
    let verdict:Option<String>=connection.query_row(
        "SELECT e.verdict FROM questions q JOIN entry_checks e ON e.turn_id=q.turn_id WHERE q.answered_by=?1",
        [turn_id],|row|row.get(0),
    ).optional()?;
    verdict.map(|verdict|EntryVerdict::parse(&verdict)).transpose()
}

/// O pedido anterior do mesmo chat, quando ele foi respondido: o veredito que
/// a portaria lhe deu e a hora da resposta. É contra ele que um pedido curto
/// ("pode implementar", "não funcionou") é lido como continuação.
pub fn previous_answer(connection:&Connection,turn_id:&str)->Result<Option<(EntryVerdict,DateTime<Utc>)>> {
    let found:Option<(String,Option<String>)>=connection.query_row(
        "SELECT e.verdict,(SELECT MAX(m.created_at) FROM messages m WHERE m.turn_id=previous.id AND m.role='assistant')
         FROM turns current JOIN turns previous ON previous.chat_id=current.chat_id AND previous.ordinal<current.ordinal
           JOIN entry_checks e ON e.turn_id=previous.id
         WHERE current.id=?1 AND previous.status=?2 ORDER BY previous.ordinal DESC LIMIT 1",
        params![turn_id,TurnStatus::Answered.as_str()],|row|Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let Some((verdict,Some(at)))=found else { return Ok(None) };
    Ok(Some((EntryVerdict::parse(&verdict)?,parse_time(&at)?)))
}

/// Se `turn_id` responde à confirmação que a portaria fez no lugar do agente.
pub fn answers_gate(connection:&Connection,turn_id:&str)->Result<bool> {
    Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM questions WHERE answered_by=?1 AND source=?2)",params![turn_id,crate::gatekeeper::GATE_SOURCE],|row|row.get(0))?)
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
    // Uma linha com veredito que esta versão não conhece sai do feed, não o feed inteiro.
    rows.into_iter().filter(|row|EntryVerdict::parse(&row.8).is_ok()).map(|(id,at,chat_id,chat_code,ordinal,prompt,score,demand,verdict,scope_,criteria,source,note)|Ok(EntryCheck{
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
    rows.into_iter().filter(|row|ExitVerdict::parse(&row.9).is_ok()).map(|(id,at,chat_id,turn_id,chat_code,ordinal,kind,target,rule,verdict)|Ok(ExitCheck{
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
    fn a_chats_first_request_becomes_turn_01() {
        let connection=bench("XY4T9B");
        let turn=open_turn(&connection,"chat-1").expect("turno");

        assert_eq!(turn.ordinal,1);
        assert_eq!(turn.code,"XY4T9B·01","o código do chat carrega a posição do pedido");
        assert_eq!(turn.status,TurnStatus::Queued,"o pedido acabou de ser aceito e espera a vez");
    }

    #[test]
    fn each_new_request_gets_the_next_number_in_its_chat() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");

        let first=open_turn(&connection,"chat-1").expect("turno");
        let second=open_turn(&connection,"chat-1").expect("turno");
        let other_chat_turn=open_turn(&connection,"chat-2").expect("turno");

        assert_eq!([first.ordinal,second.ordinal],[1,2]);
        assert_eq!(second.code,"XY4T9B·02");
        assert_eq!(other_chat_turn.code,"K7M2QX·01","a contagem é por chat, não global");
        assert_ne!(first.id,second.id,"dois turnos nunca compartilham o id");
    }

    #[test]
    fn retrying_names_the_turn_and_spends_no_new_number() {
        let connection=bench("XY4T9B");
        let first=open_or_reopen(&connection,"chat-1",None).expect("turno");
        set_status(&connection,&first.id,TurnStatus::Failed).expect("falhou");

        let again=open_or_reopen(&connection,"chat-1",Some(&first.id)).expect("retentativa");
        assert_eq!(again.id,first.id,"a retentativa é o mesmo pedido");
        assert_eq!(again.code,"XY4T9B·01","o código do balão não muda ao retentar");
        assert_eq!(again.status,TurnStatus::Queued,"o pedido voltou para a fila");

        let following=open_or_reopen(&connection,"chat-1",None).expect("turno");
        assert_eq!(following.ordinal,2,"a retentativa não queimou o número 2");
    }

    #[test]
    fn a_turn_from_another_chat_is_never_reopened_here() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        let foreign=open_turn(&connection,"chat-2").expect("turno");

        assert!(open_or_reopen(&connection,"chat-1",Some(&foreign.id)).is_err(),"o pedido de outro chat não se reabre aqui");
        assert!(open_or_reopen(&connection,"chat-1",Some("inexistente")).is_err(),"turno que não existe não vira turno novo em silêncio");
    }

    #[test]
    fn a_flying_request_fails_when_the_app_reopens_and_the_queue_keeps_waiting() {
        let connection=bench("XY4T9B");
        let flying=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&flying.id,TurnStatus::Flying).expect("estado");
        set_partial(&connection,&flying.id,"metade da resposta").expect("rascunho");
        let answered=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&answered.id,TurnStatus::Answered).expect("estado");
        let waiting=open_turn(&connection,"chat-1").expect("turno");

        let interrupted=fail_interrupted_turns(&connection).expect("varredura");

        assert_eq!(interrupted,vec![(flying.id.clone(),"chat-1".to_string())],"só o que estava no ar");
        assert_eq!(status_of(&connection,&flying.id),TurnStatus::Failed,"não recomeça sozinho: o reenvio é de quem pediu");
        let partial:Option<String>=connection.query_row("SELECT partial FROM turns WHERE id=?1",[&flying.id],|row|row.get(0)).expect("rascunho");
        assert_eq!(partial,None,"o rascunho sai junto");
        assert_eq!(status_of(&connection,&answered.id),TurnStatus::Answered,"quem já tinha resposta não é tocado");
        assert_eq!(status_of(&connection,&waiting.id),TurnStatus::Queued,"o que nunca começou continua na fila");
        assert!(reopen_turn(&connection,&flying.id).is_ok(),"o falho se reenvia");
    }

    #[test]
    fn retrying_reopens_the_same_turn_without_a_new_number() {
        let connection=bench("XY4T9B");
        let first=open_turn(&connection,"chat-1").expect("turno");
        set_status(&connection,&first.id,TurnStatus::Failed).expect("estado");

        let retried=reopen_turn(&connection,&first.id).expect("retentativa");

        assert_eq!(retried.id,first.id,"a retentativa é o mesmo pedido");
        assert_eq!(retried.code,"XY4T9B·01");
        assert_eq!(retried.status,TurnStatus::Queued,"retentar põe o pedido de volta na fila, não direto no ar");
        assert_eq!(open_turn(&connection,"chat-1").expect("seguinte").ordinal,2,"a numeração continua de onde parou");
    }

    #[test]
    fn the_entry_verdict_comes_back_whole_from_the_database() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        let this_turn=open_turn(&connection,"chat-1").expect("turno");
        let request_text="Corrigir o cálculo do frete em src/checkout.rs; pronto quando o teste de frete passar";
        let check=judge(&this_turn,request_text,&heuristic_entry(request_text),"heurística local");

        record_entry(&connection,&check).expect("gravar");
        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.len(),1);
        assert_eq!(feed.entries[0].turn,"XY4T9B·01","o cartão da portaria cita o turno");
        assert_eq!(feed.entries[0],check,"o check volta do banco igual ao que entrou, medidores inclusive");
    }

    #[test]
    fn retrying_replaces_the_exits_instead_of_stacking_them() {
        let connection=bench("XY4T9B");
        let this_turn=open_turn(&connection,"chat-1").expect("turno");
        record_exits(&connection,&this_turn,&[ExitCheck::new(&this_turn,"command","rm -rf build",Some("permissions.shell · deny".into()))]).expect("primeira tentativa");

        record_exits(&connection,&this_turn,&[ExitCheck::new(&this_turn,"command","cargo test",None)]).expect("retentativa");

        let feed=feed(&connection,None).expect("feed");
        assert_eq!(feed.exits.len(),1,"o que a tentativa anterior pediu deixou de valer");
        assert_eq!(feed.exits[0].target,"cargo test");
        assert_eq!(feed.tally.held,0,"o placar não carrega uma regra que deixou de ser tocada");
    }

    #[test]
    fn the_tally_counts_everything_not_just_what_fits_on_screen() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        for _ in 0..FEED_WINDOW+40 {
            let this_turn=open_turn(&connection,"chat-1").expect("turno");
            record_entry(&connection,&judge(&this_turn,"x",&heuristic_entry("x"),"heurística local")).expect("gravar");
        }

        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.len(),FEED_WINDOW,"a tela recebe só uma janela");
        assert_eq!(u32::from(feed.tally.passed+feed.tally.asked+feed.tally.blocked),(FEED_WINDOW+40) as u32,"o placar conta os que já saíram de vista");
    }

    #[test]
    fn the_turn_view_carries_the_code_the_status_and_both_verdicts() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        let this_turn=open_turn(&connection,"chat-1").expect("turno");
        let check=judge(&this_turn,"x",&heuristic_entry("x"),"heurística local");
        record_entry(&connection,&check).expect("entrada");
        record_exits(&connection,&this_turn,&[
            ExitCheck::new(&this_turn,"command","cargo test",None),
            ExitCheck::new(&this_turn,"command","rm -rf build",Some("permissions.shell · deny".into())),
        ]).expect("saídas");
        set_status(&connection,&this_turn.id,TurnStatus::Answered).expect("estado");

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views.len(),1);
        assert_eq!(views[0].code,"XY4T9B·01");
        assert_eq!(views[0].status,TurnStatus::Answered);
        assert_eq!(views[0].entry,Some(check.verdict));
        assert_eq!(views[0].exit,Some(ExitVerdict::Held),"uma regra tocada pinta a resposta inteira, mesmo com outra saída limpa");
    }

    #[test]
    fn a_turn_that_passed_no_gate_has_nothing_to_paint() {
        let connection=bench("XY4T9B");
        open_turn(&connection,"chat-1").expect("turno");

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views[0].status,TurnStatus::Queued);
        assert_eq!(views[0].entry,None);
        assert_eq!(views[0].exit,None,"nada saiu, nada a colorir");
    }

    #[test]
    fn a_chats_turns_come_in_the_order_they_were_requested() {
        let connection=bench("XY4T9B");
        for _ in 0..3 {open_turn(&connection,"chat-1").expect("turno");}

        let views=views_for_chat(&connection,"chat-1").expect("visões");

        assert_eq!(views.iter().map(|view|view.code.as_str()).collect::<Vec<_>>(),vec!["XY4T9B·01","XY4T9B·02","XY4T9B·03"]);
    }

    /// A coluna é uma chegada: o que acabou de acontecer fica em cima.
    #[test]
    fn the_feed_shows_the_newest_first() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        for request_text in ["primeiro","segundo","terceiro"] {
            let this_turn=open_turn(&connection,"chat-1").expect("turno");
            record_entry(&connection,&judge(&this_turn,request_text,&heuristic_entry(request_text),"heurística local")).expect("gravar");
            record_exits(&connection,&this_turn,&[ExitCheck::new(&this_turn,"command",request_text,None)]).expect("saídas");
        }

        let feed=feed(&connection,None).expect("feed");

        assert_eq!(feed.entries.iter().map(|check|check.prompt.as_str()).collect::<Vec<_>>(),vec!["terceiro","segundo","primeiro"]);
        assert_eq!(feed.exits[0].target,"terceiro");
    }

    #[test]
    fn a_projects_gate_only_sees_its_own_chats() {
        use crate::gatekeeper::{heuristic_entry,judge};
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        for chat in ["chat-1","chat-2"] {
            let this_turn=open_turn(&connection,chat).expect("turno");
            record_entry(&connection,&judge(&this_turn,"x",&heuristic_entry("x"),"heurística local")).expect("gravar");
            record_exits(&connection,&this_turn,&[ExitCheck::new(&this_turn,"file",".env",Some("privacy.deny · .env".into()))]).expect("saídas");
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
    fn a_request_starts_queued_and_only_flies_on_its_turn() {
        let connection=bench("XY4T9B");
        let turn=open_turn(&connection,"chat-1").expect("turno");

        assert_eq!(turn.status,TurnStatus::Queued,"o pedido gravado espera a vez, ele não sai voando");
    }

    #[test]
    fn the_queue_delivers_requests_in_the_order_they_were_sent() {
        let connection=bench("XY4T9B");
        let first=open_turn(&connection,"chat-1").expect("primeiro");
        let second=open_turn(&connection,"chat-1").expect("segundo");

        let next_up=next_queued(&connection).expect("consulta").expect("há fila");
        assert_eq!(next_up.id,first.id,"quem chegou antes é atendido antes");
        set_status(&connection,&first.id,TurnStatus::Answered).expect("respondido");
        let next_up=next_queued(&connection).expect("consulta").expect("ainda há fila");
        assert_eq!(next_up.id,second.id,"o seguinte só é chamado depois que o anterior sai");
        set_status(&connection,&second.id,TurnStatus::Answered).expect("respondido");
        assert!(next_queued(&connection).expect("consulta").is_none(),"fila vazia não chama ninguém");
    }

    #[test]
    fn while_a_request_is_flying_the_queue_does_not_call_the_next() {
        let connection=bench("XY4T9B");
        let flying=open_turn(&connection,"chat-1").expect("primeiro");
        let waiting=open_turn(&connection,"chat-1").expect("segundo");
        set_status(&connection,&flying.id,TurnStatus::Flying).expect("saiu");

        assert!(is_flying(&connection).expect("consulta"),"há um pedido sendo atendido");
        assert_eq!(next_queued(&connection).expect("consulta").map(|turn|turn.id),Some(waiting.id),"o seguinte está lá, esperando a vez");
    }

    #[test]
    fn one_chats_queue_does_not_run_over_anothers() {
        let connection=bench("XY4T9B");
        connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES('chat-2','K7M2QX','p','Outro','','')",[]).expect("segundo chat");
        let here=open_turn(&connection,"chat-1").expect("aqui");
        let there=open_turn(&connection,"chat-2").expect("ali");

        assert_eq!(queue_depth(&connection,"chat-1").expect("fila"),1);
        assert_eq!(queue_depth(&connection,"chat-2").expect("fila"),1);
        assert_eq!(place_in_queue(&connection,&here.id).expect("posição"),Some(1));
        assert_eq!(place_in_queue(&connection,&there.id).expect("posição"),Some(1),"cada chat tem a sua própria fila");
    }

    /// O ciclo da pergunta: nasce pendente com o turno que a fez, é encerrada
    /// pelo turno que a responde e não volta. O `pending_question` é o que veste
    /// a caixa de enviar mensagem — depois do desfecho ela não veste mais nada.
    #[test]
    fn a_question_starts_pending_and_the_answer_turn_closes_it() {
        let connection=bench("XY4T9B");
        let question_turn=open_turn(&connection,"chat-1").expect("turno");
        ask(&connection,&question_turn.id,"single","Qual provedor?",&["Anthropic".into(),"OpenAI".into()],"jev").expect("pergunta");

        let open_question=pending_question(&connection,"chat-1").expect("consulta").expect("pergunta em aberto");
        assert_eq!(open_question.turn_id,question_turn.id);
        assert_eq!(open_question.status,QUESTION_PENDING);
        assert_eq!(open_question.options,vec!["Anthropic".to_string(),"OpenAI".to_string()]);
        assert_eq!(open_question.code,question_turn.code,"a caixa mostra de qual pedido veio a pergunta");

        let answer_turn=open_turn(&connection,"chat-1").expect("turno-resposta");
        assert!(settle_question(&connection,&question_turn.id,QUESTION_ANSWERED,Some(&answer_turn.id)).expect("encerrar"));
        assert!(pending_question(&connection,"chat-1").expect("consulta").is_none(),"respondida não trava mais a caixa");
        assert_eq!(question_of(&connection,&question_turn.id).expect("consulta").expect("pergunta").status,QUESTION_ANSWERED);
    }

    /// Duas mãos na mesma pergunta — o clique repetido, a janela aberta duas
    /// vezes — e só a primeira encerra. A segunda descobre pelo retorno que
    /// chegou tarde, em vez de mandar um pedido a mais para o modelo.
    #[test]
    fn the_same_question_is_not_closed_twice() {
        let connection=bench("XY4T9B");
        let question_turn=open_turn(&connection,"chat-1").expect("turno");
        ask(&connection,&question_turn.id,"noul","Devo seguir?",&[],"jev").expect("pergunta");

        assert!(settle_question(&connection,&question_turn.id,QUESTION_DISMISSED,None).expect("ignorar"));
        assert!(!settle_question(&connection,&question_turn.id,QUESTION_ANSWERED,None).expect("segunda tentativa"),"o segundo desfecho não vale");
        assert_eq!(question_of(&connection,&question_turn.id).expect("consulta").expect("pergunta").status,QUESTION_DISMISSED,"quem ignorou primeiro decidiu");
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
