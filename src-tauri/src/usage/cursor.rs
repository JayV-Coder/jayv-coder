//! O que o Cursor CLI diz do próprio gasto.
//!
//! Em `stream-json` o último evento é o `result`, com o `usage` do pedido em
//! camelCase (`inputTokens`, `outputTokens`, `cacheReadTokens`,
//! `cacheWriteTokens`). Custo ele não informa, e o limite do plano não sai
//! fora do modo interativo.

use super::{Precision, Spend};
use serde_json::Value;

fn number(object:&Value,key:&str)->u64 { object.get(key).and_then(Value::as_u64).unwrap_or(0) }

/// O gasto de um `result` com `usage`. `None` para qualquer outro evento, e
/// também para o `result` sem conta: aí vale a estimativa.
pub fn spend(event:&Value,model:&str)->Option<Spend> {
    if event.get("type").and_then(Value::as_str)!=Some("result") { return None; }
    let usage=event.get("usage").filter(|usage|usage.is_object())?;
    Some(Spend{
        input_tokens:number(usage,"inputTokens"),
        output_tokens:number(usage,"outputTokens"),
        cache_read_tokens:number(usage,"cacheReadTokens"),
        cache_write_tokens:number(usage,"cacheWriteTokens"),
        duration_ms:number(event,"duration_ms"),
        success:event.get("is_error").and_then(Value::as_bool)!=Some(true),
        ..Spend::new("cursor",model,Precision::Reported)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn the_result_carries_the_reported_tokens() {
        let event=json!({"type":"result","subtype":"success","duration_ms":3715,"is_error":false,"result":"pong","usage":{"inputTokens":14006,"outputTokens":35,"cacheReadTokens":3840,"cacheWriteTokens":0}});
        let found=spend(&event,"auto").expect("o result com usage");
        assert_eq!((found.input_tokens,found.output_tokens,found.cache_read_tokens,found.cache_write_tokens),(14006,35,3840,0));
        assert_eq!(found.duration_ms,3715);
        assert_eq!(found.source,"cursor");
        assert!(found.success);
    }

    #[test] fn without_usage_nothing_is_invented() {
        assert!(spend(&json!({"type":"result","subtype":"success","result":"pong"}),"auto").is_none());
        assert!(spend(&json!({"type":"assistant","message":{"content":[]}}),"auto").is_none());
    }
}
