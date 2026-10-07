//! Os recursos do plano de quem usa (`rpc/my_features`): o admin do sistema
//! liga e desliga cada recurso, escolhe em que planos ele entra e, em cada
//! plano, se ele é opcional ou travado ligado. A lista desce na volta do sync e
//! fica num cache local, fora da fila: só desce.
//!
//! Dois sentidos, sempre por cima das configurações de quem usa:
//! - o que não está no plano fica desligado (`restrict_*`), como a política de
//!   LLM — só aperta;
//! - o que é travado fica ligado (`enforce_*`): o núcleo (redação de
//!   segredos, arquivos sensíveis, sessões do agente, cache de contexto,
//!   roteamento adaptativo e as portarias) e o que o admin travou no plano.
//!
//! O núcleo vem também da lista embutida (`CORE`): sem cache, com o servidor
//! antigo ou com um cache ilegível, ele continua travado. Sem cache, o resto
//! vale como antes — quem já usava um recurso não o perde por estar offline.

use crate::core_settings::CoreSettings;
use crate::llm::{AgentId, ClaudeOptions, LlmSettings};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS plan_features (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  plan TEXT,
  features TEXT NOT NULL,
  fetched_at TEXT NOT NULL
);";

/// As chaves dos recursos que o núcleo aplica. As outras (páginas, painéis)
/// são só da tela.
pub const ADAPTIVE_ROUTING:&str="adaptiveRouting";
pub const SECOND_OPINION:&str="secondOpinion";
pub const PLAN_FIRST:&str="planFirst";
pub const PARALLEL_TASKS:&str="parallelTasks";
pub const SECRET_REDACTION:&str="secretRedaction";
pub const SENSITIVE_FILES:&str="sensitiveFiles";
pub const AGENT_SESSIONS:&str="agentSessions";
pub const CONTEXT_CACHE:&str="contextCache";
pub const ENTRY_GATE:&str="entryGate";
pub const EXIT_GATE:&str="exitGate";
pub const LIVE_FILES:&str="liveFiles";
pub const ANSWER_RECALL:&str="answerRecall";
pub const PROJECT_NOTES:&str="projectNotes";
pub const SYMBOL_INDEX:&str="symbolIndex";
pub const LEAN_CODE:&str="leanCode";
pub const MCP:&str="mcp";
pub const SKILLS:&str="skills";
pub const KILO_CODE:&str="kiloCode";
pub const GATEWAY_PROVIDERS:&str="gatewayProviders";

/// O núcleo: o que nenhum plano tira e ninguém desliga. A mesma lista da
/// migração `core_features` (`features.core`).
pub const CORE:[&str;7]=[ENTRY_GATE,EXIT_GATE,SECRET_REDACTION,SENSITIVE_FILES,AGENT_SESSIONS,CONTEXT_CACHE,ADAPTIVE_ROUTING];

/// Os arquivos que nunca vão para o contexto com `sensitiveFiles` travado.
/// Quem usa acrescenta; tirar, não tira.
pub const SENSITIVE_PATTERNS:[&str;9]=[".env",".env.*","*.pem","*.key","*.p12","*.pfx","*.secret",".ssh/**","secrets/**"];

/// O cache de contexto mais curto que o núcleo aceita, em segundos.
/// O teto de pedidos ao mesmo tempo, o mesmo do banco (`plans.max_concurrent_turns`).
pub const MAX_CONCURRENT_TURNS:u32=8;
pub const MIN_CACHE_TTL:u64=300;

pub use crate::cloud::remote::{PlanLimits, RemoteFeatures};

/// Os recursos que valem agora. `known` nulo: nenhuma lista desceu ainda, e o
/// que não é núcleo vale como antes.
#[derive(Debug,Clone,PartialEq)]
pub struct Entitlements { known:Option<BTreeSet<String>>, locked:BTreeSet<String>, defaults:BTreeMap<String,bool>, limits:PlanLimits }

impl Default for Entitlements {
    fn default()->Self { Self{known:None,locked:core(),defaults:BTreeMap::new(),limits:PlanLimits::default()} }
}

fn core()->BTreeSet<String> { CORE.iter().map(|key|key.to_string()).collect() }

impl Entitlements {
    pub fn of(features:&[String])->Self { Self{known:Some(features.iter().cloned().chain(core()).collect()),..Self::default()} }

