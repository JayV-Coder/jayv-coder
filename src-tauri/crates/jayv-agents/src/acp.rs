//! O JayV como cliente do Agent Client Protocol (ACP): o protocolo aberto com
//! que um editor conversa com um agente de código pela entrada e saída padrão
//! (JSON-RPC 2.0, uma mensagem por linha). Kimi Code (`kimi acp`), OpenCode
//! (`opencode acp`), OpenHands (`openhands acp`), Devin (`devin acp`) e o
//! Gemini CLI (`gemini --acp`) falam ACP; um mod do catálogo só diz o comando.
//!
//! Cada pedido abre o agente, inicia (ou retoma) a sessão, escolhe o modo e o
//! modelo que o próprio agente anuncia, manda o pedido e lê o que ele narra:
//! o texto é a resposta, as ferramentas são o relato. Quando o agente pede
//! permissão, quem responde é o JayV, pelo que vale no pedido:
//!
//! - no planejamento, só ler, buscar, pensar e consultar a web;
//! - no desenvolvimento, tudo, menos o comando que a política da organização
//!   bloqueia;
//! - com o agente travado (a política com agentes seguros), os comandos só
//!   rodam quando o pedido os liberou.
//!
//! O agente que tem modo só leitura (`plan`) entra nele no planejamento: é a
//! trava dele, e a recusa do JayV é a segunda. O JayV não oferece ao agente
//! nem o sistema de arquivos nem o terminal do editor: o agente usa os dele, e
//! a escrita que ele faz sem perguntar é a que o modo dele permite.

use crate::config::{AcpConfig, ProviderConfig};
use crate::i18n::Text;
use crate::llm::command_blocked;
use crate::model::{ChatMessage, ProviderResponse};
use crate::progress::{Beat, Pulse, StopReason};
use crate::providers::{quiet, tail, text_of, ListedModel, Provider, Tree, Workdir, FAILURE_ANNOUNCED, FAILURE_STDERR, MESSAGE_BREAK};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Split};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

/// A versão do protocolo que o JayV fala (o número maior do ACP).
const PROTOCOL_VERSION:u64=1;
/// O erro do ACP para "entre na sua conta antes".
const AUTH_REQUIRED:i64=-32000;
/// O que o agente pode fazer no planejamento: nada que mude o projeto.
const READ_KINDS:[&str;4]=["read","search","think","fetch"];
/// O modelo que deixa o agente com o dele: o que o mod traz de fábrica, antes
/// de a descoberta listar os de verdade.
pub const AGENT_DEFAULT:&str="default";
/// Quanto o agente tem para fechar o pedido depois do "Parar".
const CANCEL_GRACE:Duration=Duration::from_secs(5);
/// Quanto o agente tem para sair depois de o JayV fechar a entrada dele.
const EXIT_GRACE:Duration=Duration::from_secs(2);
/// O tanto do canal de erro guardado para explicar uma falha.
const COMPLAINT_LIMIT:usize=16_000;

pub struct AcpProvider { name:String, config:ProviderConfig, workdir:Workdir }

impl AcpProvider {
    pub fn new(name:String,config:ProviderConfig,workdir:Workdir)->Self { Self{name,config,workdir} }

    fn acp(&self)->AcpConfig { self.config.acp.clone().unwrap_or_default() }

    /// O pedido em texto, como o dos agentes de linha de comando: o ACP leva
    /// blocos de conteúdo, e a conversa vai num bloco só.
    fn prompt(messages:&[ChatMessage])->String { messages.iter().map(|m|format!("{}: {}",m.role,m.content)).collect::<Vec<_>>().join("\n\n") }

    fn silence(&self)->Duration { Duration::from_secs(self.config.timeout.max(1)) }

