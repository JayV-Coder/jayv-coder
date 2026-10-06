//! O JayV como cliente MCP, para os agentes que só falam por API (OpenRouter e
//! LiteLLM): eles não sobem servidor nenhum, então o app sobe, lista as
//! ferramentas, entrega a lista ao modelo e roda as chamadas que ele pedir.
//!
//! Dois transportes, os mesmos de `mcp::McpServer`: `stdio` (um processo que
//! fala JSON-RPC por linhas) e `http` (um POST por mensagem, com a resposta em
//! JSON ou em `text/event-stream`).

use crate::mcp::{McpServer,HTTP};
use anyhow::{anyhow,bail,Result};
use serde_json::{json,Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt,AsyncWriteExt,BufReader};
use tokio::process::{Child,ChildStdin,ChildStdout,Command};

const PROTOCOL:&str="2025-03-26";
const LIST_LIMIT:Duration=Duration::from_secs(30);
const CALL_LIMIT:Duration=Duration::from_secs(120);
/// O que uma ferramenta devolve ao modelo, no máximo: a resposta de um
/// servidor sem limite estouraria a janela de contexto.
const OUTPUT_LIMIT:usize=24_000;

/// Uma ferramenta que o servidor oferece.
#[derive(Debug,Clone,PartialEq)]
pub struct Tool { pub name:String, pub description:String, pub schema:Value }

enum Transport {
    Stdio{child:Child,stdin:ChildStdin,lines:tokio::io::Lines<BufReader<ChildStdout>>},
    Http{client:reqwest::Client,url:String,headers:Vec<(String,String)>,session:Option<String>},
}

pub struct Client { pub server:String, transport:Transport, next:u64 }

