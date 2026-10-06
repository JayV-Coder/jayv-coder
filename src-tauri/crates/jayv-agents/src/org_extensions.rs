//! Os servidores MCP e as skills que a organização dá aos membros: o owner e
//! os maintainers os cadastram no painel do site, e o app os recebe pela
//! sincronização (`my_org_extensions`), como a política de LLM: só descem e
//! ficam num cache local, fora da fila de sync.
//!
//! Valem para os projetos da organização (o mesmo vínculo da política) e se
//! somam ao que a pessoa já tem. No app aparecem como "da organização" e não
//! se editam nem se removem: a lista sai do que o servidor devolveu a cada
//! volta. O servidor da organização pode levar segredos (variáveis de
//! ambiente, cabeçalhos): eles ficam no cache só para entregar ao agente, e
//! a tela nunca os recebe (`listing`).

use crate::mcp::McpServer;
use crate::skills::Skill;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS org_extensions (
  project_id TEXT NOT NULL,
  org_slug TEXT NOT NULL,
  own INTEGER NOT NULL,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  payload TEXT NOT NULL,
  PRIMARY KEY (project_id, org_slug, kind, name)
);";

const MCP:&str="mcp";
const SKILL:&str="skill";
const DESCRIPTION_MAX:usize=1024;

/// Uma linha de `my_org_extensions`: o que uma organização dá a um projeto.
/// `project_id` vazio não é projeto nenhum: só serve para a tela listar o
/// que a organização dá.
#[derive(Debug,Clone,Default,Deserialize)]
#[serde(default)]
pub struct Row { pub project_id:String, pub org_slug:String, pub own:bool, pub mcp:Vec<Value>, pub skills:Vec<Value> }

/// Uma skill da organização como o servidor a manda.
#[derive(Debug,Deserialize,Serialize)]
struct SkillPayload { name:String, description:String, body:String }

