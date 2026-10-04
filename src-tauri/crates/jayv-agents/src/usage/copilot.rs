//! O que o Copilot CLI diz do próprio gasto.
//!
//! Ele não narra em JSON nem mostra a conta no `--silent`, mas grava as
//! estatísticas do fim da execução no arquivo de `--usage-output-file`. O
//! formato desse arquivo não é documentado: aqui ele é lido sem supor onde
//! cada número mora — um objeto com tokens de entrada ou saída é um modelo,
//! e qualquer campo com "premium" no nome conta as premium requests.

use super::{Precision, Spend};
use serde_json::Value;

const INPUT:[&str;4]=["inputTokens","input_tokens","promptTokens","prompt_tokens"];
const OUTPUT:[&str;4]=["outputTokens","output_tokens","completionTokens","completion_tokens"];
const CACHE_READ:[&str;4]=["cacheReadTokens","cache_read_tokens","cachedInputTokens","cached_input_tokens"];
const CACHE_WRITE:[&str;2]=["cacheWriteTokens","cache_write_tokens"];

fn first(object:&serde_json::Map<String,Value>,keys:&[&str])->Option<u64> { keys.iter().find_map(|key|object.get(*key).and_then(Value::as_u64)) }

/// Os modelos achados no arquivo: o nome é a chave do objeto (ou o campo
/// `model` dele) e os números são os tokens que ele trouxer.
fn models(value:&Value,name:Option<&str>,found:&mut Vec<(String,u64,u64,u64,u64)>) {
    match value {
        Value::Object(object)=>{
            let (input,output)=(first(object,&INPUT),first(object,&OUTPUT));
            if input.is_some()||output.is_some() {
                let model=object.get("model").and_then(Value::as_str).or(name).unwrap_or("unknown").to_string();
                found.push((model,input.unwrap_or(0),output.unwrap_or(0),first(object,&CACHE_READ).unwrap_or(0),first(object,&CACHE_WRITE).unwrap_or(0)));
                return;
            }
            for (key,child) in object { models(child,Some(key),found); }
        }
        Value::Array(items)=>for item in items { models(item,name,found); },
        _=>{}
    }
}

/// As premium requests, onde quer que estejam.
pub fn premium_requests(value:&Value)->Option<f64> {
    match value {
        Value::Object(object)=>object.iter().find_map(|(key,child)|{
            if key.to_lowercase().contains("premium") { if let Some(count)=child.as_f64() { return Some(count); } }
            premium_requests(child)
        }),
        Value::Array(items)=>items.iter().find_map(premium_requests),
        _=>None,
    }
}

/// Os gastos do arquivo. Vazio quando ele não trouxe tokens: quem chama cai
/// na estimativa.
pub fn spends(file:&Value,model:&str)->Vec<Spend> {
    let mut found=Vec::new();
    models(file,None,&mut found);
    found.into_iter().map(|(name,input,output,cache_read,cache_write)|Spend{
        input_tokens:input,output_tokens:output,cache_read_tokens:cache_read,cache_write_tokens:cache_write,
        ..Spend::new("copilot",if name=="unknown" {model.to_string()} else {name},Precision::Reported)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn models_are_found_wherever_they_are() {
        let file=json!({"session":{"premiumRequests":2,"models":{"model-a":{"inputTokens":1200,"outputTokens":80,"cacheReadTokens":300}}}});
        let spends=spends(&file,"fallback");
        assert_eq!(spends.len(),1);
        assert_eq!((spends[0].model.as_str(),spends[0].input_tokens,spends[0].output_tokens,spends[0].cache_read_tokens),("model-a",1200,80,300));
        assert_eq!(premium_requests(&file),Some(2.0));
    }

    #[test] fn a_list_with_model_fields_works_too() {
        let file=json!({"usage":[{"model":"model-b","prompt_tokens":10,"completion_tokens":2}]});
        let spends=spends(&file,"fallback");
        assert_eq!(spends[0].model,"model-b");
        assert_eq!(spends[0].output_tokens,2);
    }

    /// Um arquivo sem tokens não vira gasto zero "informado".
    #[test] fn a_file_without_tokens_gives_nothing() {
        assert!(spends(&json!({"duration":12}),"m").is_empty());
        assert!(spends(&Value::Null,"m").is_empty());
        assert_eq!(premium_requests(&json!({"x":1})),None);
    }
}
