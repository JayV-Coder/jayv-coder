//! O catálogo dos mods: o manifesto de cada mod e o cache local do último
//! catálogo que esta versão entendeu.
//!
//! O catálogo desce na volta do sync e fica num cache fora da fila: só desce.
//! O formato genérico (`SCHEMA` 2) diz, para cada mod, o que ele traz a cada
//! ponto de extensão (`contributes`). O formato antigo (`LEGACY_SCHEMA` 1, do
//! app 0.94) ainda se lê — o servidor sem a rota nova, o cache gravado pela
//! versão anterior — e vira o genérico na leitura.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// O formato do catálogo genérico.
pub const SCHEMA:u64=2;
/// O formato antigo, que esta versão ainda lê.
pub const LEGACY_SCHEMA:u64=1;

pub const CACHE:&str="
CREATE TABLE IF NOT EXISTS mod_catalog (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  mods TEXT NOT NULL,
  fetched_at TEXT NOT NULL
);";

/// Um mod como o catálogo o descreve. Os textos vêm nos dez idiomas; o resto
/// dos campos é lido com padrão, e campo desconhecido é ignorado — o
/// repositório pode acrescentar campos sem quebrar o app antigo.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Manifest {
    pub id:String,
    /// A versão do próprio mod.
    #[serde(default,skip_serializing_if="Option::is_none")] pub version:Option<String>,
    #[serde(default)] pub category:String,
    /// O recurso do plano que o mod pede; nulo, nenhum.
    #[serde(default)] pub plan:Option<String>,
    /// A versão do app a partir da qual o mod vale.
    #[serde(default)] pub min_app:Option<String>,
    /// Instalado, e ligado, no login de quem ainda não o tem; quem o
    /// desinstalou não o recebe de volta.
    #[serde(default)] pub preinstalled:bool,
    /// Essencial: o app precisa dele. Fica sempre instalado e ligado; ninguém
    /// o desliga nem o desinstala (o plano continua mandando no que ele traz).
    #[serde(default,skip_serializing_if="std::ops::Not::not")] pub required:bool,
    /// Os mods de que ele precisa instalados e ligados.
    #[serde(default,skip_serializing_if="Vec::is_empty")] pub requires:Vec<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub author:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub docs:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub homepage:Option<String>,
    /// O ícone do mod: o SVG que o catálogo traz, mostrado como imagem pela
    /// tela (nunca dentro da página). Nulo sem ícone ou com um que não é SVG.
    #[serde(default,skip_serializing_if="Option::is_none")] pub icon:Option<String>,
    #[serde(default)] pub title:BTreeMap<String,String>,
    #[serde(default)] pub summary:BTreeMap<String,String>,
    /// O que o mod traz para cada ponto de extensão, pela chave do ponto.
    #[serde(default)] pub contributes:BTreeMap<String,Value>,
}

/// O tamanho máximo do SVG de um ícone (o validador do repositório de mods
/// aceita até 16 kB).
pub const ICON_MAX:usize=16_000;

/// Os motores que vêm instalados de fábrica, para o catálogo antigo, que não
/// diz: os mesmos que o genérico marca `preinstalled`. Regra do dono: só os
/// essenciais se instalam sozinhos, e motor nenhum é essencial — quem usa
/// instala o seu (Claude Code, Codex, Ollama…) pelo catálogo.
const LEGACY_PREINSTALLED:[&str;0]=[];

/// O id de um mod: o nome da pasta no repositório.
pub fn valid_id(id:&str)->bool {
    let mut chars=id.chars();
    (2..=40).contains(&id.len())&&chars.next().is_some_and(|first|first.is_ascii_lowercase())&&chars.all(|char|char.is_ascii_alphanumeric()||char=='-')
}

/// O mod no formato antigo: um tipo (`llm`, `feature`, `template`) e o id do
/// motor ou a definição do mod pronto.
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct Legacy {
    id:String,
    kind:String,
    #[serde(default)] category:String,
    #[serde(default)] plan:Option<String>,
    #[serde(default)] default_on:Option<bool>,
    #[serde(default)] min_app:Option<String>,
    #[serde(default)] docs:Option<String>,
    #[serde(default)] homepage:Option<String>,
    #[serde(default)] title:BTreeMap<String,String>,
    #[serde(default)] summary:BTreeMap<String,String>,
    #[serde(default)] template:Option<Value>,
}

impl Manifest {
    /// Um mod do formato genérico, ou do antigo convertido. Nulo: não se lê,
    /// ou não traz nada a ponto nenhum.
    pub fn read(value:&Value)->Option<Self> {
        let mut manifest=if value.get("contributes").is_some() { serde_json::from_value::<Manifest>(value.clone()).ok()? } else { Self::from_legacy(value)? };
        manifest.icon=manifest.icon.filter(|svg|svg.len()<=ICON_MAX&&svg.starts_with("<svg")&&svg.ends_with("</svg>"));
        (valid_id(&manifest.id)&&!manifest.contributes.is_empty()).then_some(manifest)
    }

