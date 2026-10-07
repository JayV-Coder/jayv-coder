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

/// Todo agente recebe os servidores, cada um do jeito que sabe: Claude, Codex
/// e Copilot pela linha de comando, o Kilo Code pela configuração em linha
/// (`KILO_CONFIG_CONTENT`), o Cursor por um bloco gerido do `~/.cursor/mcp.json`
/// e os gateways de API pelo próprio JayV (`mcp_client`). Só com o "Aprovar
/// servidores MCP" do agente ligado (`AgentSettings::with_mcp`).
pub fn receives(_agent:AgentId)->bool { true }

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
        AgentId::Claude|AgentId::Cursor|AgentId::Kilo|AgentId::Openrouter|AgentId::Litellm=>vec![],
    }
}

/// A configuração em linha do Kilo Code (`KILO_CONFIG_CONTENT`, que ele soma à
/// dele): os servidores e a permissão das ferramentas deles, que rodam sem
/// pergunta. As ferramentas chegam ao modelo como `<servidor>_<ferramenta>`.
pub fn kilo_config(servers:&[McpServer])->Option<String> {
    let mut mcp=Map::new();
    let mut permission=Map::new();
    for server in servers.iter().filter(|server|server.serves(AgentId::Kilo)) {
        let entry=match server.transport.as_str() {
            HTTP=>json!({"type":"remote","url":server.url,"headers":server.headers,"enabled":true}),
            _=>{ let mut command=vec![server.command.clone()]; command.extend(server.args.clone()); json!({"type":"local","command":command,"environment":server.env,"enabled":true}) }
        };
        mcp.insert(server.name.clone(),entry);
        permission.insert(format!("{}_*",server.name.replace('-',"_")),json!("allow"));
    }
    (!mcp.is_empty()).then(||json!({"mcp":mcp,"permission":permission}).to_string())
}

/// O arquivo do Cursor com os servidores do JayV, e o outro, ao lado, com os
/// nomes que o JayV pôs nele — só esses saem na volta seguinte.
const CURSOR_FILE:&str="mcp.json";
const CURSOR_MANAGED:&str="jayv-mcp.json";

/// Põe os servidores no `~/.cursor/mcp.json`, único caminho do Cursor: ele
/// não recebe MCP pela linha de comando. O que o JayV pôs numa vez e saiu da
/// lista sai do arquivo; o que a pessoa escreveu lá (outro nome, ou o mesmo)
/// nunca é tocado. Sem servidores e sem nada posto antes, não escreve.
pub fn sync_cursor(servers:&[McpServer])->Result<()> {
    let Some(home)=dirs::home_dir() else { return Ok(()) };
    sync_cursor_in(&home.join(".cursor"),servers)
}

fn sync_cursor_in(dir:&std::path::Path,servers:&[McpServer])->Result<()> {
    let file=dir.join(CURSOR_FILE);
    let managed_file=dir.join(CURSOR_MANAGED);
    let before:Vec<String>=std::fs::read_to_string(&managed_file).ok().and_then(|text|serde_json::from_str(&text).ok()).unwrap_or_default();
    let mine:Vec<&McpServer>=servers.iter().filter(|server|server.serves(AgentId::Cursor)).collect();
    if mine.is_empty()&&before.is_empty() { return Ok(()); }
    let mut config=match std::fs::read_to_string(&file) {
        Ok(text) if text.trim().is_empty()=>json!({}),
        Ok(text)=>serde_json::from_str::<Value>(&text).ok().filter(Value::is_object).ok_or_else(||anyhow::Error::new(Text::new("mcp.cursorFile").with("path",file.display().to_string())))?,
        Err(_)=>json!({}),
    };
    let object=config.as_object_mut().expect("objeto");
    let map=object.entry("mcpServers").or_insert_with(||json!({}));
    let Some(map)=map.as_object_mut() else { bail!(Text::new("mcp.cursorFile").with("path",file.display().to_string())) };
    for name in &before { map.remove(name); }
    let mut placed=Vec::new();
    for server in mine {
        if map.contains_key(&server.name) { continue; }
        map.insert(server.name.clone(),if server.transport==HTTP { json!({"url":server.url,"headers":server.headers}) } else { json!({"command":server.command,"args":server.args,"env":server.env}) });
        placed.push(server.name.clone());
    }
    if map.is_empty()&&placed.is_empty()&&!file.exists() { return Ok(()); }
    std::fs::create_dir_all(dir)?;
    write_private(&file,&serde_json::to_string_pretty(&config)?)?;
    if placed!=before { write_private(&managed_file,&serde_json::to_string(&placed)?)?; }
    Ok(())
}

