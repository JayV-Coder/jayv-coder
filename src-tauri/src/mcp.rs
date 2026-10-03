//! `jayv mcp`: o índice de símbolos do JayV como servidor MCP, para o agente
//! perguntar onde algo é definido, quem usa e o que uma mudança afeta, em vez
//! de varrer a pasta com grep e abrir arquivo por arquivo.
//!
//! Fala JSON-RPC 2.0 pela entrada e saída padrão, uma mensagem por linha, como
//! o transporte stdio do MCP pede. Só lê: nenhuma ferramenta muda nada. O
//! firewall de contexto vale aqui como na busca — arquivo sensível nem entra
//! no índice, e arquivo só local não aparece nas respostas.

use crate::{firewall::ContextFirewall, rag::RepositoryRag};
use serde_json::{json, Value};
use std::{io::{BufRead, Write}, time::{Duration, Instant}};

/// A versão do protocolo quando o cliente não diz qual quer.
const PROTOCOL_VERSION:&str="2025-06-18";
/// O agente edita enquanto pergunta: passado isto, a pasta é lida de novo
/// (só os arquivos que mudaram são analisados outra vez).
const FRESHNESS:Duration=Duration::from_secs(10);
const LIST_LIMIT:usize=30;
const IMPACT_DEPTH:usize=2;

pub struct Server { rag:RepositoryRag, firewall:ContextFirewall, indexed_at:Option<Instant> }

impl Server {
    pub fn new(rag:RepositoryRag,firewall:ContextFirewall)->Self { Self{rag,firewall,indexed_at:None} }

    fn refresh(&mut self) {
        if self.indexed_at.is_some_and(|at|at.elapsed()<FRESHNESS) { return; }
        if let Err(error)=self.rag.index(&self.firewall) { eprintln!("jayv mcp: index failed ({error:#})"); }
        self.indexed_at=Some(Instant::now());
    }

    fn shown(&self,path:&str)->bool { !self.firewall.check_file(path).local_only }

    /// A resposta a uma mensagem, ou nada quando ela é notificação.
    pub fn handle(&mut self,message:&Value)->Option<Value> {
        let id=message.get("id").cloned()?;
        let method=message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params=message.get("params").cloned().unwrap_or(Value::Null);
        let result=match method {
            "initialize"=>Ok(json!({
                "protocolVersion":params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION),
                "capabilities":{"tools":{}},
                "serverInfo":{"name":"jayv","version":env!("CARGO_PKG_VERSION")},
                "instructions":"Symbol index of this repository, read from the code with tree-sitter. Prefer these tools over grep to find where something is defined, who uses it, and what a change affects. Links between files are matched by name, so confirm them in the code before relying on them.",
            })),
            "ping"=>Ok(json!({})),
            "tools/list"=>Ok(json!({"tools":tools()})),
            "tools/call"=>Ok(self.call(params.get("name").and_then(Value::as_str).unwrap_or_default(),params.get("arguments").unwrap_or(&Value::Null))),
            _=>Err(json!({"code":-32601,"message":format!("method not found: {method}")})),
        };
        Some(match result { Ok(result)=>json!({"jsonrpc":"2.0","id":id,"result":result}), Err(error)=>json!({"jsonrpc":"2.0","id":id,"error":error}) })
    }

    fn call(&mut self,tool:&str,arguments:&Value)->Value {
        let argument=|key:&str|arguments.get(key).and_then(Value::as_str).map(str::trim).filter(|value|!value.is_empty());
        self.refresh();
        let text=match tool {
            "find_symbol"=>argument("name").map(|name|self.find_symbol(name)),
            "symbol_references"=>argument("name").map(|name|self.symbol_references(name)),
            "file_links"=>argument("path").map(|path|self.file_links(path)),
            "change_impact"=>argument("path").map(|path|self.change_impact(path)),
            _=>return failure(format!("unknown tool: {tool}")),
        };
        match text { Some(text)=>json!({"content":[{"type":"text","text":text}]}), None=>failure("missing argument".into()) }
    }

    fn find_symbol(&self,name:&str)->String {
        let found=self.rag.symbols().find(name).into_iter().filter(|(path,_)|self.shown(path)).take(LIST_LIMIT).map(|(path,definition)|format!("{path}:{} {} {}",definition.line,definition.kind,definition.signature)).collect::<Vec<_>>();
        if found.is_empty() { format!("No definition of `{name}` in the index.") } else { found.join("\n") }
    }

    fn symbol_references(&self,name:&str)->String {
        let found=self.listed(self.rag.symbols().references(name));
        if found.is_empty() { format!("No file references `{name}`.") } else { found }
    }

    fn file_links(&self,path:&str)->String {
        let path=path.trim_start_matches("./");
        let Some(definitions)=self.rag.symbols().definitions(path).filter(|_|self.shown(path)) else { return format!("`{path}` is not in the index (unsupported language, ignored or private)."); };
        let defined=definitions.iter().take(LIST_LIMIT).map(|definition|format!("  L{} {}",definition.line,definition.signature)).collect::<Vec<_>>().join("\n");
        let uses=self.listed(self.rag.symbols().uses(path));
        let used_by=self.listed(self.rag.symbols().used_by(path));
        format!("{path}\ndefines:\n{defined}\nuses:\n{}\nused by:\n{}",indent(&uses),indent(&used_by))
    }

    fn change_impact(&self,path:&str)->String {
        let path=path.trim_start_matches("./");
        let affected=self.rag.symbols().impact(path,IMPACT_DEPTH);
        let listed=self.listed(affected.iter().map(String::as_str).collect());
        if listed.is_empty() { format!("Nothing in the index uses `{path}`.") } else { format!("Files that use {path}, directly or through one other file:\n{listed}") }
    }

    fn listed(&self,paths:Vec<&str>)->String {
        let shown=paths.into_iter().filter(|path|self.shown(path)).collect::<Vec<_>>();
        let more=shown.len().saturating_sub(LIST_LIMIT);
        let mut lines=shown.into_iter().take(LIST_LIMIT).map(str::to_string).collect::<Vec<_>>();
        if more>0 { lines.push(format!("(+{more} more)")); }
        lines.join("\n")
    }
}