    /// O que o servidor devolveu, com o núcleo garantido.
    pub fn from_remote(remote:&RemoteFeatures)->Self {
        let mut entitlements=Self::of(&remote.features);
        entitlements.locked.extend(remote.locked.iter().cloned());
        if let Some(known)=&mut entitlements.known { known.extend(remote.locked.iter().cloned()); }
        entitlements.defaults=remote.defaults.clone();
        entitlements.limits=remote.limits.clone();
        entitlements
    }

    pub fn allows(&self,feature:&str)->bool { self.locks(feature)||self.known.as_ref().is_none_or(|known|known.contains(feature)) }

    /// Ligado sem interruptor.
    pub fn locks(&self,feature:&str)->bool { self.locked.contains(feature) }

    pub fn limits(&self)->&PlanLimits { &self.limits }

    /// Quantos pedidos de chats diferentes correm ao mesmo tempo: o número do
    /// plano, entre 1 e 8; sem plano (ou sem a migração), um de cada vez.
    pub fn concurrent_turns(&self)->usize { self.limits.max_concurrent_turns.map_or(1,|turns|turns.clamp(1,MAX_CONCURRENT_TURNS) as usize) }

    /// As configurações do Jev sem os recursos que o plano não tem.
    pub fn restrict_core(&self,settings:&CoreSettings)->CoreSettings {
        let mut settings=settings.clone();
        settings.adaptive_routing&=self.allows(ADAPTIVE_ROUTING);
        settings.review_changes&=self.allows(SECOND_OPINION);
        settings.plan_first&=self.allows(PLAN_FIRST);
        settings.parallel_tasks&=self.allows(PARALLEL_TASKS);
        settings
    }

    /// As configurações do Jev com o que é travado ligado. Roda por último,
    /// depois do plano e da política da organização: nada abaixo dele afrouxa.
    pub fn enforce_core(&self,settings:&CoreSettings)->CoreSettings {
        let mut settings=settings.clone();
        if self.locks(SECRET_REDACTION) { settings.privacy.redact_secrets=true; }
        if self.locks(SENSITIVE_FILES) {
            for pattern in SENSITIVE_PATTERNS { if !settings.privacy.deny.iter().any(|known|known==pattern) { settings.privacy.deny.push(pattern.to_string()); } }
        }
        if self.locks(CONTEXT_CACHE) { settings.cache_ttl=settings.cache_ttl.max(MIN_CACHE_TTL); }
        if self.locks(AGENT_SESSIONS) { settings.keep_session_model=true; }
        if self.locks(ADAPTIVE_ROUTING) { settings.adaptive_routing=true; }
        if self.locks(SECOND_OPINION) { settings.review_changes=true; }
        if self.locks(PLAN_FIRST) { settings.plan_first=true; }
        if self.locks(PARALLEL_TASKS) { settings.parallel_tasks=true; }
        settings
    }

    /// As duas coisas, na ordem certa: tira o que o plano não tem e liga o
    /// que ele trava.
    pub fn apply_core(&self,settings:&CoreSettings)->CoreSettings { self.enforce_core(&self.restrict_core(settings)) }

    /// Os agentes com o que o plano tira desligado e o que ele trava ligado:
    /// o Kilo Code e os gateways de API (OpenRouter e LiteLLM) que o plano não
    /// tem saem do roteamento, e do Claude, as sessões guardadas e o índice de
    /// símbolos.
    pub fn apply_llm(&self,settings:&LlmSettings)->LlmSettings {
        let mut settings=settings.clone();
        for agent in settings.agents.iter_mut() {
            let included=match agent.id { AgentId::Kilo=>self.allows(KILO_CODE), AgentId::Openrouter|AgentId::Litellm=>self.allows(GATEWAY_PROVIDERS), _=>true };
            if !included { agent.enabled=false; }
        }
        for agent in settings.agents.iter_mut().filter(|agent|agent.id==AgentId::Claude) {
            let Ok(mut options)=serde_json::from_value::<ClaudeOptions>(agent.options.clone()) else { continue };
            if self.locks(AGENT_SESSIONS) { options.persist_sessions=true; }
            if !self.allows(SYMBOL_INDEX) { options.symbol_tools=false; }
            if self.locks(SYMBOL_INDEX) { options.symbol_tools=true; }
            if let Ok(value)=serde_json::to_value(options) { agent.options=value; }
        }
        settings
    }

    /// Os servidores MCP só chegam aos agentes com o recurso no plano.
    pub fn mcp_servers<T>(&self,servers:Vec<T>)->Vec<T> { if self.allows(MCP) { servers } else { vec![] } }

