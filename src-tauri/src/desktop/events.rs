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
/// Um arquivo da pasta do chat mudou durante o pedido (ou, com `file` nulo,
/// um pedido novo começou a olhar a pasta).
pub const LIVE_EVENT:&str="live-file";
/// Um item do menu da bandeja que leva a uma tela (ver `tray`).
pub const TRAY_EVENT:&str="tray-action";
/// O núcleo achou uma versão nova no repositório de releases.
pub const UPDATE_EVENT:&str="update-found";
/// A conexão com o Supabase mudou: online, offline, sessão vencida, sem login.
pub const LINK_EVENT:&str="link-changed";
/// Chegaram idiomas ou traduções novos no cache.
pub const TRANSLATIONS_EVENT:&str="translations-updated";
/// Os modelos dos agentes foram trocados pela lista que os CLIs deram.
pub const MODELS_EVENT:&str="models-updated";
/// As configurações gravadas na tela de Configurações já valem: `now` diz se o
/// orquestrador as recebeu na hora ou se um pedido no ar as deixou para o próximo.
pub const SETTINGS_EVENT:&str="settings-applied";
/// Entrou gasto novo no banco: a tela refaz a conta do escopo que mostra.
pub const USAGE_EVENT:&str="usage-recorded";
/// Chegou uma leitura nova do limite de um plano.
pub const QUOTA_EVENT:&str="quota-changed";
/// O banco aberto mudou de ambiente, ou projetos de outro ambiente saíram dele:
/// a tela relê os projetos e as configurações.
pub const ENVIRONMENT_EVENT:&str="environment-changed";

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct EnvironmentEvent{pub environment:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SettingsEvent{pub now:bool}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct LinkEvent{pub link:crate::sync::Link}

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

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct UsageEvent{pub project_id:Option<String>,pub chat_id:Option<String>}

/// `crossed` é o patamar (80, 95 ou 100) que a leitura acabou de passar,
/// subindo; vazio quando não passou nenhum.
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuotaEvent{pub quota:crate::usage::Quota,pub crossed:Option<u8>}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct LiveEvent{pub chat_id:String,pub turn_id:String,pub file:Option<crate::live_files::Change>}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct TrayAction{pub action:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct UpdateFound{pub version:String}
