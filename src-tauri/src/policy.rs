//! A política de LLM da organização dona do projeto (etapa C). Ela chega
//! pronta do servidor (`my_project_policies`, já com a junção da política da
//! organização e da do repositório) e fica num cache local, fora da fila de
//! sync: só desce. No atendimento, as configurações de quem usa passam por
//! ela antes de chegar ao orquestrador.
//!
//! A política só aperta: desliga agente e modelo, troca os modos sem trava dos
//! agentes por um com trava, soma padrões de privacidade e endurece as regras
//! da portaria de saída. Nunca liga o que quem usa desligou.

use crate::core_settings::CoreSettings;
use crate::llm::LlmSettings;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS project_policies (
  project_id TEXT PRIMARY KEY,
  org_slug TEXT NOT NULL,
  policy TEXT NOT NULL,
  fetched_at TEXT NOT NULL
);";

/// A política efetiva de um projeto, como `llm_policy_of` a devolve.
#[derive(Debug,Clone,Default,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct LlmPolicy {
    /// Os agentes permitidos; `None` é todos.
    pub agents:Option<Vec<String>>,
    /// `agente/modelo`.
    pub blocked_models:Vec<String>,
    pub deny:Vec<String>,
    pub local_only:Vec<String>,
    pub safe_agents:bool,
    pub redact_secrets:bool,
    pub min_read:Option<String>,
    pub min_write:Option<String>,
    pub min_shell:Option<String>,
}

/// A política de um projeto e a organização que a define.
#[derive(Debug,Clone,PartialEq)]
pub struct ProjectPolicy { pub org_slug:String, pub policy:LlmPolicy }

/// `allow` < `ask` < `deny`. Valor desconhecido conta como `deny`: uma regra
/// que esta versão não entende não pode afrouxar nada.
fn strictness(rule:&str)->usize { match rule { "allow"=>0, "ask"=>1, _=>2 } }

fn stricter(mine:&str,floor:Option<&str>)->String {
    match floor { Some(floor) if strictness(floor)>strictness(mine)=>if strictness(floor)>=2 {"deny".into()} else {floor.into()}, _=>mine.into() }
}

fn joined(mine:&[String],extra:&[String])->Vec<String> {
    let mut all=mine.to_vec();
    for pattern in extra.iter().map(|pattern|pattern.trim()).filter(|pattern|!pattern.is_empty()) { if !all.iter().any(|known|known==pattern) { all.push(pattern.to_string()); } }
    all
}

impl LlmPolicy {
    pub fn allows_agent(&self,agent:&str)->bool { self.agents.as_ref().is_none_or(|agents|agents.iter().any(|allowed|allowed==agent)) }

    /// Os agentes e modelos de quem usa, com o que a política proíbe desligado.
    pub fn restrict_llm(&self,settings:&LlmSettings)->LlmSettings {
        let agents=settings.agents.iter().map(|agent|{
            let mut agent=if self.safe_agents { agent.without_unsafe_modes() } else { agent.clone() };
            agent.enabled=agent.enabled && self.allows_agent(agent.id.key());
            agent
        }).collect();
        let models=settings.models.iter().map(|model|{
            let mut model=model.clone();
            model.enabled=model.enabled && self.allows_agent(model.agent.key()) && !self.blocked_models.contains(&crate::llm::model_key(&model));
            model
        }).collect();
        LlmSettings{agents,models}
    }

    /// As configurações do Jev de quem usa, sem nada mais frouxo que a
    /// política.
    pub fn restrict_core(&self,settings:&CoreSettings)->CoreSettings {
        let mut settings=settings.clone();
        settings.exit_rules.read=stricter(&settings.exit_rules.read,self.min_read.as_deref());
        settings.exit_rules.write=stricter(&settings.exit_rules.write,self.min_write.as_deref());
        settings.exit_rules.shell=stricter(&settings.exit_rules.shell,self.min_shell.as_deref());
        settings.privacy.deny=joined(&settings.privacy.deny,&self.deny);
        settings.privacy.local_only=joined(&settings.privacy.local_only,&self.local_only);
        settings.privacy.redact_secrets|=self.redact_secrets;
        settings
    }
}

/// Uma linha de `my_project_policies`.
#[derive(Debug,Clone,Deserialize)]
pub struct RemotePolicy { pub project_id:String, pub org_slug:String, pub policy:Value }

