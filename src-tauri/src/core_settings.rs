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
}

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
        for (rule,value) in [("read",&self.exit_rules.read),("write",&self.exit_rules.write),("shell",&self.exit_rules.shell)] {
            if !PERMISSIONS.contains(&value.as_str()) { bail!(Text::new("core.permission").with("rule",rule).with("value",value)); }
        }
        self.privacy.deny=patterns(&self.privacy.deny)?;
        self.privacy.local_only=patterns(&self.privacy.local_only)?;
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
        let saved=save(&connection,&wanted).expect("save");
        assert_eq!(saved.privacy.deny,vec![".env","*.pem"]);
        assert_eq!(load(&connection,&CoreSettings::from_config(&Config::default())).expect("load"),saved);
        let mut config=Config::default();
        saved.apply(&mut config);
        assert_eq!((config.jev.adaptive_routing.confidence_threshold,config.permissions.write.as_str()),(0.8,"allow"));
    }

    #[test] fn values_outside_the_ranges_never_reach_the_database() {
        let base=CoreSettings::from_config(&Config::default());
        let key=|settings:CoreSettings|crate::i18n::Text::from(settings.validate().unwrap_err()).key;
        assert_eq!(key(CoreSettings{confidence_threshold:0.2,..base.clone()}),"core.confidence");
        let mut budgets=base.budgets.clone(); budgets.insert("medium".into(),10);
        assert_eq!(key(CoreSettings{budgets,..base.clone()}),"core.budget");
        assert_eq!(key(CoreSettings{exit_rules:ExitRules{shell:"maybe".into(),..base.exit_rules.clone()},..base.clone()}),"core.permission");
        assert_eq!(key(CoreSettings{privacy:Privacy{deny:vec!["[".into()],..base.privacy.clone()},..base.clone()}),"core.patternInvalid");
    }
}
