//! Os recursos do plano de quem usa (`rpc/my_features`): o admin do sistema
//! liga e desliga cada recurso e escolhe em que planos ele entra. A lista
//! desce na volta do sync e fica num cache local, fora da fila: só desce.
//!
//! Sem cache (o servidor sem a migração dos planos, ou nunca houve uma volta
//! com rede), vale tudo: quem já usava um recurso não o perde por estar
//! offline. Com cache, o que não está na lista fica desligado no atendimento,
//! por cima das configurações de quem usa — como a política de LLM, só aperta.

use crate::core_settings::CoreSettings;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

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

pub use crate::cloud::remote::RemoteFeatures;

/// Os recursos que valem agora. `None`: nenhuma lista desceu ainda, vale tudo.
#[derive(Debug,Clone,Default,PartialEq)]
pub struct Entitlements(Option<BTreeSet<String>>);

impl Entitlements {
    pub fn of(features:&[String])->Self { Self(Some(features.iter().cloned().collect())) }

    pub fn allows(&self,feature:&str)->bool { self.0.as_ref().is_none_or(|known|known.contains(feature)) }

    /// As configurações do Jev sem os recursos que o plano não tem.
    pub fn restrict_core(&self,settings:&CoreSettings)->CoreSettings {
        let mut settings=settings.clone();
        settings.adaptive_routing&=self.allows(ADAPTIVE_ROUTING);
        settings.review_changes&=self.allows(SECOND_OPINION);
        settings.plan_first&=self.allows(PLAN_FIRST);
        settings.parallel_tasks&=self.allows(PARALLEL_TASKS);
        settings
    }
}

pub fn replace(connection:&Connection,remote:&RemoteFeatures)->Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO plan_features(id,plan,features,fetched_at) VALUES(1,?1,?2,?3)",
        params![remote.plan,serde_json::to_string(&remote.features)?,chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// O cache. Uma lista que esta versão não consegue ler vale como vazia: um
/// formato novo não pode liberar o que o plano não tem.
pub fn load(connection:&Connection)->Result<Entitlements> {
    let raw:Option<String>=connection.query_row("SELECT features FROM plan_features WHERE id=1",[],|row|row.get(0)).optional()?;
    Ok(match raw {
        None=>Entitlements::default(),
        Some(raw)=>Entitlements::of(&serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()),
    })
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::config::Config;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(SCHEMA).expect("cache");
        connection
    }

    fn everything_on()->CoreSettings {
        let mut settings=CoreSettings::from_config(&Config::default());
        settings.adaptive_routing=true; settings.review_changes=true; settings.plan_first=true; settings.parallel_tasks=true;
        settings
    }

    #[test] fn without_a_list_everything_is_allowed() {
        let connection=database();
        let entitlements=load(&connection).expect("lê");
        assert!(entitlements.allows(SECOND_OPINION));
        assert_eq!(entitlements.restrict_core(&everything_on()),everything_on());
    }

    #[test] fn features_outside_the_plan_are_switched_off() {
        let connection=database();
        replace(&connection,&RemoteFeatures{plan:Some("free".into()),features:vec![PLAN_FIRST.into(),"stats".into()]}).expect("grava");
        let restricted=load(&connection).expect("lê").restrict_core(&everything_on());
        assert!(restricted.plan_first);
        assert!(!restricted.review_changes && !restricted.parallel_tasks && !restricted.adaptive_routing);
    }

    #[test] fn the_plan_never_turns_on_what_the_user_turned_off() {
        let mut mine=everything_on(); mine.plan_first=false;
        let restricted=Entitlements::of(&[PLAN_FIRST.into()]).restrict_core(&mine);
        assert!(!restricted.plan_first);
    }

    #[test] fn the_cache_is_replaced_and_an_unreadable_one_allows_nothing() {
        let connection=database();
        replace(&connection,&RemoteFeatures{plan:None,features:vec![SECOND_OPINION.into()]}).expect("grava");
        replace(&connection,&RemoteFeatures{plan:None,features:vec![]}).expect("troca");
        assert!(!load(&connection).expect("lê").allows(SECOND_OPINION),"a lista nova vale inteira");
        connection.execute("UPDATE plan_features SET features='{\"x\":1}'",[]).expect("estraga");
        assert!(!load(&connection).expect("lê").allows(PLAN_FIRST));
    }

    #[test] fn the_rpc_answer_is_read_with_extra_fields() {
        let remote:RemoteFeatures=serde_json::from_str(r#"{"plan":"pro","admin":true,"features":["stats"]}"#).expect("lê");
        assert_eq!(remote,RemoteFeatures{plan:Some("pro".into()),features:vec!["stats".into()]});
    }
}
