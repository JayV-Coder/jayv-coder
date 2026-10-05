//! A medida que o JayV deve a quem o usa: quantos pedidos e quantos dólares
//! até a tarefa ficar pronta, com o JayV e com o agente sozinho.
//!
//! Cada tarefa traz o roteiro de um iniciante (o pedido vago, o "não
//! funcionou", o pedido mais preciso) e um comando que diz se ela ficou
//! pronta. Os dois lados rodam o mesmo roteiro, um pedido por volta, até o
//! comando passar ou as voltas acabarem:
//!
//! - cada lado numa cópia própria do projeto (`git worktree` quando é um
//!   repositório, cópia da pasta quando não é): um não encontra o que o outro
//!   fez, nem o cache de prompt aquecido por ele;
//! - a ordem alterna de uma tarefa para a outra;
//! - o lado do JayV passa pela portaria como no app — a continuação herda o
//!   veredito, o barrado conta a volta sem chamar agente, e o "perguntar" é
//!   confirmado com o pedido reescrito (contado à parte);
//! - o veredito sai do custo em dólar que os agentes informam; sem ele, dos
//!   tokens ponderados (o cache lido custa um décimo).

use crate::{gatekeeper::{self, EntryVerdict}, orchestrator::Orchestrator, parallel, progress::Pulse, turns::{Turn, TurnStatus}, usage::{self, Entry, Scope, Spend}};
use anyhow::{anyhow, bail, Context as _, Result};
use serde::Deserialize;
use std::{path::{Path, PathBuf}, process::Command, time::Duration};

/// Quantas voltas uma tarefa tem quando o arquivo não diz.
const DEFAULT_ROUNDS:usize=5;
/// Quanto tempo o comando de verificação pode levar.
const VERIFY_TIMEOUT:Duration=Duration::from_secs(600);
/// O peso do token lido do cache no veredito sem custo em dólar.
const CACHE_READ_WEIGHT:f64=0.1;
/// O peso do token escrito no cache, idem.
const CACHE_WRITE_WEIGHT:f64=1.25;

/// O arquivo de tarefas (YAML).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Suite {
    /// A pasta do projeto, relativa ao arquivo. Sem ela, a raiz do `--root`.
    #[serde(default)] pub repository: Option<PathBuf>,
    pub tasks: Vec<Task>,
}

/// Uma tarefa: o roteiro do iniciante e o comando que diz se ficou pronta.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Task {
    pub id: String,
    /// Os pedidos, um por volta. Acabando o roteiro antes das voltas, o último
    /// se repete — é o que um iniciante faz.
    pub script: Vec<String>,
    /// Roda na pasta da cópia (`sh -c` ou `cmd /C`); saída 0 é pronta.
    pub verify: String,
    #[serde(default = "default_rounds")] pub max_rounds: usize,
}

fn default_rounds()->usize { DEFAULT_ROUNDS }

/// Lê o arquivo de tarefas e devolve também a pasta do projeto.
pub fn load(path:&Path,root:&Path)->Result<(Suite,PathBuf)> {
    let text=std::fs::read_to_string(path).with_context(||format!("cannot read {}",path.display()))?;
    let suite:Suite=serde_yaml::from_str(&text).with_context(||format!("{} is not a task file",path.display()))?;
    if suite.tasks.is_empty() { bail!("{} has no tasks",path.display()); }
    for task in &suite.tasks {
        if task.script.iter().all(|prompt|prompt.trim().is_empty()) { bail!("task `{}` has no requests",task.id); }
        if task.verify.trim().is_empty() { bail!("task `{}` has no verify command",task.id); }
        if task.max_rounds==0 { bail!("task `{}` has no rounds",task.id); }
    }
    let project=match &suite.repository { Some(folder)=>path.parent().unwrap_or(Path::new(".")).join(folder), None=>root.to_path_buf() };
    let project=project.canonicalize().with_context(||format!("the project folder {} does not exist",project.display()))?;
    Ok((suite,project))
}

/// O que um lado gastou.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tally { pub calls: u64, pub input_tokens: u64, pub cache_read_tokens: u64, pub cache_write_tokens: u64, pub output_tokens: u64, pub cost_usd: Option<f64>, pub unpriced: u64, pub failures: u64 }