    /// O mod do formato antigo como um mod genérico: o motor de LLM, o recurso
    /// ou o agente do mod pronto. Os recursos ligados de fábrica se instalam
    /// sozinhos, como no app antigo; dos motores, só os de
    /// `LEGACY_PREINSTALLED` (hoje nenhum): o formato antigo não sabe dizer
    /// quais são.
    fn from_legacy(value:&Value)->Option<Self> {
        let legacy:Legacy=serde_json::from_value(value.clone()).ok()?;
        let (point,contribution,preinstalled)=match legacy.kind.as_str() {
            "llm"=>("llmEngines",json!([legacy.id]),LEGACY_PREINSTALLED.contains(&legacy.id.as_str())),
            "feature"=>("features",json!([legacy.id]),legacy.default_on.unwrap_or(true)),
            "template"=>("agents",json!([legacy.template?]),false),
            _=>return None,
        };
        Some(Self{
            id:legacy.id,version:None,category:legacy.category,plan:legacy.plan,min_app:legacy.min_app,preinstalled,required:false,requires:vec![],author:None,
            docs:legacy.docs,homepage:legacy.homepage,icon:None,title:legacy.title,summary:legacy.summary,contributes:BTreeMap::from([(point.to_string(),contribution)]),
        })
    }
}

/// A versão do app, dita na partida (`set_app_version`). Sem ela (os testes),
/// todo mod vale.
static APP_VERSION:OnceLock<String>=OnceLock::new();

pub fn set_app_version(version:&str) { let _=APP_VERSION.set(version.to_string()); }

pub fn app_version()->Option<&'static str> { APP_VERSION.get().map(String::as_str) }

fn parts(version:&str)->[u64;3] {
    let mut parts=[0;3];
    for (slot,part) in parts.iter_mut().zip(version.split('-').next().unwrap_or_default().split('.')) { *slot=part.parse().unwrap_or(0); }
    parts
}

/// A versão `version` é no mínimo `minimum`.
pub fn at_least(version:&str,minimum:&str)->bool { parts(minimum)<=parts(version) }

/// O mod vale nesta versão do app: sem `minApp`, ou com ela até a instalada.
pub fn fits(manifest:&Manifest,version:Option<&str>)->bool {
    match (manifest.min_app.as_deref(),version) {
        (Some(min),Some(current))=>at_least(current,min),
        _=>true,
    }
}

/// O catálogo que desceu, se esta versão o entende: um dos dois formatos e uma
/// lista de mods. Cada mod que não se lê sai; o resto fica. O primeiro de dois
/// mods com o mesmo id fica.
pub fn parse(value:&Value)->Option<Vec<Manifest>> {
    let schema=value.get("schema").and_then(Value::as_u64)?;
    if schema!=SCHEMA&&schema!=LEGACY_SCHEMA { return None; }
    let mods=value.get("mods")?.as_array()?;
    let mut seen=BTreeSet::new();
    Some(mods.iter().filter_map(|value|{
        // Cada formato só lê o seu: um mod sem `contributes` no genérico, ou
        // com ele no antigo, está quebrado.
        let generic=value.get("contributes").is_some();
        if generic!=(schema==SCHEMA) { return None; }
        Manifest::read(value)
    }).filter(|manifest|seen.insert(manifest.id.clone())).collect())
}

/// Troca o cache pelo catálogo que desceu. Falso, sem mexer no cache, quando
/// esta versão não o entende.
pub fn replace(connection:&Connection,value:&Value)->Result<bool> {
    let Some(mods)=parse(value) else { return Ok(false) };
    connection.execute("INSERT OR REPLACE INTO mod_catalog(id,mods,fetched_at) VALUES(1,?1,?2)",params![serde_json::to_string(&mods)?,chrono::Utc::now().to_rfc3339()])?;
    Ok(true)
}

