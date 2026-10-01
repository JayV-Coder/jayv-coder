//! O que o Codex diz do próprio gasto.
//!
//! Com `exec --json` cada passo sai como um evento: a fala do agente num
//! `item.completed` do tipo `agent_message` e a conta no `turn.completed`. A
//! entrada que ele conta já inclui a que veio do cache; aqui ela é separada,
//! como o Claude faz, para que "entrada" signifique a mesma coisa nos dois.
//!
//! Os limites do plano o Codex grava nos arquivos de sessão
//! (`~/.codex/sessions/**/*.jsonl`), no evento `token_count`.

use super::{Precision, Quota, Spend};
use chrono::{DateTime, SecondsFormat};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn number(object:&Value,key:&str)->u64 { object.get(key).and_then(Value::as_u64).unwrap_or(0) }

/// O gasto de um `turn.completed`.
pub fn spend(event:&Value,model:&str)->Option<Spend> {
    if event.get("type").and_then(Value::as_str)!=Some("turn.completed") { return None; }
    let usage=event.get("usage")?;
    let cached=number(usage,"cached_input_tokens");
    Some(Spend{
        input_tokens:number(usage,"input_tokens").saturating_sub(cached),
        output_tokens:number(usage,"output_tokens"),
        cache_read_tokens:cached,
        cache_write_tokens:number(usage,"cache_write_input_tokens"),
        ..Spend::new("codex",model,Precision::Reported)
    })
}

/// A fala do agente num evento do `--json`.
pub fn said(event:&Value)->Option<String> {
    if event.get("type").and_then(Value::as_str)!=Some("item.completed") { return None; }
    let item=event.get("item")?;
    if item.get("type").and_then(Value::as_str)!=Some("agent_message") { return None; }
    item.get("text").and_then(Value::as_str).map(|text|format!("{text}\n"))
}

/// A falha que o Codex anuncia na própria saída. Só o `turn.failed`: o
/// `error` solto é aviso de reconexão, depois do qual o turno ainda pode
/// terminar bem — e, se não terminar, o processo sai com erro e o motivo
/// vem do canal de erro.
pub fn failure(event:&Value)->Option<String> {
    if event.get("type").and_then(Value::as_str)!=Some("turn.failed") { return None; }
    event.pointer("/error/message").and_then(Value::as_str).map(str::to_string).or_else(||Some("turn failed".into()))
}

fn window_name(minutes:u64)->String {
    match minutes { 300=>"session".into(), 10_080=>"week".into(), 43_200|44_640=>"month".into(), other=>format!("{other}m") }
}

/// Os limites de um evento que traga `rate_limits`, onde quer que ele esteja.
/// Janela vazia (`primary: null`) não é leitura: é o Codex sem dizer.
pub fn quotas(event:&Value)->Vec<Quota> {
    let Some(limits)=["/rate_limits","/payload/rate_limits","/msg/rate_limits"].iter().find_map(|pointer|event.pointer(pointer)) else { return vec![] };
    let plan=limits.get("plan_type").and_then(Value::as_str).map(str::to_string);
    ["primary","secondary"].iter().filter_map(|slot|{
        let window=limits.get(*slot).filter(|window|window.is_object())?;
        let minutes=window.get("window_minutes").and_then(Value::as_u64)?;
        let resets=window.get("resets_at").and_then(Value::as_i64).and_then(|at|DateTime::from_timestamp(at,0)).map(|at|at.to_rfc3339_opts(SecondsFormat::Secs,true));
        Some(Quota{agent:"codex".into(),window:window_name(minutes),used_percent:window.get("used_percent").and_then(Value::as_f64),resets_at:resets,plan:plan.clone()})
    }).collect()
}

/// Onde o Codex guarda as sessões: `$CODEX_HOME/sessions` ou
/// `~/.codex/sessions`.
pub fn sessions_dir()->Option<PathBuf> {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(||dirs::home_dir().map(|home|home.join(".codex"))).map(|home|home.join("sessions"))
}

