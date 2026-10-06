//! Os servidores MCP que o desenvolvedor configura para os agentes: o
//! JayV os entrega a cada CLI na linha de comando, do jeito que ela sabe
//! receber, sem mexer nos arquivos de configuração de nenhuma.
//!
//! Moram no banco local, como os agentes: são deste computador (o comando
//! roda aqui) e podem levar segredos nas variáveis de ambiente, que não
//! sobem para o servidor.
//!
//! Chegam pela tela (Configurações › MCP) ou pelo chat (`/mcp` seguido da
//! configuração, de um comando ou de uma descrição): o texto colado é lido
//! aqui; o que não é configuração vai a um modelo, que devolve o rascunho.
//! Nos dois casos o servidor só é gravado depois que o desenvolvedor confere.

use crate::i18n::Text;
use crate::llm::AgentId;
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS mcp_servers (
  name TEXT PRIMARY KEY,
  server TEXT NOT NULL,
  updated_at TEXT NOT NULL
);";

pub const STDIO:&str="stdio";
pub const HTTP:&str="http";

/// Um servidor MCP. `stdio` sobe `command` com `args` e `env`; `http` fala
/// com `url`, com `headers`. `agents` são os agentes que o recebem (vazio é
/// todos os que sabem receber).
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",default)]
pub struct McpServer {
    pub name:String,
    pub transport:String,
    pub command:String,
    pub args:Vec<String>,
    pub env:BTreeMap<String,String>,
    pub url:String,
    pub headers:BTreeMap<String,String>,
    pub enabled:bool,
    pub agents:Vec<String>,
}

impl Default for McpServer {
    fn default()->Self { Self{name:String::new(),transport:STDIO.into(),command:String::new(),args:vec![],env:BTreeMap::new(),url:String::new(),headers:BTreeMap::new(),enabled:true,agents:vec![]} }
}

/// O Cursor não recebe MCP pela linha de comando: ele lê o `.cursor/mcp.json`
/// do projeto ou da pasta do usuário.
pub fn receives(agent:AgentId)->bool { !matches!(agent,AgentId::Cursor) }

impl McpServer {
    /// O servidor limpo, ou o motivo de não ser aceito. O nome vira chave de
    /// configuração no Codex e prefixo das ferramentas no Claude
    /// (`mcp__<nome>`): só letras, dígitos, `-` e `_`.
    pub fn checked(mut self)->Result<Self> {
        self.name=self.name.trim().to_string();
        if self.name.is_empty()||self.name.len()>40||!self.name.chars().all(|char|char.is_ascii_alphanumeric()||char=='-'||char=='_') { bail!(Text::new("mcp.invalid.name")); }
        if self.name=="jayv" { bail!(Text::new("mcp.invalid.reserved")); }
        self.command=self.command.trim().to_string();
        self.url=self.url.trim().to_string();
        self.args=self.args.into_iter().map(|arg|arg.trim().to_string()).filter(|arg|!arg.is_empty()).collect();
        self.env=self.env.into_iter().map(|(key,value)|(key.trim().to_string(),value)).filter(|(key,_)|!key.is_empty()).collect();
        self.headers=self.headers.into_iter().map(|(key,value)|(key.trim().to_string(),value)).filter(|(key,_)|!key.is_empty()).collect();
        if self.env.keys().any(|key|!key.chars().all(|char|char.is_ascii_alphanumeric()||char=='_')) { bail!(Text::new("mcp.invalid.env")); }
        self.agents.retain(|agent|AgentId::ALL.iter().any(|known|known.key()==agent&&receives(*known)));
        match self.transport.as_str() {
            STDIO=>{ if self.command.is_empty() { bail!(Text::new("mcp.invalid.command")); } self.url.clear(); self.headers.clear(); }
            HTTP=>{ if !(self.url.starts_with("https://")||self.url.starts_with("http://")) { bail!(Text::new("mcp.invalid.url")); } self.command.clear(); self.args.clear(); self.env.clear(); }
            _=>bail!(Text::new("mcp.invalid.transport")),
        }
        Ok(self)
    }

    pub fn serves(&self,agent:AgentId)->bool { self.enabled&&receives(agent)&&(self.agents.is_empty()||self.agents.iter().any(|known|known==agent.key())) }