impl Client {
    /// Sobe (ou chama) o servidor e faz o aperto de mão do protocolo.
    pub async fn connect(server:&McpServer)->Result<Self> {
        let transport=if server.transport==HTTP {
            let client=crate::lockdown::http_client(CALL_LIMIT).build()?;
            Transport::Http{client,url:server.url.clone(),headers:server.headers.iter().map(|(key,value)|(key.clone(),value.clone())).collect(),session:None}
        } else {
            let found=crate::llm::locate(&server.command);
            let (program,lead)=match &found { Some(path)=>crate::llm::launcher(path), None=>(server.command.clone().into(),vec![]) };
            let mut process=Command::new(&program);
            crate::providers::quiet(&mut process);
            if let Some(path)=found.as_deref().and_then(crate::llm::agent_path) { process.env("PATH",path); }
            process.args(lead).args(&server.args).envs(&server.env).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
            let mut child=process.spawn().map_err(|error|anyhow!("MCP server {}: {error}",server.name))?;
            let stdin=child.stdin.take().ok_or_else(||anyhow!("MCP server {}: no stdin",server.name))?;
            let stdout=child.stdout.take().ok_or_else(||anyhow!("MCP server {}: no stdout",server.name))?;
            Transport::Stdio{child,stdin,lines:BufReader::new(stdout).lines()}
        };
        let mut client=Self{server:server.name.clone(),transport,next:0};
        client.call("initialize",json!({"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"jayv","version":env!("CARGO_PKG_VERSION")}}),LIST_LIMIT).await?;
        client.notify("notifications/initialized").await?;
        Ok(client)
    }

    /// Todas as ferramentas, página a página.
    pub async fn tools(&mut self)->Result<Vec<Tool>> {
        let mut tools=Vec::new();
        let mut cursor:Option<String>=None;
        for _ in 0..20 {
            let params=cursor.as_ref().map(|cursor|json!({"cursor":cursor})).unwrap_or_else(||json!({}));
            let page=self.call("tools/list",params,LIST_LIMIT).await?;
            for tool in page.get("tools").and_then(Value::as_array).into_iter().flatten() {
                let Some(name)=tool.get("name").and_then(Value::as_str) else { continue };
                tools.push(Tool{name:name.into(),description:tool.get("description").and_then(Value::as_str).unwrap_or_default().into(),schema:tool.get("inputSchema").cloned().unwrap_or_else(||json!({"type":"object"}))});
            }
            cursor=page.get("nextCursor").and_then(Value::as_str).map(String::from);
            if cursor.is_none() { break; }
        }
        Ok(tools)
    }

    /// Roda uma ferramenta e devolve o texto dela. O erro que o servidor
    /// relata (`isError`) volta como texto, para o modelo ler e se corrigir.
    pub async fn run(&mut self,tool:&str,arguments:Value)->Result<String> {
        let result=self.call("tools/call",json!({"name":tool,"arguments":arguments}),CALL_LIMIT).await?;
        let text=result.get("content").and_then(Value::as_array).into_iter().flatten()
            .map(|part|part.get("text").and_then(Value::as_str).map(String::from).unwrap_or_else(||format!("[{}]",part.get("type").and_then(Value::as_str).unwrap_or("content"))))
            .collect::<Vec<_>>().join("\n");
        let text=if text.is_empty() { result.get("structuredContent").map(Value::to_string).unwrap_or_default() } else { text };
        let text=if text.chars().count()>OUTPUT_LIMIT { format!("{}\n[truncated]",text.chars().take(OUTPUT_LIMIT).collect::<String>()) } else { text };
        Ok(if result.get("isError").and_then(Value::as_bool).unwrap_or(false) { format!("Error: {text}") } else { text })
    }

    async fn notify(&mut self,method:&str)->Result<()> {
        self.send(&json!({"jsonrpc":"2.0","method":method})).await.map(|_|())
    }

    async fn call(&mut self,method:&str,params:Value,limit:Duration)->Result<Value> {
        self.next+=1;
        let id=self.next;
        let message=json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let server=self.server.clone();
        tokio::time::timeout(limit,self.exchange(id,&message)).await.map_err(|_|anyhow!("MCP server {server}: {method} timed out"))?
    }

    async fn exchange(&mut self,id:u64,message:&Value)->Result<Value> {
        let reply=self.send(message).await?;
        let response=match reply { Some(response)=>response, None=>self.read(id).await? };
        if let Some(error)=response.get("error") { bail!("MCP server {}: {}",self.server,error.get("message").and_then(Value::as_str).unwrap_or("error")); }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Envia a mensagem. No HTTP a resposta vem junto e já é devolvida; no
    /// stdio ela é lida à parte (`read`).
    async fn send(&mut self,message:&Value)->Result<Option<Value>> {
        let wanted=message.get("id").and_then(Value::as_u64);
        match &mut self.transport {
            Transport::Stdio{stdin,..}=>{
                let mut line=message.to_string();
                line.push('\n');
                stdin.write_all(line.as_bytes()).await?;
                stdin.flush().await?;
                Ok(None)
            }
            Transport::Http{client,url,headers,session}=>{
                let mut request=client.post(url.as_str()).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version",PROTOCOL).json(message);
                for (key,value) in headers.iter() { request=request.header(key.as_str(),value.as_str()); }
                if let Some(session)=session.as_deref() { request=request.header("Mcp-Session-Id",session); }
                let response=request.send().await.map_err(|error|anyhow!("MCP server {}: {error}",self.server))?;
                let status=response.status();
                if let Some(id)=response.headers().get("mcp-session-id").and_then(|value|value.to_str().ok()) { *session=Some(id.to_string()); }
                let streamed=response.headers().get("content-type").and_then(|value|value.to_str().ok()).is_some_and(|kind|kind.contains("event-stream"));
                let body=response.text().await?;
                if !status.is_success() { bail!("MCP server {}: HTTP {status}",self.server); }
                let Some(wanted)=wanted else { return Ok(None) };
                let found=if streamed { body.lines().filter_map(|line|line.strip_prefix("data:")).filter_map(|data|serde_json::from_str::<Value>(data.trim()).ok()).find(|event|event.get("id").and_then(Value::as_u64)==Some(wanted)) } else { serde_json::from_str::<Value>(&body).ok() };
                found.map(Some).ok_or_else(||anyhow!("MCP server {}: no answer to the request",self.server))
            }
        }
    }

    /// A resposta de `id` no stdio, pulando o que o servidor diz por conta
    /// própria (notificações, `ping`).
    async fn read(&mut self,id:u64)->Result<Value> {
        let Transport::Stdio{lines,..}=&mut self.transport else { bail!("not a stdio server") };
        loop {
            let line=lines.next_line().await?.ok_or_else(||anyhow!("MCP server {}: closed the connection",self.server))?;
            let Ok(value)=serde_json::from_str::<Value>(line.trim()) else { continue };
            if value.get("id").and_then(Value::as_u64)==Some(id)&&value.get("method").is_none() { return Ok(value); }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Transport::Stdio{child,..}=&mut self.transport { let _=child.start_kill(); }
    }
}

/// O nome da ferramenta como a API de ferramentas da OpenAI o aceita: letras,
/// dígitos, `_` e `-`, até 64.
pub fn api_name(server:&str,tool:&str)->String {
    let clean=|text:&str|text.chars().map(|char|if char.is_ascii_alphanumeric()||char=='-'||char=='_' {char} else {'_'}).collect::<String>();
    let name=format!("{}__{}",clean(server),clean(tool));
    name.chars().take(64).collect()
}

/// Os servidores conectados e as ferramentas de cada um. O que não sobe fica
/// de fora e é dito no log: um servidor fora do ar não derruba o pedido.
pub struct Toolbox { clients:Vec<Client>, tools:Vec<(usize,Tool,String)> }

impl Toolbox {
    pub async fn open(servers:&[McpServer])->Self {
        let mut clients=Vec::new();
        let mut tools=Vec::new();
        for server in servers {
            let opened=async { let mut client=Client::connect(server).await?; let found=client.tools().await?; Ok::<_,anyhow::Error>((client,found)) }.await;
            match opened {
                Ok((client,found))=>{
                    let index=clients.len();
                    clients.push(client);
                    for tool in found {
                        let name=api_name(&server.name,&tool.name);
                        if !tools.iter().any(|(_,_,known):&(usize,Tool,String)|*known==name) { tools.push((index,tool,name)); }
                    }
                }
                Err(error)=>eprintln!("mcp: {} fora do pedido ({error:#})",server.name),
            }
        }
        Self{clients,tools}
    }

    pub fn is_empty(&self)->bool { self.tools.is_empty() }

    /// As definições no formato `tools` da API de chat da OpenAI.
    pub fn definitions(&self)->Vec<Value> {
        self.tools.iter().map(|(_,tool,name)|json!({"type":"function","function":{"name":name,"description":tool.description,"parameters":tool.schema}})).collect()
    }

    /// `servidor/ferramenta`, para a faixa de etapas.
    pub fn label(&self,api:&str)->String {
        self.tools.iter().find(|(_,_,name)|name==api).map(|(index,tool,_)|format!("{}/{}",self.clients[*index].server,tool.name)).unwrap_or_else(||api.to_string())
    }

    /// Roda a ferramenta pedida pelo modelo. Falha vira texto: o modelo lê e
    /// decide o que fazer.
    pub async fn run(&mut self,api:&str,arguments:Value)->String {
        let Some((index,tool,_))=self.tools.iter().find(|(_,_,name)|name==api).cloned() else { return format!("Error: unknown tool {api}") };
        match self.clients[index].run(&tool.name,arguments).await { Ok(text)=>text, Err(error)=>format!("Error: {error:#}") }
    }
}

#[cfg(all(test,unix))]
mod tests {
    use super::*;

    /// Um servidor MCP de verdade, em `sh`: responde na ordem em que o cliente
    /// pergunta (aperto de mão, a notificação, a lista e a chamada), com uma
    /// notificação solta no meio que o cliente tem de pular.
    const SERVER:&str=r#"read a; echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{}}}'; read b; read c; echo '{"jsonrpc":"2.0","method":"notifications/message","params":{}}'; echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"d","inputSchema":{"type":"object"}}]}}'; read d; echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"pong"}],"isError":false}}'"#;

    #[tokio::test] async fn a_stdio_server_is_listed_and_called() {
        let server=McpServer{name:"fake".into(),command:"sh".into(),args:vec!["-c".into(),SERVER.into()],..Default::default()};
        let mut toolbox=Toolbox::open(&[server]).await;
        assert!(!toolbox.is_empty());
        assert_eq!(toolbox.definitions()[0]["function"]["name"],"fake__echo");
        assert_eq!(toolbox.label("fake__echo"),"fake/echo");
        assert_eq!(toolbox.run("fake__echo",json!({})).await,"pong");
        assert!(toolbox.run("nope",json!({})).await.starts_with("Error:"));
    }

    #[tokio::test] async fn a_command_that_does_not_exist_leaves_the_toolbox_empty() {
        let server=McpServer{name:"gone".into(),command:"jayv-no-such-program".into(),..Default::default()};
        assert!(Toolbox::open(&[server]).await.is_empty());
    }

    #[test] fn api_names_fit_the_tool_api() {
        assert_eq!(api_name("my-git","list.issues"),"my-git__list_issues");
        assert_eq!(api_name(&"s".repeat(40),&"t".repeat(40)).len(),64);
    }
}