/// Grava por um arquivo ao lado e renomeia: dois pedidos em paralelo nunca
/// deixam o arquivo pela metade. Só o dono lê (pode levar segredos).
fn write_private(path:&std::path::Path,text:&str)->Result<()> {
    let temporary=path.with_extension(format!("tmp{}",std::process::id()));
    std::fs::write(&temporary,text)?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&temporary,std::fs::Permissions::from_mode(0o600))?; }
    std::fs::rename(&temporary,path)?;
    Ok(())
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

/// Os quatro servidores oficiais que já vêm cadastrados, como entradas comuns
/// (desligar, editar e remover valem para eles). `fetch` e `git` não existem
/// no npm: os oficiais são em Python e sobem pelo `uvx`, que precisa estar
/// instalado.
pub fn defaults()->Vec<McpServer> {
    let stdio=|name:&str,command:&str,args:&[&str]|McpServer{name:name.into(),command:command.into(),args:args.iter().map(|arg|arg.to_string()).collect(),..Default::default()};
    vec![
        stdio("sequential-thinking","npx",&["-y","@modelcontextprotocol/server-sequential-thinking"]),
        stdio("fetch","uvx",&["mcp-server-fetch"]),
        stdio("git","uvx",&["mcp-server-git"]),
        stdio("memory","npx",&["-y","@modelcontextprotocol/server-memory"]),
    ]
}

const DEFAULTS_SEEDED:&str="mcp_defaults_v1";

