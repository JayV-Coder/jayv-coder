//! O que o JayV gastou e o que o Jev fez, contado onde acontece.
//!
//! Quem gasta — um provedor, a chamada ao Jev, o portão — não sabe de chat nem
//! de banco: ele só diz `usage::spend(..)`. O escopo (projeto, chat, turno) vem
//! de quem abriu o atendimento, por `within`, e a gravação é de quem instalou a
//! pia, por `install`. Sem pia — a linha de comando, os testes — tudo vira
//! nada, e o núcleo roda igual.

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod quota;
pub mod store;

use serde::{Deserialize, Serialize};
use std::{future::Future, sync::{LazyLock, RwLock}};
use tokio::sync::mpsc::UnboundedSender;

/// De onde vem o número: a ferramenta informou, o app calculou, ou é um turno
/// de antes desta contagem existir (calculado por caracteres ÷ 4).
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Precision { Reported, Estimated, Legacy }

impl Precision {
    pub fn as_str(&self)->&'static str { match self { Self::Reported=>"reported", Self::Estimated=>"estimated", Self::Legacy=>"legacy" } }
}

/// Uma chamada a um modelo. `source` é `claude`, `codex`, `copilot`, `cursor`, `cursor`,
/// `http:<provedor>` ou `jev:<conjunto>`. O custo só existe quando a
/// ferramenta o informou.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Spend {
    pub source:String,
    pub model:String,
    pub input_tokens:u64,
    pub output_tokens:u64,
    pub cache_read_tokens:u64,
    pub cache_write_tokens:u64,
    pub cost_usd:Option<f64>,
    /// Pedidos cobrados pelo plano (as premium requests do Copilot), quando
    /// a ferramenta os conta.
    pub requests:Option<f64>,
    pub duration_ms:u64,
    pub success:bool,
    pub precision:Precision,
}

impl Spend {
    pub fn new(source:impl Into<String>,model:impl Into<String>,precision:Precision)->Self {
        Self{source:source.into(),model:model.into(),input_tokens:0,output_tokens:0,cache_read_tokens:0,cache_write_tokens:0,cost_usd:None,requests:None,duration_ms:0,success:true,precision}
    }
}

/// Algo que o Jev fez: um veredito, um acerto do cache, um segredo retirado,
/// tokens poupados. `amount` é a quantidade (1 para um veredito, N tokens
/// para uma economia).
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct JevMark { pub kind:String, pub amount:f64, pub precision:Precision }

impl JevMark {
    pub fn count(kind:impl Into<String>,amount:u64)->Self { Self{kind:kind.into(),amount:amount as f64,precision:Precision::Reported} }
    pub fn saved(cause:&str,tokens:u64)->Self { Self{kind:format!("saved_tokens:{cause}"),amount:tokens as f64,precision:Precision::Estimated} }
}

/// Uma leitura do limite do plano de um agente. `window` é `session`, `week`
/// ou `month`; `used_percent` vazio é "a ferramenta não disse".
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Quota {
    pub agent:String,
    pub window:String,
    pub used_percent:Option<f64>,
    pub resets_at:Option<String>,
    pub plan:Option<String>,
}

/// A quem o gasto pertence. Fora de um atendimento — o batismo de um chat
/// sem escopo, a leitura do limite — os campos ficam vazios e o gasto conta
/// só no total.
#[derive(Debug,Clone,Default,PartialEq,Eq)]
pub struct Scope { pub project_id:Option<String>, pub chat_id:Option<String>, pub turn_id:Option<String> }

#[derive(Debug,Clone,PartialEq)]
pub enum Entry { Spend(Scope,Spend), Jev(Scope,JevMark), Quota(Quota) }

type Sink=UnboundedSender<Entry>;

#[derive(Clone)]
struct Frame { scope:Scope, sink:Option<Sink> }

tokio::task_local! { static FRAME:Frame; }

static SINK:LazyLock<RwLock<Option<Sink>>>=LazyLock::new(||RwLock::new(None));

/// A pia do aplicativo: tudo que for gasto daqui em diante sai por ela.
pub fn install(sink:Sink) { if let Ok(mut current)=SINK.write() { *current=Some(sink); } }

/// Roda `work` com este escopo: todo gasto dentro dele — no mesmo task —
/// pertence a este projeto, chat e turno.
pub async fn within<F:Future>(scope:Scope,work:F)->F::Output {
    let sink=FRAME.try_with(|frame|frame.sink.clone()).ok().flatten();
    FRAME.scope(Frame{scope,sink},work).await
}

