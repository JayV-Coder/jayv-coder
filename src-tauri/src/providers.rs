use crate::{config::ProviderConfig, model::{ChatMessage, ProviderResponse}};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::HashMap, future::Future, net::IpAddr, process::Stdio, time::{Instant, SystemTime, UNIX_EPOCH}};
use reqwest::{header::{HeaderMap, RETRY_AFTER}, StatusCode};
use tokio::{io::AsyncWriteExt, process::Command, time::{sleep, timeout, Duration}};

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn is_local(&self) -> bool { false }
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> Result<ProviderResponse>;
}

pub fn build_providers(configs: &HashMap<String, ProviderConfig>) -> HashMap<String, Box<dyn Provider>> {
    configs.iter().filter(|(_,config)|config.is_executable()).filter_map(|(name, config)| {
        let provider: Box<dyn Provider> = match config.kind.as_str() {
            "openai" => Box::new(HttpProvider::openai(name.clone(), config.clone())),
            "anthropic" => Box::new(HttpProvider::anthropic(name.clone(), config.clone())),
            "openai-compatible" => Box::new(HttpProvider::compatible(name.clone(), config.clone())),
            "cli" => Box::new(CliProvider { name:name.clone(), config:config.clone() }),
            _ => return None,
        }; Some((name.clone(),provider))
    }).collect()
}

pub async fn discover_models(name: &str, config: &ProviderConfig) -> Result<Vec<String>> {
    if config.kind=="cli" { return Err(anyhow!("provedores CLI não oferecem descoberta automática de modelos")); }
    let default_base=match config.kind.as_str() {
        "openai"=>"https://api.openai.com/v1",
        "anthropic"=>"https://api.anthropic.com/v1",
        "openai-compatible"=>config.base_url.as_deref().filter(|value|!value.trim().is_empty()).ok_or_else(||anyhow!("informe a URL base do provedor"))?,
        _=>return Err(anyhow!("tipo de provedor não suportado")),
    };
    let client=reqwest::Client::builder().timeout(Duration::from_secs(config.timeout.max(5))).build()?;
    let mut request=client.get(format!("{}/models",default_base.trim_end_matches('/')));
    if config.kind=="anthropic" {
        let key=config.api_key.as_deref().filter(|key|!key.trim().is_empty()).ok_or_else(||anyhow!("informe a API key para carregar os modelos"))?;
        request=request.header("x-api-key",key).header("anthropic-version","2023-06-01");
    } else if let Some(key)=config.api_key.as_deref().filter(|key|!key.trim().is_empty()) {
        request=request.bearer_auth(key);
    } else if config.kind=="openai" {
        return Err(anyhow!("informe a API key para carregar os modelos"));
    }
    let response=request.send().await.map_err(|error|request_error(name,error))?;
    let status=response.status();
    let body:Value=response.json().await.context("resposta inválida ao carregar modelos")?;
    if !status.is_success(){return Err(anyhow!("o provider retornou {status}: {}",provider_error(&body)));}
    let mut models=body.get("data").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|item|item.get("id").and_then(Value::as_str))
        .map(str::to_string).collect::<Vec<_>>();
    models.sort();
    models.dedup();
    if models.is_empty(){return Err(anyhow!("o provider não retornou modelos disponíveis"));}
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

