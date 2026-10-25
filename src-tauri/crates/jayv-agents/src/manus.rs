//! A Manus como agente de API (o protocolo `manus` dos mods). A Manus não
//! conversa por mensagens: ela recebe uma tarefa, trabalha nela na nuvem dela
//! e narra o que faz numa lista de eventos. O JayV cria a tarefa
//! (`task.create`), lê os eventos novos de tempos em tempos
//! (`task.listMessages`) até a Manus parar, e a fala dela é a resposta. O
//! pedido seguinte do chat continua a mesma tarefa (`task.sendMessage`).
//!
//! A Manus não mexe no projeto do computador: trabalha só na nuvem dela, e o
//! que ela produz chega como texto e links de arquivos.

use crate::config::ProviderConfig;
use crate::i18n::Text;
use crate::model::{ChatMessage, ProviderResponse};
use crate::progress::{Beat, Pulse};
use crate::providers::{provider_error, request_error, retry_after, with_retry, Provider, RetryError, RetryPolicy, MESSAGE_BREAK};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// O endereço da API de tarefas.
const BASE:&str="https://api.manus.ai/v2";
/// Os perfis de agente que a API aceita no lugar do modelo.
pub const PROFILES:[&str;3]=["standard","lite","max"];
/// De quanto em quanto tempo os eventos novos são lidos.
const POLL:Duration=Duration::from_secs(3);
/// O tamanho do texto de uma mensagem: a API recusa acima de uns 5 mil
/// tokens.
const PROMPT_LIMIT:usize=18_000;
/// Quantos eventos cada leitura traz.
const PAGE:u32=200;

pub struct ManusProvider { name:String, config:ProviderConfig, client:reqwest::Client }

impl ManusProvider {
    pub fn new(name:String,config:ProviderConfig)->Self {
        let client=crate::lockdown::http_client(Duration::from_secs(config.timeout.clamp(30,120))).build().expect("HTTP client");
        Self{name,config,client}
    }

    fn base(&self)->String { self.config.base_url.as_deref().filter(|base|!base.trim().is_empty()).unwrap_or(BASE).trim_end_matches('/').to_string() }

    fn authorized(&self,request:reqwest::RequestBuilder)->reqwest::RequestBuilder {
        match self.config.api_key.as_deref().filter(|key|!key.is_empty()) { Some(key)=>request.header("x-manus-api-key",key), None=>request }
    }

    /// Uma chamada à API, repetida enquanto o erro for passageiro (rede, 429,
    /// 5xx). A resposta com `ok: false` é recusa da Manus, e não se repete.
    async fn call(&self,method:reqwest::Method,path:&str,query:&[(&str,String)],body:Option<&Value>)->Result<Value> {
        let url=format!("{}/{path}",self.base());
        with_retry(RetryPolicy::default(),|_attempt| {
            let mut request=self.authorized(self.client.request(method.clone(),&url)).query(query);
            if let Some(body)=body { request=request.json(body); }
            async move {
                let response=request.send().await.map_err(|error|RetryError::from_transport(&self.name,error))?;
                let status=response.status();
                let after=retry_after(response.headers());
                let text=response.text().await.map_err(|error|RetryError::from_transport(&self.name,error))?;
                let body=serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
                if !status.is_success() { return Err(RetryError::from_status(status,after,anyhow!("provider returned {status}: {}",manus_error(&body)))); }
                if body.get("ok").and_then(Value::as_bool)==Some(false) { return Err(RetryError::fatal(anyhow!("Manus refused the request: {}",manus_error(&body)))); }
                Ok(body)
            }
        }).await
    }

    /// O último evento da tarefa, para a continuação ler só o que vier depois.
    async fn last_event(&self,task:&str)->Result<Option<String>> {
        let body=self.call(reqwest::Method::GET,"task.listMessages",&[("task_id",task.to_string()),("order","desc".into()),("limit","1".into()),("verbose","true".into())],None).await?;
        Ok(body.get("messages").and_then(Value::as_array).and_then(|events|events.first()).and_then(|event|event.get("id")).and_then(Value::as_str).map(str::to_string))
    }

