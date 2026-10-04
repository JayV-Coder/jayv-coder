//! O que o Claude Code diz do próprio gasto.
//!
//! Em `stream-json` o último evento é o `result`, com o `usage` do pedido, o
//! `modelUsage` separado por modelo (subagentes inclusive) e o custo que a
//! API cobraria. O `/usage` responde no `--print` sem chamar o modelo, em
//! texto: é dele que sai o limite da janela de 5 horas e o da semana.

use super::{Precision, Quota, Spend};
use serde_json::Value;

fn tokens(object:&Value,keys:&[&str])->u64 { keys.iter().find_map(|key|object.get(key).and_then(Value::as_u64)).unwrap_or(0) }

/// Os gastos de um evento `result`: um por modelo do `modelUsage` e, sem
/// ele, um só com o `usage`. `None` para qualquer outro evento.
pub fn spends(event:&Value,model:&str,duration_ms:u64)->Option<Vec<Spend>> {
    if event.get("type").and_then(Value::as_str)!=Some("result") { return None; }
    let success=event.get("is_error").and_then(Value::as_bool)!=Some(true);
    let duration=event.get("duration_ms").and_then(Value::as_u64).unwrap_or(duration_ms);
    let by_model=event.get("modelUsage").and_then(Value::as_object).filter(|models|!models.is_empty());
    if let Some(models)=by_model {
        return Some(models.iter().enumerate().map(|(index,(name,usage))|Spend{
            input_tokens:tokens(usage,&["inputTokens"]),
            output_tokens:tokens(usage,&["outputTokens"]),
            cache_read_tokens:tokens(usage,&["cacheReadInputTokens"]),
            cache_write_tokens:tokens(usage,&["cacheCreationInputTokens"]),
            cost_usd:usage.get("costUSD").and_then(Value::as_f64),
            // A duração é do pedido inteiro; dividi-la entre os modelos seria
            // inventar. Ela fica no primeiro.
            duration_ms:if index==0 {duration} else {0},
            success,
            ..Spend::new("claude",name.as_str(),Precision::Reported)
        }).collect());
    }
    let usage=event.get("usage").cloned().unwrap_or(Value::Null);
    Some(vec![Spend{
        input_tokens:tokens(&usage,&["input_tokens"]),
        output_tokens:tokens(&usage,&["output_tokens"]),
        cache_read_tokens:tokens(&usage,&["cache_read_input_tokens"]),
        cache_write_tokens:tokens(&usage,&["cache_creation_input_tokens"]),
        cost_usd:event.get("total_cost_usd").and_then(Value::as_f64).filter(|cost|*cost>0.0),
        duration_ms:duration,
        success,
        ..Spend::new("claude",model,Precision::Reported)
    }])
}

/// O aviso de limite que o Claude manda no meio do fluxo. O conteúdo dele
/// muda entre versões; o que importa é que o limite andou e vale reler.
pub fn is_rate_limit(event:&Value)->bool { event.get("type").and_then(Value::as_str)==Some("rate_limit_event") }

