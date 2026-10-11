//! Os mods do catálogo (`JayV-Coder/jayv-coder-mods`). O núcleo é Rust
//! compilado no instalador e não executa nada baixado: cada mod do catálogo é
//! um manifesto que liga um motor que o app já tem — uma integração com LLM
//! (`llm`) ou um recurso opcional (`feature`) — ou um mod pronto que a pessoa
//! instala como um mod seu (`template`).
//!
//! O catálogo desce na volta do sync pela função `mods` do Supabase e fica num
//! cache local, fora da fila: só desce. Quem usa liga e desliga cada recurso na
//! aba Mods; a escolha mora em `account_settings` (chave `mods`) e sincroniza
//! com o ambiente.
//!
//! O que fica desligado (`disabled`) entra no `Entitlements` como o que o plano
//! não tem: o recurso cujo mod saiu do catálogo, pede uma versão mais nova do
//! app ou foi desligado por quem usa, e o motor de LLM fora do catálogo. Sem
//! catálogo nenhum (offline desde a instalação, servidor sem a função), nada
//! sai do ar por falta dele: vale só a escolha de quem usa.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// O formato do catálogo que esta versão entende. Um catálogo de outro formato
/// não substitui o cache: vale o último que ela entendeu.
pub const SCHEMA:u64=1;

pub const CACHE:&str="
CREATE TABLE IF NOT EXISTS mod_catalog (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  mods TEXT NOT NULL,
  fetched_at TEXT NOT NULL
);";

/// A chave das escolhas de quem usa em `account_settings`.
pub const STATES_KEY:&str="mods";

pub const LLM:&str="llm";
pub const FEATURE:&str="feature";
pub const TEMPLATE:&str="template";

/// Os recursos opcionais que viraram mods: os do plano que quem usa pode tirar
/// do app (o núcleo, a conta e as organizações ficam fora). A mesma lista do
/// `engines.json` do repositório de mods.
pub const FEATURE_MODS:[&str;15]=[
    "stats","gateBoard","liveFiles","chatSearch","projectNotes","secondOpinion","planFirst","parallelTasks",
    "answerRecall","leanCode","symbolIndex","mcp","skills","skillsHub","conversationFind",
];

/// Um mod como o catálogo o descreve. Os textos vêm nos dez idiomas; o resto
/// dos campos é lido com padrão, e campo desconhecido é ignorado — o
/// repositório pode acrescentar campos sem quebrar o app antigo.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Manifest {
    pub id:String,
    pub kind:String,
    #[serde(default)] pub category:String,
    /// O recurso do plano que liga o mod; nulo, nenhum.
    #[serde(default)] pub plan:Option<String>,
    /// Só no recurso: se ele nasce ligado.
    #[serde(default,skip_serializing_if="Option::is_none")] pub default_on:Option<bool>,
    /// A versão do app a partir da qual o mod vale.
    #[serde(default)] pub min_app:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub docs:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub homepage:Option<String>,
    #[serde(default)] pub title:BTreeMap<String,String>,
    #[serde(default)] pub summary:BTreeMap<String,String>,
    /// Só no mod pronto: a definição que vira o mod de quem instala.
    #[serde(default,skip_serializing_if="Option::is_none")] pub template:Option<Value>,
}

/// A versão do app, dita na partida (`set_app_version`). Sem ela (os testes do
/// crate), todo mod vale.
static APP_VERSION:OnceLock<String>=OnceLock::new();

pub fn set_app_version(version:&str) { let _=APP_VERSION.set(version.to_string()); }

pub fn app_version()->Option<&'static str> { APP_VERSION.get().map(String::as_str) }

fn parts(version:&str)->[u64;3] {
    let mut parts=[0;3];
    for (slot,part) in parts.iter_mut().zip(version.split('-').next().unwrap_or_default().split('.')) { *slot=part.parse().unwrap_or(0); }
    parts
}

/// O mod vale nesta versão: sem `minApp`, ou com ela até a instalada.
pub fn usable(manifest:&Manifest,version:Option<&str>)->bool {
    match (manifest.min_app.as_deref(),version) {
        (Some(min),Some(current))=>parts(min)<=parts(current),
        _=>true,
    }
}

/// O catálogo que desceu, se esta versão o entende: o formato certo e uma
/// lista de mods. Cada mod que não se lê sai; o resto fica.
pub fn parse(value:&Value)->Option<Vec<Manifest>> {
    if value.get("schema").and_then(Value::as_u64)!=Some(SCHEMA) { return None; }
    let mods=value.get("mods")?.as_array()?;
    let mut seen=BTreeSet::new();
    Some(mods.iter().filter_map(|mod_value|serde_json::from_value::<Manifest>(mod_value.clone()).ok())
        .filter(|manifest|[LLM,FEATURE,TEMPLATE].contains(&manifest.kind.as_str()))
        .filter(|manifest|seen.insert((manifest.kind.clone(),manifest.id.clone())))
        .collect())
}

