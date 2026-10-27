//! `jayv run` sem interface, para CI: o pedido passa pela portaria como no
//! app, o agente trabalha em build, o validador roda as etapas do projeto
//! (testes, lint, typecheck e build) e o reparo reenvia a falha ao agente até
//! o limite. O processo sai com um código que o pipeline entende e grava um
//! resumo em Markdown para o PR (e, no GitHub Actions, no resumo do job).
//!
//! Códigos de saída:
//! - 0: validação passou (ou nada falhou, quando o teste não é exigido);
//! - 1: o agente falhou;
//! - 2: a validação falhou depois dos reparos;
//! - 3: a portaria barrou o pedido;
//! - 4: a portaria precisa de resposta (rode com `--yes` para confirmar);
//! - 5: nada falhou, mas não havia teste para provar (`--require-tests`).

use crate::{bench::{self, Tally}, gatekeeper::EntryVerdict, orchestrator::Orchestrator, progress::Pulse, turns::{Turn, TurnStatus}, usage::Entry, validator::{self, Manual, Outcome, Verdict}};
use serde::Serialize;
use std::{path::Path, process::Command};

/// Como o `jayv run` roda.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Rodar o validador depois do agente.
    pub validate: bool,
    /// Reparos depois de uma validação que falhou.
    pub max_repairs: u32,
    /// Confirmar o "perguntar" da portaria, como quem responde "pode seguir".
    pub assume_yes: bool,
    /// Sem teste para provar, sair com 5 em vez de 0.
    pub require_tests: bool,
    /// Comandos do validador escolhidos na linha de comando (vazio: detectar).
    pub manual: Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit { Passed, AgentFailed, ValidationFailed, Blocked, NeedsAnswer, Unproven }

impl Exit {
    pub fn code(self)->i32 { match self { Self::Passed=>0, Self::AgentFailed=>1, Self::ValidationFailed=>2, Self::Blocked=>3, Self::NeedsAnswer=>4, Self::Unproven=>5 } }
}

/// Tudo o que aconteceu, para o resumo e o JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub request: String,
    pub gate: String,
    /// O texto da portaria quando ela barrou ou perguntou.
    pub gate_reply: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub answer: Option<String>,
    pub error: Option<String>,
    /// Uma rodada do validador por tentativa (a primeira e cada reparo).
    pub rounds: Vec<Vec<Outcome>>,
    pub verdict: Option<Verdict>,
    pub repairs: u32,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Só quando toda chamada informou o custo.
    pub cost_usd: Option<f64>,
    pub changed_files: Vec<String>,
    pub exit: Exit,
}

impl Report {
    fn new(request:&str)->Self {
        Self{request:request.into(),gate:"pass".into(),gate_reply:None,agent:None,model:None,answer:None,error:None,rounds:vec![],verdict:None,repairs:0,calls:0,input_tokens:0,output_tokens:0,cost_usd:None,changed_files:vec![],exit:Exit::Passed}
    }
    fn spend(&mut self,tally:&Tally) {
        self.calls=tally.calls; self.input_tokens=tally.input_tokens; self.output_tokens=tally.output_tokens; self.cost_usd=tally.priced();
    }
}