    async fn converse(&self,messages:&[ChatMessage],model:&str,resume:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> {
        if let Some(reason)=pulse.stop().reason() { return Err(reason.text().into()); }
        crate::guard::enter()?;
        let started=Instant::now();
        let prompt=compact(messages);
        let content=json!({"content":prompt});
        let (task,mut after)=match resume {
            Some(task)=>{
                let after=self.last_event(task).await.map_err(|error|anyhow!("could not resume the session {task}: {error:#}"))?;
                self.call(reqwest::Method::POST,"task.sendMessage",&[],Some(&json!({"task_id":task,"message":content}))).await
                    .map_err(|error|anyhow!("could not resume the session {task}: {error:#}"))?;
                (task.to_string(),after)
            }
            None=>{
                let mut body=json!({"message":content,"interactive_mode":false});
                if !model.is_empty()&&model!=crate::acp::AGENT_DEFAULT { body["agent_profile"]=json!(model); }
                let created=self.call(reqwest::Method::POST,"task.create",&[],Some(&body)).await.map_err(|error|request_failure(&self.name,error))?;
                let task=created.get("task_id").and_then(Value::as_str).map(str::to_string).ok_or_else(||anyhow!("Manus created a task without an id"))?;
                if let Some(url)=created.get("task_url").and_then(Value::as_str) { pulse.beat(Beat::Agent{line:format!("task: {url}")}); }
                (task,None)
            }
        };
        let mut response=String::new();
        let mut heard=Instant::now();
        let silence=Duration::from_secs(self.config.timeout.max(30));
        loop {
            let mut query=vec![("task_id",task.clone()),("order","asc".into()),("limit",PAGE.to_string()),("verbose","true".into())];
            if let Some(start)=&after { query.push(("start_event_id",start.clone())); }
            let page=tokio::select! {
                biased;
                reason=pulse.stop().stopped()=>{ self.stop(&task).await; return Err(reason.text().into()); }
                page=self.call(reqwest::Method::GET,"task.listMessages",&query,None)=>page?,
            };
            let events=page.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
            let mut finished=false;
            for event in &events {
                let id=event.get("id").and_then(Value::as_str).map(str::to_string);
                // `start_event_id` é inclusivo: o evento de onde a leitura
                // partiu já foi lido.
                if id.is_some()&&id==after { continue; }
                heard=Instant::now();
                match read(event) {
                    Event::Said(text)=>{
                        if !response.trim().is_empty() { let gap=format!("\n\n{MESSAGE_BREAK}\n"); response.push_str(&gap); pulse.beat(Beat::Chunk{text:gap}); }
                        response.push_str(&text);
                        pulse.beat(Beat::Chunk{text});
                    }
                    Event::Step(line)=>pulse.beat(Beat::Agent{line}),
                    Event::Done=>finished=true,
                    Event::Failed(reason)=>return Err(Text::new("provider.failed").with("provider",&self.name).with("exit",Text::new("provider.exit.announced")).with("reason",reason).with("origin",crate::providers::FAILURE_ANNOUNCED).into()),
                    Event::Nothing=>{}
                }
                if id.is_some() { after=id; }
            }
            if finished { break; }
            if page.get("has_more").and_then(Value::as_bool)==Some(true) { continue; }
            if heard.elapsed()>silence { self.stop(&task).await; return Err(Text::new("provider.silent").with("provider",&self.name).with("seconds",silence.as_secs()).into()); }
            tokio::select! {
                biased;
                reason=pulse.stop().stopped()=>{ self.stop(&task).await; return Err(reason.text().into()); }
                _=tokio::time::sleep(POLL)=>{}
            }
        }
        let meter=crate::usage::Meter::new(&self.name,model,"manus");
        let (input,output)=meter.settle(&prompt,&response,true,None);
        Ok(ProviderResponse{response,input_tokens:input as usize,output_tokens:output as usize,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis(),session:Some(task)})
    }

    /// Para a tarefa na Manus: largada, ela seguiria gastando créditos.
    async fn stop(&self,task:&str) {
        let _=self.call(reqwest::Method::POST,"task.stop",&[],Some(&json!({"task_id":task}))).await;
    }
}

#[async_trait]
impl Provider for ManusProvider {
    fn name(&self)->&str { &self.name }
    async fn chat(&self,messages:&[ChatMessage],model:&str)->Result<ProviderResponse> { self.converse(messages,model,None,&Pulse::silent()).await }
    async fn chat_stream(&self,messages:&[ChatMessage],model:&str,pulse:&Pulse)->Result<ProviderResponse> { self.converse(messages,model,None,pulse).await }
    fn resumes(&self)->bool { true }
    async fn chat_turn(&self,messages:&[ChatMessage],model:&str,_effort:Option<&str>,resume:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> { self.converse(messages,model,resume,pulse).await }
    async fn chat_once(&self,messages:&[ChatMessage],model:&str,_effort:Option<&str>,pulse:&Pulse)->Result<ProviderResponse> { self.converse(messages,model,None,pulse).await }
}

/// O que um evento da tarefa significa para quem espera.
#[derive(Debug,PartialEq)]
enum Event { Said(String), Step(String), Done, Failed(String), Nothing }

/// Um evento de `task.listMessages`. A fala da Manus é a resposta (com os
/// arquivos que ela anexou, como links); a ferramenta e o passo do plano são
/// relato; a parada encerra o pedido — inclusive quando ela espera uma
/// resposta, que vai no pedido seguinte do chat.
fn read(event:&Value)->Event {
    let kind=event.get("type").and_then(Value::as_str).unwrap_or_default();
    let payload=event.get(kind).cloned().unwrap_or(Value::Null);
    let field=|name:&str|payload.get(name).and_then(Value::as_str).map(str::trim).filter(|text|!text.is_empty()).map(str::to_string);
    match kind {
        "assistant_message"=>{
            let mut text=field("content").unwrap_or_default();
            for attachment in payload.get("attachments").and_then(Value::as_array).into_iter().flatten() {
                let url=["url","file_url","fileUrl","download_url"].iter().find_map(|key|attachment.get(*key).and_then(Value::as_str));
                let name=["name","file_name","fileName","title"].iter().find_map(|key|attachment.get(*key).and_then(Value::as_str)).or(url);
                if let (Some(url),Some(name))=(url,name) { text.push_str(&format!("\n\n[{name}]({url})")); }
            }
            if text.trim().is_empty() { Event::Nothing } else { Event::Said(text) }
        }
        "status_update"=>match payload.get("agent_status").and_then(Value::as_str).unwrap_or_default() {
            "stopped"|"waiting"=>Event::Done,
            "error"=>Event::Failed(field("description").or_else(||field("brief")).unwrap_or_else(||"the task failed".into())),
            _=>field("brief").map(|brief|Event::Step(format!("status: {brief}"))).unwrap_or(Event::Nothing),
        },
        "error_message"=>Event::Failed(field("content").or_else(||field("error_type")).unwrap_or_else(||"the task failed".into())),
        "tool_used"=>field("brief").or_else(||field("tool")).map(|brief|Event::Step(format!("tool_use: {brief}"))).unwrap_or(Event::Nothing),
        "new_plan_step"=>field("title").map(|title|Event::Step(format!("plan: {title}"))).unwrap_or(Event::Nothing),
        _=>Event::Nothing,
    }
}

/// O pedido no tamanho que a Manus aceita: a última mensagem inteira (cortada
/// no fim, se nem ela couber) e, no espaço que sobrar, as instruções e o
/// histórico mais recente.
fn compact(messages:&[ChatMessage])->String {
    let render=|message:&ChatMessage|format!("{}: {}",message.role,message.content);
    let Some((last,earlier))=messages.split_last() else { return String::new() };
    let mut last=render(last);
    if last.chars().count()>PROMPT_LIMIT { last=last.chars().take(PROMPT_LIMIT).collect(); }
    let mut room=PROMPT_LIMIT.saturating_sub(last.chars().count());
    let mut kept=Vec::new();
    // As instruções do sistema primeiro, depois a conversa de trás para frente.
    let (system,chat):(Vec<&ChatMessage>,Vec<&ChatMessage>)=earlier.iter().partition(|message|message.role=="system");
    for message in system.into_iter().chain(chat.into_iter().rev()) {
        let text=render(message);
        let size=text.chars().count()+2;
        if size>room { continue; }
        room-=size;
        kept.push((message.role=="system",text));
    }
    let (system,mut chat):(Vec<_>,Vec<_>)=kept.into_iter().partition(|(system,_)|*system);
    chat.reverse();
    system.into_iter().chain(chat).map(|(_,text)|text).chain(std::iter::once(last)).collect::<Vec<_>>().join("\n\n")
}

/// O motivo que a Manus dá numa recusa (`error.message`, `message` ou o
/// formato da OpenAI).
fn manus_error(body:&Value)->String {
    body.pointer("/error/message").or_else(||body.get("message")).or_else(||body.get("error")).and_then(Value::as_str).map(str::to_string).unwrap_or_else(||provider_error(body))
}

/// A falha ao criar a tarefa, com o motivo de rede quando foi a rede.
fn request_failure(name:&str,error:anyhow::Error)->anyhow::Error {
    match error.downcast::<reqwest::Error>() { Ok(error)=>request_error(name,error), Err(error)=>error }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role:&str,content:&str)->ChatMessage { ChatMessage{role:role.into(),content:content.into()} }

