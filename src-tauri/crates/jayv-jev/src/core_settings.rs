//! O que o desenvolvedor ajusta no Jev e no aplicativo pela tela de
//! configurações: roteamento, orçamentos, cache, regras da portaria de saída e
//! privacidade. Mora no banco do usuário, ao lado dos agentes; o `config.yaml`,
//! quando existe, só dá os valores de partida.

use crate::config::Config;
use crate::i18n::Text;
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY:&str="core_settings";

pub const COMPLEXITIES:[&str;4]=["trivial","simple","medium","complex"];
pub const PERMISSIONS:[&str;3]=["allow","ask","deny"];
pub const CONFIDENCE_RANGE:(f64,f64)=(0.5,0.95);
pub const BUDGET_RANGE:(usize,usize)=(500,200_000);
pub const CACHE_TTL_RANGE:(u64,u64)=(0,86_400);
pub const TURN_CEILING_RANGE:(u64,u64)=(5,240);
const PATTERN_MAX:usize=200;

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ExitRules { pub read:String, pub write:String, pub shell:String }

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Privacy { pub deny:Vec<String>, pub local_only:Vec<String>, pub redact_secrets:bool }

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct CoreSettings {
    /// O histórico de desempenho de cada modelo pesa na escolha.
    pub adaptive_routing:bool,
    /// Abaixo desta confiança o Jev amplia o orçamento e pede esclarecimento.
    pub confidence_threshold:f64,
    /// Modelos gratuitos (locais) ganham preferência no roteamento.
    pub prefer_local:bool,
    /// Tokens de contexto por complexidade (`trivial`…`complex`).
    pub budgets:BTreeMap<String,usize>,
    /// Quanto tempo, em segundos, um contexto montado é reaproveitado.
    pub cache_ttl:u64,
    pub exit_rules:ExitRules,
    pub privacy:Privacy,
    /// Os agentes na ordem de preferência para desempatar. Vazia, os
    /// empatados se espalham entre os chats.
    #[serde(default)] pub agent_order:Vec<String>,
    /// Um agente de outro provedor revisa o que o modo build mudou.
    #[serde(default)] pub review_changes:bool,
    /// Num pedido complexo do modo build, um modelo de raciocínio planeja
    /// antes de o agente construir.
    #[serde(default)] pub plan_first:bool,
    /// Num pedido complexo do modo build, partes do pedido vão a agentes
    /// diferentes ao mesmo tempo.
    #[serde(default)] pub parallel_tasks:bool,
    /// Num chat com sessão viva, o modelo da sessão fica (ligado por padrão).
    #[serde(default="keep_by_default")] pub keep_session_model:bool,
    /// A sessão do agente atravessa a troca entre planejamento e build.
    #[serde(default)] pub resume_across_modes:bool,
    /// O teto total de um pedido, em minutos (30 por padrão).
    #[serde(default="crate::config::default_turn_ceiling")] pub turn_ceiling_minutes:u64,
}

fn keep_by_default()->bool { true }

impl CoreSettings {
    /// Os valores de partida: os do `config.yaml` ou, sem ele, os padrões.
    pub fn from_config(config:&Config)->Self {
        Self{
            adaptive_routing:config.jev.adaptive_routing.enabled,
            confidence_threshold:config.jev.adaptive_routing.confidence_threshold,
            prefer_local:config.jev.optimization.prefer_local,
            budgets:COMPLEXITIES.iter().map(|level|(level.to_string(),config.budgets.get(*level).copied().unwrap_or(12_000))).collect(),
            cache_ttl:config.jev.context.cache_ttl,
            exit_rules:ExitRules{read:config.permissions.read.clone(),write:config.permissions.write.clone(),shell:config.permissions.shell.clone()},
            privacy:Privacy{deny:config.privacy.deny.clone(),local_only:config.privacy.local_only.clone(),redact_secrets:config.privacy.redact_secrets},
            agent_order:config.jev.agent_order.clone(),
            review_changes:config.jev.review_changes,
            plan_first:config.jev.plan_first,
            parallel_tasks:config.jev.parallel_tasks,
            keep_session_model:config.jev.keep_session_model,
            resume_across_modes:config.jev.resume_across_modes,
            turn_ceiling_minutes:config.jev.turn_ceiling_minutes,
        }
    }

