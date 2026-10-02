//! A leitura do limite do plano de cada agente.
//!
//! O Claude responde ao `/usage` no `--print` sem chamar o modelo; o Codex
//! deixa a leitura nos arquivos de sessão. O `/usage` do Cursor e o do Copilot
//! só existem no modo interativo, então a leitura vem de onde eles tiram os
//! números: o Cursor pergunta ao painel da conta com o login que a CLI
//! guardou, e o Copilot guarda a resposta da conta num cache em disco. Toda
//! leitura é da conta inteira — inclui o uso fora do JayV — e a que falha diz
//! por quê, sem número inventado.

use super::{codex, Quota};
use crate::i18n::Text;
use chrono::{DateTime, SecondsFormat};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::{LazyLock, Mutex}, time::{Duration, Instant}};

/// O menor intervalo entre duas leituras do mesmo agente pedidas pelo fluxo.
pub const MIN_INTERVAL:Duration=Duration::from_secs(120);
const TIMEOUT:Duration=Duration::from_secs(30);

pub enum Reading { Read(Vec<Quota>), Unavailable(Text) }

static LAST:LazyLock<Mutex<HashMap<String,Instant>>>=LazyLock::new(Default::default);

/// Se já passou tempo bastante desde a última leitura deste agente — e, se
/// passou, marca esta como a última.
fn due(last:&mut HashMap<String,Instant>,agent:&str,now:Instant,min:Duration)->bool {
    if last.get(agent).is_some_and(|at|now.duration_since(*at)<min) { return false; }
    last.insert(agent.to_string(),now);
    true
}

/// Lê o limite de um agente, agora.
pub async fn read(agent:&str,command:&str)->Reading {
    match agent {
        "claude"=>read_claude(command).await,
        "codex"=>{
            let found=codex::sessions_dir().map(|dir|codex::latest_session_quotas(&dir)).unwrap_or_default();
            if found.is_empty() { Reading::Unavailable(Text::new("usage.quota.noData").with("agent","Codex")) } else { Reading::Read(found) }
        }
        "cursor"=>read_cursor().await,
        _=>match copilot_cache().and_then(|path|std::fs::read_to_string(path).ok()) {
            None=>Reading::Unavailable(Text::new("usage.quota.noData").with("agent","Copilot")),
            Some(text)=>match copilot_quotas(&text) {
                Some(found) if !found.is_empty()=>Reading::Read(found),
                Some(_)=>Reading::Unavailable(Text::new("usage.quota.noData").with("agent","Copilot")),
                None=>Reading::Unavailable(Text::new("usage.quota.unreadable").with("agent","Copilot")),
            },
        },
    }
}

/// O painel da conta do Cursor, o mesmo que o `/usage` da CLI consulta.
const CURSOR_DASHBOARD:&str="https://api2.cursor.sh/aiserver.v1.DashboardService";

/// Onde o Cursor CLI guarda o login.
fn cursor_auth()->Option<PathBuf> {
    if cfg!(target_os="macos") { return dirs::home_dir().map(|home|home.join(".cursor").join("auth.json")); }
    dirs::config_dir().map(|dir|dir.join(if cfg!(windows) {"Cursor"} else {"cursor"}).join("auth.json"))
}

async fn read_cursor()->Reading {
    let token=cursor_auth().and_then(|path|std::fs::read_to_string(path).ok())
        .and_then(|text|serde_json::from_str::<Value>(&text).ok())
        .and_then(|auth|auth.get("accessToken").and_then(Value::as_str).map(str::to_string))
        .filter(|token|!token.is_empty());
    let Some(token)=token else { return Reading::Unavailable(Text::new("usage.quota.signedOut").with("agent","Cursor")) };
    let Ok(client)=reqwest::Client::builder().timeout(TIMEOUT).build() else { return Reading::Unavailable(Text::new("usage.quota.silent").with("agent","Cursor")) };
    let call=|method:&'static str|{
        let request=client.post(format!("{CURSOR_DASHBOARD}/{method}")).bearer_auth(&token).header("Connect-Protocol-Version","1").json(&json!({}));
        async move { request.send().await.ok()?.error_for_status().ok()?.json::<Value>().await.ok() }
    };
    let (usage,plan)=tokio::join!(call("GetCurrentPeriodUsage"),call("GetPlanInfo"));
    let Some(usage)=usage else { return Reading::Unavailable(Text::new("usage.quota.silent").with("agent","Cursor")) };
    match cursor_quotas(&usage,plan.as_ref()) {
        Some(found)=>Reading::Read(found),
        None=>Reading::Unavailable(Text::new("usage.quota.unreadable").with("agent","Cursor")),
    }
}

