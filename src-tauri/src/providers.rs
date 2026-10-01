use crate::{config::ProviderConfig, i18n::Text, model::{ChatMessage, ProviderResponse}, progress::{Beat, Pulse}};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::HashMap, future::Future, net::IpAddr, path::PathBuf, process::Stdio, sync::{Arc, RwLock}, time::{Instant, SystemTime, UNIX_EPOCH}};
use reqwest::{header::{HeaderMap, RETRY_AFTER}, StatusCode};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::Command, time::{sleep, timeout, Duration}};

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn is_local(&self) -> bool { false }
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> Result<ProviderResponse>;

    /// A mesma conversa, contada enquanto acontece. O padrão é a resposta
    /// inteira num pedaço só: um provedor que não saiba transmitir continua
    /// funcionando, e a tela mostra o texto de uma vez em vez de não mostrar
    /// nada. Quem sabe transmitir sobrescreve.
    async fn chat_stream(&self, messages:&[ChatMessage], model:&str, pulse:&Pulse) -> Result<ProviderResponse> {
        let response=self.chat(messages,model).await?;
        pulse.beat(Beat::Chunk{text:response.response.clone()});
        Ok(response)
    }
}

/// A pasta que os agentes de linha de comando enxergam. Eles leem o
/// repositório em que são abertos, então esta é a única forma de o chat de um
/// projeto receber respostas sobre o repositório desse projeto. Um só valor
/// compartilhado: trocar de projeto o move para todos os agentes de uma vez.
#[derive(Clone,Default)]
pub struct Workdir(Arc<RwLock<Option<PathBuf>>>);

impl Workdir {
    pub fn focus(&self,root:PathBuf) { if let Ok(mut current)=self.0.write() {*current=Some(root);} }
    /// A pasta corrente, se houver uma. Um cadeado envenenado não derruba o
    /// pedido: o agente roda onde estiver, como rodava antes.
    fn current(&self)->Option<PathBuf> { self.0.read().ok().and_then(|current|current.clone()) }
}

pub fn build_providers(configs: &HashMap<String, ProviderConfig>, workdir: &Workdir) -> HashMap<String, Box<dyn Provider>> {
    configs.iter().filter(|(_,config)|config.is_executable()).filter_map(|(name, config)| {
        let provider: Box<dyn Provider> = match config.kind.as_str() {
            "openai" => Box::new(HttpProvider::openai(name.clone(), config.clone())),
            "anthropic" => Box::new(HttpProvider::anthropic(name.clone(), config.clone())),
            "openai-compatible" => Box::new(HttpProvider::compatible(name.clone(), config.clone())),
            "cli" => Box::new(CliProvider { name:name.clone(), config:config.clone(), workdir:workdir.clone() }),
            _ => return None,
        }; Some((name.clone(),provider))
    }).collect()
}

pub async fn discover_models(name: &str, config: &ProviderConfig) -> Result<Vec<String>> {
    if config.kind=="cli" { return Err(anyhow!("CLI providers do not offer model discovery")); }
    let default_base=match config.kind.as_str() {
        "openai"=>"https://api.openai.com/v1",
        "anthropic"=>"https://api.anthropic.com/v1",
        "openai-compatible"=>config.base_url.as_deref().filter(|value|!value.trim().is_empty()).ok_or_else(||anyhow!("the provider base URL is required"))?,
        _=>return Err(anyhow!("unsupported provider kind")),
    };
    let client=reqwest::Client::builder().timeout(Duration::from_secs(config.timeout.max(5))).build()?;
    let mut request=client.get(format!("{}/models",default_base.trim_end_matches('/')));
    if config.kind=="anthropic" {
        let key=config.api_key.as_deref().filter(|key|!key.trim().is_empty()).ok_or_else(||anyhow!("an API key is required to load the models"))?;
        request=request.header("x-api-key",key).header("anthropic-version","2023-06-01");
    } else if let Some(key)=config.api_key.as_deref().filter(|key|!key.trim().is_empty()) {
        request=request.bearer_auth(key);
    } else if config.kind=="openai" {
        return Err(anyhow!("an API key is required to load the models"));
    }
    let response=request.send().await.map_err(|error|request_error(name,error))?;
    let status=response.status();
    let body:Value=response.json().await.context("invalid response while loading models")?;
    if !status.is_success(){return Err(anyhow!("the provider returned {status}: {}",provider_error(&body)));}
    let mut models=body.get("data").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|item|item.get("id").and_then(Value::as_str))
        .map(str::to_string).collect::<Vec<_>>();
    models.sort();
    models.dedup();
    if models.is_empty(){return Err(anyhow!("the provider returned no available models"));}
    Ok(models)
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy { pub attempts:u32, pub base_delay:Duration, pub max_delay:Duration }
impl Default for RetryPolicy { fn default()->Self { Self{attempts:3,base_delay:Duration::from_millis(500),max_delay:Duration::from_secs(8)} } }
impl RetryPolicy {
    pub fn delay(&self,attempt:u32,retry_after:Option<Duration>)->Duration {
        if let Some(after)=retry_after { return after.min(self.max_delay); }
        let ceiling=self.base_delay.saturating_mul(1u32<<attempt.saturating_sub(1).min(10)).min(self.max_delay).as_millis() as u64;
        let half=ceiling/2;
        Duration::from_millis(half+jitter(half))
    }
}

pub enum RetryError { Retryable{error:anyhow::Error,after:Option<Duration>}, Fatal(anyhow::Error) }
impl RetryError {
    pub fn retryable(error:anyhow::Error,after:Option<Duration>)->Self { Self::Retryable{error,after} }
    pub fn fatal(error:anyhow::Error)->Self { Self::Fatal(error) }
    pub fn from_status(status:StatusCode,after:Option<Duration>,error:anyhow::Error)->Self { if is_retryable_status(status.as_u16()){Self::Retryable{error,after}}else{Self::Fatal(error)} }
    pub fn from_transport(provider:&str,error:reqwest::Error)->Self { let retryable=is_retryable_transport(&error); let error=request_error(provider,error); if retryable{Self::Retryable{error,after:None}}else{Self::Fatal(error)} }
    pub fn into_error(self)->anyhow::Error { match self { Self::Retryable{error,..}|Self::Fatal(error)=>error } }
}