    /// As skills só chegam ao Jev com o recurso no plano.
    pub fn skills<T>(&self,skills:Vec<T>)->Vec<T> { if self.allows(SKILLS) { skills } else { vec![] } }

    /// O código enxuto da conta, pelo plano.
    pub fn lean_code(&self,mine:bool)->bool { self.locks(LEAN_CODE)||(mine&&self.allows(LEAN_CODE)) }

    /// Os valores de partida do plano, para quem nunca gravou as
    /// configurações do Jev.
    pub fn seed_core(&self,defaults:&CoreSettings)->CoreSettings {
        let mut defaults=defaults.clone();
        if let Some(&on)=self.defaults.get(SECOND_OPINION) { defaults.review_changes=on; }
        if let Some(&on)=self.defaults.get(PLAN_FIRST) { defaults.plan_first=on; }
        if let Some(&on)=self.defaults.get(PARALLEL_TASKS) { defaults.parallel_tasks=on; }
        defaults
    }
}

/// O que o servidor devolveu além da lista, guardado ao lado dela.
#[derive(Debug,Clone,Default,serde::Serialize,serde::Deserialize)]
#[serde(default)]
struct Extra { locked:Vec<String>, defaults:BTreeMap<String,bool>, limits:PlanLimits }

/// A coluna nova do cache, no banco que já existia.
pub fn ensure(connection:&Connection)->Result<()> {
    connection.execute_batch(SCHEMA)?;
    let present:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('plan_features') WHERE name='extra')",[],|row|row.get(0))?;
    if !present { connection.execute("ALTER TABLE plan_features ADD COLUMN extra TEXT NOT NULL DEFAULT '{}'",[])?; }
    Ok(())
}

pub fn replace(connection:&Connection,remote:&RemoteFeatures)->Result<()> {
    let extra=Extra{locked:remote.locked.clone(),defaults:remote.defaults.clone(),limits:remote.limits.clone()};
    connection.execute(
        "INSERT OR REPLACE INTO plan_features(id,plan,features,fetched_at,extra) VALUES(1,?1,?2,?3,?4)",
        params![remote.plan,serde_json::to_string(&remote.features)?,chrono::Utc::now().to_rfc3339(),serde_json::to_string(&extra)?],
    )?;
    Ok(())
}