    /// Escreve estes valores por cima da configuração do orquestrador.
    pub fn apply(&self,config:&mut Config) {
        config.jev.adaptive_routing.enabled=self.adaptive_routing;
        config.jev.adaptive_routing.confidence_threshold=self.confidence_threshold;
        config.jev.optimization.prefer_local=self.prefer_local;
        for (level,budget) in &self.budgets { config.budgets.insert(level.clone(),*budget); }
        config.jev.context.cache_ttl=self.cache_ttl;
        config.permissions.read=self.exit_rules.read.clone();
        config.permissions.write=self.exit_rules.write.clone();
        config.permissions.shell=self.exit_rules.shell.clone();
        config.privacy.deny=self.privacy.deny.clone();
        config.privacy.local_only=self.privacy.local_only.clone();
        config.privacy.redact_secrets=self.privacy.redact_secrets;
        config.jev.agent_order=self.agent_order.clone();
        config.jev.review_changes=self.review_changes;
        config.jev.plan_first=self.plan_first;
        config.jev.parallel_tasks=self.parallel_tasks;
        config.jev.keep_session_model=self.keep_session_model;
        config.jev.resume_across_modes=self.resume_across_modes;
        config.jev.turn_ceiling_minutes=self.turn_ceiling_minutes;
    }

    /// Confere tudo e devolve a versão limpa: padrões sem espaço nas pontas,
    /// sem linha vazia e sem repetição.
    pub fn validate(mut self)->Result<Self> {
        if !(CONFIDENCE_RANGE.0..=CONFIDENCE_RANGE.1).contains(&self.confidence_threshold) {
            bail!(Text::new("core.confidence").with("min",format!("{:.2}",CONFIDENCE_RANGE.0)).with("max",format!("{:.2}",CONFIDENCE_RANGE.1)));
        }
        for level in COMPLEXITIES {
            let budget=self.budgets.get(level).copied().unwrap_or(0);
            if !(BUDGET_RANGE.0..=BUDGET_RANGE.1).contains(&budget) {
                bail!(Text::new("core.budget").with("level",Text::new(&format!("complexity.{level}"))).with("min",BUDGET_RANGE.0).with("max",BUDGET_RANGE.1));
            }
        }
        self.budgets.retain(|level,_|COMPLEXITIES.contains(&level.as_str()));
        if !(CACHE_TTL_RANGE.0..=CACHE_TTL_RANGE.1).contains(&self.cache_ttl) { bail!(Text::new("core.cacheTtl").with("max",CACHE_TTL_RANGE.1)); }
        if !(TURN_CEILING_RANGE.0..=TURN_CEILING_RANGE.1).contains(&self.turn_ceiling_minutes) {
            bail!(Text::new("core.turnCeiling").with("min",TURN_CEILING_RANGE.0).with("max",TURN_CEILING_RANGE.1));
        }
        for (rule,value) in [("read",&self.exit_rules.read),("write",&self.exit_rules.write),("shell",&self.exit_rules.shell)] {
            if !PERMISSIONS.contains(&value.as_str()) { bail!(Text::new("core.permission").with("rule",rule).with("value",value)); }
        }
        self.privacy.deny=patterns(&self.privacy.deny)?;
        self.privacy.local_only=patterns(&self.privacy.local_only)?;
        let mut order:Vec<String>=Vec::new();
        for agent in &self.agent_order {
            if !crate::llm::AgentId::ALL.iter().any(|known|known.key()==agent) { bail!(Text::new("core.agentOrder").with("agent",agent)); }
            if !order.contains(agent) { order.push(agent.clone()); }
        }
        self.agent_order=order;
        Ok(self)
    }
}

fn patterns(given:&[String])->Result<Vec<String>> {
    let mut kept:Vec<String>=Vec::new();
    for pattern in given.iter().map(|pattern|pattern.trim()).filter(|pattern|!pattern.is_empty()) {
        if pattern.chars().count()>PATTERN_MAX { bail!(Text::new("core.patternLong").with("max",PATTERN_MAX)); }
        if globset::Glob::new(pattern).is_err() { bail!(Text::new("core.patternInvalid").with("pattern",pattern)); }
        if !kept.iter().any(|known|known==pattern) { kept.push(pattern.to_string()); }
    }
    Ok(kept)
}