/// O uso incluído no plano do Cursor: o total e as duas partes que o
/// `/usage` mostra — os modelos do Auto e os escolhidos pelo nome.
pub fn cursor_quotas(usage:&Value,plan:Option<&Value>)->Option<Vec<Quota>> {
    let used=usage.get("planUsage")?;
    let number=|value:&Value|value.as_f64().or_else(||value.as_str().and_then(|text|text.parse().ok()));
    let end=usage.get("billingCycleEnd").and_then(number).filter(|end|*end>0.0)
        .or_else(||plan.and_then(|plan|plan.pointer("/planInfo/billingCycleEnd")).and_then(number));
    let resets=end.and_then(|end|DateTime::from_timestamp_millis(end as i64)).map(|at|at.to_rfc3339_opts(SecondsFormat::Secs,true));
    let plan=plan.and_then(|plan|plan.pointer("/planInfo/planName")).and_then(Value::as_str).map(str::to_string);
    let total=used.get("totalPercentUsed").and_then(number).or_else(||{
        let (spent,limit)=(used.get("includedSpend").and_then(number)?,used.get("limit").and_then(number)?);
        (limit>0.0).then(||spent/limit*100.0)
    }).unwrap_or(0.0);
    let quota=|window:&str,used_percent:f64|Quota{agent:"cursor".into(),window:window.into(),used_percent:Some(used_percent),resets_at:resets.clone(),plan:plan.clone()};
    Some(vec![
        quota("month",total),
        quota("auto",used.get("autoPercentUsed").and_then(number).unwrap_or(0.0)),
        quota("api",used.get("apiPercentUsed").and_then(number).unwrap_or(0.0)),
    ])
}

/// O cache em que o Copilot CLI guarda a resposta da conta.
fn copilot_cache()->Option<PathBuf> {
    let file="copilot-user-cache.json";
    [dirs::cache_dir(),dirs::home_dir().map(|home|home.join(".cache"))].into_iter().flatten()
        .map(|dir|dir.join("copilot").join(file)).find(|path|path.is_file())
}