/// Troca o cache pelo catálogo que desceu. Falso, sem mexer no cache, quando
/// esta versão não o entende.
pub fn replace(connection:&Connection,value:&Value)->Result<bool> {
    let Some(mods)=parse(value) else { return Ok(false) };
    connection.execute("INSERT OR REPLACE INTO mod_catalog(id,mods,fetched_at) VALUES(1,?1,?2)",params![serde_json::to_string(&mods)?,chrono::Utc::now().to_rfc3339()])?;
    Ok(true)
}

/// O catálogo em cache e quando desceu. Nulo: nenhum desceu ainda.
pub fn catalog(connection:&Connection)->Result<Option<(Vec<Manifest>,String)>> {
    let raw:Option<(String,String)>=connection.query_row("SELECT mods,fetched_at FROM mod_catalog WHERE id=1",[],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    Ok(raw.and_then(|(mods,at)|serde_json::from_str::<Vec<Manifest>>(&mods).ok().map(|mods|(mods,at))))
}

/// As escolhas de quem usa (`id` → ligado). Ilegível vale como nenhuma.
pub fn states(connection:&Connection)->Result<BTreeMap<String,bool>> {
    let value:Option<String>=connection.query_row("SELECT value FROM account_settings WHERE key=?1",[STATES_KEY],|row|row.get(0)).optional()?;
    Ok(value.and_then(|value|serde_json::from_str(&value).ok()).unwrap_or_default())
}

/// Grava as escolhas, só as dos recursos que viraram mods: um id qualquer não
/// entra na configuração sincronizada.
pub fn save_states(connection:&Connection,states:&BTreeMap<String,bool>)->Result<BTreeMap<String,bool>> {
    let kept:BTreeMap<String,bool>=states.iter().filter(|(id,_)|FEATURE_MODS.contains(&id.as_str())).map(|(id,on)|(id.clone(),*on)).collect();
    connection.execute(
        "INSERT INTO account_settings(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        params![STATES_KEY,serde_json::to_string(&kept)?,chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(kept)
}

/// O recurso ligado pela escolha de quem usa ou, sem escolha, pelo padrão do
/// mod (ligado, sem catálogo).
pub fn chosen(states:&BTreeMap<String,bool>,manifest:Option<&Manifest>,id:&str)->bool {
    states.get(id).copied().unwrap_or_else(||manifest.and_then(|manifest|manifest.default_on).unwrap_or(true))
}

/// O que os mods desligam: os recursos (`FEATURE_MODS`) e os motores de LLM
/// do app (`builtin`). `catalog` nulo: nenhum catálogo desceu, e só vale a
/// escolha de quem usa.
pub fn disabled(catalog:Option<&[Manifest]>,states:&BTreeMap<String,bool>,version:Option<&str>,builtin:&[&str])->BTreeSet<String> {
    let find=|kind:&str,id:&str|catalog.map(|mods|mods.iter().find(|manifest|manifest.kind==kind&&manifest.id==id&&usable(manifest,version)));
    let mut off=BTreeSet::new();
    for id in FEATURE_MODS {
        let present=find(FEATURE,id);
        let on=match present {
            Some(None)=>false,
            Some(Some(manifest))=>chosen(states,Some(manifest),id),
            None=>chosen(states,None,id),
        };
        if !on { off.insert(id.to_string()); }
    }
    for id in builtin { if matches!(find(LLM,id),Some(None)) { off.insert(id.to_string()); } }
    off
}

/// O que os mods desligam neste banco, com o catálogo em cache e as escolhas
/// gravadas.
pub fn disabled_in(connection:&Connection,builtin:&[&str])->Result<BTreeSet<String>> {
    let cached=catalog(connection)?;
    Ok(disabled(cached.as_ref().map(|(mods,_)|mods.as_slice()),&states(connection)?,app_version(),builtin))
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(CACHE).expect("cache");
        connection.execute_batch("CREATE TABLE account_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);").expect("conta");
        connection
    }

    fn feature(id:&str,default_on:bool)->Value { json!({"id":id,"kind":"feature","category":"project","plan":id,"defaultOn":default_on,"minApp":"0.94.0","title":{"en":id},"summary":{"en":id}}) }
    fn llm(id:&str)->Value { json!({"id":id,"kind":"llm","category":"agents","plan":null,"minApp":"0.94.0","title":{"en":id},"summary":{"en":id}}) }
    fn all_features()->Vec<Value> { FEATURE_MODS.iter().map(|id|feature(id,true)).collect() }

    #[test] fn an_unknown_schema_or_shape_keeps_the_cache() {
        let connection=database();
        let mut mods=all_features(); mods.push(llm("claude"));
        assert!(replace(&connection,&json!({"schema":1,"mods":mods})).expect("grava"));
        assert!(!replace(&connection,&json!({"schema":2,"mods":[]})).expect("formato novo"));
        assert!(!replace(&connection,&json!({"mods":[]})).expect("sem formato"));
        assert!(!replace(&connection,&json!("x")).expect("lixo"));
        assert_eq!(catalog(&connection).expect("lê").expect("cache").0.len(),16,"vale o último que esta versão entendeu");
    }

    #[test] fn broken_unknown_and_repeated_mods_are_dropped() {
        let parsed=parse(&json!({"schema":1,"mods":[feature("stats",true),feature("stats",false),{"id":"x"},{"id":"y","kind":"plugin"},llm("claude"),{"id":"z","kind":"feature","future":{"kept":false}}]})).expect("lê");
        assert_eq!(parsed.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["stats","claude","z"]);
        assert_eq!(parsed[0].default_on,Some(true),"o primeiro de dois repetidos fica");
    }

    #[test] fn without_a_catalog_only_the_choice_of_the_user_counts() {
        let off=disabled(None,&BTreeMap::new(),Some("0.94.0"),&["claude"]);
        assert!(off.is_empty(),"sem catálogo, nada sai do ar: {off:?}");
        let states=BTreeMap::from([("stats".to_string(),false)]);
        assert_eq!(disabled(None,&states,Some("0.94.0"),&["claude"]),BTreeSet::from(["stats".to_string()]));
    }

    #[test] fn the_catalog_decides_what_exists() {
        let mut mods=all_features(); mods.retain(|value|value["id"]!="mcp"); mods.push(llm("claude"));
        let parsed=parse(&json!({"schema":1,"mods":mods})).expect("lê");
        let off=disabled(Some(&parsed),&BTreeMap::new(),Some("0.94.0"),&["claude","codex"]);
        assert_eq!(off,BTreeSet::from(["mcp".to_string(),"codex".to_string()]),"o recurso e o motor fora do catálogo saem");
    }

    #[test] fn a_mod_for_a_newer_app_waits_for_it() {
        let mut stats=feature("stats",true); stats["minApp"]=json!("0.95.0");
        let mut mods=all_features(); mods.retain(|value|value["id"]!="stats"); mods.push(stats);
        let parsed=parse(&json!({"schema":1,"mods":mods})).expect("lê");
        assert!(disabled(Some(&parsed),&BTreeMap::new(),Some("0.94.3"),&[]).contains("stats"));
        assert!(!disabled(Some(&parsed),&BTreeMap::new(),Some("0.95.0"),&[]).contains("stats"));
        assert!(!disabled(Some(&parsed),&BTreeMap::new(),Some("1.0.0-beta"),&[]).contains("stats"));
        assert!(!disabled(Some(&parsed),&BTreeMap::new(),None,&[]).contains("stats"),"sem versão dita, vale");
    }

    #[test] fn the_default_of_the_mod_counts_until_the_user_chooses() {
        let mut mods=all_features(); mods.retain(|value|value["id"]!="leanCode"); mods.push(feature("leanCode",false));
        let parsed=parse(&json!({"schema":1,"mods":mods})).expect("lê");
        assert!(disabled(Some(&parsed),&BTreeMap::new(),None,&[]).contains("leanCode"),"nasce desligado");
        let states=BTreeMap::from([("leanCode".to_string(),true),("stats".to_string(),false)]);
        let off=disabled(Some(&parsed),&states,None,&[]);
        assert!(!off.contains("leanCode")&&off.contains("stats"),"a escolha de quem usa vence o padrão");
    }

    #[test] fn only_feature_mods_are_saved_and_unreadable_states_count_as_none() {
        let connection=database();
        let saved=save_states(&connection,&BTreeMap::from([("stats".to_string(),false),("whatever".to_string(),false)])).expect("grava");
        assert_eq!(saved,BTreeMap::from([("stats".to_string(),false)]));
        assert_eq!(states(&connection).expect("lê"),saved);
        connection.execute("UPDATE account_settings SET value='[1,2]' WHERE key='mods'",[]).expect("estraga");
        assert!(states(&connection).expect("lê").is_empty());
    }
}