/// Troca o cache inteiro pelo que o servidor devolveu: projeto que saiu da
/// lista deixou de ter política.
pub fn replace_all(connection:&mut Connection,rows:&[RemotePolicy])->Result<()> {
    let now=chrono::Utc::now().to_rfc3339();
    let transaction=connection.transaction()?;
    transaction.execute("DELETE FROM project_policies",[])?;
    for row in rows {
        transaction.execute("INSERT OR REPLACE INTO project_policies(project_id,org_slug,policy,fetched_at) VALUES(?1,?2,?3,?4)",params![row.project_id,row.org_slug,row.policy.to_string(),now])?;
    }
    transaction.commit()?;
    Ok(())
}

/// A política do projeto do chat, se houver. Uma política que esta versão não
/// consegue ler vale como a mais rígida que ela conhece, e não como nenhuma.
pub fn for_chat(connection:&Connection,chat_id:&str)->Result<Option<ProjectPolicy>> {
    let row:Option<(String,String)>=connection.query_row(
        "SELECT p.org_slug,p.policy FROM chats c JOIN project_policies p ON p.project_id=c.project_id WHERE c.id=?1",[chat_id],
        |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    Ok(row.map(|(org_slug,raw)|ProjectPolicy{org_slug,policy:serde_json::from_str(&raw).unwrap_or_else(|_|LlmPolicy::unreadable())}))
}

impl LlmPolicy {
    fn unreadable()->Self {
        Self{agents:Some(vec![]),safe_agents:true,redact_secrets:true,min_read:Some("deny".into()),min_write:Some("deny".into()),min_shell:Some("deny".into()),..Self::default()}
    }
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::config::Config;
    use crate::llm::{AgentId, AgentModel, AgentSettings};
    use serde_json::json;

    fn agent(id:AgentId,options:Value)->AgentSettings { AgentSettings{id,enabled:true,command:id.binary().into(),timeout:300,options} }
    fn model(agent:AgentId,name:&str,enabled:bool)->AgentModel { AgentModel{agent,model:name.into(),enabled,capabilities:vec!["chat".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000} }
    fn settings()->LlmSettings {
        LlmSettings{
            agents:vec![
                agent(AgentId::Claude,json!({"permissionMode":"bypassPermissions"})),
                agent(AgentId::Codex,json!({"sandbox":"danger-full-access","networkAccess":true})),
                agent(AgentId::Copilot,json!({"toolAccess":"all"})),
                AgentSettings{enabled:false,..agent(AgentId::Cursor,json!({"force":true,"approveMcps":true,"sandbox":"disabled"}))},
            ],
            models:vec![model(AgentId::Claude,"opus",true),model(AgentId::Claude,"sonnet",true),model(AgentId::Codex,"o3",false),model(AgentId::Cursor,"auto",true)],
        }
    }
    fn policy(value:Value)->LlmPolicy { serde_json::from_value(value).expect("política") }

    #[test] fn agents_and_models_outside_the_policy_are_switched_off() {
        let restricted=policy(json!({"agents":["claude","codex","cursor"],"blocked_models":["claude/opus"]})).restrict_llm(&settings());
        let enabled=|id:AgentId|restricted.agents.iter().find(|agent|agent.id==id).expect("agente").enabled;
        assert!(enabled(AgentId::Claude) && enabled(AgentId::Codex));
        assert!(!enabled(AgentId::Copilot),"fora da lista");
        assert!(!enabled(AgentId::Cursor),"a política não liga o que quem usa desligou");
        let on=restricted.models.iter().filter(|model|model.enabled).map(crate::llm::model_key).collect::<Vec<_>>();
        assert_eq!(on,["claude/sonnet","cursor/auto"],"o3 já estava desligado e opus é bloqueado");
    }

    #[test] fn without_a_list_every_agent_is_allowed() {
        let restricted=LlmPolicy::default().restrict_llm(&settings());
        assert_eq!(restricted.agents.iter().filter(|agent|agent.enabled).count(),3);
        assert_eq!(restricted.agents[0].options,settings().agents[0].options,"sem safe_agents, as opções ficam");
    }

    #[test] fn safe_agents_turns_off_the_unguarded_modes_of_all_four() {
        let restricted=policy(json!({"safe_agents":true})).restrict_llm(&settings());
        let options=|id:AgentId|restricted.agents.iter().find(|agent|agent.id==id).expect("agente").options.clone();
        assert_eq!(options(AgentId::Claude)["permissionMode"],"default");
        assert_eq!((options(AgentId::Codex)["sandbox"].clone(),options(AgentId::Codex)["networkAccess"].clone()),(json!("workspace-write"),json!(false)));
        assert_eq!(options(AgentId::Copilot)["toolAccess"],"edits");
        assert_eq!((options(AgentId::Cursor)["force"].clone(),options(AgentId::Cursor)["approveMcps"].clone(),options(AgentId::Cursor)["sandbox"].clone()),(json!(false),json!(false),json!("enabled")));
        let claude=restricted.agents.iter().find(|agent|agent.id==AgentId::Claude).expect("claude");
        assert!(!claude.args().contains(&"bypassPermissions".to_string()),"a linha de comando sai das opções novas");
    }

    #[test] fn safe_agents_keeps_the_modes_that_already_have_a_guard() {
        let mut guarded=settings();
        guarded.agents[0].options=json!({"permissionMode":"plan","effort":"high"});
        guarded.agents[1].options=json!({"sandbox":"read-only"});
        let restricted=policy(json!({"safe_agents":true})).restrict_llm(&guarded);
        assert_eq!(restricted.agents[0].options["permissionMode"],"plan");
        assert_eq!(restricted.agents[0].options["effort"],"high","o resto das opções fica");
        assert_eq!(restricted.agents[1].options["sandbox"],"read-only","read-only não vira workspace-write");
    }

    #[test] fn the_core_settings_only_get_stricter() {
        let mine=CoreSettings::from_config(&Config::default());
        let restricted=policy(json!({"deny":["secrets/**",".env"," "],"local_only":["vendor/**"],"min_read":"ask","min_write":"allow","min_shell":"deny"})).restrict_core(&mine);
        assert_eq!((restricted.exit_rules.read.as_str(),restricted.exit_rules.write.as_str(),restricted.exit_rules.shell.as_str()),("ask","ask","deny"),"write de quem usa já era ask");
        assert!(restricted.privacy.deny.starts_with(&mine.privacy.deny));
        assert_eq!(restricted.privacy.deny.iter().filter(|pattern|*pattern==".env").count(),1,"sem repetição");
        assert!(restricted.privacy.deny.contains(&"secrets/**".to_string()) && restricted.privacy.local_only.contains(&"vendor/**".to_string()));
        assert!(!restricted.privacy.deny.iter().any(|pattern|pattern.trim().is_empty()));
        assert!(restricted.clone().validate().is_ok(),"o resultado passa na validação das configurações");

        let mut loose=mine.clone(); loose.privacy.redact_secrets=false;
        assert!(policy(json!({"redact_secrets":true})).restrict_core(&loose).privacy.redact_secrets);
        assert!(!LlmPolicy::default().restrict_core(&loose).privacy.redact_secrets,"política desligada não liga nada");
        assert_eq!(stricter("allow",Some("talvez")),"deny","regra desconhecida aperta");
    }

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch("CREATE TABLE chats (id TEXT PRIMARY KEY, project_id TEXT NOT NULL);").expect("chats");
        connection.execute_batch(SCHEMA).expect("cache");
        connection.execute_batch("INSERT INTO chats VALUES ('c1','p1'),('c2','p2');").expect("linhas");
        connection
    }

    #[test] fn the_cache_is_replaced_whole_and_read_by_chat() {
        let mut connection=database();
        let row=|project:&str,policy:Value|RemotePolicy{project_id:project.into(),org_slug:"acme".into(),policy};
        replace_all(&mut connection,&[row("p1",json!({"agents":["codex"]})),row("p2",json!({}))]).expect("grava");
        let first=for_chat(&connection,"c1").expect("lê").expect("tem política");
        assert_eq!((first.org_slug.as_str(),first.policy.agents.clone()),("acme",Some(vec!["codex".to_string()])));
        assert!(for_chat(&connection,"c2").expect("lê").is_some());
        replace_all(&mut connection,&[row("p1",json!({}))]).expect("grava");
        assert!(for_chat(&connection,"c2").expect("lê").is_none(),"o projeto que saiu da lista perdeu a política");
        assert!(for_chat(&connection,"nenhum").expect("lê").is_none());
    }

    #[test] fn an_unreadable_policy_is_the_strictest_one() {
        let mut connection=database();
        replace_all(&mut connection,&[RemotePolicy{project_id:"p1".into(),org_slug:"acme".into(),policy:json!({"agents":"todos"})}]).expect("grava");
        let read=for_chat(&connection,"c1").expect("lê").expect("tem política").policy;
        assert!(!read.allows_agent("claude"));
        assert_eq!(read.min_shell.as_deref(),Some("deny"));
    }
}