/// Cadastra os servidores de `defaults` uma única vez por instalação: quem
/// remove um deles não o vê voltar, e um servidor da pessoa com o mesmo nome
/// fica como está.
pub fn seed_defaults(connection:&Connection)->Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    if connection.query_row("SELECT 1 FROM app_metadata WHERE key=?1",[DEFAULTS_SEEDED],|_|Ok(())).is_ok() { return Ok(()); }
    let now=chrono::Utc::now().to_rfc3339();
    for server in defaults() { connection.execute("INSERT OR IGNORE INTO mcp_servers(name,server,updated_at) VALUES(?1,?2,?3)",params![server.name,serde_json::to_string(&server)?,now])?; }
    connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,'1')",[DEFAULTS_SEEDED])?;
    Ok(())
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

    #[test] fn the_four_official_servers_come_once_and_stay_removed() {
        let mut connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(SCHEMA).expect("esquema");
        seed_defaults(&connection).expect("semeia");
        let names=|connection:&Connection|load(connection).expect("lê").into_iter().map(|server|server.name).collect::<Vec<_>>();
        assert_eq!(names(&connection),vec!["fetch","git","memory","sequential-thinking"]);
        assert!(load(&connection).expect("lê").iter().all(|server|server.enabled&&server.clone().checked().is_ok()));
        let kept:Vec<McpServer>=load(&connection).expect("lê").into_iter().filter(|server|server.name!="git").collect();
        save(&mut connection,kept).expect("remove");
        seed_defaults(&connection).expect("de novo");
        assert_eq!(names(&connection),vec!["fetch","memory","sequential-thinking"],"o removido não volta");
    }

    #[test] fn a_persons_own_server_with_a_default_name_is_not_overwritten() {
        let mut connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(SCHEMA).expect("esquema");
        save(&mut connection,vec![McpServer{name:"fetch".into(),command:"mine".into(),..Default::default()}]).expect("grava");
        seed_defaults(&connection).expect("semeia");
        let fetch=load(&connection).expect("lê").into_iter().find(|server|server.name=="fetch").expect("fetch");
        assert_eq!(fetch.command,"mine");
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

    #[test] fn every_agent_is_served_unless_the_server_names_others() {
        let servers=parse(r#"{"mcpServers":{"github":{"command":"npx","args":["pkg"]}}}"#);
        assert!(AgentId::ALL.into_iter().all(|agent|servers[0].serves(agent)));
        let only=McpServer{agents:vec!["litellm".into()],..servers[0].clone()};
        assert!(only.serves(AgentId::Litellm)&&!only.serves(AgentId::Kilo));
        assert_eq!(only.clone().checked().expect("válido").agents,vec!["litellm".to_string()]);
    }

    #[test] fn kilo_gets_inline_config_with_the_tool_permission() {
        let servers=parse(r#"{"mcpServers":{"my-git":{"command":"npx","args":["-y","pkg"],"env":{"T":"x"}},"docs":{"url":"https://mcp.example.com/mcp","headers":{"A":"b"}}}}"#);
        let config:Value=serde_json::from_str(&kilo_config(&servers).expect("configuração")).expect("json");
        assert_eq!(config["mcp"]["my-git"],json!({"type":"local","command":["npx","-y","pkg"],"environment":{"T":"x"},"enabled":true}));
        assert_eq!(config["mcp"]["docs"]["type"],"remote");
        assert_eq!(config["permission"]["my_git_*"],"allow");
        assert!(kilo_config(&[]).is_none());
    }

    #[test] fn the_cursor_file_keeps_what_the_person_wrote_and_removes_only_what_jayv_placed() {
        let dir=tempfile::tempdir().expect("pasta");
        let file=dir.path().join(CURSOR_FILE);
        std::fs::write(&file,r#"{"mcpServers":{"mine":{"command":"x"},"github":{"command":"own"}},"other":1}"#).expect("escrita");
        let servers=parse(r#"{"mcpServers":{"github":{"command":"npx","args":["pkg"]},"docs":{"url":"https://mcp.example.com/mcp"}}}"#);
        sync_cursor_in(dir.path(),&servers).expect("sincroniza");
        let read=||serde_json::from_str::<Value>(&std::fs::read_to_string(&file).expect("leitura")).expect("json");
        let config=read();
        assert_eq!(config["mcpServers"]["github"]["command"],"own","o nome da pessoa vence");
        assert_eq!(config["mcpServers"]["docs"]["url"],"https://mcp.example.com/mcp");
        assert_eq!(config["mcpServers"]["mine"]["command"],"x");
        assert_eq!(config["other"],1);
        sync_cursor_in(dir.path(),&[]).expect("limpa");
        let config=read();
        assert!(config["mcpServers"].get("docs").is_none(),"o que o JayV pôs sai");
        assert_eq!(config["mcpServers"]["github"]["command"],"own");
        assert!(config["mcpServers"].get("mine").is_some());
    }

    #[test] fn without_servers_and_without_a_file_nothing_is_written() {
        let dir=tempfile::tempdir().expect("pasta");
        sync_cursor_in(&dir.path().join(".cursor"),&[]).expect("nada");
        assert!(!dir.path().join(".cursor").exists());
    }

    #[test] fn a_cursor_file_that_is_not_json_is_left_alone() {
        let dir=tempfile::tempdir().expect("pasta");
        std::fs::write(dir.path().join(CURSOR_FILE),"{ não é json").expect("escrita");
        assert!(sync_cursor_in(dir.path(),&parse(r#"{"mcpServers":{"docs":{"url":"https://m.example.com/mcp"}}}"#)).is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join(CURSOR_FILE)).expect("leitura"),"{ não é json");
    }
}