    /// O servidor no formato `mcpServers` que o Claude (`--mcp-config`) e o
    /// Copilot (`--additional-mcp-config`) leem. O Copilot pede a lista das
    /// ferramentas liberadas: todas, porque ninguém responde à aprovação.
    fn entry(&self,copilot:bool)->Value {
        let mut entry=match self.transport.as_str() {
            HTTP=>json!({"type":"http","url":self.url,"headers":self.headers}),
            _=>json!({"type":if copilot {"local"} else {"stdio"},"command":self.command,"args":self.args,"env":self.env}),
        };
        if copilot { entry["tools"]=json!(["*"]); }
        entry
    }

    /// As chaves `-c` do Codex (`mcp_servers.<nome>.…`), em TOML.
    fn codex_overrides(&self)->Vec<String> {
        let key=|field:&str|format!("mcp_servers.{}.{field}",self.name);
        let toml=|value:&str|serde_json::to_string(value).unwrap_or_default();
        let table=|map:&BTreeMap<String,String>|format!("{{{}}}",map.iter().map(|(name,value)|format!("{}={}",toml(name),toml(value))).collect::<Vec<_>>().join(","));
        let mut overrides=Vec::new();
        match self.transport.as_str() {
            HTTP=>{
                overrides.push(format!("{}={}",key("url"),toml(&self.url)));
                if !self.headers.is_empty() { overrides.push(format!("{}={}",key("http_headers"),table(&self.headers))); }
            }
            _=>{
                overrides.push(format!("{}={}",key("command"),toml(&self.command)));
                overrides.push(format!("{}=[{}]",key("args"),self.args.iter().map(|arg|toml(arg)).collect::<Vec<_>>().join(",")));
                if !self.env.is_empty() { overrides.push(format!("{}={}",key("env"),table(&self.env))); }
            }
        }
        overrides
    }
}

/// A configuração `mcpServers` dos servidores que o agente recebe, com os que
/// já vinham (o índice de símbolos do JayV no Claude).
pub fn config_json(servers:&[McpServer],agent:AgentId,mut base:Map<String,Value>)->Option<String> {
    for server in servers.iter().filter(|server|server.serves(agent)) { base.insert(server.name.clone(),server.entry(agent==AgentId::Copilot)); }
    (!base.is_empty()).then(||json!({"mcpServers":base}).to_string())
}

/// Os argumentos que levam os servidores à linha de comando do agente. As
/// ferramentas deles rodam sem pergunta: sem terminal ninguém aprova, e quem
/// as configurou já as quis. O Claude as recebe no `--allowedTools` como
/// `mcp__<nome>`, à parte (`claude_tools`).
pub fn args_for(servers:&[McpServer],agent:AgentId)->Vec<String> {
    match agent {
        AgentId::Codex=>servers.iter().filter(|server|server.serves(agent)).flat_map(|server|server.codex_overrides()).flat_map(|value|["-c".to_string(),value]).collect(),
        AgentId::Copilot=>config_json(servers,agent,Map::new()).map(|config|vec!["--additional-mcp-config".to_string(),config]).unwrap_or_default(),
        AgentId::Claude|AgentId::Cursor=>vec![],
    }
}

pub fn claude_tools(servers:&[McpServer])->Vec<String> {
    servers.iter().filter(|server|server.serves(AgentId::Claude)).map(|server|format!("mcp__{}",server.name)).collect()
}

pub fn load(connection:&Connection)->Result<Vec<McpServer>> {
    let mut statement=connection.prepare("SELECT server FROM mcp_servers ORDER BY name")?;
    let rows=statement.query_map([],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().filter_map(|json|serde_json::from_str(&json).ok()).collect())
}

/// Grava a lista inteira, como a tela a mostra: o que saiu dela sai do banco.
pub fn save(connection:&mut Connection,servers:Vec<McpServer>)->Result<Vec<McpServer>> {
    let servers=servers.into_iter().map(McpServer::checked).collect::<Result<Vec<_>>>()?;
    let mut names=std::collections::HashSet::new();
    if let Some(repeated)=servers.iter().find(|server|!names.insert(server.name.clone())) { bail!(Text::new("mcp.invalid.duplicate").with("name",&repeated.name)); }
    let transaction=connection.transaction()?;
    transaction.execute("DELETE FROM mcp_servers",[])?;
    let now=chrono::Utc::now().to_rfc3339();
    for server in &servers { transaction.execute("INSERT INTO mcp_servers(name,server,updated_at) VALUES(?1,?2,?3)",params![server.name,serde_json::to_string(server)?,now])?; }
    transaction.commit()?;
    Ok(servers)
}