impl Tally {
    fn add(&mut self, spend:&Spend) {
        self.calls+=1;
        self.input_tokens+=spend.input_tokens;
        self.cache_read_tokens+=spend.cache_read_tokens;
        self.cache_write_tokens+=spend.cache_write_tokens;
        self.output_tokens+=spend.output_tokens;
        match spend.cost_usd { Some(cost)=>self.cost_usd=Some(self.cost_usd.unwrap_or(0.0)+cost), None=>self.unpriced+=1 }
        if !spend.success { self.failures+=1; }
    }
    fn merge(&mut self, other:&Tally) {
        self.calls+=other.calls; self.input_tokens+=other.input_tokens; self.cache_read_tokens+=other.cache_read_tokens;
        self.cache_write_tokens+=other.cache_write_tokens; self.output_tokens+=other.output_tokens; self.unpriced+=other.unpriced; self.failures+=other.failures;
        if let Some(cost)=other.cost_usd { self.cost_usd=Some(self.cost_usd.unwrap_or(0.0)+cost); }
    }
    /// O custo em dólar, só quando toda chamada o informou.
    pub fn priced(&self)->Option<f64> { if self.unpriced==0 { self.cost_usd.or(Some(0.0)) } else { None } }
    /// Os tokens com o peso do que eles custam: o lido do cache, um décimo.
    pub fn weighted_tokens(&self)->f64 { self.input_tokens as f64+self.output_tokens as f64+self.cache_read_tokens as f64*CACHE_READ_WEIGHT+self.cache_write_tokens as f64*CACHE_WRITE_WEIGHT }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side { Jayv, Direct }

impl Side {
    fn name(&self)->&'static str { match self { Self::Jayv=>"jayv", Self::Direct=>"direct" } }
}

/// Um lado numa tarefa: as voltas que deu, se chegou lá e quanto custou.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub side: Side,
    pub rounds: usize,
    pub passed: bool,
    /// O que o agente gastou.
    pub agent: Tally,
    /// O que o Jev gastou (só o lado do JayV): sai de outra cota.
    pub jev: Tally,
    pub resumed: u64,
    pub blocked: usize,
    /// Os "perguntar" confirmados com o pedido reescrito.
    pub confirmations: usize,
}

