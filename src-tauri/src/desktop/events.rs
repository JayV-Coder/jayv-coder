//! Os avisos que o núcleo manda à tela. Cada módulo do desktop emite por aqui,
//! e é só por estes nomes e formatos que a interface escuta o que acontece.

use crate::gatekeeper::{EntryCheck, ExitCheck};
use serde::Serialize;
use serde_json::Value;

pub const ENTRY_EVENT:&str="gate-entry";
pub const EXIT_EVENT:&str="gate-exit";
pub const RENAME_EVENT:&str="chat-renamed";
pub const PROMPT_EVENT:&str="chat-prompt";
pub const TURN_EVENT:&str="turn-settled";
/// Uma etapa do pedido, gravada e desenhada.
pub const BEAT_EVENT:&str="turn-beat";
/// Um pedaço da resposta. Vai para a tela a cada chegada e para o disco com
/// folga: são dois ritmos diferentes de propósito.
pub const CHUNK_EVENT:&str="turn-chunk";

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct EntryEvent{pub check:EntryCheck}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ExitEvent{pub checks:Vec<ExitCheck>}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct RenameEvent{pub chat_id:String,pub title:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct PromptEvent{pub chat_id:String}

/// A fila andou: um pedido entrou no ar ou acabou de se fechar.
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct TurnEvent{pub chat_id:String,pub turn_id:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct BeatEvent{pub chat_id:String,pub turn_id:String,pub seq:u32,pub kind:String,pub detail:Value}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ChunkEvent{pub chat_id:String,pub turn_id:String,pub text:String}