/// As cotas da leitura mais recente do cache do Copilot. O arquivo começa com
/// comentários `//`; as cotas ilimitadas ou que o plano não tem ficam de fora.
pub fn copilot_quotas(text:&str)->Option<Vec<Quota>> {
    let json=text.lines().filter(|line|!line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let cache:Value=serde_json::from_str(&json).ok()?;
    let entries=cache.get("copilotUserCache")?.as_object()?;
    let latest=entries.values().filter(|entry|entry.get("response").is_some())
        .max_by(|left,right|left.get("retrievedAt").and_then(Value::as_str).unwrap_or("").cmp(right.get("retrievedAt").and_then(Value::as_str).unwrap_or("")))?;
    let response=&latest["response"];
    let plan=response.get("copilot_plan").and_then(Value::as_str).map(str::to_string);
    let resets=response.get("quota_reset_date_utc").or_else(||response.get("quota_reset_date")).and_then(Value::as_str).map(str::to_string);
    let mut found=Vec::new();
    for window in ["premium_interactions","chat","completions"] {
        let Some(snapshot)=response.pointer(&format!("/quota_snapshots/{window}")) else { continue };
        let limited=snapshot.get("unlimited").and_then(Value::as_bool)==Some(false);
        let has=snapshot.get("has_quota").and_then(Value::as_bool).unwrap_or(true);
        let entitled=snapshot.get("entitlement").and_then(Value::as_f64).is_some_and(|entitlement|entitlement>0.0);
        let Some(remaining)=snapshot.get("percent_remaining").and_then(Value::as_f64) else { continue };
        if limited&&has&&entitled {
            let window=if window=="premium_interactions" {"premium"} else {window};
            found.push(Quota{agent:"copilot".into(),window:window.into(),used_percent:Some((100.0-remaining).clamp(0.0,100.0)),resets_at:resets.clone(),plan:plan.clone()});
        }
    }
    Some(found)
}

async fn read_claude(command:&str)->Reading {
    let Some(path)=crate::llm::locate(command) else { return Reading::Unavailable(Text::new("usage.quota.notInstalled").with("agent","Claude Code")) };
    let (program,lead)=crate::llm::launcher(&path);
    let mut process=tokio::process::Command::new(&program);
    if let Some(search)=crate::llm::agent_path(&path) { process.env("PATH",search); }
    let run=crate::providers::quiet(&mut process).args(lead).args(["--print","/usage","--output-format","json","--no-session-persistence"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).output();
    let output=match tokio::time::timeout(TIMEOUT,run).await {
        Ok(Ok(output))=>output,
        _=>return Reading::Unavailable(Text::new("usage.quota.silent").with("agent","Claude Code")),
    };
    match super::claude::parse_usage_text(&String::from_utf8_lossy(&output.stdout)) {
        Some(found)=>Reading::Read(found),
        None=>Reading::Unavailable(Text::new("usage.quota.unreadable").with("agent","Claude Code")),
    }
}

/// Lê e grava, se o fluxo pediu e o intervalo mínimo já passou. Roda em
/// segundo plano: o pedido que disparou a leitura não espera por ela.
pub fn refresh(agent:&str,command:&str) {
    if !super::is_installed()||!matches!(agent,"claude"|"codex"|"cursor"|"copilot") { return; }
    let allowed=LAST.lock().map(|mut last|due(&mut last,agent,Instant::now(),MIN_INTERVAL)).unwrap_or(false);
    if !allowed { return; }
    let (agent,command)=(agent.to_string(),command.to_string());
    tokio::spawn(async move { if let Reading::Read(found)=read(&agent,&command).await { for quota in found { super::quota(quota); } } });
}

/// O estado de um agente depois de uma leitura pedida pela tela.
#[derive(Debug,Clone,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct QuotaStatus { pub agent:String, pub problem:Option<Text> }

/// Lê todos os agentes, sem intervalo mínimo: quem pediu foi a tela.
pub async fn read_all(agents:&[(String,String)])->Vec<QuotaStatus> {
    let mut statuses=Vec::new();
    for (agent,command) in agents {
        let problem=match read(agent,command).await {
            Reading::Read(found)=>{ for quota in found { super::quota(quota); } None }
            Reading::Unavailable(reason)=>Some(reason),
        };
        if let Ok(mut last)=LAST.lock() { last.insert(agent.clone(),Instant::now()); }
        statuses.push(QuotaStatus{agent:agent.clone(),problem});
    }
    statuses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn the_minimum_interval_holds_per_agent() {
        let mut last=HashMap::new();
        let now=Instant::now();
        assert!(due(&mut last,"claude",now,MIN_INTERVAL));
        assert!(!due(&mut last,"claude",now+Duration::from_secs(5),MIN_INTERVAL));
        assert!(due(&mut last,"codex",now,MIN_INTERVAL),"cada agente tem o seu relógio");
        assert!(due(&mut last,"claude",now+MIN_INTERVAL,MIN_INTERVAL));
    }

    #[tokio::test] async fn a_missing_claude_says_why() {
        let Reading::Unavailable(reason)=read("claude","/nenhum/caminho/claude-inexistente").await else { panic!("leu um CLI que não existe") };
        assert_eq!(reason.key,"usage.quota.notInstalled");
    }

    #[test] fn the_cursor_dashboard_becomes_three_bars() {
        let usage=json!({"billingCycleEnd":"1793631008332","planUsage":{"autoPercentUsed":4,"apiPercentUsed":0,"totalPercentUsed":2}});
        let plan=json!({"planInfo":{"planName":"Pro","billingCycleEnd":"1793631008332"}});
        let found=cursor_quotas(&usage,Some(&plan)).expect("leitura");
        assert_eq!(found.iter().map(|quota|(quota.window.as_str(),quota.used_percent)).collect::<Vec<_>>(),[("month",Some(2.0)),("auto",Some(4.0)),("api",Some(0.0))]);
        assert_eq!(found[0].plan.as_deref(),Some("Pro"));
        assert!(found[0].resets_at.as_deref().is_some_and(|at|at=="2026-11-02T14:50:08Z"));
        assert!(cursor_quotas(&json!({"error":"unauthenticated"}),None).is_none(),"sem planUsage não há número");
    }

    #[test] fn the_copilot_cache_gives_the_latest_limited_quotas() {
        let text=r#"// Disposable cache for Copilot user responses.
{"copilotUserCache":{
  "v1:a":{"retrievedAt":"2026-10-02T10:00:00.000Z","response":{"copilot_plan":"individual","quota_snapshots":{"chat":{"entitlement":200,"percent_remaining":50,"unlimited":false,"has_quota":true}}}},
  "v1:b":{"retrievedAt":"2026-10-02T16:00:00.000Z","response":{"copilot_plan":"individual","quota_reset_date_utc":"2026-11-01T00:00:00.000Z","quota_snapshots":{
    "chat":{"entitlement":200,"percent_remaining":99.5,"unlimited":false,"has_quota":true},
    "completions":{"entitlement":0,"percent_remaining":100,"unlimited":true,"has_quota":true},
    "premium_interactions":{"entitlement":300,"percent_remaining":75,"unlimited":false,"has_quota":true}}}}}}"#;
        let found=copilot_quotas(text).expect("leitura");
        assert_eq!(found.iter().map(|quota|(quota.window.as_str(),quota.used_percent.map(|used|used.round()))).collect::<Vec<_>>(),[("premium",Some(25.0)),("chat",Some(1.0))]);
        assert_eq!(found[0].plan.as_deref(),Some("individual"));
        assert_eq!(found[0].resets_at.as_deref(),Some("2026-11-01T00:00:00.000Z"));
        assert!(copilot_quotas("não é json").is_none());
    }
}
