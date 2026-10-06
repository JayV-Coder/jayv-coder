//! A trava de consumo: o que impede um pedido de gastar a cota do plano do
//! desenvolvedor sozinho. Vale para todo agente e provedor que o app abre
//! (Claude Code, Codex, Copilot, Cursor, Kilo Code, OpenRouter, LiteLLM): quem
//! abre o processo ou a chamada passa por aqui antes e durante.
//!
//! São três travas, e nenhuma depende de o agente informar o gasto:
//! 1. **Aberturas por pedido**: plano, divisão, revisão, plano B e sessão
//!    perdida abrem agentes extras; passado o teto, nenhum outro abre.
//! 2. **Limite do plano**: com a leitura mais recente do limite do agente
//!    (janela de sessão, semana, mês) em `QUOTA_STOP_PERCENT` ou mais, o
//!    agente não abre — a leitura é refeita na hora para confirmar.
//! 3. **Fôlego de uma execução**: o número de passos de ferramenta e, quando o
//!    agente conta (o Claude conta a cada mensagem), os tokens ponderados pelo
//!    preço relativo. Passado o teto, a execução é parada e o chat diz por quê.

use crate::i18n::Text;
use crate::progress::StopReason;
use crate::usage::Quota;
use serde_json::Value;
use std::{collections::HashMap, sync::{atomic::{AtomicU32, Ordering}, Arc, LazyLock, Mutex}};

/// Quantos agentes e chamadas de modelo um pedido pode abrir, contando o plano
/// que ele faz antes, as partes em que se divide, a revisão, o plano B e o
/// título do chat.
pub const MAX_RUNS_PER_REQUEST:u32=12;
/// Quantos passos de ferramenta (ler arquivo, rodar comando, chamar subagente)
/// uma execução pode dar.
pub const MAX_STEPS_PER_RUN:u32=400;
/// O teto de uma execução em tokens de entrada equivalentes: entrada vale 1,
/// escrita no cache 1,25, leitura do cache 0,1 e saída 5 (a proporção dos
/// preços publicados). Um milhão equivale a alguns dólares num modelo grande.
pub const MAX_TOKENS_PER_RUN:u64=1_500_000;
/// A partir de quanto da janela do plano o agente deixa de abrir.
pub const QUOTA_STOP_PERCENT:f64=95.0;
/// As janelas que esgotam o plano inteiro. As outras (a parte do Auto no
/// Cursor, o chat do Copilot) são partes de uma janela maior.
const BLOCKING_WINDOWS:[&str;4]=["session","week","month","premium"];

tokio::task_local! { static RUNS:Arc<AtomicU32>; }

/// Roda `work` com a conta de aberturas deste pedido. Quem atende o pedido
/// abre uma por pedido; as aberturas de dentro dele contam nela.
pub async fn within<F:std::future::Future>(work:F)->F::Output { RUNS.scope(Arc::new(AtomicU32::new(0)),work).await }

/// Conta uma abertura. Fora de um pedido (a leitura de limite, os testes) não
/// há conta e tudo passa.
pub fn enter()->Result<(),Text> {
    let Ok(count)=RUNS.try_with(|runs|runs.fetch_add(1,Ordering::SeqCst)+1) else { return Ok(()) };
    if count>MAX_RUNS_PER_REQUEST { return Err(Text::new("guard.runs").with("max",MAX_RUNS_PER_REQUEST)); }
    Ok(())
}

/// A leitura mais recente de cada janela, por agente.
static LATEST:LazyLock<Mutex<HashMap<String,HashMap<String,(Option<f64>,Option<String>)>>>>=LazyLock::new(Default::default);

/// Guarda a leitura: quem lê o limite (o fim de uma execução, a tela de uso)
/// a manda também para cá.
pub fn note_quota(quota:&Quota) {
    if let Ok(mut latest)=LATEST.lock() { latest.entry(quota.agent.clone()).or_default().insert(quota.window.clone(),(quota.used_percent,quota.resets_at.clone())); }
}

/// A janela que já passou do ponto de parada na última leitura do agente.
pub fn over_limit(agent:&str)->Option<(String,f64)> {
    let latest=LATEST.lock().ok()?;
    latest.get(agent)?.iter().filter(|(window,_)|BLOCKING_WINDOWS.contains(&window.as_str())||window.starts_with("week"))
        .filter_map(|(window,(used,_))|used.filter(|used|*used>=QUOTA_STOP_PERCENT).map(|used|(window.clone(),used))).max_by(|left,right|left.1.total_cmp(&right.1))
}

