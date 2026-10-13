//! Os mods no núcleo do app: o registro dos pontos de extensão que esta versão
//! sabe aplicar e o que eles valem junto do plano.
//!
//! O sistema em si (`jayv-mods`) não sabe o que um mod faz: cada camada dá os
//! pontos do assunto dela. Aqui moram os dois que o plano governa — os motores
//! de LLM do app (`llmEngines`) e os recursos opcionais (`features`) — e o
//! registro junta o de agentes (`jayv_agents::mod_agents`). Um tipo novo de
//! funcionalidade que os mods podem trazer é um ponto novo nesta lista.
//!
//! O que os mods desligam (`disabled_in`) entra no `Entitlements` como o que o
//! plano não tem: o recurso, o motor ou o agente que nenhum mod ativo traz.
//! Sem catálogo nenhum (offline desde a instalação, servidor sem a função),
//! nada sai do ar por falta dele: vale só a escolha de quem usa.

use crate::llm::AgentId;
use anyhow::Result;
use jayv_mods::resolve::{self, Applied};
use jayv_mods::{catalog, state, ExtensionPoint, InstalledMods, Manifest, Registry};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;

pub use jayv_mods::catalog::{app_version, set_app_version};

/// Os recursos opcionais que viraram mods: os do plano que quem usa pode tirar
/// do app (o núcleo, a conta e as organizações ficam fora). A mesma lista do
/// `engines.json` do repositório de mods.
pub const FEATURE_MODS:[&str;15]=[
    "stats","gateBoard","liveFiles","chatSearch","projectNotes","secondOpinion","planFirst","parallelTasks",
    "answerRecall","leanCode","symbolIndex","mcp","skills","skillsHub","conversationFind",
];

/// Uma lista de ids que o app tem: o mod que nomeia um que esta versão não
/// tem espera um app mais novo.
fn listed(value:&Value,known:&[&str])->Option<Vec<String>> {
    let list=value.as_array()?;
    if list.is_empty() { return None; }
    list.iter().map(|item|item.as_str().filter(|id|known.contains(id)).map(str::to_string)).collect()
}

/// `llmEngines`: liga motores de integração com LLM do app. Desligado, o motor
/// sai do roteamento (`Entitlements::apply_llm`).
pub struct LlmEngines;
pub static LLM_ENGINES:LlmEngines=LlmEngines;

fn engines()->Vec<&'static str> { AgentId::ALL.iter().map(|agent|agent.key()).collect() }

impl ExtensionPoint for LlmEngines {
    fn key(&self)->&'static str { "llmEngines" }
    fn targets(&self,_manifest:&Manifest,value:&Value)->Option<Vec<String>> { listed(value,&engines()) }
    fn built_in(&self)->Vec<String> { engines().into_iter().map(str::to_string).collect() }
}

/// `features`: liga recursos opcionais do app. Desligado, o recurso vale como
/// fora do plano — menos o que o plano trava.
pub struct Features;
pub static FEATURES:Features=Features;

impl ExtensionPoint for Features {
    fn key(&self)->&'static str { "features" }
    fn targets(&self,_manifest:&Manifest,value:&Value)->Option<Vec<String>> { listed(value,&FEATURE_MODS) }
    fn built_in(&self)->Vec<String> { FEATURE_MODS.iter().map(|id|id.to_string()).collect() }
}

/// Os pontos de extensão desta versão do app.
pub static REGISTRY:Registry=Registry::new(&[&LLM_ENGINES,&FEATURES,&jayv_agents::mod_agents::AGENTS]);

/// As tabelas locais: o cache do catálogo e o registro do que esta máquina
/// instalou.
pub fn ensure(connection:&Connection)->Result<()> {
    connection.execute_batch(catalog::CACHE)?;
    connection.execute_batch(state::LEDGER)?;
    Ok(())
}

/// Troca o cache pelo catálogo que desceu. Falso quando esta versão não o
/// entende.
pub fn replace(connection:&Connection,value:&Value)->Result<bool> { catalog::replace(connection,value) }

/// O catálogo em cache e quando desceu.
pub fn cached(connection:&Connection)->Result<Option<(Vec<Manifest>,String)>> { catalog::cached(connection) }

