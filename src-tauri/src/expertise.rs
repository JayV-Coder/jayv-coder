//! O nível do desenvolvedor: o quanto a portaria e o Jev confiam no pedido.
//!
//! Cada conta escolhe o seu — `starter`, `junior`, `mid`, `senior` ou
//! `architect` — e ele anda com a conta pela sincronização. O nível não muda
//! o quanto se barra: muda o quanto se pergunta e o quanto o agente pode
//! construir sozinho. Quem está começando ouve mais perguntas e recebe mais
//! planos; quem projeta sistemas escreve pedidos curtos e recebe build até no
//! trabalho do tamanho do sistema.

use crate::local::global::JevParameters;
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const KEY:&str="expertise_level";

#[derive(Debug,Clone,Copy,PartialEq,Eq,Default,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Expertise { Starter, Junior, #[default] Mid, Senior, Architect }

impl Expertise {
    pub const ALL:[Expertise;5]=[Self::Starter,Self::Junior,Self::Mid,Self::Senior,Self::Architect];

    pub fn as_str(&self)->&'static str { match self { Self::Starter=>"starter", Self::Junior=>"junior", Self::Mid=>"mid", Self::Senior=>"senior", Self::Architect=>"architect" } }

    pub fn parse(value:&str)->Option<Self> { Self::ALL.into_iter().find(|level|level.as_str()==value) }

    /// O deslocamento da exigência: soma à clareza que a portaria pede e à
    /// confiança que o roteamento do Jev quer antes de pedir esclarecimento.
    /// `mid` é o comportamento de sempre.
    pub fn shift(&self)->f64 { match self { Self::Starter=>0.10, Self::Junior=>0.05, Self::Mid=>0.0, Self::Senior=>-0.05, Self::Architect=>-0.10 } }

    /// A maior complexidade que ainda vai em modo build. Acima dela o agente
    /// devolve um plano.
    pub fn build_ceiling(&self)->&'static str { match self { Self::Starter|Self::Junior=>"simple", Self::Mid|Self::Senior=>"medium", Self::Architect=>"complex" } }

    /// A partir de que probabilidade de apagar trabalho o pedido vira plano.
    pub fn destructive_threshold(&self)->f64 { match self { Self::Starter=>0.20, Self::Junior=>0.30, Self::Mid=>0.35, Self::Senior=>0.45, Self::Architect=>0.55 } }

    /// Os números da portaria para este nível. A exigência de cada tamanho
    /// anda com o deslocamento e a folga anda junto, ao contrário: a linha do
    /// bloqueio (exigência − folga) fica onde estava. O nível muda a faixa em
    /// que o portão pergunta, não a em que ele recusa.
    pub fn gate(&self,base:&JevParameters)->JevParameters {
        let shift=self.shift();
        let mut adjusted=base.clone();
        for (demand,original) in adjusted.scope_demand.iter_mut().zip(base.scope_demand) { *demand=(original+shift).clamp(0.05,0.95); }
        adjusted.block_margin=(base.block_margin+shift).clamp(0.0,0.9);
        adjusted
    }

    /// A confiança mínima do roteamento, para este nível.
    pub fn confidence(&self,configured:f64)->f64 { (configured+self.shift()).clamp(0.3,0.99) }

    /// Se uma complexidade cabe no modo build deste nível.
    pub fn builds(&self,complexity:&str)->bool {
        let rank=|level:&str|crate::core_settings::COMPLEXITIES.iter().position(|known|*known==level);
        match (rank(complexity),rank(self.build_ceiling())) { (Some(asked),Some(ceiling))=>asked<=ceiling, _=>false }
    }
}

/// A tabela da conta: chave e valor, sincronizada como as outras.
pub const SCHEMA:&str="CREATE TABLE IF NOT EXISTS account_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);";

/// O nível gravado na conta. Sem escolha — ou com um valor que esta versão
/// não conhece — vale `mid`, que é o comportamento de antes dos níveis.
pub fn load(connection:&Connection)->Result<Expertise> {
    let value:Option<String>=connection.query_row("SELECT value FROM account_settings WHERE key=?1",[KEY],|row|row.get(0)).optional()?;
    Ok(value.as_deref().and_then(Expertise::parse).unwrap_or_default())
}

pub fn save(connection:&Connection,level:&str)->Result<Expertise> {
    let Some(level)=Expertise::parse(level) else { bail!(crate::i18n::Text::new("expertise.invalid").with("value",level)) };
    connection.execute(
        "INSERT INTO account_settings(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        params![KEY,level.as_str(),chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(level)
}

/// Como os pedidos passaram pela portaria numa janela de dias.
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct GateHistory { pub checks:u32, pub passed:u32, pub asked:u32, pub blocked:u32 }

impl GateHistory {
    fn rate(&self,count:u32)->f64 { if self.checks==0 {0.0} else {f64::from(count)/f64::from(self.checks)} }
    pub fn pass_rate(&self)->f64 { self.rate(self.passed) }
    pub fn held_rate(&self)->f64 { self.rate(self.asked+self.blocked) }
}

/// A janela da sugestão de nível e o mínimo de pedidos para ela valer.
pub const SUGGESTION_DAYS:i64=30;
const SUGGESTION_CHECKS:u32=20;
/// Quase tudo passa de primeira: o nível pode subir.
const RAISE_PASS_RATE:f64=0.9;
/// A portaria segura muito: o nível pode descer, para o agente planejar mais.
const LOWER_HELD_RATE:f64=0.4;

pub fn gate_history(connection:&Connection,days:i64)->Result<GateHistory> {
    let since=(chrono::Utc::now()-chrono::Duration::days(days)).to_rfc3339();
    let mut statement=connection.prepare("SELECT verdict,COUNT(*) FROM entry_checks WHERE at>=?1 GROUP BY verdict")?;
    let mut history=GateHistory::default();
    for row in statement.query_map([since],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u32>(1)?)))? {
        let (verdict,count)=row?;
        history.checks+=count;
        match verdict.as_str() { "pass"=>history.passed+=count, "ask"=>history.asked+=count, "block"=>history.blocked+=count, _=>{} }
    }
    Ok(history)
}

