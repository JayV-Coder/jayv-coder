//! A leitura do limite do plano de cada agente.
//!
//! O Claude responde ao `/usage` no `--print` sem chamar o modelo; o Codex
//! deixa a leitura nos arquivos de sessão; o Copilot CLI não a diz fora do
//! modo interativo, e o Cursor CLI também não. Toda leitura é da conta inteira — inclui o uso fora do
//! JayV — e a que falha diz por quê, sem número inventado.

use super::{codex, Quota};
use crate::i18n::Text;
use std::{collections::HashMap, process::Stdio, sync::{LazyLock, Mutex}, time::{Duration, Instant}};

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
        "cursor"=>Reading::Unavailable(Text::new("usage.quota.cursorUnreported")),
        _=>Reading::Unavailable(Text::new("usage.quota.copilotUnreported")),
    }
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
    if !super::is_installed()||!matches!(agent,"claude"|"codex") { return; }
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

    #[tokio::test] async fn copilot_is_never_invented() {
        let Reading::Unavailable(reason)=read("copilot","copilot").await else { panic!("o Copilot não informa o limite") };
        assert_eq!(reason.key,"usage.quota.copilotUnreported");
    }

    #[tokio::test] async fn cursor_is_never_invented() {
        let Reading::Unavailable(reason)=read("cursor","cursor-agent").await else { panic!("o Cursor não informa o limite") };
        assert_eq!(reason.key,"usage.quota.cursorUnreported");
    }
}