/// O que está gravado, ou os valores de partida quando nada foi salvo ainda —
/// ou quando o gravado não passa mais na validação.
pub fn load(connection:&Connection,defaults:&CoreSettings)->Result<CoreSettings> {
    let stored:Option<String>=connection.query_row("SELECT value FROM app_metadata WHERE key=?1",[KEY],|row|row.get(0)).optional()?;
    Ok(stored.and_then(|raw|serde_json::from_str::<CoreSettings>(&raw).ok()).and_then(|settings|settings.validate().ok()).unwrap_or_else(||defaults.clone()))
}

pub fn save(connection:&Connection,settings:&CoreSettings)->Result<CoreSettings> {
    let settings=settings.clone().validate()?;
    connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![KEY,serde_json::to_string(&settings)?])?;
    Ok(settings)
}

#[cfg(test)] mod tests {
    use super::*;

    fn memory()->Connection { let connection=Connection::open_in_memory().expect("database"); connection.execute_batch("CREATE TABLE app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);").expect("table"); connection }

    #[test] fn nothing_saved_falls_back_to_the_configuration() {
        let defaults=CoreSettings::from_config(&Config::default());
        assert_eq!(load(&memory(),&defaults).expect("load"),defaults);
        assert_eq!(defaults.exit_rules.write,"ask");
        assert_eq!(defaults.budgets.len(),COMPLEXITIES.len());
    }

    #[test] fn a_saved_setting_comes_back_clean_and_reaches_the_configuration() {
        let connection=memory();
        let mut wanted=CoreSettings::from_config(&Config::default());
        wanted.confidence_threshold=0.8;
        wanted.exit_rules.write="allow".into();
        wanted.privacy.deny=vec![" .env ".into(),"".into(),".env".into(),"*.pem".into()];
        wanted.agent_order=vec!["codex".into(),"claude".into(),"codex".into()];
        let saved=save(&connection,&wanted).expect("save");
        assert_eq!(saved.privacy.deny,vec![".env","*.pem"]);
        assert_eq!(saved.agent_order,vec!["codex","claude"]);
        assert_eq!(load(&connection,&CoreSettings::from_config(&Config::default())).expect("load"),saved);
        let mut config=Config::default();
        saved.apply(&mut config);
        assert_eq!((config.jev.adaptive_routing.confidence_threshold,config.permissions.write.as_str()),(0.8,"allow"));
        assert_eq!(config.jev.agent_order,vec!["codex","claude"]);
    }

    /// O que foi gravado antes da ordem de preferência ainda abre, sem ordem.
    #[test] fn settings_saved_before_the_agent_order_still_load() {
        let connection=memory();
        let mut old=serde_json::to_value(CoreSettings::from_config(&Config::default())).expect("json");
        old.as_object_mut().expect("objeto").remove("agentOrder");
        old["confidenceThreshold"]=serde_json::json!(0.8);
        connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2)",params![KEY,old.to_string()]).expect("grava");
        let loaded=load(&connection,&CoreSettings::from_config(&Config::default())).expect("load");
        assert_eq!((loaded.confidence_threshold,loaded.agent_order.len()),(0.8,0));
        assert!(loaded.keep_session_model&&!loaded.resume_across_modes,"o gravado antes das opções de sessão abre com os padrões");
        assert_eq!(loaded.turn_ceiling_minutes,30,"o gravado antes do teto abre com 30 minutos");
    }

    #[test] fn values_outside_the_ranges_never_reach_the_database() {
        let base=CoreSettings::from_config(&Config::default());
        let key=|settings:CoreSettings|crate::i18n::Text::from(settings.validate().unwrap_err()).key;
        assert_eq!(key(CoreSettings{confidence_threshold:0.2,..base.clone()}),"core.confidence");
        let mut budgets=base.budgets.clone(); budgets.insert("medium".into(),10);
        assert_eq!(key(CoreSettings{budgets,..base.clone()}),"core.budget");
        assert_eq!(key(CoreSettings{exit_rules:ExitRules{shell:"maybe".into(),..base.exit_rules.clone()},..base.clone()}),"core.permission");
        assert_eq!(key(CoreSettings{privacy:Privacy{deny:vec!["[".into()],..base.privacy.clone()},..base.clone()}),"core.patternInvalid");
        assert_eq!(key(CoreSettings{agent_order:vec!["gemini".into()],..base.clone()}),"core.agentOrder");
        assert_eq!(key(CoreSettings{turn_ceiling_minutes:1,..base.clone()}),"core.turnCeiling");
        assert_eq!(key(CoreSettings{turn_ceiling_minutes:600,..base.clone()}),"core.turnCeiling");
    }
}