fn indent(text:&str)->String { if text.is_empty() { "  (none)".into() } else { text.lines().map(|line|format!("  {line}")).collect::<Vec<_>>().join("\n") } }

fn failure(text:String)->Value { json!({"content":[{"type":"text","text":text}],"isError":true}) }

fn tools()->Value {
    let name=json!({"type":"object","properties":{"name":{"type":"string","description":"Exact symbol name: function, type, class or constant."}},"required":["name"]});
    let path=json!({"type":"object","properties":{"path":{"type":"string","description":"File path relative to the repository root."}},"required":["path"]});
    json!([
        {"name":"find_symbol","description":"Where a symbol is defined: file, line, kind and signature.","inputSchema":name,"annotations":{"readOnlyHint":true}},
        {"name":"symbol_references","description":"Files that call or mention a symbol by name.","inputSchema":name,"annotations":{"readOnlyHint":true}},
        {"name":"file_links","description":"What a file defines, the files it uses and the files that use it.","inputSchema":path,"annotations":{"readOnlyHint":true}},
        {"name":"change_impact","description":"Files affected by changing a file: its users and their users.","inputSchema":path,"annotations":{"readOnlyHint":true}},
    ])
}

/// O laço do servidor: uma linha entra, uma linha sai. Linha que não é JSON
/// recebe o erro de parse do JSON-RPC; o fim da entrada encerra.
pub fn serve(rag:RepositoryRag,firewall:ContextFirewall)->anyhow::Result<()> {
    let mut server=Server::new(rag,firewall);
    let stdin=std::io::stdin();
    let mut stdout=std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line=line?;
        if line.trim().is_empty() { continue; }
        let reply=match serde_json::from_str::<Value>(&line) {
            Ok(message)=>server.handle(&message),
            Err(error)=>Some(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":error.to_string()}})),
        };
        if let Some(reply)=reply { writeln!(stdout,"{reply}")?; stdout.flush()?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(files:&[(&str,&str)])->(tempfile::TempDir,Server) {
        let dir=tempfile::tempdir().expect("repositório");
        for (path,body) in files { std::fs::write(dir.path().join(path),body).expect("arquivo"); }
        let firewall=ContextFirewall::new(Default::default());
        let rag=RepositoryRag::new(dir.path().to_path_buf());
        (dir,Server::new(rag,firewall))
    }

    fn call(server:&mut Server,tool:&str,arguments:Value)->String {
        let reply=server.handle(&json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":tool,"arguments":arguments}})).expect("resposta");
        assert_eq!(reply["id"],7);
        reply["result"]["content"][0]["text"].as_str().expect("texto").to_string()
    }

    #[test]
    fn the_handshake_lists_four_read_only_tools_and_notifications_get_no_reply() {
        let (_dir,mut server)=server(&[]);
        let hello=server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).expect("resposta");
        assert_eq!(hello["result"]["protocolVersion"],"2025-03-26");
        assert_eq!(hello["result"]["serverInfo"]["name"],"jayv");
        assert!(server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let listed=server.handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).expect("lista");
        let tools=listed["result"]["tools"].as_array().expect("ferramentas");
        assert_eq!(tools.len(),4);
        assert!(tools.iter().all(|tool|tool["annotations"]["readOnlyHint"]==true));
        assert_eq!(server.handle(&json!({"jsonrpc":"2.0","id":3,"method":"resources/list"})).expect("erro")["error"]["code"],-32601);
    }

    #[test]
    fn the_tools_answer_from_the_symbol_index() {
        let (_dir,mut server)=server(&[("cache.rs","pub fn invalidate_entries() {}\n"),("orchestrator.rs","fn run() { invalidate_entries(); }\n"),("queue.rs","fn attend() { run(); }\n")]);
        assert_eq!(call(&mut server,"find_symbol",json!({"name":"invalidate_entries"})),"cache.rs:1 function pub fn invalidate_entries()");
        assert_eq!(call(&mut server,"symbol_references",json!({"name":"invalidate_entries"})),"orchestrator.rs");
        let links=call(&mut server,"file_links",json!({"path":"./orchestrator.rs"}));
        assert!(links.contains("uses:\n  cache.rs")&&links.contains("used by:\n  queue.rs"),"{links}");
        assert!(call(&mut server,"change_impact",json!({"path":"cache.rs"})).ends_with("orchestrator.rs\nqueue.rs"));
        assert!(call(&mut server,"find_symbol",json!({"name":"missing"})).starts_with("No definition"));
        let wrong=server.handle(&json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"find_symbol","arguments":{}}})).expect("resposta");
        assert_eq!(wrong["result"]["isError"],true);
    }
}