/// O catálogo em cache e quando desceu. Nulo: nenhum desceu ainda. O cache
/// gravado pela versão anterior (o formato antigo) vira o genérico aqui.
pub fn cached(connection:&Connection)->Result<Option<(Vec<Manifest>,String)>> {
    let raw:Option<(String,String)>=connection.query_row("SELECT mods,fetched_at FROM mod_catalog WHERE id=1",[],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    Ok(raw.and_then(|(mods,at)|{
        let values:Vec<Value>=serde_json::from_str(&mods).ok()?;
        Some((values.iter().filter_map(Manifest::read).collect(),at))
    }))
}

#[cfg(test)] mod tests {
    use super::*;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(CACHE).expect("cache");
        connection
    }

    fn generic(id:&str,contributes:Value)->Value { json!({"id":id,"version":"1.0.0","category":"project","plan":null,"minApp":"0.95.0","preinstalled":true,"title":{"en":id},"summary":{"en":id},"contributes":contributes}) }

    #[test] fn both_formats_are_read_and_the_old_one_becomes_generic() {
        let mut essential=generic("stats",json!({"features":["stats"]})); essential["required"]=json!(true);
        let new=parse(&json!({"schema":2,"mods":[essential,generic("pack",json!({"features":["stats","mcp"],"agents":[{"kind":"api"}]}))]})).expect("v2");
        assert!(new[0].required&&!new[1].required,"o essencial vem dito no manifesto");
        assert_eq!(new.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["stats","pack"]);
        assert_eq!(new[1].contributes.len(),2,"um mod traz vários pontos");
        let old=parse(&json!({"schema":1,"mods":[
            {"id":"claude","kind":"llm","minApp":"0.94.0","title":{"en":"Claude"}},
            {"id":"copilot","kind":"llm"},
            {"id":"stats","kind":"feature","defaultOn":false},
            {"id":"ollama","kind":"template","template":{"kind":"api","baseUrl":"http://localhost:11434/v1"}},
            {"id":"x","kind":"plugin"}
        ]})).expect("v1");
        assert_eq!(old.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["claude","copilot","stats","ollama"],"o tipo desconhecido e o id curto saem");
        assert_eq!(old[0].contributes["llmEngines"],json!(["claude"]));
        assert!(old.iter().filter(|manifest|!manifest.contributes.contains_key("features")).all(|manifest|!manifest.preinstalled),"motor e agente nenhum se instalam sozinhos: só os essenciais");
        assert!(!old[2].preinstalled,"o recurso desligado no catálogo antigo também não");
        assert_eq!(old[3].contributes["agents"][0]["baseUrl"],"http://localhost:11434/v1");
    }

    /// O ícone chega como o SVG do catálogo; o que não é SVG, ou passa do
    /// tamanho, fica de fora sem derrubar o mod.
    #[test] fn the_icon_is_kept_only_when_it_is_an_svg() {
        let mut with_icon=generic("stats",json!({"features":["stats"]}));
        with_icon["icon"]=json!(r#"<svg viewBox="0 0 24 24"><path d="M0 0"/></svg>"#);
        let mut broken=generic("mcp",json!({"features":["mcp"]}));
        broken["icon"]=json!("icon.svg");
        let mut huge=generic("skills",json!({"features":["skills"]}));
        huge["icon"]=json!(format!("<svg>{}</svg>","x".repeat(ICON_MAX)));
        let read=parse(&json!({"schema":2,"mods":[with_icon,broken,huge]})).expect("lê");
        assert!(read[0].icon.as_deref().is_some_and(|svg|svg.starts_with("<svg")));
        assert!(read[1].icon.is_none()&&read[2].icon.is_none());
        assert_eq!(read.len(),3,"o mod continua");
        assert!(serde_json::to_value(&read[0]).expect("json")["icon"].is_string(),"a tela recebe o ícone");
    }

    #[test] fn an_unknown_schema_or_a_mixed_catalog_is_not_read() {
        assert!(parse(&json!({"schema":3,"mods":[]})).is_none());
        assert!(parse(&json!({"mods":[]})).is_none());
        assert!(parse(&json!("x")).is_none());
        let mixed=parse(&json!({"schema":2,"mods":[{"id":"stats","kind":"feature"},generic("mcp",json!({"features":["mcp"]})),generic("empty",json!({}))]})).expect("lê");
        assert_eq!(mixed.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["mcp"],"o mod do outro formato e o que não traz nada saem");
    }

    #[test] fn the_cache_keeps_the_last_catalog_this_version_understood() {
        let connection=database();
        assert!(cached(&connection).expect("lê").is_none());
        assert!(replace(&connection,&json!({"schema":2,"mods":[generic("stats",json!({"features":["stats"]}))]})).expect("grava"));
        assert!(!replace(&connection,&json!({"schema":9,"mods":[]})).expect("formato novo"));
        assert_eq!(cached(&connection).expect("lê").expect("cache").0[0].id,"stats");
        // O cache da versão anterior guardava os mods do formato antigo.
        connection.execute("UPDATE mod_catalog SET mods=?1",[r#"[{"id":"mcp","kind":"feature","defaultOn":true}]"#]).expect("cache antigo");
        let (mods,_)=cached(&connection).expect("lê").expect("cache");
        assert_eq!((mods[0].id.as_str(),mods[0].preinstalled,mods[0].contributes["features"].clone()),("mcp",true,json!(["mcp"])));
    }

    #[test] fn versions_compare_by_number() {
        let mut manifest=Manifest::read(&generic("stats",json!({"features":["stats"]}))).expect("lê");
        assert!(fits(&manifest,Some("0.95.0"))&&fits(&manifest,Some("1.0.0-beta"))&&!fits(&manifest,Some("0.94.9")));
        assert!(fits(&manifest,None),"sem versão dita, vale");
        manifest.min_app=None;
        assert!(fits(&manifest,Some("0.1.0")));
        assert!(at_least("0.10.0","0.9.0"));
    }
}
