//! O ponto de extensão `agents` dos mods do app: um mod do catálogo que traz
//! agentes de linha de comando ou de API. Instalar o mod cria, neste ambiente,
//! os agentes `mod-<id>`, `mod-<id>-2`… com a definição do catálogo — os
//! mesmos mods que a pessoa cria no site (`mods::custom`), e que sincronizam
//! como eles. Desinstalar apaga os agentes, os modelos e a chave guardada.
//! Desligar o mod tira os agentes do roteamento (`Entitlements::apply_llm`).

use crate::i18n::{reply_language, Text};
use crate::llm::{self, AgentId};
use crate::mods::custom::{CustomOptions, API, MAX_MODS, MAX_NAME};
use crate::mods::CUSTOM_PREFIX;
use anyhow::{bail, Result};
use jayv_mods::{ExtensionPoint, Manifest};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;

/// Quantos agentes um mod traz.
pub const MAX_AGENTS:usize=4;

/// Um agente como o catálogo o descreve: a definição de um mod criado, sem o
/// que é da máquina (a chave, a aprovação dos servidores MCP).
#[derive(Debug,Clone,Deserialize)]
#[serde(rename_all="camelCase")]
struct Definition {
    kind:String,
    #[serde(default)] protocol:Option<String>,
    #[serde(default)] base_url:String,
    #[serde(default)] key_required:bool,
    #[serde(default)] command:String,
    #[serde(default)] args:Vec<String>,
    #[serde(default)] plan_args:Vec<String>,
    #[serde(default)] edits:bool,
    #[serde(default)] models:Vec<String>,
    /// API Responses: o campo do pedido que leva o modelo (`preset`).
    #[serde(default)] model_field:String,
    /// ACP: as variáveis do agente e os ids dos modos dele.
    #[serde(default)] env:std::collections::BTreeMap<String,String>,
    #[serde(default)] build_mode:String,
    #[serde(default)] plan_mode:String,
}

/// Um agente pronto para gravar.
#[derive(Debug,Clone,PartialEq)]
pub struct ModAgent { pub id:AgentId, pub command:String, pub options:CustomOptions, pub models:Vec<String> }

/// O id do agente que a entrada `index` do mod vira: o mesmo em toda máquina
/// e no site.
pub fn agent_key(mod_id:&str,index:usize)->String { if index==0 { format!("{CUSTOM_PREFIX}{mod_id}") } else { format!("{CUSTOM_PREFIX}{mod_id}-{}",index+1) } }

/// O título do mod no idioma do app (o da língua, o inglês ou o primeiro).
fn title(manifest:&Manifest)->String {
    let tag=reply_language().map(|language|language.tag).unwrap_or_else(||"en".into());
    let language=tag.split('-').next().unwrap_or_default().to_string();
    let titles=&manifest.title;
    titles.get(&tag).or_else(||titles.iter().find(|(key,_)|key.split('-').next()==Some(language.as_str())).map(|(_,text)|text))
        .or_else(||titles.get("en")).or_else(||titles.values().next()).cloned().unwrap_or_else(||manifest.id.clone())
}

/// O nome do agente: o título do mod e, do segundo em diante, o número.
fn name(title:&str,index:usize)->String {
    let suffix=if index==0 { String::new() } else { format!(" {}",index+1) };
    let room=MAX_NAME-suffix.chars().count();
    format!("{}{suffix}",title.trim().chars().take(room).collect::<String>().trim_end())
}

/// Os agentes do mod, conferidos como os mods criados. Nulo quando esta versão
/// não lê a contribuição.
pub fn agents_of(manifest:&Manifest,value:&Value)->Option<Vec<ModAgent>> {
    let list=value.as_array()?;
    if list.is_empty()||list.len()>MAX_AGENTS { return None; }
    let title=title(manifest);
    list.iter().enumerate().map(|(index,item)|{
        let definition:Definition=serde_json::from_value(item.clone()).ok()?;
        let id=AgentId::parse(&agent_key(&manifest.id,index)).ok()?;
        let options=CustomOptions{
            name:name(&title,index),kind:definition.kind,args:definition.args,plan_args:definition.plan_args,edits:definition.edits,
            protocol:definition.protocol.unwrap_or_else(||crate::mods::custom::OPENAI.into()),base_url:definition.base_url,key_required:definition.key_required,
            model_field:definition.model_field,env:definition.env,build_mode:definition.build_mode,plan_mode:definition.plan_mode,
            ..Default::default()
        }.checked().ok()?;
        let command=if options.kind==API { String::new() } else { definition.command.trim().to_string() };
        if options.kind!=API&&(command.is_empty()||command.chars().any(char::is_whitespace)) { return None; }
        if definition.models.iter().any(|model|!llm::valid_id(model)) { return None; }
        Some(ModAgent{id,command,options,models:definition.models})
    }).collect()
}

pub struct AgentsPoint;

/// O ponto, para o registro do núcleo.
pub static AGENTS:AgentsPoint=AgentsPoint;