/// O arquivo de sessão mais novo. Os nomes levam data e hora, e as pastas
/// são `AAAA/MM/DD`: a maior trilha em ordem alfabética é a mais recente.
fn newest(dir:&Path)->Option<PathBuf> {
    walkdir::WalkDir::new(dir).max_depth(4).into_iter().filter_map(Result::ok)
        .filter(|entry|entry.file_type().is_file()&&entry.path().extension().is_some_and(|ext|ext=="jsonl"))
        .map(|entry|entry.into_path()).max()
}

/// A última leitura de limite gravada nas sessões, inclusive as feitas fora
/// do JayV. Vazio quando o Codex não está aqui ou ainda não leu limite.
pub fn latest_session_quotas(dir:&Path)->Vec<Quota> {
    let Some(file)=newest(dir) else { return vec![] };
    let Ok(text)=std::fs::read_to_string(file) else { return vec![] };
    text.lines().rev().filter(|line|line.contains("rate_limits")).find_map(|line|{
        let found=serde_json::from_str::<Value>(line).ok().map(|event|quotas(&event)).unwrap_or_default();
        (!found.is_empty()).then_some(found)
    }).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn the_completed_turn_separates_the_cache() {
        let event=json!({"type":"turn.completed","usage":{"input_tokens":25302,"cached_input_tokens":16896,"output_tokens":278}});
        let spend=spend(&event,"model-x").unwrap();
        assert_eq!((spend.input_tokens,spend.cache_read_tokens,spend.output_tokens),(8406,16896,278));
        assert_eq!(spend.precision,Precision::Reported);
        assert!(spend.cost_usd.is_none());
    }

    #[test] fn the_agent_message_is_the_answer() {
        assert_eq!(said(&json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"pronto"}})).as_deref(),Some("pronto\n"));
        assert!(said(&json!({"type":"item.completed","item":{"type":"command_execution","command":"ls"}})).is_none());
        assert_eq!(failure(&json!({"type":"turn.failed","error":{"message":"quota"}})).as_deref(),Some("quota"));
        // O `error` solto é o aviso de reconexão: o Codex continua e o turno
        // pode terminar bem. Só o `turn.failed` encerra.
        assert_eq!(failure(&json!({"type":"error","message":"Reconnecting... 1/5"})),None);
    }

    #[test] fn the_limits_come_with_window_and_reset() {
        let event=json!({"type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":98.0,"window_minutes":300,"resets_at":1790019761},"secondary":{"used_percent":15.0,"window_minutes":10080,"resets_at":1790606561},"plan_type":"plus"}}});
        let quotas=quotas(&event);
        assert_eq!(quotas.len(),2);
        assert_eq!((quotas[0].window.as_str(),quotas[0].used_percent),("session",Some(98.0)));
        assert_eq!(quotas[1].window,"week");
        assert_eq!(quotas[0].plan.as_deref(),Some("plus"));
        assert!(quotas[0].resets_at.as_deref().unwrap().ends_with('Z'));
    }

    #[test] fn an_empty_window_is_not_a_reading() {
        assert!(quotas(&json!({"rate_limits":{"limit_id":"premium","primary":null,"secondary":null}})).is_empty());
    }

    #[test] fn the_newest_session_with_limits_wins() {
        let dir=tempfile::tempdir().unwrap();
        let day=dir.path().join("2026/09/25");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-a.jsonl"),"{\"rate_limits\":{\"primary\":{\"used_percent\":1.0,\"window_minutes\":300}}}\n").unwrap();
        let later=dir.path().join("2026/09/26");
        std::fs::create_dir_all(&later).unwrap();
        std::fs::write(later.join("rollout-b.jsonl"),"{\"payload\":{\"rate_limits\":{\"primary\":{\"used_percent\":40.0,\"window_minutes\":300}}}}\n{\"payload\":{\"rate_limits\":{\"primary\":null}}}\n").unwrap();
        let quotas=latest_session_quotas(dir.path());
        assert_eq!(quotas[0].used_percent,Some(40.0));
        assert!(latest_session_quotas(&dir.path().join("nada")).is_empty());
    }
}