    #[test] fn the_events_of_a_task_become_speech_steps_and_the_end() {
        let said=json!({"id":"e1","type":"assistant_message","assistant_message":{"content":"Here is the report.","attachments":[{"name":"report.pdf","url":"https://files.manus.im/r.pdf"}]}});
        assert_eq!(read(&said),Event::Said("Here is the report.\n\n[report.pdf](https://files.manus.im/r.pdf)".into()));
        assert_eq!(read(&json!({"type":"tool_used","tool_used":{"tool":"browser","brief":"Opening the site"}})),Event::Step("tool_use: Opening the site".into()));
        assert_eq!(read(&json!({"type":"status_update","status_update":{"agent_status":"stopped"}})),Event::Done);
        assert_eq!(read(&json!({"type":"status_update","status_update":{"agent_status":"waiting","status_detail":{"waiting_for_event_type":"messageAskUser"}}})),Event::Done,"a pergunta da Manus vai no pedido seguinte");
        assert_eq!(read(&json!({"type":"status_update","status_update":{"agent_status":"error","description":"Out of credits"}})),Event::Failed("Out of credits".into()));
        assert_eq!(read(&json!({"type":"error_message","error_message":{"error_type":"quota","content":"No credits left"}})),Event::Failed("No credits left".into()));
        assert_eq!(read(&json!({"type":"user_message","user_message":{"content":"hi"}})),Event::Nothing);
    }

    #[test] fn a_long_conversation_keeps_the_last_message_and_what_fits() {
        let long="x".repeat(PROMPT_LIMIT);
        let messages=vec![message("system","be brief"),message("user",&long),message("assistant","old answer"),message("user","the new question")];
        let prompt=compact(&messages);
        assert!(prompt.starts_with("system: be brief"),"{}",&prompt[..40]);
        assert!(prompt.ends_with("user: the new question"));
        assert!(prompt.contains("assistant: old answer"));
        assert!(!prompt.contains(&long),"o que não cabe fica de fora");
        assert!(prompt.chars().count()<=PROMPT_LIMIT);
        let huge=compact(&[message("user",&"y".repeat(PROMPT_LIMIT*2))]);
        assert_eq!(huge.chars().count(),PROMPT_LIMIT);
    }
}
