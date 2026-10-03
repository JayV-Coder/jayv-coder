//! A medida que o JayV deve a quem o usa: o mesmo roteiro de pedidos mandado
//! direto ao agente e mandado pelo JayV, com os tokens que cada lado gastou de
//! verdade — os que o agente informou, cache incluído —, lado a lado.

use crate::{orchestrator::Orchestrator, progress::Pulse, usage::{self, Entry, Scope, Spend}};
use anyhow::{anyhow, Result};

const DIRECT_SESSION:&str="bench-direct";
const JAYV_SESSION:&str="bench-jayv";

/// O que um lado gastou no roteiro inteiro.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tally { pub calls: u64, pub input_tokens: u64, pub cache_read_tokens: u64, pub cache_write_tokens: u64, pub output_tokens: u64, pub cost_usd: Option<f64>, pub failures: u64 }

impl Tally {
    fn add(&mut self, spend:&Spend) {
        self.calls+=1;
        self.input_tokens+=spend.input_tokens;
        self.cache_read_tokens+=spend.cache_read_tokens;
        self.cache_write_tokens+=spend.cache_write_tokens;
        self.output_tokens+=spend.output_tokens;
        if let Some(cost)=spend.cost_usd { self.cost_usd=Some(self.cost_usd.unwrap_or(0.0)+cost); }
        if !spend.success { self.failures+=1; }
    }
    /// Tudo que entrou no modelo, lido do cache ou não.
    pub fn total_input(&self)->u64 { self.input_tokens+self.cache_read_tokens+self.cache_write_tokens }
}

/// O roteiro rodado pelos dois caminhos. `jev` é o que o próprio Jev gastou
/// para rotear: entra na conta do JayV, mas à parte, porque sai de outra cota.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Comparison { pub direct: Tally, pub jayv: Tally, pub jev: Tally }

/// Roda cada pedido pelo JayV e, com o mesmo agente e modelo que o JayV
/// escolheu, direto. Os dois lados seguem a própria conversa do começo ao fim.
pub async fn compare(orchestrator:&mut Orchestrator,prompts:&[String])->Result<Comparison> {
    let mut comparison=Comparison::default();
    for prompt in prompts {
        let (result,spends)=collect(orchestrator.process(prompt,Some(JAYV_SESSION),&Pulse::silent())).await;
        for spend in &spends { if spend.source.starts_with("jev") { comparison.jev.add(spend) } else { comparison.jayv.add(spend) } }
        if result.result.is_none() { return Err(anyhow!("the JayV run failed on {prompt:?}: {}",result.error.unwrap_or_default())); }
        let (direct,spends)=collect(orchestrator.direct(prompt,DIRECT_SESSION,&result.model_selection)).await;
        for spend in &spends { comparison.direct.add(spend); }
        direct?;
    }
    Ok(comparison)
}

/// Roda `work` com uma pia só dele e devolve o que foi gasto lá dentro.
async fn collect<F:std::future::Future>(work:F)->(F::Output,Vec<Spend>) {
    let (sink,mut entries)=tokio::sync::mpsc::unbounded_channel();
    let output=usage::within_sink(Scope::default(),sink,work).await;
    let mut spends=vec![];
    while let Ok(entry)=entries.try_recv() { if let Entry::Spend(_,spend)=entry { spends.push(spend); } }
    (output,spends)
}

/// O relatório para o terminal.
pub fn report(comparison:&Comparison)->String {
    let row=|name:&str,tally:&Tally|format!("{name:<8} {:>6} {:>12} {:>12} {:>12} {:>10} {:>10}",tally.calls,tally.input_tokens,tally.cache_read_tokens,tally.cache_write_tokens,tally.output_tokens,tally.cost_usd.map_or("-".into(),|cost|format!("{cost:.4}")));
    let mut lines=vec![format!("{:<8} {:>6} {:>12} {:>12} {:>12} {:>10} {:>10}","","calls","input","cache read","cache write","output","usd"),row("direct",&comparison.direct),row("jayv",&comparison.jayv),row("jev",&comparison.jev)];
    let before=comparison.direct.total_input()+comparison.direct.output_tokens;
    let after=comparison.jayv.total_input()+comparison.jayv.output_tokens;
    if before>0 { lines.push(format!("JayV used {:.1}% of the tokens of the direct agent (Jev routing not included).",after as f64*100.0/before as f64)); }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Precision;

    #[test] fn the_report_compares_every_token_the_agent_was_charged_for() {
        let mut comparison=Comparison::default();
        comparison.direct.add(&Spend{input_tokens:100,cache_read_tokens:900,output_tokens:200,..Spend::new("claude","sonnet",Precision::Reported)});
        comparison.jayv.add(&Spend{input_tokens:100,cache_read_tokens:400,output_tokens:100,..Spend::new("claude","sonnet",Precision::Reported)});
        assert_eq!(comparison.direct.total_input(),1000);
        assert!(report(&comparison).contains("JayV used 50.0%"),"o cache lido conta: {}",report(&comparison));
    }
}