/// Como `within`, com uma pia própria. Serve aos testes, que rodam em
/// paralelo e não podem dividir a pia do aplicativo.
pub async fn within_sink<F:Future>(scope:Scope,sink:Sink,work:F)->F::Output {
    FRAME.scope(Frame{scope,sink:Some(sink)},work).await
}

/// O escopo do atendimento em curso, se houver.
pub fn current_scope()->Scope { FRAME.try_with(|frame|frame.scope.clone()).unwrap_or_default() }

fn send(entry:Entry) {
    let local=FRAME.try_with(|frame|frame.sink.clone()).ok().flatten();
    let sink=local.or_else(||SINK.read().ok().and_then(|sink|sink.clone()));
    // Uma pia fechada não derruba o pedido: o turno vale mais que a conta dele.
    if let Some(sink)=sink { let _=sink.send(entry); }
}

pub fn spend(spend:Spend) { send(Entry::Spend(current_scope(),spend)); }
pub fn mark(mark:JevMark) { send(Entry::Jev(current_scope(),mark)); }
pub fn quota(quota:Quota) { send(Entry::Quota(quota)); }

/// Se o aplicativo instalou a pia. Sem ela ninguém grava, e não vale abrir
/// processo nenhum para ler limite.
pub fn is_installed()->bool { SINK.read().map(|sink|sink.is_some()).unwrap_or(false) }

/// Tokens de um texto, à moda de sempre: caracteres ÷ 4. Só para o que a
/// ferramenta não contou — e o registro vai marcado como `estimated`.
pub fn estimate(text:&str)->u64 { (text.chars().count() as u64).div_ceil(4) }

/// A conta de uma execução de agente de linha de comando, colhida linha a
/// linha da saída dele. Cada agente fala do gasto do seu jeito; o que nenhum
/// disse vira estimativa no fim, marcada como tal.
pub struct Meter {
    agent:String,
    model:String,
    command:String,
    started:std::time::Instant,
    spends:Vec<Spend>,
    quotas:Vec<Quota>,
    rate_limited:bool,
}

impl Meter {
    pub fn new(agent:&str,model:&str,command:&str)->Self {
        Self{agent:agent.into(),model:model.into(),command:command.into(),started:std::time::Instant::now(),spends:vec![],quotas:vec![],rate_limited:false}
    }

    /// Lê uma linha da saída. Texto que não é JSON não conta nada.
    pub fn read(&mut self,line:&str) {
        let Ok(event)=serde_json::from_str::<serde_json::Value>(line.trim()) else { return };
        let elapsed=self.elapsed();
        match self.agent.as_str() {
            "claude"=>{
                if let Some(found)=claude::spends(&event,&self.model,elapsed) { self.spends.extend(found); }
                if claude::is_rate_limit(&event) { self.rate_limited=true; }
            }
            "codex"=>{
                if let Some(mut found)=codex::spend(&event,&self.model) { found.duration_ms=elapsed; self.spends.push(found); }
                self.quotas.extend(codex::quotas(&event));
            }
            "cursor"=>{
                if let Some(mut found)=cursor::spend(&event,&self.model) { if found.duration_ms==0 { found.duration_ms=elapsed; } self.spends.push(found); }
            }
            _=>{}
        }
    }

    fn elapsed(&self)->u64 { self.started.elapsed().as_millis() as u64 }

    /// Fecha a conta e a envia. `usage_file` é o que o Copilot gravou no fim.
    /// Devolve a entrada e a saída somadas, para o resumo do turno.
    pub fn settle(mut self,prompt:&str,response:&str,success:bool,usage_file:Option<&serde_json::Value>)->(u64,u64) {
        if let Some(file)=usage_file {
            let mut found=copilot::spends(file,&self.model);
            let requests=copilot::premium_requests(file);
            if let Some(first)=found.first_mut() { first.requests=requests; }
            else if requests.is_some() {
                // Pedidos contados sem tokens: os pedidos são exatos, os
                // tokens não — o registro inteiro fica como estimado.
                found.push(Spend{requests,..self.estimated(prompt,response)});
            }
            self.spends.extend(found);
        }
        if self.spends.is_empty() { self.spends.push(self.estimated(prompt,response)); }
        let elapsed=self.elapsed();
        if self.spends.iter().all(|spend|spend.duration_ms==0) { if let Some(first)=self.spends.first_mut() { first.duration_ms=elapsed; } }
        for spend in &mut self.spends { if !success { spend.success=false; } }
        let totals=self.spends.iter().fold((0,0),|(input,output),spend|(input+spend.input_tokens+spend.cache_read_tokens+spend.cache_write_tokens,output+spend.output_tokens));
        for found in self.spends.drain(..) { self::spend(found); }
        for found in self.quotas.drain(..) { self::quota(found); }
        // O Claude avisou que o limite andou; o Codex acabou de gravar o dele
        // na sessão. Os dois valem uma releitura.
        if self.rate_limited||self.agent=="codex" { quota::refresh(&self.agent,&self.command); }
        totals
    }