impl Run {
    fn new(side:Side)->Self { Self{side,rounds:0,passed:false,agent:Tally::default(),jev:Tally::default(),resumed:0,blocked:0,confirmations:0} }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResult { pub id: String, pub first: Side, pub jayv: Run, pub direct: Run }

/// Roda todas as tarefas pelos dois lados.
pub async fn run(orchestrator:&mut Orchestrator,suite:&Suite,project:&Path)->Result<Vec<TaskResult>> {
    let mut results=Vec::new();
    for (index,task) in suite.tasks.iter().enumerate() {
        let order=if index%2==0 { [Side::Jayv,Side::Direct] } else { [Side::Direct,Side::Jayv] };
        let mut runs=Vec::new();
        for side in order {
            let copy=Copy::make(project)?;
            orchestrator.focus_on(copy.project())?;
            let session=format!("bench-{}-{}-{}",task.id,side.name(),uuid::Uuid::new_v4().simple());
            let run=match side { Side::Jayv=>run_jayv(orchestrator,task,&session,copy.project()).await?, Side::Direct=>run_direct(orchestrator,task,&session,copy.project()).await? };
            orchestrator.memory.clear_session(&session);
            runs.push(run);
        }
        let take=|side:Side,runs:&mut Vec<Run>|{ let at=runs.iter().position(|run|run.side==side).expect("os dois lados rodaram"); runs.remove(at) };
        let jayv=take(Side::Jayv,&mut runs);
        let direct=take(Side::Direct,&mut runs);
        results.push(TaskResult{id:task.id.clone(),first:order[0],jayv,direct});
    }
    Ok(results)
}

/// O pedido da volta: o do roteiro, ou o último quando o roteiro acabou.
fn prompt_at(task:&Task,round:usize)->&str {
    let prompts=task.script.iter().filter(|prompt|!prompt.trim().is_empty()).collect::<Vec<_>>();
    prompts.get(round).or(prompts.last()).map_or("",|prompt|prompt.as_str())
}

/// O lado do JayV, com a portaria como no app.
async fn run_jayv(orchestrator:&mut Orchestrator,task:&Task,session:&str,folder:&Path)->Result<Run> {
    let mut run=Run::new(Side::Jayv);
    let mut previous:Option<EntryVerdict>=None;
    for round in 0..task.max_rounds {
        let prompt=prompt_at(task,round).to_string();
        run.rounds=round+1;
        let turn=Turn{id:format!("{session}-{round}"),chat_id:session.into(),code:format!("BENCH·{:02}",round+1),ordinal:round as u32+1,status:TurnStatus::Flying,created_at:chrono::Utc::now()};
        let (check,spends)=collect(gate(orchestrator,&turn,&prompt,session)).await;
        tally(&mut run,&spends);
        let mut check=check;
        if gatekeeper::is_continuation(&prompt) { if let Some(origin)=previous { check=check.inherit(origin); } }
        match check.verdict {
            EntryVerdict::Block=>{
                // Barrado não chama agente: a volta conta e o roteiro segue.
                run.blocked+=1;
                orchestrator.memory.add_message(session,"user",prompt.clone());
                orchestrator.memory.add_message(session,"assistant",crate::i18n::for_model(&check.reply()));
                continue;
            }
            EntryVerdict::Ask=>{ run.confirmations+=1; check=check.confirmed(); }
            EntryVerdict::Pass=>{}
        }
        previous=Some(check.verdict);
        if crate::router::is_complaint(&prompt) { orchestrator.mark_last_failed(session); orchestrator.pending_retry=true; }
        orchestrator.pending_gate_passed=Some(check.verdict==EntryVerdict::Pass);
        orchestrator.pending_brief=check.refined_prompt(&prompt);
        let (result,spends)=collect(orchestrator.process(&prompt,Some(session),&Pulse::silent())).await;
        tally(&mut run,&spends);
        if result.result.is_none() { eprintln!("bench: {} volta {} falhou ({})",task.id,round+1,result.error.unwrap_or_default()); }
        if verify(&task.verify,folder)? { run.passed=true; break; }
    }
    Ok(run)
}

/// A portaria do pedido: o Jev quando há sessão, as heurísticas quando não.
async fn gate(orchestrator:&Orchestrator,turn:&Turn,prompt:&str,session:&str)->gatekeeper::EntryCheck {
    let level=orchestrator.expertise;
    if crate::jev::is_configured() {
        let project=orchestrator.rag.project_info();
        match gatekeeper::evaluate_entry(prompt,&project.name,&project.languages,&orchestrator.recent_turns_ahead(session)).await {
            Ok(reading)=>return gatekeeper::judge_for(turn,prompt,&reading,"jev",level),
            Err(error)=>eprintln!("bench: o Jev não respondeu, usando heurísticas locais ({error})"),
        }
    }
    gatekeeper::judge_for(turn,prompt,&gatekeeper::heuristic_entry(prompt),crate::asking::LOCAL_SOURCE,level)
}

/// O lado direto: o mesmo roteiro mandado cru ao agente, com a sessão dele
/// seguindo de um pedido para o outro.
async fn run_direct(orchestrator:&mut Orchestrator,task:&Task,session:&str,folder:&Path)->Result<Run> {
    let mut run=Run::new(Side::Direct);
    let selection=orchestrator.build_selection();
    if selection.provider=="jev" { bail!("no configured agent can run the direct side"); }
    for round in 0..task.max_rounds {
        let prompt=prompt_at(task,round).to_string();
        run.rounds=round+1;
        let (result,spends)=collect(orchestrator.direct(&prompt,session,&selection)).await;
        tally(&mut run,&spends);
        if let Err(error)=result { eprintln!("bench: {} volta {} do lado direto falhou ({error:#})",task.id,round+1); }
        if verify(&task.verify,folder)? { run.passed=true; break; }
    }
    Ok(run)
}

fn tally(run:&mut Run,entries:&[Entry]) {
    for entry in entries {
        match entry {
            Entry::Spend(_,spend) if spend.source.starts_with("jev")=>run.jev.add(spend),
            Entry::Spend(_,spend)=>run.agent.add(spend),
            Entry::Jev(_,mark) if mark.kind=="session_resumed"=>run.resumed+=mark.amount as u64,
            _=>{}
        }
    }
}

/// Roda `work` com uma pia só dele e devolve o que foi gasto lá dentro.
async fn collect<F:std::future::Future>(work:F)->(F::Output,Vec<Entry>) {
    let (sink,mut received)=tokio::sync::mpsc::unbounded_channel();
    let output=usage::within_sink(Scope::default(),sink,work).await;
    let mut entries=vec![];
    while let Ok(entry)=received.try_recv() { entries.push(entry); }
    (output,entries)
}

/// Roda o comando de verificação na cópia. Saída 0 é tarefa pronta.
fn verify(command:&str,folder:&Path)->Result<bool> {
    let mut process=if cfg!(windows) { let mut process=Command::new("cmd"); process.args(["/C",command]); process } else { let mut process=Command::new("sh"); process.args(["-c",command]); process };
    let mut child=process.current_dir(folder).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().with_context(||format!("cannot run `{command}`"))?;
    let started=std::time::Instant::now();
    loop {
        if let Some(status)=child.try_wait()? { return Ok(status.success()); }
        if started.elapsed()>VERIFY_TIMEOUT { let _=child.kill(); return Ok(false); }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Uma cópia do projeto, só de um lado de uma tarefa. Some ao sair de cena.
struct Copy { dir:PathBuf, project:PathBuf, top:Option<PathBuf> }

impl Copy {
    fn make(project:&Path)->Result<Self> {
        let dir=parallel::worktree_dir(0);
        if let Some((top,prefix))=parallel::repository(project) {
            let base=parallel::base(&top)?;
            parallel::add_worktree(&top,&base,&dir)?;
            return Ok(Self{project:dir.join(prefix),dir,top:Some(top)});
        }
        copy_folder(project,&dir)?;
        Ok(Self{project:dir.clone(),dir,top:None})
    }
    fn project(&self)->&Path { &self.project }
}

impl Drop for Copy {
    fn drop(&mut self) {
        match &self.top { Some(top)=>parallel::remove_worktree(top,&self.dir), None=>{ let _=std::fs::remove_dir_all(&self.dir); } }
    }
}

fn copy_folder(from:&Path,to:&Path)->Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry=entry?;
        let relative=entry.path().strip_prefix(from).map_err(|error|anyhow!("{error}"))?;
        let target=to.join(relative);
        if entry.file_type().is_dir() { std::fs::create_dir_all(&target)?; } else if entry.file_type().is_file() { std::fs::copy(entry.path(),&target)?; }
    }
    Ok(())
}

/// O relatório para o terminal.
pub fn report(results:&[TaskResult])->String {
    let mut lines=vec![format!("{:<24} {:<7} {:>6} {:>6} {:>8} {:>12} {:>10} {:>7}","task","side","rounds","done","resumed","tokens*","usd","asked")];
    let usd=|tally:&Tally|tally.priced().map_or("-".into(),|cost|format!("{cost:.4}"));
    let row=|id:&str,run:&Run,first:bool|format!("{:<24} {:<7} {:>6} {:>6} {:>8} {:>12.0} {:>10} {:>7}",id,format!("{}{}",run.side.name(),if first {"¹"} else {""}),run.rounds,if run.passed {"yes"} else {"no"},run.resumed,run.agent.weighted_tokens(),usd(&run.agent),if run.side==Side::Jayv { format!("{}/{}",run.confirmations,run.blocked) } else { "-".into() });
    let (mut jayv,mut direct,mut jev)=(Tally::default(),Tally::default(),Tally::default());
    let (mut jayv_rounds,mut direct_rounds,mut jayv_done,mut direct_done)=(0,0,0,0);
    for result in results {
        lines.push(row(&result.id,&result.jayv,result.first==Side::Jayv));
        lines.push(row(&result.id,&result.direct,result.first==Side::Direct));
        jayv.merge(&result.jayv.agent); direct.merge(&result.direct.agent); jev.merge(&result.jayv.jev);
        jayv_rounds+=result.jayv.rounds; direct_rounds+=result.direct.rounds;
        jayv_done+=usize::from(result.jayv.passed); direct_done+=usize::from(result.direct.passed);
    }
    let total=results.len();
    lines.push(String::new());
    lines.push(format!("Done: JayV {jayv_done}/{total} in {jayv_rounds} rounds; direct {direct_done}/{total} in {direct_rounds} rounds."));
    match (jayv.priced(),direct.priced()) {
        (Some(mine),Some(theirs)) if theirs>0.0=>lines.push(format!("Cost: JayV ${mine:.4} vs direct ${theirs:.4} ({:.1}% of the direct agent).",mine*100.0/theirs)),
        _=>{ let (mine,theirs)=(jayv.weighted_tokens(),direct.weighted_tokens()); if theirs>0.0 { lines.push(format!("Cost: not every call reported dollars; weighted tokens JayV {mine:.0} vs direct {theirs:.0} ({:.1}%).",mine*100.0/theirs)); } }
    }
    lines.push(format!("Jev (paid by JayV, not your quota): {} calls, {} input tokens.",jev.calls,jev.input_tokens));
    lines.push("¹ ran first. tokens* = input + output + cache read × 0.1 + cache write × 1.25. asked = confirmed/blocked at the gate.".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Precision;

    #[test] fn the_verdict_uses_dollars_and_weighs_the_cache_read_at_a_tenth() {
        let mut priced=Tally::default();
        priced.add(&Spend{input_tokens:100,cache_read_tokens:1_000,output_tokens:50,cost_usd:Some(0.02),..Spend::new("claude","sonnet",Precision::Reported)});
        assert_eq!(priced.priced(),Some(0.02));
        assert_eq!(priced.weighted_tokens(),250.0,"o cache lido custa um décimo");
        priced.add(&Spend{input_tokens:10,..Spend::new("codex","gpt",Precision::Estimated)});
        assert_eq!(priced.priced(),None,"sem o dólar de todas as chamadas, não há custo para comparar");
    }

    #[test] fn a_task_file_needs_requests_a_check_and_rounds() {
        let dir=std::env::temp_dir().join(format!("jayv-bench-{}",uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("project")).unwrap();
        let file=dir.join("tasks.yaml");
        std::fs::write(&file,"repository: project\ntasks:\n  - id: one\n    script: [\"fix it\", \"still fails\"]\n    verify: test -f done\n").unwrap();
        let (suite,project)=load(&file,Path::new(".")).expect("arquivo válido");
        assert_eq!((suite.tasks[0].max_rounds,project.ends_with("project")),(DEFAULT_ROUNDS,true));
        assert_eq!((prompt_at(&suite.tasks[0],0),prompt_at(&suite.tasks[0],1),prompt_at(&suite.tasks[0],4)),("fix it","still fails","still fails"),"o último pedido se repete");
        std::fs::write(&file,"tasks:\n  - id: one\n    script: [\"fix it\"]\n    verify: \"\"\n").unwrap();
        assert!(load(&file,&dir).is_err(),"sem comando de verificação não há o que medir");
        let _=std::fs::remove_dir_all(&dir);
    }

    /// O exemplo que vai com o app abre: dez tarefas, cada uma com roteiro
    /// de iniciante e verificação, apontando para a pasta de exemplo.
    #[test] fn the_example_tasks_load() {
        let file=Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/tasks.yaml");
        let (suite,project)=load(&file,Path::new(".")).expect("o exemplo abre");
        assert_eq!(suite.tasks.len(),10);
        assert!(project.join("tests").is_dir()&&project.join("shop").is_dir());
        for task in &suite.tasks { assert!(task.script.len()>=2&&task.verify.contains("unittest"),"{}",task.id); }
    }