/// O que os mods desligam neste banco.
pub fn disabled_in(connection:&Connection)->Result<BTreeSet<String>> { resolve::disabled_in(connection,&REGISTRY,app_version()) }

/// O que a aba Mods mostra.
#[derive(Debug,Clone,PartialEq)]
pub struct View {
    pub catalog:Option<Vec<Manifest>>,
    pub fetched_at:Option<String>,
    /// O estado que conta: o gravado mais o que vem de fábrica.
    pub state:InstalledMods,
    /// Os mods que valem agora.
    pub active:BTreeSet<String>,
    pub disabled:BTreeSet<String>,
}

pub fn view(connection:&Connection)->Result<View> {
    let cached=catalog::cached(connection)?;
    let (catalog,fetched_at)=match cached { Some((mods,at))=>(Some(mods),Some(at)), None=>(None,None) };
    let mods=catalog.as_deref().unwrap_or_default();
    let state=resolve::effective(state::stored(connection)?.as_ref(),mods,&state::legacy(connection)?);
    let active=resolve::active(&state,mods,app_version(),&REGISTRY);
    Ok(View{active,disabled:disabled_in(connection)?,catalog,fetched_at,state})
}

/// Grava o estado que a tela pediu, rodando nesta máquina o que instalar e
/// desinstalar pede. Uma falha volta como erro, sem gravar nada.
pub fn apply(connection:&mut Connection,wanted:InstalledMods)->Result<Applied> {
    let catalog=catalog::cached(connection)?.map(|(mods,_)|mods).unwrap_or_default();
    let before=resolve::effective(state::stored(connection)?.as_ref(),&catalog,&state::legacy(connection)?);
    resolve::apply(connection,&REGISTRY,&catalog,&before,wanted,app_version(),true)
}

/// A instalação sozinha, no fim de cada volta do sync. Nulo: nada mudou.
pub fn reconcile(connection:&mut Connection)->Result<Option<Applied>> { resolve::reconcile(connection,&REGISTRY,app_version()) }