    fn estimated(&self,prompt:&str,response:&str)->Spend {
        Spend{input_tokens:estimate(prompt),output_tokens:estimate(response),..Spend::new(self.agent.as_str(),self.model.as_str(),Precision::Estimated)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn scope()->Scope { Scope{project_id:Some("p1".into()),chat_id:Some("c1".into()),turn_id:Some("t1".into())} }

    #[tokio::test] async fn a_spend_inside_within_carries_the_scope() {
        let (sink,mut entries)=mpsc::unbounded_channel();
        within_sink(scope(),sink,async { spend(Spend::new("claude","sonnet",Precision::Reported)); }).await;
        let Some(Entry::Spend(found,spend))=entries.recv().await else { panic!("o gasto não chegou à pia") };
        assert_eq!(found,scope());
        assert_eq!(spend.source,"claude");
    }

    /// O batismo do chat, por exemplo, roda dentro do atendimento mas abre
    /// outro escopo: a pia continua a mesma.
    #[tokio::test] async fn a_nested_scope_keeps_the_sink() {
        let (sink,mut entries)=mpsc::unbounded_channel();
        within_sink(scope(),sink,async {
            within(Scope::default(),async { mark(JevMark::count("cache_hit",1)); }).await;
        }).await;
        let Some(Entry::Jev(found,mark))=entries.recv().await else { panic!("a marca não chegou") };
        assert_eq!(found,Scope::default());
        assert_eq!(mark.kind,"cache_hit");
    }

    #[test] fn without_a_sink_nothing_breaks() {
        spend(Spend::new("codex","gpt",Precision::Estimated));
        mark(JevMark::saved("blocked",10));
        assert_eq!(current_scope(),Scope::default());
    }

    async fn collect(work:impl Future<Output=()>)->Vec<Entry> {
        let (sink,mut entries)=mpsc::unbounded_channel();
        within_sink(Scope::default(),sink,work).await;
        let mut found=vec![];
        while let Ok(entry)=entries.try_recv() { found.push(entry); }
        found
    }

    /// O agente que não disse nada vira estimativa, e não gasto zero.
    #[tokio::test] async fn a_silent_agent_is_estimated() {
        let entries=collect(async {
            let meter=Meter::new("copilot","model-a","copilot");
            assert_eq!(meter.settle("abcdefgh","abcd",true,None),(2,1));
        }).await;
        let [Entry::Spend(_,spend)]=entries.as_slice() else { panic!("{entries:?}") };
        assert_eq!(spend.precision,Precision::Estimated);
    }

    #[tokio::test] async fn the_claude_result_replaces_the_estimate() {
        let entries=collect(async {
            let mut meter=Meter::new("claude","alias","claude");
            meter.read("texto solto");
            meter.read(r#"{"type":"result","is_error":false,"usage":{"input_tokens":3,"output_tokens":9,"cache_read_input_tokens":100}}"#);
            assert_eq!(meter.settle("prompt","resposta",true,None),(103,9));
        }).await;
        let [Entry::Spend(_,spend)]=entries.as_slice() else { panic!("{entries:?}") };
        assert_eq!(spend.precision,Precision::Reported);
    }

    #[tokio::test] async fn a_failed_run_still_counts() {
        let entries=collect(async {
            let mut meter=Meter::new("codex","model-x","codex");
            meter.read(r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":4,"output_tokens":1}}"#);
            meter.settle("p","",false,None);
        }).await;
        let [Entry::Spend(_,spend)]=entries.as_slice() else { panic!("{entries:?}") };
        assert!(!spend.success);
        assert_eq!(spend.input_tokens,6);
    }

    #[tokio::test] async fn copilot_requests_ride_on_the_first_model() {
        let entries=collect(async {
            let file=serde_json::json!({"premiumRequests":1,"models":{"model-a":{"inputTokens":5,"outputTokens":2}}});
            Meter::new("copilot","model-a","copilot").settle("p","r",true,Some(&file));
        }).await;
        let [Entry::Spend(_,spend)]=entries.as_slice() else { panic!("{entries:?}") };
        assert_eq!(spend.requests,Some(1.0));
        assert_eq!(spend.precision,Precision::Reported);
    }

    #[test] fn the_estimate_rounds_up() {
        assert_eq!(estimate(""),0);
        assert_eq!(estimate("abcde"),2);
    }
}