/// Sem janela de console para o processo filho. No Windows, um app de janela
/// que abre um programa de console ganha um terminal piscando a cada pedido.
pub fn quiet(process:&mut Command)->&mut Command {
    #[cfg(windows)] { const CREATE_NO_WINDOW:u32=0x0800_0000; process.creation_flags(CREATE_NO_WINDOW); }
    process
}

pub fn is_retryable_status(status:u16)->bool { matches!(status,429|500|502|503|504|529) }
pub fn is_retryable_transport(error:&reqwest::Error)->bool { error.is_timeout()||error.is_connect() }
pub fn retry_after(headers:&HeaderMap)->Option<Duration> {
    headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse::<f64>().ok().filter(|seconds|seconds.is_finite()&&*seconds>=0.0).map(Duration::from_secs_f64)
}

pub async fn with_retry<T,F,Fut>(policy:RetryPolicy,mut operation:F)->Result<T>
where F:FnMut(u32)->Fut, Fut:Future<Output=std::result::Result<T,RetryError>> {
    let mut attempt=1;
    loop {
        match operation(attempt).await {
            Ok(value)=>return Ok(value),
            Err(RetryError::Fatal(error))=>return Err(error),
            Err(RetryError::Retryable{error,after})=>{
                if attempt>=policy.attempts.max(1) { return Err(error); }
                sleep(policy.delay(attempt,after)).await;
                attempt+=1;
            }
        }
    }
}

pub fn is_loopback_url(url:&str)->bool { reqwest::Url::parse(url.trim()).ok().and_then(|parsed|parsed.host_str().map(is_loopback_host)).unwrap_or(false) }
fn is_loopback_host(host:&str)->bool {
    let host=host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.');
    host.eq_ignore_ascii_case("localhost")||host.parse::<IpAddr>().is_ok_and(|address|address.is_loopback())
}
fn jitter(span:u64)->u64 { if span==0 {return 0;} SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed|elapsed.subsec_nanos() as u64).unwrap_or(0)%span }

/// O que uma resposta rendeu: o texto e a conta dos tokens.
#[derive(Debug,Default,PartialEq)]
struct Harvest { text:String, input_tokens:usize, output_tokens:usize }

/// Um erro que já não pode ser repetido. A tentativa seguinte só é honesta
/// enquanto nada foi dito.
fn settle(spoken:&AtomicBool,error:RetryError)->RetryError {
    if spoken.load(Ordering::Relaxed) { RetryError::Fatal(error.into_error()) } else { error }
}

/// As linhas inteiras que já chegaram, sem a quebra. O que sobrar continua no
/// balde: um pedaço da rede corta onde quiser, e meio evento não se lê.
fn ready_lines(pending:&mut Vec<u8>)->Vec<String> {
    let mut lines=Vec::new();
    while let Some(cut)=pending.iter().position(|byte|*byte==b'\n') {
        let line=pending.drain(..=cut).collect::<Vec<_>>();
        lines.push(String::from_utf8_lossy(&line).trim_end_matches(['\n','\r']).to_string());
    }
    lines
}

/// Uma linha do fluxo de eventos. Devolve o texto que ela acrescenta à resposta,
/// e vai somando os tokens que passam. Comentários, batidas de coração, o
/// `[DONE]` e os eventos que não interessam não acrescentam nada.
fn read_event(kind:&HttpKind,line:&str,harvest:&mut Harvest)->Option<String> {
    let data=line.strip_prefix("data:")?.trim();
    if data.is_empty()||data=="[DONE]" { return None; }
    let event=serde_json::from_str::<Value>(data).ok()?;
    let tokens=|pointer:&str|event.pointer(pointer).and_then(Value::as_u64).map(|value|value as usize);
    if let Some(input)=tokens("/usage/input_tokens").or_else(||tokens("/message/usage/input_tokens")).or_else(||tokens("/usage/prompt_tokens")) { harvest.input_tokens=input; }
    if let Some(output)=tokens("/usage/output_tokens").or_else(||tokens("/message/usage/output_tokens")).or_else(||tokens("/usage/completion_tokens")) { harvest.output_tokens=output; }
    match kind {
        HttpKind::OpenAi=>event.pointer("/choices/0/delta/content").and_then(Value::as_str).map(str::to_string),
        HttpKind::Anthropic=>{
            if event.get("type").and_then(Value::as_str)!=Some("content_block_delta") { return None; }
            event.pointer("/delta/text").and_then(Value::as_str).map(str::to_string)
        }
    }
}