/// O nível que o histórico da portaria sugere, um degrau acima ou abaixo do
/// atual — ou nenhum. É só sugestão: quem troca é o desenvolvedor.
pub fn suggestion(current:Expertise,history:&GateHistory)->Option<Expertise> {
    if history.checks<SUGGESTION_CHECKS { return None; }
    let index=Expertise::ALL.iter().position(|level|*level==current)?;
    if history.pass_rate()>=RAISE_PASS_RATE { return Expertise::ALL.get(index+1).copied(); }
    if history.held_rate()>=LOWER_HELD_RATE { return index.checked_sub(1).map(|lower|Expertise::ALL[lower]); }
    None
}

#[cfg(test)]
mod tests {
    #[test] fn the_gate_history_suggests_one_step_and_only_with_enough_requests() {
        let history=|passed,asked,blocked|GateHistory{checks:passed+asked+blocked,passed,asked,blocked};
        assert_eq!(suggestion(Expertise::Mid,&history(19,0,0)),None,"pouco histórico não sugere");
        assert_eq!(suggestion(Expertise::Mid,&history(19,1,0)),Some(Expertise::Senior));
        assert_eq!(suggestion(Expertise::Architect,&history(30,0,0)),None,"não há acima do topo");
        assert_eq!(suggestion(Expertise::Mid,&history(10,6,4)),Some(Expertise::Junior));
        assert_eq!(suggestion(Expertise::Starter,&history(5,10,10)),None);
        assert_eq!(suggestion(Expertise::Mid,&history(16,3,1)),None,"no meio, fica");
    }

    #[test] fn the_history_counts_only_the_window() {
        let connection=Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE entry_checks(turn_id TEXT PRIMARY KEY,at TEXT NOT NULL,verdict TEXT NOT NULL);").unwrap();
        let now=chrono::Utc::now();
        for (id,age,verdict) in [("a",1,"pass"),("b",2,"ask"),("c",3,"block"),("d",90,"pass")] {
            connection.execute("INSERT INTO entry_checks VALUES(?1,?2,?3)",params![id,(now-chrono::Duration::days(age)).to_rfc3339(),verdict]).unwrap();
        }
        assert_eq!(gate_history(&connection,30).unwrap(),GateHistory{checks:3,passed:1,asked:1,blocked:1});
    }

    use super::*;

    #[test] fn mid_is_what_the_gate_always_did() {
        let base=JevParameters::default();
        assert_eq!(Expertise::default(),Expertise::Mid);
        assert_eq!(Expertise::Mid.gate(&base),base);
        assert_eq!(Expertise::Mid.confidence(0.7),0.7);
    }

    #[test] fn the_level_moves_the_question_band_not_the_block_line() {
        let base=JevParameters::default();
        let starter=Expertise::Starter.gate(&base);
        let architect=Expertise::Architect.gate(&base);
        for size in 0..3 {
            assert!(starter.scope_demand[size]>base.scope_demand[size] && architect.scope_demand[size]<base.scope_demand[size]);
            let block=|parameters:&JevParameters|((parameters.scope_demand[size]-parameters.block_margin)*1000.0).round();
            assert_eq!(block(&starter),block(&base),"a linha do bloqueio não anda");
            assert_eq!(block(&architect),block(&base));
        }
    }

    #[test] fn each_level_builds_up_to_its_ceiling() {
        assert!(Expertise::Starter.builds("simple") && !Expertise::Starter.builds("medium"));
        assert!(Expertise::Mid.builds("medium") && !Expertise::Mid.builds("complex"));
        assert!(Expertise::Architect.builds("complex"));
        assert!(!Expertise::Architect.builds("whatever"));
        let thresholds=Expertise::ALL.map(|level|level.destructive_threshold());
        assert!(thresholds.windows(2).all(|pair|pair[0]<pair[1]),"quem sabe mais é avisado mais tarde");
    }

    #[test] fn the_choice_is_saved_and_an_unknown_value_falls_back_to_mid() {
        let connection=Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        assert_eq!(load(&connection).unwrap(),Expertise::Mid);
        assert_eq!(save(&connection,"architect").unwrap(),Expertise::Architect);
        assert_eq!(load(&connection).unwrap(),Expertise::Architect);
        assert!(save(&connection,"guru").is_err());
        connection.execute("UPDATE account_settings SET value='guru'",[]).unwrap();
        assert_eq!(load(&connection).unwrap(),Expertise::Mid);
    }
}