impl ExtensionPoint for AgentsPoint {
    fn key(&self)->&'static str { "agents" }

    fn targets(&self,manifest:&Manifest,value:&Value)->Option<Vec<String>> {
        Some(agents_of(manifest,value)?.into_iter().map(|agent|agent.id.key().to_string()).collect())
    }

    /// Cria os agentes que faltam. O que já existe — instalado em outra
    /// máquina e sincronizado, ou mexido pela pessoa — fica como está. O de
    /// API que pede chave nasce desligado: ligado sem a chave, ele travaria o
    /// salvar das configurações.
    fn install(&self,connection:&Connection,manifest:&Manifest,value:&Value)->Result<()> {
        let Some(agents)=agents_of(manifest,value) else { bail!(Text::new("mods.agentsInvalid").with("name",title(manifest))) };
        let exists=|id:&str|->Result<bool> { Ok(connection.query_row("SELECT COUNT(*) FROM llm_agents WHERE id=?1",[id],|row|row.get::<_,i64>(0))?>0) };
        let mut fresh=Vec::new();
        for agent in agents { if !exists(agent.id.key())? { fresh.push(agent); } }
        if fresh.is_empty() { return Ok(()); }
        let customs:i64=connection.query_row("SELECT COUNT(*) FROM llm_agents WHERE id LIKE 'mod-%'",[],|row|row.get(0))?;
        if customs as usize+fresh.len()>MAX_MODS { bail!(Text::new("mods.tooMany").with("max",MAX_MODS as u32)); }
        let now=chrono::Utc::now().to_rfc3339();
        let mut position:i64=connection.query_row("SELECT COALESCE(MAX(position),-1)+1 FROM llm_models",[],|row|row.get(0))?;
        for agent in fresh {
            let enabled=!(agent.options.kind==API&&agent.options.key_required);
            connection.execute(
                "INSERT INTO llm_agents(id,enabled,command,timeout,options,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![agent.id.key(),enabled,agent.command,300_i64,serde_json::to_string(&agent.options)?,now],
            )?;
            let edits=agent.options.kind!=API&&agent.options.edits;
            let capabilities=if edits { r#"["chat","code","reasoning","tools"]"# } else { r#"["chat","reasoning"]"# };
            for model in &agent.models {
                connection.execute(
                    "INSERT OR IGNORE INTO llm_models(agent,model,enabled,capabilities,cost_class,speed,context_window,position) VALUES(?1,?2,1,?3,'medium','medium',128000,?4)",
                    params![agent.id.key(),model,capabilities,position],
                )?;
                position+=1;
            }
        }
        Ok(())
    }

    /// Apaga os agentes do mod, com os modelos e a chave. O manifesto é o de
    /// quando ele foi instalado; os ids saem da conta das entradas, mesmo que
    /// esta versão já não leia alguma.
    fn uninstall(&self,connection:&Connection,manifest:&Manifest,value:&Value)->Result<()> {
        let count=value.as_array().map_or(0,Vec::len).min(MAX_AGENTS);
        for index in 0..count {
            let id=agent_key(&manifest.id,index);
            connection.execute("DELETE FROM llm_models WHERE agent=?1",[&id])?;
            connection.execute("DELETE FROM llm_agents WHERE id=?1",[&id])?;
            connection.execute("DELETE FROM llm_secrets WHERE agent=?1",[&id])?;
        }
        llm::remember_secrets(connection)?;
        Ok(())
    }
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        llm::ensure(&connection).expect("tabelas");
        connection
    }

    fn manifest(id:&str,agents:Value)->Manifest {
        Manifest::read(&json!({"id":id,"version":"1.0.0","title":{"en":"Local AI","pt-BR":"IA local"},"contributes":{"agents":agents}})).expect("mod")
    }