/// O cache. Uma lista que esta versão não consegue ler vale como vazia: um
/// formato novo não pode liberar o que o plano não tem. O núcleo vale sempre.
pub fn load(connection:&Connection)->Result<Entitlements> {
    let raw:Option<(String,String)>=connection.query_row("SELECT features,extra FROM plan_features WHERE id=1",[],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    Ok(match raw {
        None=>Entitlements::default(),
        Some((features,extra))=>{
            let extra=serde_json::from_str::<Extra>(&extra).unwrap_or_default();
            Entitlements::from_remote(&RemoteFeatures{plan:None,features:serde_json::from_str::<Vec<String>>(&features).unwrap_or_default(),locked:extra.locked,defaults:extra.defaults,limits:extra.limits})
        }
    })
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::config::Config;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        ensure(&connection).expect("cache");
        connection
    }

    fn remote(features:&[&str])->RemoteFeatures { RemoteFeatures{plan:None,features:features.iter().map(|key|key.to_string()).collect(),..Default::default()} }

    fn everything_on()->CoreSettings {
        let mut settings=CoreSettings::from_config(&Config::default());
        settings.adaptive_routing=true; settings.review_changes=true; settings.plan_first=true; settings.parallel_tasks=true;
        settings
    }

    fn everything_off()->CoreSettings {
        let mut settings=CoreSettings::from_config(&Config::default());
        settings.adaptive_routing=false; settings.keep_session_model=false; settings.cache_ttl=0;
        settings.privacy.redact_secrets=false; settings.privacy.deny=vec![];
        settings
    }

    /// Quantos pedidos ao mesmo tempo: o número do plano, entre 1 e 8; sem
    /// número, um de cada vez.
    #[test] fn the_plan_says_how_many_turns_run_at_once() {
        let with=|turns:Option<u32>|Entitlements::from_remote(&RemoteFeatures{limits:crate::cloud::remote::PlanLimits{max_concurrent_turns:turns,..Default::default()},..remote(&[])});
        assert_eq!(Entitlements::default().concurrent_turns(),1);
        assert_eq!(with(None).concurrent_turns(),1);
        assert_eq!(with(Some(3)).concurrent_turns(),3);
        assert_eq!(with(Some(0)).concurrent_turns(),1);
        assert_eq!(with(Some(50)).concurrent_turns(),8);
    }

    #[test] fn without_a_list_everything_is_allowed() {
        let connection=database();
        let entitlements=load(&connection).expect("lê");
        assert!(entitlements.allows(SECOND_OPINION));
        assert_eq!(entitlements.restrict_core(&everything_on()),everything_on());
    }

    #[test] fn features_outside_the_plan_are_switched_off() {
        let connection=database();
        replace(&connection,&RemoteFeatures{plan:Some("free".into()),..remote(&[PLAN_FIRST,"stats"])}).expect("grava");
        let restricted=load(&connection).expect("lê").restrict_core(&everything_on());
        assert!(restricted.plan_first);
        assert!(!restricted.review_changes && !restricted.parallel_tasks);
        assert!(restricted.adaptive_routing,"o roteamento adaptativo é núcleo: nenhum plano o tira");
    }

    #[test] fn the_plan_never_turns_on_what_the_user_turned_off() {
        let mut mine=everything_on(); mine.plan_first=false;
        let restricted=Entitlements::of(&[PLAN_FIRST.into()]).restrict_core(&mine);
        assert!(!restricted.plan_first);
    }

    #[test] fn the_cache_is_replaced_and_an_unreadable_one_allows_nothing() {
        let connection=database();
        replace(&connection,&remote(&[SECOND_OPINION])).expect("grava");
        replace(&connection,&remote(&[])).expect("troca");
        assert!(!load(&connection).expect("lê").allows(SECOND_OPINION),"a lista nova vale inteira");
        connection.execute("UPDATE plan_features SET features='{\"x\":1}'",[]).expect("estraga");
        assert!(!load(&connection).expect("lê").allows(PLAN_FIRST));
        assert!(load(&connection).expect("lê").locks(SECRET_REDACTION),"o núcleo vale mesmo com o cache ilegível");
    }

    #[test] fn the_rpc_answer_is_read_with_extra_fields() {
        let remote:RemoteFeatures=serde_json::from_str(r#"{"plan":"pro","admin":true,"features":["stats"]}"#).expect("lê");
        assert_eq!(remote,RemoteFeatures{plan:Some("pro".into()),features:vec!["stats".into()],..Default::default()});
        let newer:RemoteFeatures=serde_json::from_str(r#"{"plan":"team","features":["stats"],"locked":["parallelTasks"],"defaults":{"planFirst":false},"limits":{"jevDailyLimit":2000,"maxConcurrentTurns":3}}"#).expect("lê");
        assert_eq!(newer.limits,PlanLimits{jev_daily_limit:Some(2000),max_concurrent_turns:Some(3)});
        assert_eq!(newer.defaults.get(PLAN_FIRST),Some(&false));
    }

    /// O núcleo fica ligado por cima do que quem usa desligou — com o cache,
    /// sem ele e com o servidor antigo, que não manda `locked`.
    #[test] fn the_core_stays_on_whatever_the_user_saved() {
        for entitlements in [Entitlements::default(),Entitlements::of(&["stats".into()]),Entitlements::from_remote(&remote(&[]))] {
            let enforced=entitlements.apply_core(&everything_off());
            assert!(enforced.privacy.redact_secrets);
            assert!(enforced.adaptive_routing);
            assert!(enforced.keep_session_model);
            assert_eq!(enforced.cache_ttl,MIN_CACHE_TTL);
            for pattern in SENSITIVE_PATTERNS { assert!(enforced.privacy.deny.iter().any(|known|known==pattern),"{pattern}"); }
        }
        let mut mine=everything_off(); mine.privacy.deny=vec!["*.sqlite".into(),".env".into()]; mine.cache_ttl=7_200;
        let enforced=Entitlements::default().apply_core(&mine);
        assert_eq!(enforced.privacy.deny.iter().filter(|pattern|*pattern==".env").count(),1,"sem repetir");
        assert!(enforced.privacy.deny.contains(&"*.sqlite".to_string()),"o que quem usa acrescentou fica");
        assert_eq!(enforced.cache_ttl,7_200,"acima do mínimo, vale o de quem usa");
    }

    /// O admin trava um opcional ligado num plano: ele liga mesmo que quem
    /// usa tenha desligado; o que o plano não tem continua desligado.
    #[test] fn a_feature_the_admin_locked_is_on() {
        let team=Entitlements::from_remote(&RemoteFeatures{locked:vec![PARALLEL_TASKS.into(),LEAN_CODE.into()],..remote(&[SECOND_OPINION])});
        let mut mine=everything_on(); mine.parallel_tasks=false; mine.review_changes=false;
        let applied=team.apply_core(&mine);
        assert!(applied.parallel_tasks,"travado pelo admin");
        assert!(!applied.review_changes,"opcional: vale o de quem usa");
        assert!(!applied.plan_first,"fora do plano");
        assert!(team.lean_code(false),"código enxuto travado");
        assert!(!Entitlements::of(&[]).lean_code(true),"fora do plano, desligado");
    }

    /// As sessões do Claude ficam guardadas com `agentSessions`; o índice de
    /// símbolos segue o plano.
    #[test] fn the_claude_options_follow_the_plan() {
        let mut settings=LlmSettings{agents:vec![crate::llm::AgentSettings{id:AgentId::Claude,enabled:true,command:"claude".into(),timeout:300,options:serde_json::json!({"persistSessions":false,"symbolTools":true})}],models:vec![]};
        let claude=|settings:&LlmSettings|serde_json::from_value::<ClaudeOptions>(settings.agents[0].options.clone()).expect("opções");
        let applied=Entitlements::of(&[]).apply_llm(&settings);
        assert!(claude(&applied).persist_sessions,"sessões são núcleo");
        assert!(!claude(&applied).symbol_tools,"fora do plano");
        settings.agents[0].options=serde_json::json!({"symbolTools":false});
        let locked=Entitlements::from_remote(&RemoteFeatures{locked:vec![SYMBOL_INDEX.into()],..remote(&[])});
        assert!(claude(&locked.apply_llm(&settings)).symbol_tools,"travado ligado");
    }

    /// Kilo Code, gateways, MCP e skills seguem o plano; sem lista, valem como antes.
    #[test] fn the_plan_gates_agents_mcp_and_skills() {
        let agent=|id|crate::llm::AgentSettings{id,enabled:true,command:String::new(),timeout:300,options:serde_json::json!({})};
        let settings=LlmSettings{agents:vec![agent(AgentId::Claude),agent(AgentId::Kilo),agent(AgentId::Openrouter),agent(AgentId::Litellm)],models:vec![]};
        let enabled=|settings:&LlmSettings|settings.agents.iter().map(|agent|agent.enabled).collect::<Vec<_>>();
        assert_eq!(enabled(&Entitlements::default().apply_llm(&settings)),[true,true,true,true]);
        let without=Entitlements::of(&[]);
        assert_eq!(enabled(&without.apply_llm(&settings)),[true,false,false,false]);
        assert_eq!(enabled(&Entitlements::of(&[KILO_CODE.into()]).apply_llm(&settings)),[true,true,false,false]);
        assert_eq!(enabled(&Entitlements::of(&[GATEWAY_PROVIDERS.into()]).apply_llm(&settings)),[true,false,true,true]);
        assert!(without.mcp_servers(vec![1]).is_empty()&&without.skills(vec![1]).is_empty());
        assert_eq!(Entitlements::of(&[MCP.into(),SKILLS.into()]).mcp_servers(vec![1]),vec![1]);
        assert_eq!(Entitlements::default().skills(vec![1]),vec![1]);
    }

    /// O valor de partida do plano vale para quem nunca gravou.
    #[test] fn the_plan_seeds_the_starting_values() {
        let plan=Entitlements::from_remote(&RemoteFeatures{defaults:BTreeMap::from([(SECOND_OPINION.to_string(),true),(PLAN_FIRST.to_string(),false)]),..remote(&[SECOND_OPINION,PLAN_FIRST])});
        let seeded=plan.seed_core(&CoreSettings::from_config(&Config::default()));
        assert!(seeded.review_changes&&!seeded.plan_first);
    }

    /// O cache que veio antes desta versão (sem a coluna `extra`) ganha a
    /// coluna e continua lido.
    #[test] fn an_old_cache_gains_the_extra_column() {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(SCHEMA).expect("cache antigo");
        connection.execute("INSERT INTO plan_features(id,plan,features,fetched_at) VALUES(1,'free','[\"stats\"]','2026-10-01T00:00:00Z')",[]).expect("linha antiga");
        ensure(&connection).expect("coluna nova");
        let entitlements=load(&connection).expect("lê");
        assert!(entitlements.allows("stats")&&!entitlements.allows(SECOND_OPINION));
        assert!(entitlements.locks(AGENT_SESSIONS));
    }
}