/// Roda o pedido de ponta a ponta na pasta `root`.
pub async fn run(orchestrator:&mut Orchestrator,request:&str,root:&Path,options:&Options)->Report {
    let mut report=Report::new(request);
    let mut agent=Tally::default();
    let session=format!("ci-{}",uuid::Uuid::new_v4().simple());
    let turn=Turn{id:format!("{session}-1"),chat_id:session.clone(),code:"CI·01".into(),ordinal:1,status:TurnStatus::Flying,created_at:chrono::Utc::now()};

    let (check,_)=bench::collect(bench::gate(orchestrator,&turn,request,&session)).await;
    let mut check=check;
    match check.verdict {
        EntryVerdict::Block=>{
            report.gate="block".into();
            report.gate_reply=Some(gate_text(&check));
            report.exit=Exit::Blocked;
            return finish(report,root);
        }
        EntryVerdict::Ask if !options.assume_yes=>{
            report.gate="ask".into();
            report.gate_reply=Some(gate_text(&check));
            report.exit=Exit::NeedsAnswer;
            return finish(report,root);
        }
        EntryVerdict::Ask=>{ report.gate="confirmed".into(); check=check.confirmed(); }
        EntryVerdict::Pass=>{}
    }

    orchestrator.pending_gate_passed=Some(check.verdict==EntryVerdict::Pass);
    orchestrator.pending_brief=check.refined_prompt(request);
    orchestrator.pending_work_mode=Some(crate::workspace::MODE_BUILD.into());
    let (result,entries)=bench::collect(orchestrator.process(request,Some(&session),&Pulse::silent())).await;
    add(&mut agent,&entries);
    report.agent=Some(result.model_selection.provider.clone());
    report.model=Some(result.model_selection.model_name.clone());
    match result.result {
        Some(response)=>report.answer=Some(response.response),
        None=>{
            report.error=Some(result.error.unwrap_or_else(||"the agent did not answer".into()));
            report.exit=Exit::AgentFailed;
            report.spend(&agent);
            return finish(report,root);
        }
    }

    if options.validate {
        loop {
            let outcomes=validate(root,&options.manual).await;
            let verdict=validator::verdict(&outcomes);
            let repairable=verdict==Verdict::Failed&&validator::repairable(&outcomes);
            report.rounds.push(outcomes.clone());
            report.verdict=Some(verdict);
            if !repairable||report.repairs>=options.max_repairs { break; }
            report.repairs+=1;
            let prompt=validator::repair_prompt(&outcomes,report.repairs,options.max_repairs);
            orchestrator.pending_gate_passed=Some(true);
            orchestrator.pending_work_mode=Some(crate::workspace::MODE_BUILD.into());
            let (result,entries)=bench::collect(orchestrator.process(&prompt,Some(&session),&Pulse::silent())).await;
            add(&mut agent,&entries);
            if result.result.is_none() { break; }
        }
        report.exit=match report.verdict {
            Some(Verdict::Failed)=>Exit::ValidationFailed,
            Some(Verdict::Unproven) if options.require_tests=>Exit::Unproven,
            _=>Exit::Passed,
        };
    }
    orchestrator.memory.clear_session(&session);
    report.spend(&agent);
    finish(report,root)
}

/// Por que a portaria segurou o pedido, para quem lê o log do CI.
fn gate_text(check:&crate::gatekeeper::EntryCheck)->String {
    let missing=check.failing().iter().map(|criterion|criterion.id.replace('_'," ")).collect::<Vec<_>>();
    let mut text=format!("Score {} out of 100; this request needs {}.",check.score,check.demand);
    if !missing.is_empty() { text.push_str(&format!(" Missing: {}.",missing.join(", "))); }
    text.push_str(" Say what to change, where, and how to know it is done.");
    text
}

fn add(tally:&mut Tally,entries:&[Entry]) {
    // O Jev sai de outra cota: o custo do CI é o do agente.
    for entry in entries { if let Entry::Spend(_,spend)=entry { if !spend.source.starts_with("jev") { tally.add(spend); } } }
}

/// O validador do app na pasta, sem política de organização: no CI, quem
/// decide o que roda é o pipeline.
async fn validate(root:&Path,manual:&Manual)->Vec<Outcome> {
    let plan=validator::plan(root,manual);
    let pulse=Pulse::silent();
    let mut outcomes=Vec::new();
    for step in &plan.steps {
        outcomes.push(validator::run_step(root,step,&validator::Open,&|text:&str|text.to_string(),pulse.stop(),validator::STEP_TIMEOUT).await);
    }
    outcomes
}

fn finish(mut report:Report,root:&Path)->Report {
    report.changed_files=changed_files(root);
    report
}

/// Os arquivos que mudaram na pasta, pelo git (vazio fora de um repositório).
fn changed_files(root:&Path)->Vec<String> {
    let Ok(output)=Command::new("git").args(["status","--porcelain","--untracked-files=all"]).current_dir(root).output() else { return vec![] };
    if !output.status.success() { return vec![]; }
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|line|line.get(3..)).map(|path|path.trim().to_string()).filter(|path|!path.is_empty()&&!path.ends_with(".jev_performance.json")).collect()
}