/// As linhas `Current session: 9% used · resets Oct 1, 9pm (America/Recife)`
/// e `Current week (all models): 19% used · resets …`. O horário fica como o
/// Claude escreveu: ele vem no fuso da conta, e a tela o mostra assim.
/// `None` quando nenhuma linha casou — o texto mudou e não se inventa número.
pub fn parse_usage_text(text:&str)->Option<Vec<Quota>> {
    let text=serde_json::from_str::<Value>(text).ok().and_then(|value|value.get("result").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(||text.to_string());
    let mut found=Vec::new();
    for line in text.lines().map(str::trim) {
        let Some(rest)=line.strip_prefix("Current ") else { continue };
        let Some((label,reading))=rest.split_once(':') else { continue };
        let Some((percent,_))=reading.trim().split_once('%') else { continue };
        let Ok(percent)=percent.trim().parse::<f64>() else { continue };
        let window=match label.trim() {
            "session"=>"session".to_string(),
            "week (all models)"|"week"=>"week".to_string(),
            other=>match other.strip_prefix("week (").and_then(|inner|inner.strip_suffix(')')) {
                Some(inner)=>format!("week:{}",inner.trim().to_lowercase().replace(' ',"_")),
                None=>other.to_lowercase().replace(' ',"_"),
            },
        };
        let resets=reading.split_once("resets").map(|(_,when)|when.trim().to_string()).filter(|when|!when.is_empty());
        found.push(Quota{agent:"claude".into(),window,used_percent:Some(percent),resets_at:resets,plan:None});
    }
    (!found.is_empty()).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn the_result_splits_by_model_with_cost() {
        let event=json!({"type":"result","subtype":"success","is_error":false,"duration_ms":4200,"total_cost_usd":0.31,
            "usage":{"input_tokens":12,"output_tokens":300,"cache_read_input_tokens":9000,"cache_creation_input_tokens":800},
            "modelUsage":{
                "model-big":{"inputTokens":10,"outputTokens":250,"cacheReadInputTokens":9000,"cacheCreationInputTokens":800,"costUSD":0.3},
                "model-small":{"inputTokens":2,"outputTokens":50,"cacheReadInputTokens":0,"cacheCreationInputTokens":0,"costUSD":0.01}}});
        let spends=spends(&event,"alias",1).unwrap();
        assert_eq!(spends.len(),2);
        let big=spends.iter().find(|spend|spend.model=="model-big").unwrap();
        assert_eq!((big.input_tokens,big.output_tokens,big.cache_read_tokens,big.cache_write_tokens),(10,250,9000,800));
        assert_eq!(big.cost_usd,Some(0.3));
        assert_eq!(spends.iter().map(|spend|spend.duration_ms).sum::<u64>(),4200);
        assert!(spends.iter().all(|spend|spend.precision==Precision::Reported&&spend.source=="claude"));
    }

    #[test] fn without_model_usage_the_totals_are_used() {
        let event=json!({"type":"result","is_error":true,"total_cost_usd":0.0,"usage":{"input_tokens":5,"output_tokens":0}});
        let spends=spends(&event,"alias",77).unwrap();
        assert_eq!(spends.len(),1);
        assert_eq!(spends[0].model,"alias");
        assert_eq!(spends[0].input_tokens,5);
        assert_eq!(spends[0].cost_usd,None);
        assert!(!spends[0].success);
        assert_eq!(spends[0].duration_ms,77);
    }

    #[test] fn other_events_are_not_spends() {
        assert!(spends(&json!({"type":"assistant"}),"m",0).is_none());
        assert!(is_rate_limit(&json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}})));
    }

    #[test] fn the_usage_text_gives_session_and_week() {
        let text="You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 9% used · resets Oct 1, 9pm (America/Recife)\nCurrent week (all models): 19% used · resets Oct 7, 9pm (America/Recife)\nCurrent week (Sonnet only): 3% used · resets Oct 7, 9pm (America/Recife)\n";
        let quotas=parse_usage_text(&json!({"type":"result","result":text}).to_string()).unwrap();
        assert_eq!(quotas.len(),3);
        assert_eq!((quotas[0].window.as_str(),quotas[0].used_percent),("session",Some(9.0)));
        assert_eq!(quotas[0].resets_at.as_deref(),Some("Oct 1, 9pm (America/Recife)"));
        assert_eq!((quotas[1].window.as_str(),quotas[1].used_percent),("week",Some(19.0)));
        assert_eq!(quotas[2].window,"week:sonnet_only");
    }

    /// Texto de outra versão: nada de número inventado.
    #[test] fn an_unknown_usage_text_reads_nothing() {
        assert!(parse_usage_text("Usage is shown in the web dashboard.").is_none());
        assert!(parse_usage_text("Current session: lots used").is_none());
    }
}