/// Lê o que foi colado no chat: a configuração `mcpServers` do Claude Desktop
/// e do Cursor, a `servers` do VS Code, um servidor sozinho, a linha do
/// `claude mcp add`, ou o comando que sobe o servidor (`npx -y …`). Devolve
/// vazio quando o texto não é nada disso — aí ele vai ao modelo.
pub fn parse(text:&str)->Vec<McpServer> {
    let text=text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    if let Some(value)=json_in(text) { return from_json(&value); }
    from_command(text).into_iter().collect()
}

fn json_in(text:&str)->Option<Value> {
    if let Ok(value)=serde_json::from_str::<Value>(text) { return value.is_object().then_some(value); }
    // Um par `"nome": {…}` sozinho, como vem copiado do meio de um arquivo.
    serde_json::from_str::<Value>(&format!("{{{}}}",text.trim_end_matches(','))).ok().filter(Value::is_object)
}

fn from_json(value:&Value)->Vec<McpServer> {
    let map=value.get("mcpServers").or_else(||value.get("servers")).or_else(||value.get("mcp").and_then(|mcp|mcp.get("servers")));
    if let Some(Value::Object(map))=map { return map.iter().filter_map(|(name,entry)|one(name,entry)).collect(); }
    // Um servidor sozinho, com ou sem nome dentro.
    if value.get("command").is_some()||value.get("url").is_some() {
        let name=value.get("name").and_then(Value::as_str).map(String::from).unwrap_or_else(||name_from(value.get("command").and_then(Value::as_str).unwrap_or_default(),&strings(value.get("args")),value.get("url").and_then(Value::as_str).unwrap_or_default()));
        return one(&name,value).into_iter().collect();
    }
    // `{"github": {"command": …}}`: o nome é a chave.
    if let Value::Object(map)=value { return map.iter().filter(|(_,entry)|entry.get("command").is_some()||entry.get("url").is_some()).filter_map(|(name,entry)|one(name,entry)).collect(); }
    vec![]
}

fn one(name:&str,entry:&Value)->Option<McpServer> {
    let text=|key:&str|entry.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let map=|key:&str|entry.get(key).and_then(Value::as_object).map(|map|map.iter().map(|(key,value)|(key.clone(),value.as_str().map(String::from).unwrap_or_else(||value.to_string()))).collect()).unwrap_or_default();
    let url=text("url");
    let server=if !url.is_empty() {
        McpServer{name:clean_name(name),transport:HTTP.into(),url,headers:map("headers"),..Default::default()}
    } else {
        McpServer{name:clean_name(name),transport:STDIO.into(),command:text("command"),args:strings(entry.get("args")),env:map("env"),..Default::default()}
    };
    server.checked().ok()
}