/// O resumo para o PR, em Markdown e em inglês (o CI não tem idioma).
pub fn summary(report:&Report)->String {
    let icon=match report.exit { Exit::Passed=>"✅", Exit::Unproven=>"⚪", _=>"❌" };
    let headline=match report.exit {
        Exit::Passed=>match report.verdict { Some(Verdict::Passed)=>"Validation passed".to_string(), Some(Verdict::Unproven)=>"Nothing failed, but no test proved the change".to_string(), _=>"Done (validation skipped)".to_string() },
        Exit::AgentFailed=>"The agent failed".into(),
        Exit::ValidationFailed=>format!("Validation failed after {} repair(s)",report.repairs),
        Exit::Blocked=>"The gate blocked the request".into(),
        Exit::NeedsAnswer=>"The gate needs an answer (run with --yes to confirm)".into(),
        Exit::Unproven=>"No test proved the change".into(),
    };
    let mut lines=vec![format!("## {icon} JayV: {headline}"),String::new(),format!("> {}",report.request.replace('\n',"\n> ")),String::new()];
    let mut facts=vec![format!("**Gate:** {}",report.gate)];
    if let (Some(agent),Some(model))=(&report.agent,&report.model) { facts.push(format!("**Agent:** {agent} · {model}")); }
    facts.push(format!("**Repairs:** {}",report.repairs));
    let cost=report.cost_usd.map_or_else(||"not reported".to_string(),|cost|format!("${cost:.4}"));
    facts.push(format!("**Usage:** {} calls · {} input / {} output tokens · {cost}",report.calls,report.input_tokens,report.output_tokens));
    lines.push(facts.join(" · "));
    if let Some(reply)=&report.gate_reply { lines.push(String::new()); lines.push(reply.clone()); }
    if let Some(error)=&report.error { lines.push(String::new()); lines.push(format!("**Error:** {error}")); }
    if let Some(last)=report.rounds.last() {
        lines.push(String::new());
        lines.push("| Step | Command | Result | Time |".into());
        lines.push("|---|---|---|---|".into());
        for outcome in last {
            lines.push(format!("| {} | `{}` | {} | {:.1}s |",outcome.kind.as_str(),outcome.command.replace('|',"\\|"),outcome.status.as_str(),outcome.millis as f64/1000.0));
        }
        for outcome in last.iter().filter(|outcome|outcome.status.broke()) {
            lines.push(String::new());
            lines.push(format!("<details><summary>{} output</summary>\n\n```\n{}\n```\n</details>",outcome.kind.as_str(),outcome.tail));
        }
    }
    if !report.changed_files.is_empty() {
        lines.push(String::new());
        lines.push(format!("<details><summary>{} changed file(s)</summary>\n",report.changed_files.len()));
        for file in &report.changed_files { lines.push(format!("- `{file}`")); }
        lines.push("</details>".into());
    }
    lines.join("\n")+"\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use validator::{Kind, Status};

    fn outcome(kind:Kind,status:Status)->Outcome { Outcome{kind,command:format!("run {}",kind.as_str()),status,exit_code:Some(if status==Status::Passed {0} else {1}),millis:1500,tail:"boom".into(),blocked_by:vec![]} }

    #[test] fn exit_codes_are_stable() {
        let codes=[Exit::Passed,Exit::AgentFailed,Exit::ValidationFailed,Exit::Blocked,Exit::NeedsAnswer,Exit::Unproven].map(Exit::code);
        assert_eq!(codes,[0,1,2,3,4,5]);
    }

    #[test] fn the_summary_shows_the_last_round_and_the_failing_output() {
        let mut report=Report::new("fix the | parser");
        report.agent=Some("claude".into()); report.model=Some("sonnet".into());
        report.rounds=vec![vec![outcome(Kind::Test,Status::Failed)],vec![outcome(Kind::Test,Status::Failed),outcome(Kind::Lint,Status::Passed)]];
        report.verdict=Some(Verdict::Failed); report.repairs=1; report.exit=Exit::ValidationFailed;
        report.changed_files=vec!["src/parser.rs".into()];
        let text=summary(&report);
        assert!(text.starts_with("## ❌ JayV: Validation failed after 1 repair(s)"));
        assert!(text.contains("| lint | `run lint` | passed | 1.5s |"));
        assert_eq!(text.matches("<summary>test output</summary>").count(),1,"só a última rodada");
        assert!(text.contains("- `src/parser.rs`"));
        assert!(text.contains("not reported"));
    }

    #[test] fn a_blocked_request_explains_why() {
        let mut report=Report::new("do stuff");
        report.gate="block".into(); report.gate_reply=Some("Too vague.".into()); report.exit=Exit::Blocked;
        let text=summary(&report);
        assert!(text.contains("The gate blocked the request")&&text.contains("Too vague."));
    }

    #[test] fn changed_files_come_from_git() {
        let dir=tempfile::tempdir().expect("pasta");
        assert!(changed_files(dir.path()).is_empty(),"fora do git, nada");
        let git=|args:&[&str]|Command::new("git").args(args).current_dir(dir.path()).output().expect("git");
        git(&["init","-q"]);
        std::fs::write(dir.path().join("new.txt"),"x").expect("arquivo");
        assert_eq!(changed_files(dir.path()),vec!["new.txt".to_string()]);
    }
}