    async fn converse(&self,messages:&[ChatMessage],model:&str,resume:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> {
        let prompt=Self::prompt(messages);
        let started=Instant::now();
        let command=self.config.command.as_deref().unwrap_or(&self.name).to_string();
        if let Some(reason)=pulse.stop().reason() { return Err(reason.text().into()); }
        crate::guard::enter()?;
        crate::guard::check_quota(&self.name,&command).await?;
        let root=self.workdir.current().filter(|root|root.is_dir());
        let cwd=root.clone().or_else(||std::env::current_dir().ok()).unwrap_or_else(std::env::temp_dir);
        let mut meter=crate::usage::Meter::new(&self.name,model,&command);
        let mut run=Run::open(&self.name,&self.config,model,root.as_deref(),pulse,self.silence())?;
        let outcome=run.turn(&self.acp(),&self.config.mcp,&cwd,model,resume,&prompt).await;
        run.close().await;
        match outcome {
            Ok(turn)=>{
                let (input,output,cached)=turn.usage;
                if input>0||output>0 { meter.reported(input,output,cached); }
                let (input,output)=meter.settle(&prompt,&run.response,true,None);
                Ok(ProviderResponse{response:std::mem::take(&mut run.response),input_tokens:input as usize,output_tokens:output as usize,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis(),session:turn.session})
            }
            Err(error)=>{ meter.settle(&prompt,&run.response,false,None); Err(error) }
        }
    }
}

#[async_trait]
impl Provider for AcpProvider {
    fn name(&self)->&str { &self.name }
    fn is_local(&self)->bool { self.config.local.unwrap_or(false) }
    async fn chat(&self,messages:&[ChatMessage],model:&str)->Result<ProviderResponse> { self.chat_stream(messages,model,&Pulse::silent()).await }
    async fn chat_stream(&self,messages:&[ChatMessage],model:&str,pulse:&Pulse)->Result<ProviderResponse> { self.chat_once(messages,model,None,pulse).await }
    async fn chat_with_effort(&self,messages:&[ChatMessage],model:&str,effort:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> { self.chat_once(messages,model,effort,pulse).await }
    fn explores(&self)->bool { true }
    /// O agente guarda as sessões dele; se não souber retomar, o pedido
    /// devolve "could not resume the session" e o orquestrador começa outra.
    fn resumes(&self)->bool { true }
    async fn chat_turn(&self,messages:&[ChatMessage],model:&str,_effort:Option<&str>,resume:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> {
        self.converse(messages,model,resume,pulse).await
    }
    async fn chat_once(&self,messages:&[ChatMessage],model:&str,_effort:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> {
        self.converse(messages,model,None,pulse).await
    }
}

/// Os modelos que o agente anuncia ao abrir uma sessão, sem mandar pedido
/// nenhum. A sessão abre numa pasta temporária, longe dos projetos; o
/// `default` vem primeiro, para o agente seguir funcionando com o modelo dele.
pub async fn discover(name:&str,config:&ProviderConfig)->Result<Vec<ListedModel>> {
    let scratch=std::env::temp_dir().join("jayv-acp-models");
    std::fs::create_dir_all(&scratch)?;
    let pulse=Pulse::silent();
    let mut run=Run::open(name,config,AGENT_DEFAULT,Some(&scratch),&pulse,Duration::from_secs(config.timeout.clamp(15,120)))?;
    let found=run.models(&[],&scratch).await;
    run.close().await;
    let mut models=vec![ListedModel{id:AGENT_DEFAULT.into(),context:None}];
    for id in found? { if id!=AGENT_DEFAULT&&!models.iter().any(|known|known.id==id) { models.push(ListedModel{id,context:None}); } }
    Ok(models)
}

/// A linha de comando do agente com o modelo no lugar de `{model}`. Sem
/// modelo escolhido (`default`), o argumento sai junto da flag que o anuncia.
fn launch_args(args:&[String],model:&str)->Vec<String> {
    let mut line=Vec::new();
    for arg in args {
        if arg.contains("{model}") {
            if model.is_empty()||model==AGENT_DEFAULT { if line.last().is_some_and(|flag:&String|flag.starts_with('-')) { line.pop(); } continue; }
            line.push(arg.replace("{model}",model));
        } else { line.push(arg.clone()); }
    }
    line
}

/// O que chega do agente: uma mensagem do protocolo, uma linha que não é do
/// protocolo (o canal de erro ou um log na saída) ou o fim da conversa.
enum Heard { Message(Value), Complaint(String), Closed }

/// O agente aberto: as três pontas, o número do próximo pedido e a árvore de
/// processos que cai junto com ele.
struct Wire {
    stdin:Option<ChildStdin>,
    talk:Split<BufReader<ChildStdout>>,
    grumble:Split<BufReader<ChildStderr>>,
    talking:bool,
    grumbling:bool,
    next:u64,
    child:tokio::process::Child,
    tree:Tree,
}

impl Wire {
    async fn send(&mut self,message:&Value)->Result<()> {
        let stdin=self.stdin.as_mut().ok_or_else(||anyhow!("the agent input is closed"))?;
        let mut line=serde_json::to_vec(message)?;
        line.push(b'\n');
        stdin.write_all(&line).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// A próxima coisa que o agente disse. As duas leituras podem ser largadas
    /// pelo relógio no meio: `next_segment` guarda a linha pela metade.
    async fn hear(&mut self)->std::io::Result<Heard> {
        loop {
            if !self.talking { return Ok(Heard::Closed); }
            tokio::select! {
                line=self.talk.next_segment(),if self.talking=>match line? {
                    None=>{ self.talking=false; return Ok(Heard::Closed); }
                    Some(bytes)=>{
                        let text=text_of(&bytes);
                        if text.trim().is_empty() { continue; }
                        return Ok(match serde_json::from_str::<Value>(text.trim()) { Ok(value) if value.is_object()=>Heard::Message(value), _=>Heard::Complaint(text) });
                    }
                },
                line=self.grumble.next_segment(),if self.grumbling=>match line? {
                    None=>self.grumbling=false,
                    Some(bytes)=>{ let text=text_of(&bytes); if !text.trim().is_empty() { return Ok(Heard::Complaint(text)); } }
                },
            }
        }
    }
}

/// Uma chamada de ferramenta, como o agente a narrou até agora. O pedido de
/// permissão nem sempre repete o que a chamada é: a que ele cita fica aqui.
#[derive(Debug,Clone,Default,PartialEq)]
struct ToolCall { id:String, title:String, kind:String, status:String, raw_input:Value, content:String }

impl ToolCall {
    fn from(value:&Value)->Self { let mut call=Self{id:text(value,"toolCallId"),..Self::default()}; call.merge(value); call }

    /// Soma o que a atualização trouxe; o que ela não traz fica como estava.
    fn merge(&mut self,value:&Value) {
        for (field,slot) in [("title",&mut self.title),("kind",&mut self.kind),("status",&mut self.status)] {
            if let Some(found)=value.get(field).and_then(Value::as_str).filter(|found|!found.is_empty()) { *slot=found.to_string(); }
        }
        if let Some(input)=value.get("rawInput").filter(|input|!input.is_null()&&input.as_object().is_none_or(|object|!object.is_empty())) { self.raw_input=input.clone(); }
        let content=contents(value);
        if !content.is_empty() { self.content=content; }
    }

    fn kind(&self)->&str { if self.kind.is_empty() { "other" } else { &self.kind } }

    fn open(&self)->bool { !matches!(self.status.as_str(),"completed"|"failed") }

    /// O comando que a chamada roda, de onde o agente o puser: no
    /// `rawInput` (`command`, como texto ou lista), no texto do pedido
    /// ("Running: git push") ou no título.
    fn command(&self)->Option<String> {
        let from=|value:&Value|->Option<String> {
            ["command","cmd","commandLine","script"].iter().find_map(|field|match value.get(*field) {
                Some(Value::String(command))=>Some(command.clone()),
                Some(Value::Array(parts))=>Some(parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
                _=>None,
            }).filter(|command|!command.trim().is_empty())
        };
        if let Some(command)=from(&self.raw_input) { return Some(command); }
        if let Some(command)=serde_json::from_str::<Value>(self.content.trim()).ok().as_ref().and_then(from) { return Some(command); }
        if let Some((_,command))=self.content.split_once("Running: ") { return Some(command.lines().next().unwrap_or_default().trim().to_string()).filter(|command|!command.is_empty()); }
        // O título só vale quando é o próprio comando, e não o nome da
        // ferramenta (`bash`, `Bash`, `terminal`).
        let title=self.title.trim();
        (title.contains(' ')&&!title.ends_with(':')).then(||title.to_string())
    }

    /// Como a chamada aparece nos passos do pedido.
    fn label(&self)->String {
        // O comando diz mais que o nome da ferramenta (`Bash`).
        let command=(self.kind()=="execute").then(||self.command()).flatten();
        let title=command.or_else(||Some(self.title.clone()).filter(|title|!title.is_empty())).unwrap_or_else(||self.id.clone());
        format!("{}: {}",self.kind(),title.lines().next().unwrap_or_default())
    }
}

fn text(value:&Value,field:&str)->String { value.get(field).and_then(Value::as_str).unwrap_or_default().to_string() }

/// O texto dos blocos de conteúdo de uma chamada (`content[].content.text`).
fn contents(value:&Value)->String {
    value.get("content").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|block|block.pointer("/content/text").or_else(||block.get("text")).and_then(Value::as_str))
        .collect::<Vec<_>>().join("\n")
}

/// O que vale neste pedido para responder às permissões.
#[derive(Debug,Clone,Default)]
struct Policy { planning:bool, auto:bool, granted:Vec<String>, denied:Vec<String> }

impl Policy {
    fn of(acp:&AcpConfig)->Self { Self{planning:acp.planning,auto:acp.auto,granted:acp.granted.clone(),denied:acp.denied.clone()} }

    /// Se as chamadas podem rodar; quando não, o porquê, para o relato.
    fn allows(&self,calls:&[ToolCall])->Result<(),String> {
        for call in calls {
            let kind=call.kind();
            if self.planning {
                if !READ_KINDS.contains(&kind) { return Err(call.label()); }
                continue;
            }
            if kind!="execute" { continue; }
            let Some(command)=call.command() else {
                // Sem saber o comando, não há como conferir a regra.
                if !self.denied.is_empty()||!self.auto { return Err(call.label()); }
                continue;
            };
            if self.denied.iter().any(|rule|mentions(rule,&command)) { return Err(call.label()); }
            if !self.auto&&!segments(&command).iter().all(|segment|command_blocked(&self.granted,segment)) { return Err(call.label()); }
        }
        Ok(())
    }
}

/// As palavras de um comando de shell: o que está entre espaços, aspas,
/// parênteses e os separadores (`;`, `&`, `|`).
fn words(command:&str)->Vec<&str> {
    command.split(|char:char|char.is_whitespace()||";&|()`'\"<>{}".contains(char)).filter(|word|!word.is_empty()).collect()
}

/// A regra aparece no comando, em qualquer ponto dele: `git push` está em
/// `cd app && git push`, e também em `bash -c "git push"`. Para bloquear, é
/// melhor recusar um `echo "git push"` do que deixar passar o push.
fn mentions(rule:&str,command:&str)->bool {
    let rule=words(rule);
    let command=words(command);
    !rule.is_empty()&&command.windows(rule.len()).any(|window|window==rule.as_slice())
}

/// Os comandos de uma linha de shell, sem as variáveis postas na frente
/// (`CI=1 npm test`): cada um precisa estar liberado.
fn segments(command:&str)->Vec<String> {
    command.split(['\n',';','|','&']).map(str::trim).filter(|segment|!segment.is_empty()).map(|segment|{
        segment.split_whitespace().skip_while(|word|word.contains('=')&&!word.starts_with('=')).collect::<Vec<_>>().join(" ")
    }).filter(|segment|!segment.is_empty()).collect()
}

/// A opção que responde à permissão: a de uma vez só, para o agente perguntar
/// de novo da próxima — é o JayV quem decide cada uma.
fn choose(options:&Value,allow:bool)->Option<String> {
    let wanted:[&str;2]=if allow { ["allow_once","allow_always"] } else { ["reject_once","reject_always"] };
    let options=options.as_array()?;
    wanted.iter().find_map(|kind|options.iter().find(|option|option.get("kind").and_then(Value::as_str)==Some(*kind)).and_then(|option|option.get("optionId")).and_then(Value::as_str).map(str::to_string))
}

/// Um erro que o agente devolveu a um pedido.
#[derive(Debug)]
struct RpcError { code:i64, message:String }

/// O que a sessão anunciou: as opções (`configOptions`) e, nos agentes que
/// ainda não as usam, os modos e os modelos à moda antiga.
#[derive(Debug,Default)]
struct Offer { options:Vec<Value>, modes:Value, models:Value }

impl Offer {
    fn read(&mut self,result:&Value) {
        if let Some(options)=result.get("configOptions").and_then(Value::as_array) { self.options=options.clone(); }
        if let Some(modes)=result.get("modes").filter(|modes|!modes.is_null()) { self.modes=modes.clone(); }
        if let Some(models)=result.get("models").filter(|models|!models.is_null()) { self.models=models.clone(); }
    }

    /// A opção de uma categoria (`mode`, `model`), pela categoria ou, sem ela,
    /// pelo id.
    fn option(&self,category:&str)->Option<&Value> {
        self.options.iter().find(|option|option.get("category").and_then(Value::as_str)==Some(category))
            .or_else(||self.options.iter().find(|option|option.get("id").and_then(Value::as_str)==Some(category)))
    }

    /// Os valores de uma opção, com os grupos abertos.
    fn values(option:&Value)->Vec<String> {
        option.get("options").and_then(Value::as_array).into_iter().flatten().flat_map(|entry|{
            match entry.get("options").and_then(Value::as_array) {
                Some(group)=>group.iter().filter_map(|inner|inner.get("value").and_then(Value::as_str).map(str::to_string)).collect::<Vec<_>>(),
                None=>entry.get("value").and_then(Value::as_str).map(str::to_string).into_iter().collect(),
            }
        }).collect()
    }

    fn models(&self)->Vec<String> {
        if let Some(option)=self.option("model") { return Self::values(option); }
        self.models.get("availableModels").and_then(Value::as_array).into_iter().flatten().filter_map(|model|model.get("modelId").and_then(Value::as_str).map(str::to_string)).collect()
    }
}

/// O fim de um pedido que deu certo.
struct Turn { session:Option<String>, usage:(u64,u64,u64) }

/// Uma conversa com o agente, do arranque ao fim.
struct Run<'a> {
    name:String,
    wire:Wire,
    pulse:&'a Pulse,
    silence:Duration,
    policy:Policy,
    calls:HashMap<String,ToolCall>,
    order:Vec<String>,
    response:String,
    last_message:Option<String>,
    worked:bool,
    replaying:bool,
    prompting:bool,
    stopped:Option<StopReason>,
    /// O que o JayV recusou neste pedido, para explicar uma resposta vazia.
    denied:Vec<String>,
    complaint:String,
    watch:crate::guard::Watch,
    offer:Offer,
    current_session:Option<String>,
}

impl<'a> Run<'a> {
    /// Abre o agente, como os de linha de comando: o caminho achado com a
    /// extensão, o PATH da pasta dele, um grupo de processos só dele e a pasta
    /// do projeto.
    fn open(name:&str,config:&ProviderConfig,model:&str,root:Option<&Path>,pulse:&'a Pulse,silence:Duration)->Result<Self> {
        let command=config.command.as_deref().ok_or_else(||anyhow!("ACP provider has no command"))?;
        let found=crate::llm::locate(command).ok_or_else(||anyhow::Error::new(Text::new("provider.notInstalled").with("provider",name).with("command",command)))?;
        let (program,lead)=crate::llm::launcher(&found);
        let mut process=Command::new(&program);
        quiet(&mut process);
        if let Some(path)=crate::llm::agent_path(&found) { process.env("PATH",path); }
        for (key,value) in config.acp.iter().flat_map(|acp|acp.env.iter()) { process.env(key,value); }
        process.args(lead).args(launch_args(&config.args,model)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(unix)] process.process_group(0);
        if let Some(root)=root { process.current_dir(root); }
        let mut child=process.spawn().map_err(|error|anyhow::Error::new(Text::new("provider.start").with("provider",name).with("path",found.display().to_string()).with("reason",error.to_string())))?;
        let tree=Tree::of(&child);
        let stdin=child.stdin.take();
        let talk=BufReader::new(child.stdout.take().ok_or_else(||anyhow!("ACP provider gave no output channel"))?).split(b'\n');
        let grumble=BufReader::new(child.stderr.take().ok_or_else(||anyhow!("ACP provider gave no error channel"))?).split(b'\n');
        let wire=Wire{stdin,talk,grumble,talking:true,grumbling:true,next:0,child,tree};
        Ok(Self{
            name:name.to_string(),wire,pulse,silence,policy:Policy::of(&config.acp.clone().unwrap_or_default()),calls:HashMap::new(),order:vec![],
            response:String::new(),last_message:None,worked:false,replaying:false,prompting:false,stopped:None,denied:vec![],complaint:String::new(),
            watch:crate::guard::Watch::default(),offer:Offer::default(),current_session:None,
        })
    }

    /// Fecha a entrada do agente, dá a ele um instante para sair sozinho e
    /// derruba o que sobrar da árvore.
    async fn close(&mut self) {
        drop(self.wire.stdin.take());
        match timeout(EXIT_GRACE,self.wire.child.wait()).await {
            Ok(Ok(_))=>{ self.wire.tree.kill(); }
            _=>self.wire.tree.kill(),
        }
    }

    /// Um pedido ao agente e a resposta dele. Enquanto ela não vem, o agente
    /// narra e pede permissões, e cada coisa é atendida na hora.
    async fn call(&mut self,method:&str,params:Value)->Result<std::result::Result<Value,RpcError>> {
        self.wire.next+=1;
        let id=self.wire.next;
        self.wire.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})).await.map_err(|error|self.broken(error))?;
        let pulse=self.pulse;
        let mut deadline:Option<Instant>=None;
        loop {
            let wait=deadline.map_or(self.silence,|deadline|deadline.saturating_duration_since(Instant::now()));
            let heard=tokio::select! {
                biased;
                reason=pulse.stop().stopped(),if deadline.is_none()=>Err(reason),
                heard=timeout(wait,self.wire.hear())=>Ok(heard),
            };
            match heard {
                Err(reason)=>{
                    self.stopped=Some(reason);
                    if !self.prompting { return Err(reason.text().into()); }
                    // O agente fecha o pedido com `cancelled` depois de parar o
                    // que estava fazendo; quem não fechar a tempo cai junto.
                    let session=self.session_of_prompt();
                    let _=self.wire.send(&json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}})).await;
                    deadline=Some(Instant::now()+CANCEL_GRACE);
                }
                Ok(Err(_))=>return Err(match self.stopped { Some(reason)=>reason.text().into(), None=>Text::new("provider.silent").with("provider",&self.name).with("seconds",self.silence.as_secs()).into() }),
                Ok(Ok(Err(error)))=>return Err(error.into()),
                Ok(Ok(Ok(Heard::Closed)))=>return Err(self.ended()),
                Ok(Ok(Ok(Heard::Complaint(line))))=>self.complain(&line),
                Ok(Ok(Ok(Heard::Message(message))))=>{
                    if message.get("method").is_some() {
                        if message.get("id").is_some() { self.answer(&message).await?; } else { self.update(&message); }
                        continue;
                    }
                    if message.get("id").and_then(Value::as_u64)!=Some(id) { continue; }
                    if let Some(error)=message.get("error") {
                        return Ok(Err(RpcError{code:error.get("code").and_then(Value::as_i64).unwrap_or(0),message:describe(error)}));
                    }
                    return Ok(Ok(message.get("result").cloned().unwrap_or(Value::Null)));
                }
            }
        }
    }

    /// A sessão do pedido em andamento, para o `session/cancel`.
    fn session_of_prompt(&self)->String { self.current_session.clone().unwrap_or_default() }

    fn complain(&mut self,line:&str) {
        if self.complaint.len()>COMPLAINT_LIMIT { let cut=self.complaint.len()-COMPLAINT_LIMIT/2; let cut=(cut..self.complaint.len()).find(|at|self.complaint.is_char_boundary(*at)).unwrap_or(0); self.complaint.drain(..cut); }
        self.complaint.push_str(line);
        self.complaint.push('\n');
    }

    fn broken(&self,error:anyhow::Error)->anyhow::Error {
        if self.complaint.trim().is_empty() { return error; }
        self.failed(tail(&self.complaint,800),FAILURE_STDERR)
    }

    /// O agente fechou a saída antes de responder.
    fn ended(&self)->anyhow::Error {
        let reason=tail(&self.complaint,800);
        if reason.is_empty() { Text::new("provider.failedSilent").with("provider",&self.name).with("exit",Text::new("provider.exit.announced")).with("command",&self.name).into() }
        else { self.failed(reason,FAILURE_STDERR) }
    }

    fn failed(&self,reason:&str,origin:&str)->anyhow::Error {
        Text::new("provider.failed").with("provider",&self.name).with("exit",Text::new("provider.exit.announced")).with("reason",reason).with("origin",origin).into()
    }

    /// O erro que o agente devolveu a um pedido, para a tela. O de login vira
    /// a orientação de entrar na conta do agente.
    fn refused(&self,error:RpcError,methods:&[String])->anyhow::Error {
        if error.code==AUTH_REQUIRED||error.message.to_ascii_lowercase().contains("auth") {
            let how=if methods.is_empty() { error.message.clone() } else { methods.join("; ") };
            return Text::new("provider.acpLogin").with("provider",&self.name).with("how",how).into();
        }
        self.failed(&error.message,FAILURE_ANNOUNCED)
    }

    /// Um pedido que o agente faz ao JayV. Só a permissão é atendida: o JayV
    /// não oferece o sistema de arquivos nem o terminal do editor.
    async fn answer(&mut self,message:&Value)->Result<()> {
        let id=message.get("id").cloned().unwrap_or(Value::Null);
        let method=message.get("method").and_then(Value::as_str).unwrap_or_default();
        if method!="session/request_permission" {
            return self.wire.send(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("{method} is not offered by this client")}})).await;
        }
        let params=message.get("params").cloned().unwrap_or(Value::Null);
        let asked=params.get("toolCall").cloned().unwrap_or(Value::Null);
        let calls=self.subjects(&asked);
        let verdict=if self.stopped.is_some() { Err(String::new()) } else { self.policy.allows(&calls) };
        let outcome=match &verdict {
            _ if self.stopped.is_some()=>json!({"outcome":"cancelled"}),
            Ok(())=>choose(params.get("options").unwrap_or(&Value::Null),true).map(|option|json!({"outcome":"selected","optionId":option})).unwrap_or_else(||json!({"outcome":"cancelled"})),
            Err(_)=>choose(params.get("options").unwrap_or(&Value::Null),false).map(|option|json!({"outcome":"selected","optionId":option})).unwrap_or_else(||json!({"outcome":"cancelled"})),
        };
        if let Err(label)=&verdict { if !label.is_empty() { self.pulse.beat(Beat::Agent{line:format!("denied {label}")}); self.denied.push(label.clone()); } }
        self.wire.send(&json!({"jsonrpc":"2.0","id":id,"result":{"outcome":outcome}})).await
    }

    /// As chamadas a que a permissão se refere: a que ela cita, com o que a
    /// narração já disse dela; ou, quando ela cita uma confirmação genérica
    /// (o OpenHands pede "Confirm Agent Actions"), as chamadas que ainda estão
    /// abertas.
    fn subjects(&self,asked:&Value)->Vec<ToolCall> {
        let id=text(asked,"toolCallId");
        if let Some(known)=self.calls.get(&id) { let mut call=known.clone(); call.merge(asked); return vec![call]; }
        let open=self.order.iter().filter_map(|id|self.calls.get(id)).filter(|call|call.open()).cloned().collect::<Vec<_>>();
        if open.is_empty() { vec![ToolCall::from(asked)] } else { open }
    }

    /// Uma notificação do agente: a fala vira resposta, as ferramentas viram
    /// relato, e o resto (comandos, título, uso do contexto) não interessa a
    /// quem espera. Na retomada por `session/load` o agente repete a conversa
    /// inteira, e nada disso é novo.
    fn update(&mut self,message:&Value) {
        if message.get("method").and_then(Value::as_str)!=Some("session/update") { return; }
        let Some(update)=message.pointer("/params/update") else { return };
        match update.get("sessionUpdate").and_then(Value::as_str).unwrap_or_default() {
            "agent_message_chunk" if !self.replaying=>{
                let Some(piece)=update.pointer("/content/text").and_then(Value::as_str).filter(|piece|!piece.is_empty()) else { return };
                let message_id=update.get("messageId").and_then(Value::as_str).map(str::to_string);
                let another=message_id.is_some()&&self.last_message.is_some()&&message_id!=self.last_message;
                if (self.worked||another)&&!self.response.trim().is_empty() {
                    let gap=format!("\n\n{MESSAGE_BREAK}\n");
                    self.response.push_str(&gap);
                    self.pulse.beat(Beat::Chunk{text:gap});
                }
                self.worked=false;
                if message_id.is_some() { self.last_message=message_id; }
                self.response.push_str(piece);
                self.pulse.beat(Beat::Chunk{text:piece.to_string()});
            }
            "tool_call"=>{
                let call=ToolCall::from(update);
                if call.id.is_empty() { return; }
                if !self.replaying {
                    self.worked=true;
                    self.pulse.beat(Beat::Agent{line:call.label()});
                    if let Some(reason)=self.watch.step() { self.pulse.stop().stop(reason); }
                }
                if !self.calls.contains_key(&call.id) { self.order.push(call.id.clone()); }
                self.calls.insert(call.id.clone(),call);
            }
            "tool_call_update"=>{
                let id=text(update,"toolCallId");
                match self.calls.get_mut(&id) {
                    Some(call)=>call.merge(update),
                    None if !id.is_empty()=>{ self.order.push(id.clone()); self.calls.insert(id,ToolCall::from(update)); }
                    None=>{}
                }
            }
            "config_option_update"=>{ if let Some(options)=update.get("configOptions").and_then(Value::as_array) { self.offer.options=options.clone(); } }
            _=>{}
        }
    }

    /// Começa a conversa: o aperto de mão, que diz o que o agente sabe fazer.
    async fn initialize(&mut self)->Result<Value> {
        let params=json!({
            "protocolVersion":PROTOCOL_VERSION,
            "clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},
            "clientInfo":{"name":"jayv","title":"JayV","version":env!("CARGO_PKG_VERSION")},
        });
        let result=match self.call("initialize",params).await? { Ok(result)=>result, Err(error)=>return Err(self.failed(&error.message,FAILURE_ANNOUNCED)) };
        let version=result.get("protocolVersion").and_then(Value::as_u64).unwrap_or(PROTOCOL_VERSION);
        if version!=PROTOCOL_VERSION { return Err(self.failed(&format!("the agent speaks ACP version {version}; JayV speaks version {PROTOCOL_VERSION}"),FAILURE_ANNOUNCED)); }
        Ok(result)
    }

    /// Como entrar na conta do agente, nas palavras dele.
    fn sign_in(capabilities:&Value)->Vec<String> {
        capabilities.get("authMethods").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|method|method.get("description").or_else(||method.get("name")).and_then(Value::as_str).map(str::to_string)).collect()
    }

    /// A sessão nova, na pasta do projeto, com os servidores MCP do pedido.
    async fn start(&mut self,cwd:&Path,servers:&[Value],methods:&[String])->Result<String> {
        let result=match self.call("session/new",json!({"cwd":cwd,"mcpServers":servers})).await? { Ok(result)=>result, Err(error)=>return Err(self.refused(error,methods)) };
        let Some(session)=result.get("sessionId").and_then(Value::as_str).map(str::to_string) else { return Err(self.failed("the agent opened a session without an id",FAILURE_ANNOUNCED)) };
        self.offer.read(&result);
        Ok(session)
    }

    /// A sessão de antes: `session/resume` quando o agente sabe, que não
    /// repete a conversa; senão `session/load`, que repete — e o que ele
    /// repete não é novo. Sem nenhum dos dois, ou com a sessão perdida, o erro
    /// diz "could not resume the session" e o orquestrador começa outra.
    async fn resume(&mut self,id:&str,capabilities:&Value,cwd:&Path,servers:&[Value],methods:&[String])->Result<()> {
        let agent=capabilities.get("agentCapabilities").cloned().unwrap_or(Value::Null);
        let params=json!({"sessionId":id,"cwd":cwd,"mcpServers":servers});
        let resumes=agent.pointer("/sessionCapabilities/resume").is_some_and(|resume|!resume.is_null());
        let loads=agent.get("loadSession").and_then(Value::as_bool)==Some(true);
        let answer=if resumes { self.call("session/resume",params).await? }
            else if loads { self.replaying=true; let answer=self.call("session/load",params).await; self.replaying=false; answer? }
            else { return Err(anyhow!("could not resume the session {id}: the agent does not keep sessions")) };
        match answer {
            Ok(result)=>{ self.offer.read(&result); Ok(()) }
            Err(error) if error.code==AUTH_REQUIRED=>Err(self.refused(error,methods)),
            Err(error)=>Err(anyhow!("could not resume the session {id}: {}",error.message)),
        }
    }

    /// Põe a sessão no modo do pedido, se o agente anunciou esse modo. O modo
    /// que não existe fica de fora: a recusa do JayV continua valendo.
    async fn choose_mode(&mut self,session:&str,mode:&str)->Result<()> {
        if mode.is_empty() { return Ok(()); }
        if let Some(option)=self.offer.option("mode").cloned() {
            if option.get("currentValue").and_then(Value::as_str)==Some(mode)||!Offer::values(&option).iter().any(|value|value==mode) { return Ok(()); }
            let config=text(&option,"id");
            if let Ok(result)=self.call("session/set_config_option",json!({"sessionId":session,"configId":config,"value":mode})).await? { self.offer.read(&result); }
            return Ok(());
        }
        let known=self.offer.modes.get("availableModes").and_then(Value::as_array).is_some_and(|modes|modes.iter().any(|known|known.get("id").and_then(Value::as_str)==Some(mode)));
        let current=self.offer.modes.get("currentModeId").and_then(Value::as_str)==Some(mode);
        if known&&!current { let _=self.call("session/set_mode",json!({"sessionId":session,"modeId":mode})).await?; }
        Ok(())
    }

    /// Escolhe o modelo do pedido entre os que o agente anunciou. O agente que
    /// não anuncia modelos fica com o dele (ou com o `{model}` da linha).
    async fn choose_model(&mut self,session:&str,model:&str)->Result<()> {
        if model.is_empty()||model==AGENT_DEFAULT { return Ok(()); }
        if let Some(option)=self.offer.option("model").cloned() {
            if option.get("currentValue").and_then(Value::as_str)==Some(model) { return Ok(()); }
            let config=text(&option,"id");
            return match self.call("session/set_config_option",json!({"sessionId":session,"configId":config,"value":model})).await? {
                Ok(result)=>{ self.offer.read(&result); Ok(()) }
                Err(error)=>Err(Text::new("provider.acpModel").with("provider",&self.name).with("model",model).with("reason",error.message).into()),
            };
        }
        if self.offer.models.get("availableModels").is_some() {
            if self.offer.models.get("currentModelId").and_then(Value::as_str)==Some(model) { return Ok(()); }
            return match self.call("session/set_model",json!({"sessionId":session,"modelId":model})).await? {
                Ok(_)=>Ok(()),
                Err(error)=>Err(Text::new("provider.acpModel").with("provider",&self.name).with("model",model).with("reason",error.message).into()),
            };
        }
        Ok(())
    }

    /// Um pedido inteiro: aperto de mão, sessão, modo, modelo e o pedido.
    async fn turn(&mut self,acp:&AcpConfig,servers:&[Value],cwd:&Path,model:&str,resume:Option<&str>,prompt:&str)->Result<Turn> {
        let capabilities=self.initialize().await?;
        let methods=Self::sign_in(&capabilities);
        let servers=servers.iter().filter_map(acp_server).collect::<Vec<_>>();
        let session=match resume {
            Some(id)=>{ self.resume(id,&capabilities,cwd,&servers,&methods).await?; id.to_string() }
            None=>self.start(cwd,&servers,&methods).await?,
        };
        self.current_session=Some(session.clone());
        let mode=if acp.planning { &acp.plan_mode } else { &acp.build_mode };
        self.choose_mode(&session,mode).await?;
        self.choose_model(&session,model).await?;
        self.prompting=true;
        let answer=self.call("session/prompt",json!({"sessionId":session,"prompt":[{"type":"text","text":prompt}]})).await;
        self.prompting=false;
        let result=match answer? { Ok(result)=>result, Err(error)=>return Err(self.refused(error,&methods)) };
        if let Some(reason)=self.stopped { return Err(reason.text().into()); }
        match result.get("stopReason").and_then(Value::as_str).unwrap_or("end_turn") {
            "refusal"=>return Err(self.failed("the agent refused the request",FAILURE_ANNOUNCED)),
            "cancelled"=>return Err(self.failed("the agent cancelled the request",FAILURE_ANNOUNCED)),
            _=>{}
        }
        // Recusado, o agente às vezes para sem dizer nada (o OpenCode espera
        // outra instrução): a resposta vazia diz o que foi recusado.
        if self.response.trim().is_empty()&&!self.denied.is_empty() {
            return Err(Text::new("provider.acpDenied").with("provider",&self.name).with("actions",self.denied.join("; ")).into());
        }
        let agent=capabilities.get("agentCapabilities").cloned().unwrap_or(Value::Null);
        let keeps=agent.get("loadSession").and_then(Value::as_bool)==Some(true)||agent.pointer("/sessionCapabilities/resume").is_some_and(|resume|!resume.is_null());
        let count=|field:&str|result.pointer(&format!("/usage/{field}")).and_then(Value::as_u64).unwrap_or(0);
        Ok(Turn{session:keeps.then_some(session),usage:(count("inputTokens"),count("outputTokens"),count("cachedReadTokens"))})
    }

    /// Os modelos que a sessão anuncia, para a descoberta.
    async fn models(&mut self,servers:&[Value],cwd:&Path)->Result<Vec<String>> {
        let capabilities=self.initialize().await?;
        let methods=Self::sign_in(&capabilities);
        let servers=servers.iter().filter_map(acp_server).collect::<Vec<_>>();
        self.start(cwd,&servers,&methods).await?;
        Ok(self.offer.models())
    }
}