/// A recusa de abrir um agente cujo plano está no limite. A leitura guardada
/// pode ter ficado velha (a janela reiniciou), então ela é refeita antes de
/// recusar; sem leitura nova, vale o que o agente disse por último só se a
/// refeita falhou por outro motivo que não o limite.
pub async fn check_quota(agent:&str,command:&str)->Result<(),Text> {
    let Some((window,used)) = over_limit(agent) else { return Ok(()) };
    let (window,used)=match crate::usage::quota::read(agent,command).await {
        crate::usage::quota::Reading::Read(found)=>{
            for quota in &found { crate::usage::quota(quota.clone()); }
            match over_limit(agent) { Some(still)=>still, None=>return Ok(()) }
        }
        crate::usage::quota::Reading::Unavailable(_)=>(window,used),
    };
    let name=match agent { "claude"=>"Claude Code", "codex"=>"Codex", "copilot"=>"GitHub Copilot", "cursor"=>"Cursor", other=>other };
    let label=Text::new(&format!("usage.window.{}",if window.starts_with("week") {"week"} else {window.as_str()}));
    Err(Text::new("guard.quota").with("agent",name).with("window",label).with("percent",used.round() as u64))
}

/// O peso de uma mensagem do agente em tokens de entrada equivalentes.
pub fn equivalent(usage:&Value)->f64 {
    let read=|key:&str|usage.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    read("input_tokens")+read("cache_creation_input_tokens")*1.25+read("cache_read_input_tokens")*0.1+read("output_tokens")*5.0
}

/// O fôlego de uma execução, lido evento a evento da saída do agente.
#[derive(Default)]
pub struct Watch { messages:HashMap<String,f64>, steps:u32 }

impl Watch {
    /// Um passo de ferramenta a mais. Devolve o motivo se passou do teto.
    pub fn step(&mut self)->Option<StopReason> {
        self.steps+=1;
        (self.steps>MAX_STEPS_PER_RUN).then_some(StopReason::Steps{max:MAX_STEPS_PER_RUN})
    }

    /// Um evento da saída. O `usage` de cada mensagem do Claude vem repetido a
    /// cada pedaço dela: vale o maior por `id`, e a soma das mensagens é o que
    /// a execução gastou até agora.
    pub fn read(&mut self,event:&Value)->Option<StopReason> {
        if event.get("type").and_then(Value::as_str)!=Some("assistant") { return None; }
        let message=event.get("message")?;
        let id=message.get("id").and_then(Value::as_str)?;
        let weight=equivalent(message.get("usage")?);
        let kept=self.messages.entry(id.to_string()).or_insert(0.0);
        if weight>*kept { *kept=weight; }
        (self.spent()>MAX_TOKENS_PER_RUN as f64).then_some(StopReason::Tokens{millions:(MAX_TOKENS_PER_RUN/1_000_000).max(1) as u32,tenths:(MAX_TOKENS_PER_RUN/100_000%10) as u32})
    }

    pub fn spent(&self)->f64 { self.messages.values().sum() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(id:&str,output:u64)->Value { json!({"type":"assistant","message":{"id":id,"usage":{"input_tokens":10,"cache_read_input_tokens":0,"output_tokens":output}}}) }

    #[tokio::test] async fn a_request_cannot_open_agents_without_end() {
        within(async {
            for _ in 0..MAX_RUNS_PER_REQUEST { assert!(enter().is_ok()); }
            assert_eq!(enter().unwrap_err().key,"guard.runs");
        }).await;
        assert!(enter().is_ok(),"fora de um pedido não há conta");
    }

    #[test] fn weights_follow_the_relative_prices() {
        let usage=json!({"input_tokens":100,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":10});
        assert_eq!(equivalent(&usage),100.0+125.0+100.0+50.0);
    }

    #[test] fn a_message_repeated_in_pieces_counts_once() {
        let mut watch=Watch::default();
        for _ in 0..5 { assert!(watch.read(&message("m1",1_000)).is_none()); }
        assert_eq!(watch.spent(),10.0+5_000.0);
        assert!(watch.read(&message("m2",1_000)).is_none());
        assert_eq!(watch.spent(),2.0*(10.0+5_000.0));
    }

    #[test] fn a_run_that_spends_too_much_is_stopped() {
        let mut watch=Watch::default();
        let mut stopped=None;
        for index in 0..400 { if let Some(reason)=watch.read(&message(&format!("m{index}"),10_000)) { stopped=Some(reason); break; } }
        assert!(matches!(stopped,Some(StopReason::Tokens{..})));
    }

    #[test] fn too_many_steps_stop_the_run() {
        let mut watch=Watch::default();
        for _ in 0..MAX_STEPS_PER_RUN { assert!(watch.step().is_none()); }
        assert_eq!(watch.step(),Some(StopReason::Steps{max:MAX_STEPS_PER_RUN}));
    }

    #[test] fn only_a_spent_plan_window_blocks_an_agent() {
        let reading=|agent:&str,window:&str,used:f64|Quota{agent:agent.into(),window:window.into(),used_percent:Some(used),resets_at:None,plan:None};
        note_quota(&reading("guard-test","session",40.0));
        note_quota(&reading("guard-test","auto",99.0));
        assert!(over_limit("guard-test").is_none(),"uma parte da janela não esgota o plano");
        note_quota(&reading("guard-test","session",96.0));
        assert_eq!(over_limit("guard-test").map(|(window,_)|window),Some("session".into()));
        note_quota(&reading("guard-test","session",10.0));
        assert!(over_limit("guard-test").is_none(),"a janela reiniciou");
    }
}