enum HttpKind { OpenAi, Anthropic }
struct HttpProvider { name:String, config:ProviderConfig, kind:HttpKind, client:reqwest::Client }
impl HttpProvider {
    fn openai(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::OpenAi) }
    fn anthropic(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::Anthropic) }
    fn compatible(name:String,config:ProviderConfig)->Self { Self::new(name,config,HttpKind::OpenAi) }
    fn new(name:String,config:ProviderConfig,kind:HttpKind)->Self { let client=reqwest::Client::builder().timeout(Duration::from_secs(config.timeout)).build().expect("HTTP client"); Self{name,config,kind,client} }
    async fn send(&self,url:&str,payload:&Value)->Result<Value> {
        let anthropic=matches!(self.kind,HttpKind::Anthropic);
        with_retry(RetryPolicy::default(),move |_attempt| async move {
            let mut request=self.client.post(url).json(payload);
            if anthropic { request=request.header("anthropic-version","2023-06-01"); }
            if let Some(key)=self.config.api_key.as_deref().filter(|k|!k.is_empty()) { request=if anthropic{request.header("x-api-key",key)}else{request.bearer_auth(key)}; }
            let response=request.send().await.map_err(|error|RetryError::from_transport(&self.name,error))?;
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
        match self.kind {
            HttpKind::OpenAi => {
                let base=self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1").trim_end_matches('/');
                let body=self.send(&format!("{base}/chat/completions"),&json!({"model":model,"messages":messages,"temperature":0.2})).await?;
                let text=body.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or_default().to_string();
                Ok(ProviderResponse{response:text,input_tokens:body.pointer("/usage/prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,output_tokens:body.pointer("/usage/completion_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis()})
            }
            HttpKind::Anthropic => {
                let base=self.config.base_url.as_deref().unwrap_or("https://api.anthropic.com/v1").trim_end_matches('/');
                let system=messages.iter().find(|m|m.role=="system").map(|m|m.content.clone()).unwrap_or_default();
                let chat=messages.iter().filter(|m|m.role!="system").collect::<Vec<_>>();
                let body=self.send(&format!("{base}/messages"),&json!({"model":model,"max_tokens":4096,"system":system,"messages":chat})).await?;
                let text=body.pointer("/content/0/text").and_then(Value::as_str).unwrap_or_default().to_string();
                Ok(ProviderResponse{response:text,input_tokens:body.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,output_tokens:body.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0) as usize,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis()})
            }
        }
    }
}

struct CliProvider { name:String, config:ProviderConfig }
#[async_trait]
impl Provider for CliProvider {
    fn name(&self)->&str { &self.name }
    fn is_local(&self)->bool { self.config.local.unwrap_or(false) }
    async fn chat(&self,messages:&[ChatMessage],model:&str)->Result<ProviderResponse> {
        let command=self.config.command.as_deref().ok_or_else(||anyhow!("CLI provider has no command"))?;
        let prompt=messages.iter().map(|m|format!("{}: {}",m.role,m.content)).collect::<Vec<_>>().join("\n\n");
        let args=self.config.args.iter().map(|arg|arg.replace("{model}",model)).collect::<Vec<_>>();
        let started=Instant::now();
        let mut child=Command::new(command).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().with_context(||format!("could not start CLI provider {command}"))?;
        if let Some(mut stdin)=child.stdin.take() { stdin.write_all(prompt.as_bytes()).await?; }
        let output=timeout(Duration::from_secs(self.config.timeout),child.wait_with_output()).await.map_err(|_|anyhow!("CLI provider timed out"))??;
        if !output.status.success() { return Err(anyhow!("CLI provider failed: {}",String::from_utf8_lossy(&output.stderr).trim())); }
        let response=String::from_utf8(output.stdout).context("CLI provider returned non-UTF-8 output")?;
        Ok(ProviderResponse{response,input_tokens:prompt.chars().count()/4,output_tokens:0,model:model.into(),provider:self.name.clone(),latency_ms:started.elapsed().as_millis()})
    }
}
fn provider_error(body:&Value)->String { body.pointer("/error/message").and_then(Value::as_str).or_else(||body.pointer("/error/type").and_then(Value::as_str)).unwrap_or("unknown error").to_string() }
fn request_error(provider:&str,error:reqwest::Error)->anyhow::Error {
    if error.is_timeout() {
        anyhow!("o provider `{provider}` excedeu o tempo limite; verifique o serviço e o valor de `timeout`")
    } else if error.is_connect() {
        anyhow!("não foi possível conectar ao provider `{provider}`; verifique se o serviço está em execução, a conexão de rede e `base_url`")
    } else {
        anyhow!("falha ao enviar a solicitação ao provider `{provider}`: {}",error.without_url())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn config(kind:&str)->ProviderConfig { ProviderConfig{enabled:true,kind:kind.into(),timeout:30,..Default::default()} }
    fn cli(local:Option<bool>)->CliProvider { CliProvider{name:"cli".into(),config:ProviderConfig{command:Some("ollama".into()),local,..config("cli")}} }
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
    #[tokio::test] async fn gives_up_after_the_attempt_budget() {
        let calls=Cell::new(0);
        let policy=RetryPolicy{attempts:3,base_delay:Duration::from_millis(1),max_delay:Duration::from_millis(2)};
        let result:Result<()>=with_retry(policy,|_|{calls.set(calls.get()+1); async {Err(RetryError::retryable(anyhow!("529 overloaded"),None))}}).await;
        assert!(result.is_err()); assert_eq!(calls.get(),3);
    }
    #[tokio::test] async fn stops_immediately_on_fatal_errors() {
        let calls=Cell::new(0);
        let result:Result<()>=with_retry(RetryPolicy::default(),|_|{calls.set(calls.get()+1); async {Err(RetryError::from_status(StatusCode::UNAUTHORIZED,None,anyhow!("401")))}}).await;
        assert!(result.is_err()); assert_eq!(calls.get(),1);
    }
}