enum HttpKind { OpenAi, Anthropic }
struct HttpProvider { name:String, config:ProviderConfig, kind:HttpKind, client:reqwest::Client }
impl HttpProvider {
    fn openai(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::OpenAi) }
    fn anthropic(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::Anthropic) }
    fn compatible(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::OpenAi) }
    fn new(name:String,config:ProviderConfig,kind:HttpKind)->Self { let client=reqwest::Client::builder().timeout(Duration::from_secs(config.timeout)).build().expect("HTTP client"); Self{name,config,kind,client} }
    /// O que um corpo de resposta inteiro carrega. O mesmo leitor serve à
    /// conversa comum e ao provedor que ignorou o pedido de transmissão e
    /// devolveu tudo de uma vez — sem ele, um endpoint desses devolveria texto
    /// vazio, que é exatamente o silêncio que a transmissão veio resolver.
    fn reap(&self,body:&Value)->Harvest {
        match self.kind {
            HttpKind::OpenAi=>Harvest{
                text:body.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or_default().to_string(),
                input_tokens:body.pointer("/usage/prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,
                output_tokens:body.pointer("/usage/completion_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,
            },
            HttpKind::Anthropic=>Harvest{
                text:body.pointer("/content/0/text").and_then(Value::as_str).unwrap_or_default().to_string(),
                input_tokens:body.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,
                output_tokens:body.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,
            },
        }
    }

    /// O endereço e o corpo do pedido. O mesmo para a conversa comum e para a
    /// transmitida: o que muda é o `stream`, e é isso que garante que as duas
    /// falem com o mesmo modelo, na mesma temperatura, pelo mesmo caminho.
    fn compose(&self,messages:&[ChatMessage],model:&str,flowing:bool)->(String,Value) {
        match self.kind {
            HttpKind::OpenAi=>{
                let base=self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1").trim_end_matches('/');
                let mut payload=json!({"model":model,"messages":messages,"temperature":0.2});
                if flowing {
                    payload["stream"]=json!(true);
                    // A conta dos tokens no fluxo é extra da OpenAI. Pedi-la a um
                    // servidor apenas compatível é risco de o corpo ser recusado
                    // inteiro por um campo que ele não conhece.
                    if self.config.kind=="openai" { payload["stream_options"]=json!({"include_usage":true}); }
                }
                (format!("{base}/chat/completions"),payload)
            }
            HttpKind::Anthropic=>{
                let base=self.config.base_url.as_deref().unwrap_or("https://api.anthropic.com/v1").trim_end_matches('/');
                let system=messages.iter().find(|m|m.role=="system").map(|m|m.content.clone()).unwrap_or_default();
                let chat=messages.iter().filter(|m|m.role!="system").collect::<Vec<_>>();
                let mut payload=json!({"model":model,"max_tokens":4096,"system":system,"messages":chat});
                if flowing { payload["stream"]=json!(true); }
                (format!("{base}/messages"),payload)
            }
        }
    }

    fn wrap(&self,harvest:Harvest,model:&str,started:Instant)->ProviderResponse {
        ProviderResponse{response:harvest.text,input_tokens:harvest.input_tokens,output_tokens:harvest.output_tokens,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis()}
    }

    fn ask(&self,url:&str,payload:&Value)->reqwest::RequestBuilder {
        let mut request=self.client.post(url).json(payload);
        if matches!(self.kind,HttpKind::Anthropic) { request=request.header("anthropic-version","2023-06-01"); }
        if let Some(key)=self.config.api_key.as_deref().filter(|k|!k.is_empty()) {
            request=if matches!(self.kind,HttpKind::Anthropic){request.header("x-api-key",key)}else{request.bearer_auth(key)};
        }
        request
    }

    /// A conversa transmitida. Duas regras a governam. A primeira: repetir a
    /// chamada só vale enquanto nada saiu — depois do primeiro pedaço a tela já
    /// mostrou texto, e uma segunda tentativa escreveria a resposta duas vezes,
    /// então a partir dali todo erro é fatal. A segunda: o que o servidor manda
    /// não respeita a linha do evento, um pedaço da rede pode cortar um JSON no
    /// meio, e meio JSON não se lê.
    async fn flow(&self,url:&str,payload:&Value,pulse:&Pulse)->Result<Harvest> {
        let spoken=AtomicBool::new(false);
        with_retry(RetryPolicy::default(),|_attempt| async {
            let response=self.ask(url,payload).send().await.map_err(|error|settle(&spoken,RetryError::from_transport(&self.name,error)))?;
            let status=response.status();
            let after=retry_after(response.headers());
            if !status.is_success() {
                let body=response.text().await.ok().and_then(|text|serde_json::from_str::<Value>(&text).ok()).unwrap_or(Value::Null);
                return Err(settle(&spoken,RetryError::from_status(status,after,anyhow!("provider returned {status}: {}",provider_error(&body)))));
            }
            let mut response=response;
            let (mut harvest,mut pending,mut raw,mut heard,mut ended)=(Harvest::default(),Vec::new(),String::new(),false,false);
            while !ended {
                match response.chunk().await.map_err(|error|settle(&spoken,RetryError::from_transport(&self.name,error)))? {
                    Some(piece)=>pending.extend_from_slice(&piece),
                    // O corpo acabou. A última linha pode ter vindo sem quebra
                    // no fim, e ela também é um evento.
                    None=>{ pending.push(b'\n'); ended=true; }
                }
                for line in ready_lines(&mut pending) {
                    if line.starts_with("data:") { heard=true; }
                    else if !heard && !line.trim().is_empty() { raw.push_str(&line); }
                    let Some(text)=read_event(&self.kind,&line,&mut harvest) else {continue};
                    if text.is_empty() { continue; }
                    harvest.text.push_str(&text);
                    spoken.store(true,Ordering::Relaxed);
                    pulse.beat(Beat::Chunk{text});
                }
            }
            // Nenhum evento: o provedor ignorou o pedido de transmissão e
            // respondeu à moda antiga. Ler o corpo inteiro é melhor que
            // devolver o silêncio que a transmissão veio resolver.
            if !heard {
                let body=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
                if body.is_null() { return Err(settle(&spoken,RetryError::Fatal(anyhow!("invalid provider response")))); }
                harvest=self.reap(&body);
                if !harvest.text.is_empty() { spoken.store(true,Ordering::Relaxed); pulse.beat(Beat::Chunk{text:harvest.text.clone()}); }
            }
            Ok(harvest)
        }).await
    }

    async fn send(&self,url:&str,payload:&Value)->Result<Value> {
        with_retry(RetryPolicy::default(),move |_attempt| async move {
            let response=self.ask(url,payload).send().await.map_err(|error|RetryError::from_transport(&self.name,error))?;
            let status=response.status();
            let after=retry_after(response.headers());
            let text=response.text().await.map_err(|error|RetryError::from_transport(&self.name,error))?;
            let body=serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
            if !status.is_success() { return Err(RetryError::from_status(status,after,anyhow!("provider returned {status}: {}",provider_error(&body)))); }
            if body.is_null() { return Err(RetryError::Fatal(anyhow!("invalid provider response"))); }
            Ok(body)
        }).await
    }
}

#[async_trait]
impl Provider for HttpProvider {
    fn name(&self)->&str { &self.name }
    fn is_local(&self)->bool { self.config.local.unwrap_or_else(||self.config.base_url.as_deref().is_some_and(is_loopback_url)) }
    async fn chat(&self,messages:&[ChatMessage],model:&str)->Result<ProviderResponse> {
        let started=Instant::now();
        let (url,payload)=self.compose(messages,model,false);
        let harvest=self.reap(&self.send(&url,&payload).await?);
        Ok(self.wrap(harvest,model,started))
    }

    async fn chat_stream(&self,messages:&[ChatMessage],model:&str,pulse:&Pulse)->Result<ProviderResponse> {
        let started=Instant::now();
        let (url,payload)=self.compose(messages,model,true);
        let harvest=self.flow(&url,&payload,pulse).await?;
        Ok(self.wrap(harvest,model,started))
    }
}

struct CliProvider { name:String, config:ProviderConfig, workdir:Workdir }

/// O argumento que vira o texto do pedido.
const PROMPT:&str="{prompt}";

/// O que uma linha do agente é: resposta ou relato do trabalho. A decisão é
/// pelo conteúdo, não pela configuração — `claude --print` escreve o texto
/// direto, `codex exec` narra o que faz, e nenhum dos dois avisa qual dos dois
/// está na linha. Texto que não é JSON de evento é resposta; evento com fala do
/// assistente é resposta; escrituração do agente não é nada; todo o resto é
/// relato.
fn classify(line:&str)->Option<Beat> {
    let event=serde_json::from_str::<Value>(line.trim()).ok().filter(Value::is_object);
    let Some((event,kind))=event.and_then(|event|event.get("type").and_then(Value::as_str).map(str::to_string).map(|kind|(event,kind)))
        else { return Some(Beat::Chunk{text:format!("{line}\n")}) };
    if let Some(text)=said(&event) { return Some(Beat::Chunk{text}); }
    if bookkeeping(&kind,&event) { return None; }
    Some(Beat::Agent{line:reported(&kind,&event)})
}

/// O que o agente escreve para si mesmo. Em `stream-json` a contagem de tokens
/// de raciocínio, o resultado de cada hook e a resposta pedaço a pedaço saem
/// centenas de vezes por pedido: é sinal de vida — e por isso a linha chega até
/// aqui —, mas não é etapa nenhuma para quem espera, e enfileirá-las afogaria as
/// que importam. O texto da resposta não se perde nisso: ele volta inteiro no
/// evento do assistente, e é de lá que `said` o tira.
const BOOKKEEPING:[&str;6]=["rate_limit_event","stream_event","thinking_tokens","hook_started","hook_progress","hook_response"];
fn bookkeeping(kind:&str,event:&Value)->bool {
    BOOKKEEPING.contains(&kind)||event.get("subtype").and_then(Value::as_str).is_some_and(|subtype|BOOKKEEPING.contains(&subtype))
}

/// A fala do assistente dentro de um evento, se houver. Uma mensagem inteira
/// termina em quebra de linha porque a próxima virá em outro evento; um pedaço
/// de mensagem não, porque ele continua no pedaço seguinte.
fn said(event:&Value)->Option<String> {
    if let Some(parts)=event.pointer("/message/content").and_then(Value::as_array) {
        let text=parts.iter().filter(|part|part.get("type").and_then(Value::as_str)==Some("text"))
            .filter_map(|part|part.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("");
        if !text.is_empty() { return Some(format!("{text}\n")); }
    }
    event.pointer("/delta/text").and_then(Value::as_str).map(str::to_string)
}

/// A recusa que o agente anuncia na própria saída. Em `stream-json` o Claude
/// não explica a falha no canal de erro: ele fecha com um evento `result`
/// marcado `is_error`, e o motivo — modelo inexistente, cota estourada, login
/// vencido — está no texto desse evento. Sem lê-lo, a falha chegava à tela
/// como `CLI provider failed: ` e nada mais.
fn refusal(line:&str)->Option<String> {
    let event=serde_json::from_str::<Value>(line.trim()).ok()?;
    if event.get("type").and_then(Value::as_str)!=Some("result")||event.get("is_error").and_then(Value::as_bool)!=Some(true) { return None; }
    let reason=event.get("result").and_then(Value::as_str).map(str::trim).filter(|reason|!reason.is_empty())
        .map(str::to_string)
        .or_else(||event.get("subtype").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(||"error without description".into());
    Some(reason)
}

/// O fim de um texto, até `limit` caracteres, sem cortar no meio de um
/// caractere. É o pedaço da saída que costuma dizer por que o agente parou.
fn tail(text:&str,limit:usize)->&str {
    let text=text.trim();
    let count=text.chars().count();
    if count<=limit { return text; }
    let start=text.char_indices().nth(count-limit).map(|(at,_)|at).unwrap_or(0);
    &text[start..]
}

/// O relato em uma linha. O evento cru na tela seria JSON aos olhos de quem
/// lê; o tipo mais a pista mais útil que ele traz é o que interessa.
fn reported(kind:&str,event:&Value)->String {
    let tool=event.pointer("/message/content").and_then(Value::as_array)
        .and_then(|parts|parts.iter().find_map(|part|part.get("name").and_then(Value::as_str)));
    let detail=tool.or_else(||["name","command","tool","subtype","status"].iter().find_map(|field|event.get(field).and_then(Value::as_str)));
    match detail { Some(detail)=>format!("{kind}: {detail}"), None=>kind.to_string() }
}

impl CliProvider {
    fn prompt(messages:&[ChatMessage])->String { messages.iter().map(|m|format!("{}: {}",m.role,m.content)).collect::<Vec<_>>().join("\n\n") }

    /// O pedido vai num argumento, e não na entrada padrão, quando o agente
    /// não lê a entrada — é o caso do Copilot, que só aceita `-p`.
    fn inline(&self)->bool { self.config.args.iter().any(|arg|arg==PROMPT) }

    /// A linha de comando deste pedido. O modelo reserva igual ao modelo do
    /// pedido sai inteiro: o Claude recusa os dois iguais, e a reserva é por
    /// agente enquanto o modelo é por pedido.
    fn args(&self,model:&str,prompt:&str)->Vec<String> {
        let mut args=Vec::new();
        let mut given=self.config.args.iter();
        while let Some(arg)=given.next() {
            if arg=="--fallback-model" {
                if let Some(reserve)=given.next().filter(|reserve|reserve.as_str()!=model) { args.extend([arg.clone(),reserve.clone()]); }
                continue;
            }
            args.push(if arg==PROMPT { prompt.to_string() } else { arg.replace("{model}",model) });
        }
        args
    }

    /// O agente aberto, com as três pontas na mão. Um só arranque para as duas
    /// conversas: a que espera o fim e a que acompanha.
    fn open(&self,model:&str,prompt:&str)->Result<tokio::process::Child> {
        let command=self.config.command.as_deref().ok_or_else(||anyhow!("CLI provider has no command"))?;
        // O caminho achado, com extensão: no Windows `claude` sozinho não
        // abre o `claude.cmd` do npm. Sem caminho nenhum, o agente não está
        // instalado — dizer isso poupa o desenvolvedor de caçar o erro do SO.
        let found=crate::llm::locate(command).ok_or_else(||anyhow::Error::new(Text::new("provider.notInstalled").with("provider",&self.name).with("command",command)))?;
        let (program,lead)=crate::llm::launcher(&found);
        let mut process=Command::new(&program);
        quiet(&mut process);
        if let Some(path)=crate::llm::agent_path(&found) { process.env("PATH",path); }
        process.args(lead).args(self.args(model,prompt)).stdin(if self.inline(){Stdio::null()}else{Stdio::piped()}).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        // A pasta do projeto do chat. Sem ela o agente leria o diretório de
        // onde o aplicativo subiu e responderia sobre o repositório errado.
        if let Some(root)=self.workdir.current().filter(|root|root.is_dir()) { process.current_dir(root); }
        process.spawn().map_err(|error|anyhow::Error::new(Text::new("provider.start").with("provider",&self.name).with("path",found.display().to_string()).with("reason",error.to_string())))
    }

    /// Quanto tempo o agente pode ficar calado. O `timeout` do provedor não é o
    /// prazo da resposta inteira: um pedido detalhado leva minutos de trabalho
    /// honesto, e derrubá-lo no meio era o que devolvia `CLI provider timed out`
    /// para quem escrevia demais. É o prazo entre um sinal de vida e o seguinte
    /// — uma linha na saída, uma linha no erro, o processo que termina. Agente
    /// que trabalha nunca estoura; agente travado estoura na mesma hora.
    fn silence(&self)->Duration { Duration::from_secs(self.config.timeout.max(1)) }

    /// Por que o agente falhou, com a melhor pista disponível: o que ele
    /// anunciou como erro, o que escreveu no canal de erro, o fim do que
    /// escreveu na saída — nessa ordem. Nenhuma das três é garantida, então o
    /// código de saída vai sempre junto.
    fn failure(&self,status:Option<std::process::ExitStatus>,refused:Option<String>,complaint:&str,response:&str)->anyhow::Error {
        let exit=match status.and_then(|status|status.code()) { Some(code)=>Text::new("provider.exit.code").with("code",code), None if status.is_some()=>Text::new("provider.exit.signal"), None=>Text::new("provider.exit.announced") };
        let reason=refused.filter(|reason|!reason.trim().is_empty())
            .or_else(||Some(tail(complaint,800).to_string()).filter(|text|!text.is_empty()))
            .or_else(||Some(tail(response,800).to_string()).filter(|text|!text.is_empty()));
        match reason {
            Some(reason)=>Text::new("provider.failed").with("provider",&self.name).with("exit",exit).with("reason",reason).into(),
            None=>Text::new("provider.failedSilent").with("provider",&self.name).with("exit",exit).with("command",self.config.command.as_deref().unwrap_or(&self.name)).into(),
        }
    }

    fn muteness(&self)->anyhow::Error {
        Text::new("provider.silent").with("provider",&self.name).with("seconds",self.silence().as_secs()).into()
    }
}

#[async_trait]
impl Provider for CliProvider {
    fn name(&self)->&str { &self.name }
    fn is_local(&self)->bool { self.config.local.unwrap_or(false) }
    /// A conversa sem ninguém acompanhando é a mesma conversa. Ler a saída de
    /// duas maneiras diferentes era o que fazia o título do chat vir em JSON
    /// cru: o agente que narra em eventos narra igual nas duas chamadas, e só
    /// quem lê linha a linha sabe separar a fala dele do relato do trabalho.
    async fn chat(&self,messages:&[ChatMessage],model:&str)->Result<ProviderResponse> {
        self.chat_stream(messages,model,&Pulse::silent()).await
    }

    /// A mesma conversa, acompanhada linha a linha. As duas pontas são lidas ao
    /// mesmo tempo de propósito: um agente que escreve muito no canal de erro
    /// enche o cano e para de trabalhar se ninguém estiver lendo dos dois lados.
    /// O prazo é de cada linha, não do conjunto — ver `silence`.
    async fn chat_stream(&self,messages:&[ChatMessage],model:&str,pulse:&Pulse)->Result<ProviderResponse> {
        let prompt=Self::prompt(messages);
        let started=Instant::now();
        let mut child=self.open(model,&prompt)?;
        if let Some(mut stdin)=child.stdin.take() { stdin.write_all(prompt.as_bytes()).await?; }
        let mut talk=BufReader::new(child.stdout.take().ok_or_else(||anyhow!("CLI provider gave no output channel"))?).lines();
        let mut grumble=BufReader::new(child.stderr.take().ok_or_else(||anyhow!("CLI provider gave no error channel"))?).lines();
        let (mut response,mut complaint,mut refused)=(String::new(),String::new(),None::<String>);
        let (mut talking,mut grumbling)=(true,true);
        let silence=self.silence();
        while talking||grumbling {
            // As duas leituras podem ser largadas pelo relógio no meio do
            // caminho: `next_line` guarda a linha pela metade e a devolve
            // inteira na volta, então um sinal de vida nunca se perde aqui.
            let heard=timeout(silence,async {
                tokio::select! {
                    line=talk.next_line(),if talking=>match line.context("CLI provider returned non-UTF-8 output")? {
                        None=>talking=false,
                        Some(line)=>if let Some(reason)=refusal(&line) { refused=Some(reason); } else if let Some(beat)=classify(&line) {
                            if let Beat::Chunk{text}=&beat { response.push_str(text); }
                            pulse.beat(beat);
                        },
                    },
                    line=grumble.next_line(),if grumbling=>match line.context("CLI provider returned non-UTF-8 output")? {
                        None=>grumbling=false,
                        Some(line)=>if !line.trim().is_empty() {
                            complaint.push_str(&line);
                            complaint.push('\n');
                            pulse.beat(Beat::Agent{line});
                        },
                    },
                }
                Ok::<(),anyhow::Error>(())
            }).await;
            match heard { Ok(read)=>read?, Err(_)=>{ let _=child.start_kill(); return Err(self.muteness()); } }
        }
        // As duas pontas fecharam: o que falta é o agente sair. Sem prazo aqui,
        // um processo que fechou a saída e não morreu seguraria o pedido para
        // sempre — e é por isso que este prazo não é o de trabalhar, é o de sair.
        let status=timeout(silence,child.wait()).await.map_err(|_|self.muteness())??;
        // Um `result` com `is_error` é falha mesmo quando o processo sai com
        // zero: a resposta que veio antes dele é o texto do erro, não a do
        // pedido.
        if !status.success()||refused.is_some() { return Err(self.failure(Some(status),refused,&complaint,&response)); }
        Ok(ProviderResponse{response,input_tokens:prompt.chars().count()/4,output_tokens:0,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis()})
    }
}
fn provider_error(body:&Value)->String { body.pointer("/error/message").and_then(Value::as_str).or_else(||body.pointer("/error/type").and_then(Value::as_str)).unwrap_or("unknown error").to_string() }
fn request_error(provider:&str,error:reqwest::Error)->anyhow::Error {
    if error.is_timeout() {
        Text::new("provider.timeout").with("provider",provider).into()
    } else if error.is_connect() {
        Text::new("provider.connect").with("provider",provider).into()
    } else {
        Text::new("provider.send").with("provider",provider).with("reason",error.without_url().to_string()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn config(kind:&str)->ProviderConfig { ProviderConfig{enabled:true,kind:kind.into(),timeout:30,..Default::default()} }
    fn cli(local:Option<bool>)->CliProvider { CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("ollama".into()),local,..config("cli")},workdir:Workdir::default()} }
    fn http(base:&str,local:Option<bool>)->HttpProvider { HttpProvider::compatible("http".into(),ProviderConfig{base_url:Some(base.into()),local,..config("openai-compatible")}) }

    #[test] fn cli_provider_is_remote_unless_declared_local() { assert!(!cli(None).is_local()); assert!(!cli(Some(false)).is_local()); }
    #[test] fn cli_provider_honours_explicit_local_flag() { assert!(cli(Some(true)).is_local()); }
    #[test] fn loopback_base_urls_are_local() { for base in ["http://127.0.0.1:1234/v1","http://127.0.0.2:8080","http://localhost:8080/v1","http://[::1]:9000/v1"] { assert!(http(base,None).is_local(),"{base}"); } }
    #[test] fn remote_base_urls_are_not_local() { for base in ["https://api.openai.com/v1","https://localhost.evil.com/v1","https://api.example.com/127.0.0.1/v1","https://evil.com/?h=localhost"] { assert!(!http(base,None).is_local(),"{base}"); } }
    #[test] fn http_provider_honours_explicit_local_flag() { assert!(http("https://api.openai.com/v1",Some(true)).is_local()); assert!(!http("http://127.0.0.1:1234/v1",Some(false)).is_local()); }
    #[test] fn retries_transient_statuses() { for status in [429u16,500,502,503,504,529] { assert!(is_retryable_status(status),"{status}"); } }
    #[test] fn does_not_retry_client_errors() { for status in [400u16,401,403,404,422] { assert!(!is_retryable_status(status),"{status}"); } }
    #[test] fn reads_retry_after_header() { let mut headers=HeaderMap::new(); headers.insert(RETRY_AFTER,"2".parse().expect("header")); assert_eq!(retry_after(&headers),Some(Duration::from_secs(2))); assert_eq!(retry_after(&HeaderMap::new()),None); }
    #[test] fn backoff_grows_with_jitter_and_stays_bounded() { let policy=RetryPolicy::default(); assert!(policy.delay(1,None)<=policy.base_delay); assert!(policy.delay(2,None)>=policy.base_delay/2); assert!(policy.delay(20,None)<=policy.max_delay); assert_eq!(policy.delay(1,Some(Duration::from_secs(600))),policy.max_delay); }
    /// Um agente de linha de comando lê o repositório em que ele foi aberto.
    /// Rodá-lo na pasta de onde o aplicativo subiu faria o chat de um projeto
    /// receber respostas sobre outro repositório.
    #[tokio::test] async fn o_agente_de_linha_de_comando_roda_na_pasta_do_projeto() {
        let projeto=tempfile::tempdir().expect("pasta do projeto");
        let esperado=projeto.path().canonicalize().expect("caminho real");
        let provider=CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("pwd".into()),..config("cli")},workdir:Workdir::default()};
        provider.workdir.focus(esperado.clone());

        let answer=provider.chat(&[ChatMessage{role:"user".into(),content:"onde estou?".into()}],"modelo").await.expect("pwd");

        assert_eq!(answer.response.trim(),esperado.to_string_lossy(),"o agente foi aberto na pasta do projeto do chat");
    }

    fn script(body:&str)->CliProvider {
        CliProvider{name:"agente".into(),config:ProviderConfig{command:Some("sh".into()),args:vec!["-c".into(),body.into()],..config("cli")},workdir:Workdir::default()}
    }
    fn ask()->Vec<ChatMessage> { vec![ChatMessage{role:"user".into(),content:"oi".into()}] }

    /// O Claude em `stream-json` explica a falha na saída, não no canal de
    /// erro. É esse texto que tem de chegar à tela.
    #[tokio::test] async fn a_falha_anunciada_na_saida_chega_com_o_motivo() {
        let provider=script(r#"echo '{"type":"result","subtype":"success","is_error":true,"result":"There is an issue with the selected model"}'; exit 1"#);
        let error=provider.chat(&ask(),"modelo").await.expect_err("falhou").to_string();
        assert!(error.contains("There is an issue with the selected model"),"{error}");
        assert!(error.contains("provider.failed") && error.contains("code: 1"),"{error}");
    }

    #[tokio::test] async fn a_falha_anunciada_vale_mesmo_com_saida_zero() {
        let provider=script(r#"echo '{"type":"result","is_error":true,"result":"cota esgotada"}'"#);
        let error=provider.chat(&ask(),"modelo").await.expect_err("falhou").to_string();
        assert!(error.contains("cota esgotada"),"{error}");
    }

    #[tokio::test] async fn sem_canal_de_erro_a_saida_explica_a_falha() {
        let error=script("echo 'login expirado'; exit 2").chat(&ask(),"modelo").await.expect_err("falhou").to_string();
        assert!(error.contains("login expirado")&&error.contains("code: 2"),"{error}");
    }

    #[tokio::test] async fn a_falha_muda_diz_o_codigo_e_o_que_fazer() {
        let error=script("exit 3").chat(&ask(),"modelo").await.expect_err("falhou").to_string();
        assert!(error.contains("provider.failedSilent")&&error.contains("code: 3")&&error.contains("command: sh"),"{error}");
    }

    /// O CLI do npm é um script `#!/usr/bin/env node`, e o Node do nvm mora na
    /// mesma pasta do script — fora do PATH de um app aberto pelo menu. O agente
    /// tem de abrir mesmo assim.
    #[cfg(unix)]
    #[tokio::test] async fn o_script_do_npm_acha_o_interpretador_ao_lado_dele() {
        use std::os::unix::fs::PermissionsExt;
        let bin=tempfile::tempdir().expect("bin");
        let runnable=|path:&std::path::Path,body:&str|{std::fs::write(path,body).expect("script");std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o755)).expect("chmod");};
        runnable(&bin.path().join("jayv-fake-node"),"#!/bin/sh\nshift $#\necho resposta do agente\n");
        runnable(&bin.path().join("agente"),"#!/usr/bin/env jayv-fake-node\n");
        let provider=CliProvider{name:"agente".into(),config:ProviderConfig{command:Some(bin.path().join("agente").display().to_string()),..config("cli")},workdir:Workdir::default()};
        let answer=provider.chat(&ask(),"modelo").await.expect("o agente abre com o interpretador da pasta dele");
        assert!(answer.response.contains("resposta do agente"),"{}",answer.response);
    }

    #[tokio::test] async fn o_agente_que_nao_esta_instalado_diz_isso() {
        let provider=CliProvider{name:"copilot".into(),config:ProviderConfig{command:Some("jayv-agente-que-nao-existe".into()),..config("cli")},workdir:Workdir::default()};
        let error=provider.chat(&ask(),"modelo").await.expect_err("não instalado");
        assert_eq!(crate::i18n::Text::from(error).key,"provider.notInstalled");
    }

    #[tokio::test] async fn gives_up_after_the_attempt_budget() {
        let calls=Cell::new(0);
        let policy=RetryPolicy{attempts:3,base_delay:Duration::from_millis(1),max_delay:Duration::from_millis(2)};
        let result:Result<()>=with_retry(policy,|_|{calls.set(calls.get()+1); async {Err(RetryError::retryable(anyhow!("529 overloaded"),None))}}).await;
        assert!(result.is_err()); assert_eq!(calls.get(),3);
    }
    /// Um pedaço da rede corta onde quiser, inclusive no meio de um evento.
    /// Ler meio JSON é ler nada — a metade tem de esperar pela outra.
    #[test] fn a_metade_de_um_evento_espera_pela_outra() {
        let mut balde=b"data: {\"a\":1}\ndata: {\"b\"".to_vec();
        assert_eq!(ready_lines(&mut balde),vec!["data: {\"a\":1}".to_string()]);
        assert_eq!(ready_lines(&mut balde),Vec::<String>::new(),"o que sobrou nao virou linha");
        balde.extend_from_slice(b":2}\n");
        assert_eq!(ready_lines(&mut balde),vec!["data: {\"b\":2}".to_string()]);
        assert!(balde.is_empty(),"o balde esvazia quando a linha fecha");
    }

    #[test] fn o_fluxo_da_openai_rende_texto_e_a_conta_dos_tokens() {
        let mut colheita=Harvest::default();
        assert_eq!(read_event(&HttpKind::OpenAi,"data: {\"choices\":[{\"delta\":{\"content\":\"oi\"}}]}",&mut colheita),Some("oi".into()));
        assert_eq!(read_event(&HttpKind::OpenAi,"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":7}}",&mut colheita),None);
        assert_eq!((colheita.input_tokens,colheita.output_tokens),(12,7));
    }

    #[test] fn o_fluxo_da_anthropic_rende_texto_e_a_conta_dos_tokens() {
        let mut colheita=Harvest::default();
        assert_eq!(read_event(&HttpKind::Anthropic,"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9}}}",&mut colheita),None);
        assert_eq!(read_event(&HttpKind::Anthropic,"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ola\"}}",&mut colheita),Some("ola".into()));
        assert_eq!(read_event(&HttpKind::Anthropic,"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":4}}",&mut colheita),None);
        assert_eq!((colheita.input_tokens,colheita.output_tokens),(9,4));
    }

    /// Comentário, batida de coração e despedida não são resposta. Tratá-los
    /// como texto encheria a resposta de ruído do protocolo.
    #[test] fn o_que_nao_e_evento_nao_acrescenta_nada() {
        let mut colheita=Harvest::default();
        for line in ["",": keep-alive","event: message_stop","data: [DONE]","data: nao e json","id: 7"] {
            assert_eq!(read_event(&HttpKind::OpenAi,line,&mut colheita),None,"{line}");
            assert_eq!(read_event(&HttpKind::Anthropic,line,&mut colheita),None,"{line}");
        }
        assert_eq!(colheita,Harvest::default());
    }

    /// O agente não avisa se a linha é resposta ou relato do trabalho dele, e a
    /// configuração também não: `claude --print` escreve o texto direto e
    /// `codex exec` narra. Quem voltar a decidir pelo provedor erra num dos dois.
    #[test] fn a_linha_do_agente_e_classificada_pelo_conteudo() {
        assert_eq!(classify("a resposta em texto puro"),Some(Beat::Chunk{text:"a resposta em texto puro\n".into()}));
        assert_eq!(classify("{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"oi\"}]}}"),Some(Beat::Chunk{text:"oi\n".into()}));
        assert_eq!(classify("{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Read\"}]}}"),Some(Beat::Agent{line:"assistant: Read".into()}));
        assert_eq!(classify("{\"type\":\"item_completed\",\"command\":\"cargo test\"}"),Some(Beat::Agent{line:"item_completed: cargo test".into()}));
        // Um número solto é JSON válido e não é evento nenhum.
        assert_eq!(classify("42"),Some(Beat::Chunk{text:"42\n".into()}));
    }

    /// A escrituração do agente chega dezenas de vezes por resposta. Ela conta
    /// como sinal de vida — quem lê a linha rearma o relógio do silêncio —, mas
    /// virar linha na tela empurraria a portaria e a rota para fora da vista.
    #[test] fn a_escrituracao_do_agente_nao_vira_etapa() {
        for line in [
            "{\"type\":\"system\",\"subtype\":\"thinking_tokens\",\"estimated_tokens\":42}",
            "{\"type\":\"system\",\"subtype\":\"hook_started\",\"hook_name\":\"SessionStart\"}",
            "{\"type\":\"system\",\"subtype\":\"hook_response\",\"outcome\":\"success\"}",
            "{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"allowed\"}}",
            "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ola \"}}}",
        ] { assert_eq!(classify(line),None,"{line}"); }
        assert_eq!(classify("{\"type\":\"system\",\"subtype\":\"init\",\"cwd\":\"/tmp\"}"),Some(Beat::Agent{line:"system: init".into()}),"o começo da sessão continua sendo etapa");
    }

    /// Em `stream-json` o agente conta o trabalho em eventos e a fala dele vem
    /// dentro de um deles. O que fica gravado tem de ser a fala: lida como texto
    /// cru, a resposta do chat seria o protocolo inteiro.
    #[tokio::test] async fn o_protocolo_do_agente_nao_entra_na_resposta() {
        let stream=[
            "{\"type\":\"system\",\"subtype\":\"init\",\"cwd\":\"/tmp\"}",
            "{\"type\":\"system\",\"subtype\":\"thinking_tokens\",\"estimated_tokens\":7}",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Read\"}]}}",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"ola mundo\"}]}}",
            "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"ola mundo\"}",
        ].join("\n");
        // `cat` primeiro: um agente lê o pedido inteiro antes de responder, e
        // sem isso o teste corre com o fim do processo.
        let roteiro=format!("cat >/dev/null; printf '%s' '{stream}'");
        let provider=CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("sh".into()),args:vec!["-c".into(),roteiro],..config("cli")},workdir:Workdir::default()};
        let (pulse,mut beats)=Pulse::channel();

        let answer=provider.chat_stream(&[ChatMessage{role:"user".into(),content:"oi".into()}],"modelo",&pulse).await.expect("o agente em eventos");

        assert_eq!(answer.response,"ola mundo\n","a resposta e a fala, nao o protocolo");
        drop(pulse);
        let mut etapas=Vec::new();
        while let Some(beat)=beats.recv().await { if let Beat::Agent{line}=beat { etapas.push(line); } }
        assert_eq!(etapas,vec!["system: init".to_string(),"assistant: Read".into(),"result: success".into()],"as etapas sao o trabalho, sem a escrituracao");
    }

    /// O relógio do provedor de linha de comando conta silêncio, não trabalho.
    /// Um agente que fala a cada poucos segundos há dez minutos está vivo; era
    /// o prazo do conjunto que matava o pedido detalhado no meio.
    #[tokio::test] async fn o_agente_que_fala_de_vez_em_quando_nao_estoura_o_prazo() {
        let fala="for i in 1 2 3 4; do echo linha $i; sleep 0.4; done";
        let provider=CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("sh".into()),args:vec!["-c".into(),fala.into()],timeout:1,..config("cli")},workdir:Workdir::default()};

        let answer=provider.chat(&[ChatMessage{role:"user".into(),content:"fale devagar".into()}],"modelo").await.expect("o agente falante");

        assert_eq!(answer.response,"linha 1\nlinha 2\nlinha 3\nlinha 4\n","cada linha rearmou o relogio");
        assert!(answer.latency_ms>=1_000,"a conversa passou do prazo de silencio sem estourar: {}ms",answer.latency_ms);
    }

    /// E o agente que emudece tem de cair — e cair dizendo o que houve, porque
    /// `CLI provider timed out` mandava o desenvolvedor procurar no lugar errado.
    #[tokio::test] async fn o_agente_que_emudece_cai_e_diz_por_que() {
        let provider=CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("sleep".into()),args:vec!["60".into()],timeout:1,..config("cli")},workdir:Workdir::default()};

        let erro=provider.chat(&[ChatMessage{role:"user".into(),content:"fique calado".into()}],"modelo").await.expect_err("o agente mudo");

        let erro=erro.to_string();
        assert!(erro.contains("provider.silent") && erro.contains("seconds: 1"),"o erro diz o que houve e a tela aponta o `timeout`: {erro}");
    }

    /// A conversa acompanhada tem de render a mesma resposta que a esperada, e
    /// os pedaços anunciados têm de somar exatamente ela: um pedaço a mais na
    /// tela é texto duplicado, um a menos é texto que ninguém viu chegar.
    #[tokio::test] async fn o_agente_acompanhado_anuncia_a_mesma_resposta_que_devolve() {
        let provider=CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("cat".into()),args:vec![],..config("cli")},workdir:Workdir::default()};
        let (pulse,mut beats)=Pulse::channel();

        let answer=provider.chat_stream(&[ChatMessage{role:"user".into(),content:"uma linha\noutra linha".into()}],"modelo",&pulse).await.expect("cat");

        drop(pulse);
        let mut anunciado=String::new();
        while let Some(beat)=beats.recv().await { if let Beat::Chunk{text}=beat { anunciado.push_str(&text); } }
        assert_eq!(anunciado,answer.response,"o que a tela viu e o que ficou gravado sao o mesmo texto");
        assert!(answer.response.contains("uma linha"),"a resposta chegou: {:?}",answer.response);
    }

    #[tokio::test] async fn stops_immediately_on_fatal_errors() {
        let calls=Cell::new(0);
        let result:Result<()>=with_retry(RetryPolicy::default(),|_|{calls.set(calls.get()+1); async {Err(RetryError::from_status(StatusCode::UNAUTHORIZED,None,anyhow!("401")))}}).await;
        assert!(result.is_err()); assert_eq!(calls.get(),1);
    }

    #[test] fn o_pedido_vai_no_argumento_quando_o_agente_pede() {
        let provider=CliProvider{name:"copilot".into(),config:ProviderConfig{command:Some("copilot".into()),args:vec!["-p".into(),"{prompt}".into(),"--model".into(),"{model}".into()],..config("cli")},workdir:Workdir::default()};
        assert!(provider.inline());
        assert_eq!(provider.args("gpt-5","oi {model}"),["-p","oi {model}","--model","gpt-5"]);
    }

    #[test] fn a_reserva_igual_ao_modelo_sai_da_linha() {
        let provider=CliProvider{name:"claude".into(),config:ProviderConfig{command:Some("claude".into()),args:vec!["--model".into(),"{model}".into(),"--fallback-model".into(),"haiku".into()],..config("cli")},workdir:Workdir::default()};
        assert!(!provider.inline());
        assert_eq!(provider.args("haiku",""),["--model","haiku"]);
        assert_eq!(provider.args("opus",""),["--model","opus","--fallback-model","haiku"]);
    }
}