/// O erro do JSON-RPC em uma linha: a mensagem e, quando há, o motivo que o
/// agente pôs em `data`.
fn describe(error:&Value)->String {
    let message=unwrap_message(error.get("message").and_then(Value::as_str).unwrap_or("unknown error"));
    let detail=match error.get("data") {
        Some(Value::String(detail))=>Some(detail.clone()),
        Some(Value::Object(data))=>["reason","message","details"].iter().find_map(|field|data.get(*field).and_then(Value::as_str).map(str::to_string)),
        _=>None,
    };
    match detail { Some(detail) if !detail.is_empty()&&detail!=message=>format!("{message}: {detail}"), _=>message }
}

/// A mensagem de dentro, quando o agente repassa o erro da API dele como
/// JSON dentro do texto (o Gemini CLI manda `{"error":{"message":"{…}"}}`).
fn unwrap_message(text:&str)->String {
    let mut text=text.trim().to_string();
    for _ in 0..4 {
        let Ok(value)=serde_json::from_str::<Value>(&text) else { break };
        let Some(inner)=value.pointer("/error/message").or_else(||value.get("message")).and_then(Value::as_str) else { break };
        text=inner.trim().to_string();
    }
    text
}

/// Um servidor MCP da aba MCP no formato do ACP: `stdio` com o ambiente em
/// lista; `http` com os cabeçalhos em lista.
fn acp_server(value:&Value)->Option<Value> {
    let server:crate::mcp::McpServer=serde_json::from_value(value.clone()).ok()?;
    let pairs=|map:&std::collections::BTreeMap<String,String>|map.iter().map(|(name,value)|json!({"name":name,"value":value})).collect::<Vec<_>>();
    Some(if server.url.trim().is_empty() {
        json!({"name":server.name,"command":server.command,"args":server.args,"env":pairs(&server.env)})
    } else {
        json!({"type":"http","name":server.name,"url":server.url,"headers":pairs(&server.headers)})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(kind:&str,title:&str,input:Value)->ToolCall { ToolCall{id:"c1".into(),title:title.into(),kind:kind.into(),status:"pending".into(),raw_input:input,content:String::new()} }

    #[test] fn planning_allows_only_reading() {
        let planning=Policy{planning:true,auto:true,..Policy::default()};
        assert!(planning.allows(&[call("read","README.md",Value::Null),call("search","grep",Value::Null),call("fetch","https://docs",Value::Null)]).is_ok());
        for kind in ["edit","delete","move","execute","other","switch_mode",""] { assert!(planning.allows(&[call(kind,"x",Value::Null)]).is_err(),"{kind}"); }
    }

    #[test] fn development_refuses_only_the_blocked_commands() {
        let build=Policy{auto:true,denied:vec!["git push".into(),"rm".into()],..Policy::default()};
        assert!(build.allows(&[call("edit","src/main.rs",Value::Null)]).is_ok());
        assert!(build.allows(&[call("execute","bash",json!({"command":"npm test"}))]).is_ok());
        for command in ["git push origin main","cd app && git push","bash -c \"git push -f\"","sudo rm -rf /tmp/x","CI=1 rm x"] {
            assert!(build.allows(&[call("execute","bash",json!({"command":command}))]).is_err(),"{command}");
        }
        assert!(build.allows(&[call("execute","bash",json!({"command":"git pull"}))]).is_ok(),"`git push` não bloqueia `git pull`");
        assert!(build.allows(&[call("execute","bash",Value::Null)]).is_err(),"sem saber o comando, a regra não se confere");
        assert!(Policy{auto:true,..Policy::default()}.allows(&[call("execute","bash",Value::Null)]).is_ok(),"sem regra, nada a conferir");
    }

    #[test] fn a_locked_agent_runs_only_the_granted_commands() {
        let locked=Policy{auto:false,granted:vec!["git add".into(),"npm test".into()],..Policy::default()};
        assert!(locked.allows(&[call("edit","a.rs",Value::Null)]).is_ok());
        assert!(locked.allows(&[call("execute","x",json!({"command":"git add src && npm test -- --watch=false"}))]).is_ok());
        assert!(locked.allows(&[call("execute","x",json!({"command":"git add . && git commit -m x"}))]).is_err());
        assert!(locked.allows(&[call("execute","x",json!({"command":["npm","test"]}))]).is_ok(),"o comando em lista");
    }

    /// Cada agente põe o comando num lugar: o OpenCode no `rawInput`, o Kimi
    /// só no texto do pedido, e o título às vezes é o comando.
    #[test] fn the_command_is_found_where_each_agent_puts_it() {
        assert_eq!(call("execute","bash",json!({"command":"git push"})).command().as_deref(),Some("git push"));
        let kimi=ToolCall{content:"Requesting approval to Running: git push origin main".into(),..call("","Bash",Value::Null)};
        assert_eq!(kimi.command().as_deref(),Some("git push origin main"));
        let json_text=ToolCall{content:"{\"command\": \"ls -la\"}".into(),..call("execute","Bash",Value::Null)};
        assert_eq!(json_text.command().as_deref(),Some("ls -la"));
        assert_eq!(call("execute","git status --short",Value::Null).command().as_deref(),Some("git status --short"));
        assert_eq!(call("execute","Bash",Value::Null).command(),None,"o nome da ferramenta não é o comando");
    }

    #[test] fn a_wrapped_api_error_comes_out_readable() {
        let gemini=r#"{"error":{"message":"{\n  \"error\": {\n    \"code\": 400,\n    \"message\": \"API key not valid. Please pass a valid API key.\"\n  }\n}\n","code":400}}"#;
        assert_eq!(describe(&json!({"code":-32603,"message":gemini})),"API key not valid. Please pass a valid API key.");
        assert_eq!(describe(&json!({"code":-32000,"message":"Authentication required","data":{"reason":"Log in first"}})),"Authentication required: Log in first");
    }

    #[test] fn the_permission_answer_is_a_one_time_choice() {
        let options=json!([{"optionId":"always","kind":"allow_always"},{"optionId":"once","kind":"allow_once"},{"optionId":"reject","kind":"reject_once"}]);
        assert_eq!(choose(&options,true).as_deref(),Some("once"));
        assert_eq!(choose(&options,false).as_deref(),Some("reject"));
        assert_eq!(choose(&json!([{"optionId":"yes","kind":"allow_always"}]),false),None,"sem opção de recusa, cancela");
    }

    #[test] fn the_model_placeholder_leaves_with_its_flag_on_the_default() {
        let args=vec!["acp".to_string(),"--model".into(),"{model}".into()];
        assert_eq!(launch_args(&args,"opus"),["acp","--model","opus"]);
        assert_eq!(launch_args(&args,AGENT_DEFAULT),["acp"]);
    }

    #[test] fn the_offer_reads_options_groups_and_the_old_fields() {
        let mut offer=Offer::default();
        offer.read(&json!({"configOptions":[{"id":"model","category":"model","options":[{"group":"a","name":"A","options":[{"value":"a/1"},{"value":"a/2"}]},{"group":"b","name":"B","options":[{"value":"b/1"}]}]}]}));
        assert_eq!(offer.models(),["a/1","a/2","b/1"]);
        let mut old=Offer::default();
        old.read(&json!({"models":{"availableModels":[{"modelId":"auto"},{"modelId":"gemini-3.1-pro-preview"}],"currentModelId":"auto"}}));
        assert_eq!(old.models(),["auto","gemini-3.1-pro-preview"]);
    }

    #[test] fn mcp_servers_go_in_the_acp_shape() {
        let stdio=serde_json::to_value(crate::mcp::McpServer{name:"files".into(),command:"npx".into(),args:vec!["-y".into(),"server".into()],env:[("TOKEN".to_string(),"t".to_string())].into(),..Default::default()}).expect("json");
        assert_eq!(acp_server(&stdio),Some(json!({"name":"files","command":"npx","args":["-y","server"],"env":[{"name":"TOKEN","value":"t"}]})));
        let http=serde_json::to_value(crate::mcp::McpServer{name:"docs".into(),transport:"http".into(),url:"https://mcp.example.com".into(),headers:[("Authorization".to_string(),"Bearer x".to_string())].into(),..Default::default()}).expect("json");
        assert_eq!(acp_server(&http),Some(json!({"type":"http","name":"docs","url":"https://mcp.example.com","headers":[{"name":"Authorization","value":"Bearer x"}]})));
    }

    /// Um agente ACP de mentira, em Python: anuncia modos e modelos, narra
    /// uma ferramenta, pede permissão para um comando e fala em duas
    /// mensagens. Tudo o que o JayV mandou vai para o registro.
    const FAKE_AGENT:&str=r#"