    #[test] fn the_agents_of_a_mod_follow_the_rules_of_created_mods() {
        let api=json!({"kind":"api","protocol":"openai","baseUrl":"http://localhost:8080/v1/","keyRequired":false,"models":["llama3"]});
        let cli=json!({"kind":"cli","command":"local-ai","args":["--model","{model}","{prompt}"],"planArgs":["--dry-run","{prompt}"],"edits":true});
        let mod_=manifest("local-ai",json!([api,cli]));
        let agents=agents_of(&mod_,&mod_.contributes["agents"]).expect("lê");
        assert_eq!(agents.iter().map(|agent|agent.id.key()).collect::<Vec<_>>(),["mod-local-ai","mod-local-ai-2"]);
        assert_eq!(agents[0].options.base_url,"http://localhost:8080/v1","o endereço sai limpo");
        assert_eq!((agents[0].options.name.as_str(),agents[1].options.name.as_str()),("Local AI","Local AI 2"));
        assert_eq!(AGENTS.targets(&mod_,&mod_.contributes["agents"]),Some(vec!["mod-local-ai".to_string(),"mod-local-ai-2".to_string()]));
        for broken in [json!([]),json!("x"),json!([{"kind":"cli","command":"run it","args":[]}]),json!([{"kind":"cli","command":"x","args":["{file}"]}]),json!([{"kind":"api","baseUrl":"ftp://x"}]),json!([{"kind":"api","baseUrl":"http://x","models":["bad model"]}])] {
            assert!(agents_of(&mod_,&broken).is_none(),"{broken}");
        }
        assert!(agents_of(&manifest("x".repeat(40).as_str(),json!([api])),&json!([api])).is_none(),"o id do agente passaria de 32");
    }

    /// Os agentes ACP e os de API com protocolo novo chegam do catálogo com o
    /// que é deles: as variáveis e os modos, o campo do modelo.
    #[test] fn acp_and_responses_agents_keep_their_definitions() {
        let acp=json!({"kind":"acp","command":"opencode","args":["acp"],"env":{"OPENCODE_PERMISSION":"{\"bash\":\"ask\"}"},"buildMode":"build","planMode":"plan","models":["default"]});
        let responses=json!({"kind":"api","protocol":"responses","baseUrl":"https://api.perplexity.ai/v1","keyRequired":true,"modelField":"preset","models":["fast","high"]});
        let mod_=manifest("open-agents",json!([acp,responses]));
        let agents=agents_of(&mod_,&mod_.contributes["agents"]).expect("lê");
        assert_eq!((agents[0].options.kind.as_str(),agents[0].options.build_mode.as_str(),agents[0].options.plan_mode.as_str()),("acp","build","plan"));
        assert_eq!(agents[0].options.env.get("OPENCODE_PERMISSION").map(String::as_str),Some("{\"bash\":\"ask\"}"));
        assert!(agents[0].options.edits,"o agente ACP escreve no projeto");
        assert_eq!((agents[1].options.protocol.as_str(),agents[1].options.model_field.as_str()),("responses","preset"));
        let connection=database();
        AGENTS.install(&connection,&mod_,&mod_.contributes["agents"]).expect("instala");
        let settings=llm::load(&connection).expect("lê");
        let model=settings.models.iter().find(|model|model.agent.key()=="mod-open-agents").expect("modelo");
        assert!(model.capabilities.iter().any(|capability|capability=="code"),"{:?}",model.capabilities);
        assert!(llm::validate(&settings).is_ok());
    }

    #[test] fn installing_creates_what_is_missing_and_uninstalling_removes_it() {
        let connection=database();
        let mod_=manifest("local-ai",json!([{"kind":"api","protocol":"openai","baseUrl":"http://localhost:8080/v1","keyRequired":true,"models":["llama3","qwen"]}]));
        let value=&mod_.contributes["agents"];
        AGENTS.install(&connection,&mod_,value).expect("instala");
        let settings=llm::load(&connection).expect("lê");
        let agent=settings.agents.iter().find(|agent|agent.id.key()=="mod-local-ai").expect("agente");
        assert!(!agent.enabled,"pede chave: nasce desligado");
        assert_eq!(settings.models.iter().filter(|model|model.agent==agent.id).map(|model|model.model.as_str()).collect::<Vec<_>>(),["llama3","qwen"]);
        assert!(llm::validate(&settings).is_ok(),"o que o mod grava passa no salvar");
        // O que a pessoa mudou fica: instalar de novo não pisa.
        connection.execute("UPDATE llm_agents SET timeout=900 WHERE id='mod-local-ai'",[]).expect("muda");
        AGENTS.install(&connection,&mod_,value).expect("de novo");
        assert_eq!(connection.query_row("SELECT timeout FROM llm_agents WHERE id='mod-local-ai'",[],|row|row.get::<_,i64>(0)).expect("lê"),900);
        connection.execute("INSERT INTO llm_secrets(agent,secret) VALUES('mod-local-ai','k')",[]).expect("chave");
        AGENTS.uninstall(&connection,&mod_,value).expect("desinstala");
        for table in ["llm_agents WHERE id","llm_models WHERE agent","llm_secrets WHERE agent"] {
            assert_eq!(connection.query_row(&format!("SELECT COUNT(*) FROM {table}='mod-local-ai'"),[],|row|row.get::<_,i64>(0)).expect("lê"),0,"{table}");
        }
    }

    #[test] fn a_full_environment_refuses_more_agents() {
        let connection=database();
        for index in 0..MAX_MODS { connection.execute("INSERT INTO llm_agents(id,enabled,command,timeout,options,updated_at) VALUES(?1,1,'x',300,'{}','now')",[format!("mod-own-{index}")]).expect("cria"); }
        let mod_=manifest("local-ai",json!([{"kind":"api","protocol":"openai","baseUrl":"http://localhost:8080/v1","keyRequired":false}]));
        let error=AGENTS.install(&connection,&mod_,&mod_.contributes["agents"]).expect_err("cheio");
        assert_eq!(Text::from(error).key,"mods.tooMany");
    }
}
