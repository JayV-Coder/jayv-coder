//! O ponto de extensão `skills` dos mods do app: um mod do catálogo que traz
//! skills — o nome, quando usá-la, as instruções e, se ela precisa, os
//! servidores MCP dela. Instalar o mod grava as skills em `mod_skills`;
//! desinstalar as apaga. Desligar o mod tira as skills da escolha do Jev
//! (`Entitlements::disabled`, pelo alvo `skill:<nome>`).
//!
//! Quem escolhe é o Jev, como com as skills que a pessoa instala
//! (`skill_choice`): ele lê o pedido e o nome e a descrição de cada skill. Os
//! servidores MCP da skill só chegam ao agente no pedido em que o Jev a
//! escolhe — um servidor pesado, com muitas ferramentas, não abre em toda
//! conversa.

use crate::i18n::Text;
use crate::mcp::McpServer;
use crate::skills::Skill;
use anyhow::{bail, Result};
use jayv_mods::{ExtensionPoint, Manifest};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS mod_skills (
  name TEXT PRIMARY KEY,
  mod_id TEXT NOT NULL,
  description TEXT NOT NULL,
  body TEXT NOT NULL,
  mcp TEXT NOT NULL DEFAULT '[]',
  installed_at TEXT NOT NULL
);";

/// Quantas skills um mod traz, e quantos servidores MCP cada uma.
pub const MAX_SKILLS:usize=4;
pub const MAX_SERVERS:usize=4;
const NAME_MAX:usize=64;
const DESCRIPTION_MAX:usize=1024;
/// As instruções inteiras: o modelo recebe até `skills::BODY_MAX_CHARS`.
const BODY_MAX:usize=32_000;

/// O alvo de uma skill no conjunto dos mods: o nome com prefixo, para não
/// cruzar com o id de um recurso ou de um agente.
pub fn target(name:&str)->String { format!("skill:{name}") }

#[derive(Debug,Clone,Deserialize)]
#[serde(rename_all="camelCase")]
struct Definition {
    name:String,
    description:String,
    instructions:String,
    #[serde(default)] mcp_servers:Vec<McpServer>,
}

/// Uma skill pronta para gravar.
#[derive(Debug,Clone,PartialEq)]
pub struct ModSkill { pub name:String, pub description:String, pub body:String, pub mcp:Vec<McpServer> }

fn valid_name(name:&str)->bool { !name.is_empty()&&name.len()<=NAME_MAX&&name.chars().all(|char|char.is_ascii_alphanumeric()||char=='-'||char=='_') }

/// As skills do mod, conferidas como as que a pessoa instala (o nome, a
/// descrição e as instruções) e com os servidores conferidos como os da tela
/// de MCP. Nulo quando esta versão não lê a contribuição.
pub fn skills_of(value:&Value)->Option<Vec<ModSkill>> {
    let list=value.as_array()?;
    if list.is_empty()||list.len()>MAX_SKILLS { return None; }
    let mut names=HashSet::new();
    list.iter().map(|item|{
        let definition:Definition=serde_json::from_value(item.clone()).ok()?;
        let name=definition.name.trim().to_string();
        if !valid_name(&name)||!names.insert(name.clone()) { return None; }
        let description=definition.description.split_whitespace().collect::<Vec<_>>().join(" ");
        let body=definition.instructions.trim().to_string();
        if description.is_empty()||description.chars().count()>DESCRIPTION_MAX||body.is_empty()||body.chars().count()>BODY_MAX { return None; }
        if definition.mcp_servers.len()>MAX_SERVERS { return None; }
        let mut servers=HashSet::new();
        let mcp=definition.mcp_servers.into_iter().map(|server|{
            // O servidor da skill vai a todo agente que aceita servidores MCP.
            let server=McpServer{enabled:true,agents:vec![],..server}.checked().ok()?;
            servers.insert(server.name.clone()).then_some(server)
        }).collect::<Option<Vec<_>>>()?;
        Some(ModSkill{name,description,body,mcp})
    }).collect()
}

pub struct SkillsPoint;

/// O ponto, para o registro do núcleo.
pub static SKILLS:SkillsPoint=SkillsPoint;