import json, os, sys
log = open(os.environ["FAKE_LOG"], "a")
def send(message):
    sys.stdout.write(json.dumps(message) + "\n"); sys.stdout.flush()
def update(session, body):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session, "update": body}})
options = lambda mode, model: [
    {"id": "mode", "category": "mode", "type": "select", "currentValue": mode, "options": [{"value": "default", "name": "Default"}, {"value": "plan", "name": "Plan"}]},
    {"id": "model", "category": "model", "type": "select", "currentValue": model, "options": [{"value": "m1", "name": "M1"}, {"value": "m2", "name": "M2"}]},
]
mode, model = "default", "m1"
for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params", {})
    log.write(json.dumps({"method": method, "params": params}) + "\n"); log.flush()
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}, "authMethods": [{"id": "login", "name": "Login", "description": "Run fake-agent login"}]}})
    elif method == "session/new":
        if os.environ.get("FAKE_AUTH"):
            send({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32000, "message": "Authentication required"}})
        else:
            send({"jsonrpc": "2.0", "id": message["id"], "result": {"sessionId": "s1", "configOptions": options(mode, model)}})
    elif method == "session/load":
        update(params["sessionId"], {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "OLD ANSWER"}})
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"configOptions": options(mode, model)}})
    elif method == "session/set_config_option":
        if params["configId"] == "mode": mode = params["value"]
        else: model = params["value"]
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"configOptions": options(mode, model)}})
    elif method == "session/prompt":
        session = params["sessionId"]
        update(session, {"sessionUpdate": "agent_message_chunk", "messageId": "a", "content": {"type": "text", "text": "Hello"}})
        update(session, {"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "bash", "kind": "execute", "status": "pending", "rawInput": {"command": "git push origin main"}})
        send({"jsonrpc": "2.0", "id": 900, "method": "session/request_permission", "params": {"sessionId": session, "toolCall": {"toolCallId": "t1"}, "options": [{"optionId": "yes", "kind": "allow_once", "name": "Yes"}, {"optionId": "no", "kind": "reject_once", "name": "No"}]}})
        answer = json.loads(sys.stdin.readline())
        log.write(json.dumps({"permission": answer["result"]["outcome"]}) + "\n"); log.flush()
        update(session, {"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"})
        update(session, {"sessionUpdate": "agent_message_chunk", "messageId": "b", "content": {"type": "text", "text": "World"}})
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"stopReason": "end_turn", "usage": {"inputTokens": 12, "outputTokens": 3}}})
"#;

    struct Fake { _dir:tempfile::TempDir, log:PathBuf, config:ProviderConfig }

    fn fake(acp:AcpConfig)->Option<Fake> {
        crate::llm::locate("python3")?;
        let dir=tempfile::tempdir().expect("pasta");
        let script=dir.path().join("agent.py");
        std::fs::write(&script,FAKE_AGENT).expect("script");
        let log=dir.path().join("log.jsonl");
        let env=vec![("FAKE_LOG".to_string(),log.display().to_string())];
        let config=ProviderConfig{enabled:true,kind:"acp".into(),command:Some("python3".into()),args:vec![script.display().to_string()],timeout:20,acp:Some(AcpConfig{env,..acp}),..Default::default()};
        Some(Fake{_dir:dir,log,config})
    }

    fn logged(fake:&Fake)->Vec<Value> { std::fs::read_to_string(&fake.log).unwrap_or_default().lines().filter_map(|line|serde_json::from_str(line).ok()).collect() }

    fn ask()->Vec<ChatMessage> { vec![ChatMessage{role:"user".into(),content:"push it".into()}] }

    use std::path::PathBuf;

    #[tokio::test] async fn a_development_turn_sets_the_model_answers_the_permission_and_joins_the_messages() {
        let Some(fake)=fake(AcpConfig{build_mode:"default".into(),plan_mode:"plan".into(),auto:true,..AcpConfig::default()}) else { return };
        let provider=AcpProvider::new("fake".into(),fake.config.clone(),Workdir::default());
        let answer=provider.chat_turn(&ask(),"m2",None,None,&Pulse::silent()).await.expect("responde");
        assert_eq!(answer.response,format!("Hello\n\n{MESSAGE_BREAK}\nWorld"));
        assert_eq!(answer.session.as_deref(),Some("s1"),"o agente guarda a sessão");
        assert_eq!((answer.input_tokens,answer.output_tokens),(12,3),"a conta que o agente informou");
        let log=logged(&fake);
        assert!(log.iter().any(|entry|entry["method"]=="session/set_config_option"&&entry["params"]["value"]=="m2"),"{log:?}");
        assert!(!log.iter().any(|entry|entry["method"]=="session/set_config_option"&&entry["params"]["configId"]=="mode"),"já está no modo do desenvolvimento");
        assert!(log.iter().any(|entry|entry["permission"]["optionId"]=="yes"),"nenhuma regra bloqueia: aprova uma vez");
    }

    #[tokio::test] async fn planning_enters_the_read_only_mode_and_refuses_the_command() {
        let Some(fake)=fake(AcpConfig{build_mode:"default".into(),plan_mode:"plan".into(),auto:true,..AcpConfig::default()}) else { return };
        let provider=AcpProvider::new("fake".into(),fake.config.for_planning(),Workdir::default());
        provider.chat_turn(&ask(),AGENT_DEFAULT,None,None,&Pulse::silent()).await.expect("responde");
        let log=logged(&fake);
        assert!(log.iter().any(|entry|entry["method"]=="session/set_config_option"&&entry["params"]["configId"]=="mode"&&entry["params"]["value"]=="plan"),"{log:?}");
        assert!(!log.iter().any(|entry|entry["params"]["configId"]=="model"),"`default` fica com o modelo do agente");
        assert!(log.iter().any(|entry|entry["permission"]["optionId"]=="no"),"no planejamento, o comando é recusado");
    }

    #[tokio::test] async fn a_blocked_command_is_refused_in_development() {
        let Some(fake)=fake(AcpConfig{auto:true,denied:vec!["git push".into()],..AcpConfig::default()}) else { return };
        let provider=AcpProvider::new("fake".into(),fake.config.clone(),Workdir::default());
        provider.chat_turn(&ask(),"m1",None,None,&Pulse::silent()).await.expect("responde");
        assert!(logged(&fake).iter().any(|entry|entry["permission"]["optionId"]=="no"));
    }

    /// Retomada por `session/load`: o agente repete a conversa, e a resposta
    /// não traz o que ele repetiu.
    #[tokio::test] async fn a_resumed_session_does_not_repeat_the_old_answer() {
        let Some(fake)=fake(AcpConfig{auto:true,..AcpConfig::default()}) else { return };
        let provider=AcpProvider::new("fake".into(),fake.config.clone(),Workdir::default());
        let answer=provider.chat_turn(&ask(),"m1",None,Some("s1"),&Pulse::silent()).await.expect("responde");
        assert!(!answer.response.contains("OLD ANSWER"),"{}",answer.response);
        assert!(logged(&fake).iter().any(|entry|entry["method"]=="session/load"));
    }

    #[tokio::test] async fn an_agent_that_wants_a_login_says_how() {
        let Some(mut fake)=fake(AcpConfig{auto:true,..AcpConfig::default()}) else { return };
        if let Some(acp)=fake.config.acp.as_mut() { acp.env.push(("FAKE_AUTH".into(),"1".into())); }
        let provider=AcpProvider::new("fake".into(),fake.config.clone(),Workdir::default());
        let error=provider.chat_turn(&ask(),"m1",None,None,&Pulse::silent()).await.expect_err("pede login");
        let text=Text::from(error);
        assert_eq!(text.key,"provider.acpLogin");
        assert!(format!("{:?}",text.params).contains("Run fake-agent login"));
    }

    /// Um agente de verdade, à mão: `JAYV_ACP="kimi acp"` (o comando e os
    /// argumentos), `JAYV_ACP_MODE=build|plan`, `JAYV_ACP_PLAN`/`JAYV_ACP_BUILD`
    /// (os modos), `JAYV_ACP_ENV="NOME=valor"`, `JAYV_ACP_DIR` (a pasta) e
    /// `JAYV_ACP_PROMPT`. Imprime a resposta.
    #[tokio::test] #[ignore] async fn a_real_agent() {
        let line=std::env::var("JAYV_ACP").expect("JAYV_ACP");
        let mut words=line.split_whitespace().map(str::to_string);
        let command=words.next().expect("comando");
        let env=std::env::var("JAYV_ACP_ENV").ok().and_then(|pair|pair.split_once('=').map(|(key,value)|(key.to_string(),value.to_string()))).into_iter().collect();
        let acp=AcpConfig{plan_mode:std::env::var("JAYV_ACP_PLAN").unwrap_or_default(),build_mode:std::env::var("JAYV_ACP_BUILD").unwrap_or_default(),env,auto:true,denied:vec!["git push".into()],..AcpConfig::default()};
        let config=ProviderConfig{enabled:true,kind:"acp".into(),command:Some(command),args:words.collect(),timeout:120,acp:Some(acp),..Default::default()};
        let config=if std::env::var("JAYV_ACP_MODE").as_deref()==Ok("plan") { config.for_planning() } else { config };
        let workdir=Workdir::default();
        if let Ok(dir)=std::env::var("JAYV_ACP_DIR") { workdir.focus(PathBuf::from(dir)); }
        println!("models: {:?}",discover("real",&config).await.map(|models|models.into_iter().map(|model|model.id).take(8).collect::<Vec<_>>()));
        let provider=AcpProvider::new("real".into(),config,workdir);
        let prompt=std::env::var("JAYV_ACP_PROMPT").unwrap_or_else(|_|"Create hello.txt with the word hi.".into());
        let (pulse,mut beats)=Pulse::channel();
        let printer=tokio::spawn(async move { while let Some(beat)=beats.recv().await { println!("beat: {beat:?}"); } });
        let answer=provider.chat_turn(&[ChatMessage{role:"user".into(),content:prompt}],&std::env::var("JAYV_ACP_MODEL").unwrap_or_else(|_|AGENT_DEFAULT.into()),None,None,&pulse).await;
        drop(pulse);
        let _=printer.await;
        println!("answer: {answer:#?}");
    }

    #[tokio::test] async fn discovery_lists_the_default_and_the_announced_models() {
        let Some(fake)=fake(AcpConfig::default()) else { return };
        let models=discover("fake",&fake.config).await.expect("lista");
        assert_eq!(models.iter().map(|model|model.id.as_str()).collect::<Vec<_>>(),[AGENT_DEFAULT,"m1","m2"]);
    }
}