/// Os agentes que pertencem a um mod instalado: o salvar das configurações não
/// os apaga por estarem fora de um rascunho velho — sai pelo desinstalar.
pub fn owned_agents(connection:&Connection)->Result<BTreeSet<String>> {
    let catalog=catalog::cached(connection)?.map(|(mods,_)|mods).unwrap_or_default();
    let state=resolve::effective(state::stored(connection)?.as_ref(),&catalog,&state::legacy(connection)?);
    let mut owned=BTreeSet::new();
    let ledger=state::ledger(connection)?;
    for manifest in catalog.iter().filter(|manifest|state.mods.contains_key(&manifest.id)).chain(ledger.values()) {
        if let Some(value)=manifest.contributes.get(jayv_agents::mod_agents::AGENTS.key()) {
            owned.extend(jayv_agents::mod_agents::AGENTS.targets(manifest,value).unwrap_or_default());
        }
    }
    Ok(owned)
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        ensure(&connection).expect("cache");
        crate::llm::ensure(&connection).expect("agentes");
        connection.execute_batch("CREATE TABLE account_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);").expect("conta");
        connection
    }

    fn generic(id:&str,contributes:Value,preinstalled:bool)->Value { json!({"id":id,"version":"1.0.0","category":"project","plan":null,"minApp":"0.94.0","preinstalled":preinstalled,"title":{"en":id},"summary":{"en":id},"contributes":contributes}) }

    /// Os mods de hoje, como o catálogo genérico os traz: cada recurso, cada
    /// motor e os dois mods de agentes, todos de fábrica.
    fn today()->Vec<Value> {
        let mut mods:Vec<Value>=engines().iter().map(|id|generic(id,json!({"llmEngines":[id]}),true)).collect();
        // Os recursos são essenciais; os motores e os agentes, não.
        mods.extend(FEATURE_MODS.iter().map(|id|{ let mut feature=generic(id,json!({"features":[id]}),true); feature["required"]=json!(true); feature }));
        mods.push(generic("ollama",json!({"agents":[{"kind":"api","protocol":"openai","baseUrl":"http://localhost:11434/v1","keyRequired":false,"models":[]}]}),true));
        mods.push(generic("lm-studio",json!({"agents":[{"kind":"api","protocol":"openai","baseUrl":"http://localhost:1234/v1","keyRequired":false,"models":[]}]}),true));
        mods
    }

    #[test] fn the_points_read_only_what_this_app_has() {
        let manifest=Manifest::read(&generic("pack",json!({"features":["stats","mcp"],"llmEngines":["claude"]}),false)).expect("mod");
        assert_eq!(REGISTRY.targets(&manifest),Some(BTreeSet::from(["claude".to_string(),"mcp".to_string(),"stats".to_string()])));
        let newer=Manifest::read(&generic("newer",json!({"features":["teleport"]}),false)).expect("mod");
        assert!(REGISTRY.targets(&newer).is_none(),"o recurso que este app não tem espera o app novo");
        assert_eq!(REGISTRY.keys(),["llmEngines","features","agents"]);
    }

    /// No login, os mods de hoje se instalam todos, ligados: os agentes do
    /// Ollama e do LM Studio passam a existir e nada sai do ar.
    #[test] fn signing_in_installs_every_mod_of_today() {
        let mut connection=database();
        assert!(replace(&connection,&json!({"schema":2,"mods":today()})).expect("catálogo"));
        let applied=reconcile(&mut connection).expect("aplica").expect("instalou");
        assert!(applied.failed.is_empty(),"{:?}",applied.failed);
        assert_eq!(applied.state.mods.len(),today().len());
        assert!(applied.state.mods.values().all(|installed|installed.enabled));
        let settings=crate::llm::load(&connection).expect("lê");
        for id in ["mod-ollama","mod-lm-studio"] { assert!(settings.agents.iter().any(|agent|agent.id.key()==id&&agent.enabled),"{id}"); }
        assert!(disabled_in(&connection).expect("lê").is_empty(),"tudo ligado");
        assert!(reconcile(&mut connection).expect("de novo").is_none());
        assert_eq!(owned_agents(&connection).expect("lê"),BTreeSet::from(["mod-lm-studio".to_string(),"mod-ollama".to_string()]));
        // Pela tela: o recurso essencial não desliga nem sai; o motor e o
        // agente, sim.
        let mut wanted=view(&connection).expect("lê").state;
        wanted.mods.get_mut("stats").expect("stats").enabled=false;
        wanted.mods.remove("mcp");
        wanted.mods.get_mut("codex").expect("codex").enabled=false;
        wanted.mods.remove("ollama");
        apply(&mut connection,wanted).expect("grava");
        let off=disabled_in(&connection).expect("lê");
        assert_eq!(off,BTreeSet::from(["codex".to_string(),"mod-ollama".to_string()]),"só o motor e o agente saem");
        assert!(!crate::llm::load(&connection).expect("lê").agents.iter().any(|agent|agent.id.key()=="mod-ollama"),"desinstalar apaga o agente");
        let state=view(&connection).expect("lê").state;
        assert!(state.removed.contains("ollama")&&!state.removed.contains("mcp")&&state.is_on("stats")&&state.is_on("mcp"));
        // O app 0.94 de outra máquina lê a chave antiga.
        assert_eq!(state::legacy(&connection).expect("lê").get("codex"),Some(&false));
        assert_eq!(state::legacy(&connection).expect("lê").get("stats"),Some(&true));
    }

    /// O motor que a pessoa desligou no app 0.94 continua desligado; o
    /// recurso, que agora é essencial, volta a ligar.
    #[test] fn the_choices_of_the_old_app_carry_over_except_for_essentials() {
        let mut connection=database();
        connection.execute("INSERT INTO account_settings VALUES('mods','{\"leanCode\":false,\"cursor\":false}','now')",[]).expect("escolha antiga");
        assert!(disabled_in(&connection).expect("lê").contains("leanCode"),"sem catálogo, vale a escolha");
        assert!(replace(&connection,&json!({"schema":2,"mods":today()})).expect("catálogo"));
        let applied=reconcile(&mut connection).expect("aplica").expect("instalou");
        assert!(applied.state.is_on("leanCode"),"o essencial liga");
        assert!(!applied.state.is_on("cursor")&&applied.state.mods.contains_key("cursor"),"o motor fica desligado");
        assert_eq!(disabled_in(&connection).expect("lê"),BTreeSet::from(["cursor".to_string()]));
    }
}