/// Troca o cache inteiro pelo que o servidor devolveu: o que a organização
/// tirou deixa de valer. Servidor ou skill que esta versão não aceita é
/// ignorado, não derruba a volta.
pub fn replace_all(connection:&mut Connection,rows:&[Row])->Result<()> {
    let transaction=connection.transaction()?;
    transaction.execute("DELETE FROM org_extensions",[])?;
    for row in rows {
        for value in &row.mcp {
            let Ok(server)=serde_json::from_value::<McpServer>(value.clone()) else { continue };
            let Ok(mut server)=server.checked() else { continue };
            server.enabled=true;
            transaction.execute("INSERT OR REPLACE INTO org_extensions(project_id,org_slug,own,kind,name,payload) VALUES(?1,?2,?3,?4,?5,?6)",
                params![row.project_id,row.org_slug,row.own as i64,MCP,server.name,serde_json::to_string(&server)?])?;
        }
        for value in &row.skills {
            let Ok(mut skill)=serde_json::from_value::<SkillPayload>(value.clone()) else { continue };
            skill.description=skill.description.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(DESCRIPTION_MAX).collect();
            let name_ok=!skill.name.is_empty()&&skill.name.len()<=64&&skill.name.chars().all(|char|char.is_ascii_alphanumeric()||char=='-'||char=='_');
            if !name_ok||skill.description.is_empty()||skill.body.trim().is_empty() { continue; }
            transaction.execute("INSERT OR REPLACE INTO org_extensions(project_id,org_slug,own,kind,name,payload) VALUES(?1,?2,?3,?4,?5,?6)",
                params![row.project_id,row.org_slug,row.own as i64,SKILL,skill.name,serde_json::to_string(&skill)?])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

/// Os servidores e as skills que as organizações dão ao projeto do chat. Dois
/// com o mesmo nome: ganha a organização do próprio projeto e, depois, a de
/// slug menor.
pub fn for_chat(connection:&Connection,chat_id:&str)->Result<(Vec<McpServer>,Vec<Skill>)> {
    let mut statement=connection.prepare(
        "SELECT e.org_slug,e.kind,e.payload FROM org_extensions e JOIN chats c ON c.project_id=e.project_id WHERE c.id=?1 ORDER BY e.own DESC,e.org_slug,e.name")?;
    let rows=statement.query_map([chat_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut servers,mut skills)=(vec![],vec![]);
    let (mut server_names,mut skill_names)=(HashSet::new(),HashSet::new());
    for (org,kind,payload) in rows {
        if kind==MCP {
            if let Ok(server)=serde_json::from_str::<McpServer>(&payload) { if server_names.insert(server.name.clone()) { servers.push(server); } }
        } else if let Ok(skill)=serde_json::from_str::<SkillPayload>(&payload) {
            if skill_names.insert(skill.name.clone()) {
                skills.push(Skill{name:skill.name,description:skill.description,path:String::new(),enabled:true,installed_at:String::new(),origin:Some(org),inline:Some(skill.body)});
            }
        }
    }
    Ok((servers,skills))
}

/// A lista do que o usuário tem de cada organização, para a tela: sem
/// variáveis de ambiente, cabeçalhos nem argumentos, que podem levar
/// segredos.
pub fn listing(connection:&Connection)->Result<OrgExtensions> {
    let mut statement=connection.prepare("SELECT DISTINCT org_slug,kind,name,payload FROM org_extensions ORDER BY org_slug,name")?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut listing=OrgExtensions::default();
    for (org,kind,payload) in rows {
        if kind==MCP {
            if let Ok(server)=serde_json::from_str::<McpServer>(&payload) {
                let (command,url)=(server.command.split_whitespace().next().unwrap_or_default().to_string(),host_of(&server.url));
                listing.mcp.push(OrgMcp{org,name:server.name,transport:server.transport,command,url,agents:server.agents});
            }
        } else if let Ok(skill)=serde_json::from_str::<SkillPayload>(&payload) {
            listing.skills.push(OrgSkill{org,name:skill.name,description:skill.description});
        }
    }
    Ok(listing)
}

/// O endereço sem caminho nem consulta, que podem levar um token.
fn host_of(url:&str)->String {
    let (scheme,rest)=url.split_once("://").unwrap_or(("",url));
    let host=rest.split(['/','?','#']).next().unwrap_or_default();
    let host=host.rsplit('@').next().unwrap_or_default();
    if scheme.is_empty() { host.to_string() } else { format!("{scheme}://{host}") }
}

#[derive(Debug,Clone,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub struct OrgExtensions { pub mcp:Vec<OrgMcp>, pub skills:Vec<OrgSkill> }

#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct OrgMcp { pub org:String, pub name:String, pub transport:String, pub command:String, pub url:String, pub agents:Vec<String> }

#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct OrgSkill { pub org:String, pub name:String, pub description:String }

/// Os servidores da pessoa e os da organização: com o mesmo nome, vale o da
/// organização.
pub fn merge_servers(own:Vec<McpServer>,org:Vec<McpServer>)->Vec<McpServer> {
    let taken:HashSet<&str>=org.iter().map(|server|server.name.as_str()).collect();
    let mut all:Vec<McpServer>=own.into_iter().filter(|server|!taken.contains(server.name.as_str())).collect();
    all.extend(org);
    all
}

/// As skills da pessoa e as da organização: com o mesmo nome, vale a da
/// organização.
pub fn merge_skills(own:Vec<Skill>,org:Vec<Skill>)->Vec<Skill> {
    let taken:HashSet<&str>=org.iter().map(|skill|skill.name.as_str()).collect();
    let mut all:Vec<Skill>=own.into_iter().filter(|skill|!taken.contains(skill.name.as_str())).collect();
    all.extend(org);
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch("CREATE TABLE chats(id TEXT PRIMARY KEY, project_id TEXT);").expect("chats");
        connection.execute_batch(SCHEMA).expect("esquema");
        connection.execute("INSERT INTO chats VALUES('c1','p1'),('c2','p2')",[]).expect("chats");
        connection
    }

    fn server(name:&str,command:&str)->Value { json!({"name":name,"transport":"stdio","command":command,"args":["-y","pkg"],"env":{"TOKEN":"secret"},"url":"","headers":{},"enabled":true,"agents":[]}) }
    fn skill(name:&str)->Value { json!({"name":name,"description":"Writes notes.","body":"# Steps\nDo it."}) }

    #[test] fn the_project_gets_the_servers_and_skills_of_its_organization() {
        let mut connection=database();
        replace_all(&mut connection,&[
            Row{project_id:"p1".into(),org_slug:"acme".into(),own:true,mcp:vec![server("github","npx")],skills:vec![skill("notes")]},
            Row{project_id:"".into(),org_slug:"acme".into(),own:false,mcp:vec![server("github","npx")],skills:vec![skill("notes")]},
        ]).expect("troca");
        let (servers,skills)=for_chat(&connection,"c1").expect("chat");
        assert_eq!(servers.len(),1);
        assert!(servers[0].enabled&&servers[0].env.contains_key("TOKEN"),"o agente recebe o segredo");
        assert_eq!((skills[0].origin.as_deref(),skills[0].inline.as_deref()),(Some("acme"),Some("# Steps\nDo it.")));
        let (none,_)=for_chat(&connection,"c2").expect("outro chat");
        assert!(none.is_empty(),"outro projeto não recebe");
    }

    #[test] fn the_own_organization_wins_a_name_clash_and_the_cache_is_replaced_whole() {
        let mut connection=database();
        replace_all(&mut connection,&[
            Row{project_id:"p1".into(),org_slug:"zeta".into(),own:true,mcp:vec![server("db","zeta-cmd")],skills:vec![]},
            Row{project_id:"p1".into(),org_slug:"alpha".into(),own:false,mcp:vec![server("db","alpha-cmd"),server("extra","x")],skills:vec![]},
        ]).expect("troca");
        let (servers,_)=for_chat(&connection,"c1").expect("chat");
        assert_eq!(servers.iter().map(|server|(server.name.as_str(),server.command.as_str())).collect::<Vec<_>>(),vec![("db","zeta-cmd"),("extra","x")]);
        replace_all(&mut connection,&[]).expect("vazio");
        assert!(for_chat(&connection,"c1").expect("chat").0.is_empty(),"o que a organização tirou sai");
    }

    #[test] fn invalid_entries_are_skipped() {
        let mut connection=database();
        replace_all(&mut connection,&[Row{project_id:"p1".into(),org_slug:"acme".into(),own:true,
            mcp:vec![server("jayv","x"),server("bad name","x"),json!({"name":"web","transport":"http","url":"ftp://x"}),json!("lixo")],
            skills:vec![json!({"name":"a b","description":"d","body":"b"}),json!({"name":"ok","description":"","body":"b"}),json!({"name":"ok","description":"d","body":"  "})]}]).expect("troca");
        let (servers,skills)=for_chat(&connection,"c1").expect("chat");
        assert!(servers.is_empty()&&skills.is_empty());
    }

    #[test] fn the_screen_list_never_carries_secrets() {
        let mut connection=database();
        let mut http=server("docs","");
        http["transport"]=json!("http"); http["url"]=json!("https://user:pass@docs.example.com/mcp?token=abc"); http["headers"]=json!({"Authorization":"Bearer abc"});
        replace_all(&mut connection,&[Row{project_id:"".into(),org_slug:"acme".into(),own:false,mcp:vec![server("github","npx"),http],skills:vec![skill("notes")]}]).expect("troca");
        let listing=listing(&connection).expect("lista");
        let text=serde_json::to_string(&listing).expect("json");
        assert!(!text.contains("secret")&&!text.contains("abc")&&!text.contains("pass"),"{text}");
        assert!(text.contains("https://docs.example.com")&&text.contains("\"command\":\"npx\""));
        assert_eq!((listing.mcp.len(),listing.skills.len()),(2,1));
    }

    #[test] fn the_organization_wins_over_the_persons_own_with_the_same_name() {
        let own=vec![McpServer{name:"db".into(),command:"mine".into(),..Default::default()},McpServer{name:"keep".into(),command:"k".into(),..Default::default()}];
        let org=vec![McpServer{name:"db".into(),command:"theirs".into(),..Default::default()}];
        let merged=merge_servers(own,org);
        assert_eq!(merged.iter().map(|server|server.command.as_str()).collect::<Vec<_>>(),vec!["k","theirs"]);
    }
}