fn strings(value:Option<&Value>)->Vec<String> {
    value.and_then(Value::as_array).map(|items|items.iter().filter_map(|item|item.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// A linha de comando: `claude mcp add [--transport http] <nome> [-e K=V] -- <comando…>`,
/// `codex mcp add <nome> -- <comando…>`, ou só o comando.
fn from_command(text:&str)->Option<McpServer> {
    let line=text.lines().map(str::trim).find(|line|!line.is_empty())?;
    let words=shell_words(line);
    let mut rest=words.as_slice();
    let mut name=None;
    let mut env=BTreeMap::new();
    let mut transport=STDIO.to_string();
    if rest.len()>=3&&matches!(rest[0].as_str(),"claude"|"codex"|"gemini")&&rest[1]=="mcp"&&rest[2]=="add" {
        rest=&rest[3..];
        while let Some((first,tail))=rest.split_first() {
            match first.as_str() {
                "--"=>{ rest=tail; break; }
                "-e"|"--env" if !tail.is_empty()=>{ if let Some((key,value))=tail[0].split_once('=') { env.insert(key.to_string(),value.to_string()); } rest=&tail[1..]; }
                "-t"|"--transport" if !tail.is_empty()=>{ transport=if tail[0]=="stdio" {STDIO.into()} else {HTTP.into()}; rest=&tail[1..]; }
                "-s"|"--scope" if !tail.is_empty()=>{ rest=&tail[1..]; }
                flag if flag.starts_with('-')=>{ rest=tail; }
                word if name.is_none()=>{ name=Some(word.to_string()); rest=tail; }
                _=>break,
            }
        }
    }
    let (command,args)=rest.split_first()?;
    if transport==HTTP||command.starts_with("http://")||command.starts_with("https://") {
        let url=command.to_string();
        return McpServer{name:clean_name(&name.unwrap_or_else(||name_from("",&[],&url))),transport:HTTP.into(),url,..Default::default()}.checked().ok();
    }
    // Só o comando: tem de parecer quem sobe servidor MCP, senão é texto livre.
    let launcher=matches!(command.as_str(),"npx"|"uvx"|"bunx"|"pnpm"|"docker"|"node"|"python"|"python3"|"uv"|"deno");
    if name.is_none()&&!(launcher&&args.iter().any(|arg|arg.contains("mcp")||arg.contains("modelcontextprotocol")||arg.contains("server-"))) { return None; }
    let name=name.unwrap_or_else(||name_from(command,args,""));
    McpServer{name:clean_name(&name),command:command.clone(),args:args.to_vec(),env,..Default::default()}.checked().ok()
}

/// As palavras da linha, respeitando aspas simples e duplas.
fn shell_words(line:&str)->Vec<String> {
    let mut words=Vec::new();
    let mut current=String::new();
    let mut quote=None;
    let mut started=false;
    for char in line.chars() {
        match (quote,char) {
            (Some(open),char) if char==open=>quote=None,
            (Some(_),char)=>current.push(char),
            (None,'"'|'\'')=>{ quote=Some(char); started=true; }
            (None,char) if char.is_whitespace()=>{ if started||!current.is_empty() { words.push(std::mem::take(&mut current)); started=false; } }
            (None,char)=>current.push(char),
        }
    }
    if started||!current.is_empty() { words.push(current); }
    words
}

/// Um nome a partir do pacote (`@modelcontextprotocol/server-github` vira
/// `github`) ou do endereço.
fn name_from(command:&str,args:&[String],url:&str)->String {
    if !url.is_empty() {
        let host=url.split("://").nth(1).unwrap_or(url).split(['/',':']).next().unwrap_or_default();
        let parts=host.split('.').collect::<Vec<_>>();
        return parts.iter().rev().nth(1).or(parts.first()).copied().unwrap_or("mcp").to_string();
    }
    let package=args.iter().find(|arg|!arg.starts_with('-')&&*arg!="run"&&*arg!="-y").map(String::as_str).unwrap_or(command);
    let last=package.rsplit('/').next().unwrap_or(package).split('@').next().unwrap_or(package);
    let trimmed=last.trim_start_matches("server-").trim_start_matches("mcp-server-").trim_end_matches("-mcp-server").trim_end_matches("-mcp").trim_start_matches("mcp-");
    if trimmed.is_empty() { "mcp".into() } else { trimmed.into() }
}

fn clean_name(name:&str)->String {
    let cleaned:String=name.trim().chars().map(|char|if char.is_ascii_alphanumeric()||char=='-'||char=='_' {char} else {'-'}).collect();
    let cleaned=cleaned.trim_matches('-').to_ascii_lowercase();
    cleaned.chars().take(40).collect()
}

/// O que o modelo recebe quando o texto não é configuração: devolver o
/// servidor em JSON, e nada além disso.
pub const DRAFT_INSTRUCTIONS:&str="The developer wants to add an MCP server to their coding agents and described it below. Answer with one JSON object and nothing else, in the Claude Desktop format: {\"mcpServers\":{\"<short-name>\":{\"command\":\"...\",\"args\":[...],\"env\":{...}}}} for a local server, or {\"mcpServers\":{\"<short-name>\":{\"url\":\"https://...\",\"headers\":{...}}}} for a remote one. Use the official package or endpoint you know for that service. Put placeholders like \"<YOUR_TOKEN>\" where a secret is needed; never invent a secret. If you do not know a server for it, answer {}.";

/// O rascunho que o modelo devolveu, lido como qualquer configuração colada.
pub fn from_model(answer:&str)->Vec<McpServer> {
    let start=answer.find('{');
    let end=answer.rfind('}');
    match (start,end) { (Some(start),Some(end)) if end>start=>parse(&answer[start..=end]), _=>vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn claude_desktop_and_vscode_configs_are_read() {
        let desktop=parse(r#"{"mcpServers":{"github":{"command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_PERSONAL_ACCESS_TOKEN":"<TOKEN>"}},"docs":{"url":"https://mcp.example.com/mcp"}}}"#);
        assert_eq!(desktop.len(),2);
        let github=desktop.iter().find(|server|server.name=="github").expect("github");
        assert_eq!((github.transport.as_str(),github.command.as_str(),github.args.len(),github.env.len()),(STDIO,"npx",2,1));
        assert_eq!(desktop.iter().find(|server|server.name=="docs").expect("docs").transport,HTTP);
        let vscode=parse(r#"{"servers":{"fs":{"type":"stdio","command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","."]}}}"#);
        assert_eq!(vscode[0].name,"fs");
        let pair=parse(r#""sentry": {"url": "https://mcp.sentry.dev/mcp"},"#);
        assert_eq!(pair[0].name,"sentry");
    }

    #[test] fn command_lines_are_read_and_free_text_is_not() {
        let added=parse("claude mcp add github -e GITHUB_TOKEN=abc -- npx -y @modelcontextprotocol/server-github");
        assert_eq!((added[0].name.as_str(),added[0].command.as_str(),added[0].env.get("GITHUB_TOKEN").map(String::as_str)),("github","npx",Some("abc")));
        let http=parse("claude mcp add --transport http linear https://mcp.linear.app/mcp");
        assert_eq!((http[0].name.as_str(),http[0].url.as_str()),("linear","https://mcp.linear.app/mcp"));
        let bare=parse("npx -y @modelcontextprotocol/server-postgres postgresql://localhost/db");
        assert_eq!(bare[0].name,"postgres");
        assert!(parse("quero o MCP do GitHub").is_empty(),"texto livre vai ao modelo");
    }

    #[test] fn each_agent_receives_the_servers_its_own_way() {
        let servers=parse(r#"{"mcpServers":{"github":{"command":"npx","args":["-y","pkg"],"env":{"TOKEN":"x"}},"docs":{"url":"https://mcp.example.com/mcp"}}}"#);
        let codex=args_for(&servers,AgentId::Codex);
        assert!(codex.contains(&"mcp_servers.github.command=\"npx\"".to_string()),"{codex:?}");
        assert!(codex.contains(&"mcp_servers.github.args=[\"-y\",\"pkg\"]".to_string()));
        assert!(codex.contains(&"mcp_servers.github.env={\"TOKEN\"=\"x\"}".to_string()));
        assert!(codex.contains(&"mcp_servers.docs.url=\"https://mcp.example.com/mcp\"".to_string()));
        let copilot=args_for(&servers,AgentId::Copilot);
        assert_eq!(copilot[0],"--additional-mcp-config");
        assert!(copilot[1].contains("\"type\":\"local\"")&&copilot[1].contains("\"tools\":[\"*\"]"));
        assert!(args_for(&servers,AgentId::Cursor).is_empty());
        assert_eq!(claude_tools(&servers),vec!["mcp__docs".to_string(),"mcp__github".to_string()]);
        let only_codex=McpServer{agents:vec!["codex".into()],..servers[0].clone()};
        assert!(!only_codex.serves(AgentId::Claude)&&only_codex.serves(AgentId::Codex));
        let off=McpServer{enabled:false,..servers[0].clone()};
        assert!(args_for(&[off],AgentId::Codex).is_empty());
    }

    #[test] fn bad_servers_are_refused() {
        assert!(McpServer{name:"a b".into(),command:"x".into(),..Default::default()}.checked().is_err());
        assert!(McpServer{name:"jayv".into(),command:"x".into(),..Default::default()}.checked().is_err());
        assert!(McpServer{name:"ok".into(),..Default::default()}.checked().is_err(),"stdio sem comando");
        assert!(McpServer{name:"ok".into(),transport:HTTP.into(),url:"ftp://x".into(),..Default::default()}.checked().is_err());
    }

    #[test] fn the_model_draft_is_read_like_a_pasted_config() {
        let drafted=from_model("Aqui está:\n```json\n{\"mcpServers\":{\"github\":{\"command\":\"npx\",\"args\":[\"-y\",\"@modelcontextprotocol/server-github\"]}}}\n```");
        assert_eq!(drafted[0].name,"github");
        assert!(from_model("{}").is_empty());
    }
}