    /// Um agente de mentira que só termina a tarefa na segunda volta: os dois
    /// lados precisam de duas voltas, cada um na sua cópia, e a ordem alterna.
    #[cfg(unix)]
    #[tokio::test]
    async fn each_side_runs_in_its_own_copy_until_the_check_passes() {
        use std::os::unix::fs::PermissionsExt;
        let dir=std::env::temp_dir().join(format!("jayv-bench-{}",uuid::Uuid::new_v4().simple()));
        let project=dir.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("main.py"),"def main():\n    pass\n").unwrap();
        let agent=dir.join("fake-agent.sh");
        std::fs::write(&agent,"#!/bin/sh\ncat >/dev/null\nif [ -f .round ]; then touch done; fi\ntouch .round\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"ok\",\"total_cost_usd\":0.01,\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}'\n").unwrap();
        std::fs::set_permissions(&agent,std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut orchestrator=Orchestrator::new(dir.join("missing.yaml"),project.clone()).unwrap();
        let settings=crate::llm::LlmSettings{
            agents:vec![crate::llm::AgentSettings{id:crate::llm::AgentId::Claude,enabled:true,command:agent.display().to_string(),timeout:30,options:serde_json::json!({})}],
            models:vec![crate::llm::AgentModel{agent:crate::llm::AgentId::Claude,model:"sonnet".into(),enabled:true,capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000}],
        };
        orchestrator.use_llm(&settings);
        let suite=Suite{repository:None,tasks:vec![
            Task{id:"first".into(),script:vec!["add a main function to main.py".into(),"it still fails".into()],verify:"test -f done".into(),max_rounds:3},
            Task{id:"second".into(),script:vec!["create the main function in main.py".into(),"still broken".into()],verify:"test -f done".into(),max_rounds:3},
        ]};
        let results=run(&mut orchestrator,&suite,&project).await.expect("bench");
        assert_eq!(results.iter().map(|result|result.first).collect::<Vec<_>>(),[Side::Jayv,Side::Direct],"a ordem alterna");
        for result in &results {
            for side in [&result.jayv,&result.direct] {
                assert!(side.passed,"{} {:?} chegou lá",result.id,side.side);
                assert_eq!(side.rounds,2,"{} {:?}: cada lado na sua cópia, sem achar o que o outro fez",result.id,side.side);
                assert_eq!(side.agent.priced(),Some(0.02),"o dólar que o agente informou");
            }
        }
        assert!(!project.join("done").exists()&&!project.join(".round").exists(),"o projeto original fica intocado");
        assert!(report(&results).contains("Done: JayV 2/2 in 4 rounds; direct 2/2 in 4 rounds."));
        let _=std::fs::remove_dir_all(&dir);
    }
}