impl ExtensionPoint for SkillsPoint {
    fn key(&self)->&'static str { "skills" }

    fn targets(&self,_manifest:&Manifest,value:&Value)->Option<Vec<String>> {
        Some(skills_of(value)?.iter().map(|skill|target(&skill.name)).collect())
    }

    /// Grava as skills do mod. Diferente dos agentes, a skill é do mod e não
    /// se edita: uma versão nova do mod troca o texto e os servidores. A skill
    /// com o mesmo nome de outro mod fica com o outro.
    fn install(&self,connection:&Connection,manifest:&Manifest,value:&Value)->Result<()> {
        connection.execute_batch(SCHEMA)?;
        let Some(skills)=skills_of(value) else { bail!(Text::new("mods.skillsInvalid").with("name",manifest.id.clone())) };
        let now=chrono::Utc::now().to_rfc3339();
        for skill in skills {
            let owner:Option<String>=connection.query_row("SELECT mod_id FROM mod_skills WHERE name=?1",[&skill.name],|row|row.get(0)).ok();
            if owner.as_deref().is_some_and(|owner|owner!=manifest.id) { bail!(Text::new("mods.skillTaken").with("name",skill.name)); }
            connection.execute(
                "INSERT INTO mod_skills(name,mod_id,description,body,mcp,installed_at) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(name) DO UPDATE SET description=?3,body=?4,mcp=?5,installed_at=?6",
                params![skill.name,manifest.id,skill.description,skill.body,serde_json::to_string(&skill.mcp)?,now],
            )?;
        }
        // O que a versão anterior trazia e esta não traz mais sai.
        let names:Vec<String>=skills_of(value).unwrap_or_default().into_iter().map(|skill|skill.name).collect();
        let mut statement=connection.prepare("SELECT name FROM mod_skills WHERE mod_id=?1")?;
        let stale:Vec<String>=statement.query_map([&manifest.id],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?.into_iter().filter(|name|!names.contains(name)).collect();
        for name in stale { connection.execute("DELETE FROM mod_skills WHERE name=?1",[name])?; }
        Ok(())
    }

    /// Apaga as skills do mod.
    fn uninstall(&self,connection:&Connection,manifest:&Manifest,_value:&Value)->Result<()> {
        connection.execute_batch(SCHEMA)?;
        connection.execute("DELETE FROM mod_skills WHERE mod_id=?1",[&manifest.id])?;
        Ok(())
    }
}

/// As skills que os mods instalaram, em ordem de nome: as instruções vêm em
/// `inline` (não há pasta), com o mod em `owner` e os servidores em `mcp`.
pub fn load(connection:&Connection)->Result<Vec<Skill>> {
    connection.execute_batch(SCHEMA)?;
    let mut statement=connection.prepare("SELECT name,mod_id,description,body,mcp,installed_at FROM mod_skills ORDER BY name")?;
    let rows=statement.query_map([],|row|{
        let mcp:String=row.get(4)?;
        Ok(Skill{
            name:row.get(0)?,owner:Some(row.get(1)?),description:row.get(2)?,inline:Some(row.get(3)?),enabled:true,installed_at:row.get(5)?,
            mcp:serde_json::from_str(&mcp).unwrap_or_default(),..Default::default()
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// As skills da pessoa e as dos mods: com o mesmo nome, vale a da pessoa (foi
/// ela quem instalou). As da organização entram depois, por cima das duas
/// (`org_extensions::merge_skills`).
pub fn merge(own:Vec<Skill>,from_mods:Vec<Skill>)->Vec<Skill> {
    let taken:HashSet<String>=own.iter().map(|skill|skill.name.clone()).collect();
    let mut all=own;
    all.extend(from_mods.into_iter().filter(|skill|!taken.contains(&skill.name)));
    all
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(id:&str,skills:Value)->Manifest {
        Manifest::read(&json!({"id":id,"version":"1.0.0","title":{"en":"Reverse"},"contributes":{"skills":skills}})).expect("mod")
    }

    fn reverse()->Value {
        json!([{
            "name":"reverse-engineering","description":"Inspect a compiled program without its source.",
            "instructions":"# Steps\nOpen the binary first.",
            "mcpServers":[{"name":"rea","command":"npx","args":["-y","rea-agents@1.0.0","--mcp"],"agents":["claude"],"enabled":false}]
        }])
    }

    #[test] fn the_skills_of_a_mod_follow_the_rules_of_installed_skills_and_mcp_servers() {
        let skills=skills_of(&reverse()).expect("lê");
        assert_eq!(skills.len(),1);
        assert_eq!(skills[0].mcp[0].transport,crate::mcp::STDIO);
        assert!(skills[0].mcp[0].enabled&&skills[0].mcp[0].agents.is_empty(),"o servidor da skill vai a todo agente que aceita MCP");
        let mod_=manifest("reverse",reverse());
        assert_eq!(SKILLS.targets(&mod_,&mod_.contributes["skills"]),Some(vec!["skill:reverse-engineering".to_string()]));
        for broken in [
            json!([]),json!("x"),
            json!([{"name":"bad name","description":"d","instructions":"i"}]),
            json!([{"name":"a","description":"  ","instructions":"i"}]),
            json!([{"name":"a","description":"d","instructions":""}]),
            json!([{"name":"a","description":"d","instructions":"i"},{"name":"a","description":"d","instructions":"i"}]),
            json!([{"name":"a","description":"d","instructions":"i","mcpServers":[{"name":"jayv","command":"x"}]}]),
            json!([{"name":"a","description":"d","instructions":"i","mcpServers":[{"name":"x","command":""}]}]),
            json!([{"name":"a","description":"d","instructions":"i","mcpServers":[{"name":"x","command":"y"},{"name":"x","command":"z"}]}]),
        ] {
            assert!(skills_of(&broken).is_none(),"{broken}");
        }
    }

    #[test] fn installing_records_the_skills_and_uninstalling_removes_them() {
        let connection=Connection::open_in_memory().expect("banco");
        let mod_=manifest("reverse",reverse());
        SKILLS.install(&connection,&mod_,&mod_.contributes["skills"]).expect("instala");
        let loaded=load(&connection).expect("lê");
        assert_eq!(loaded.len(),1);
        let skill=&loaded[0];
        assert_eq!((skill.owner.as_deref(),skill.inline.as_deref()),(Some("reverse"),Some("# Steps\nOpen the binary first.")));
        assert_eq!(skill.mcp[0].args,["-y","rea-agents@1.0.0","--mcp"]);
        assert!(crate::skills::read_body(skill).expect("corpo").contains("Open the binary"),"o corpo vem do banco, sem pasta");
        // Uma versão nova troca o texto e tira a skill que saiu do mod.
        let newer=manifest("reverse",json!([{"name":"reverse-engineering","description":"New.","instructions":"Newer steps."}]));
        SKILLS.install(&connection,&newer,&newer.contributes["skills"]).expect("de novo");
        let loaded=load(&connection).expect("lê");
        assert_eq!((loaded[0].description.as_str(),loaded[0].mcp.len()),("New.",0));
        // O nome que outro mod já tem fica com ele.
        let other=manifest("other",json!([{"name":"reverse-engineering","description":"d","instructions":"i"}]));
        let error=SKILLS.install(&connection,&other,&other.contributes["skills"]).expect_err("nome tomado");
        assert_eq!(Text::from(error).key,"mods.skillTaken");
        SKILLS.uninstall(&connection,&mod_,&mod_.contributes["skills"]).expect("desinstala");
        assert!(load(&connection).expect("lê").is_empty());
    }

    #[test] fn a_skill_the_person_installed_wins_over_the_one_of_a_mod() {
        let own=vec![Skill{name:"notes".into(),description:"mine".into(),..Default::default()}];
        let from_mods=vec![Skill{name:"notes".into(),description:"mod".into(),..Default::default()},Skill{name:"rea".into(),description:"mod".into(),..Default::default()}];
        let merged=merge(own,from_mods);
        assert_eq!(merged.iter().map(|skill|(skill.name.as_str(),skill.description.as_str())).collect::<Vec<_>>(),[("notes","mine"),("rea","mod")]);
    }
}
