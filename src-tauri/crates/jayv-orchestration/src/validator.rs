//! O validador: depois de um build, o JayV roda os testes, o lint, a checagem
//! de tipos e o build do projeto e diz o que passou e o que falhou, etapa por
//! etapa. É o que troca "o agente diz que funcionou" por evidência.
//!
//! O comando de cada etapa vem do projeto (`package.json`, `Cargo.toml`,
//! `pyproject.toml`, `go.mod`, `Makefile`) ou do campo manual. Etapa sem
//! comando fica como "sem comando definido", nunca como "passou": um projeto
//! sem teste não ganha selo de testado. Os comandos passam pela política da
//! organização (`Judge`) antes de rodar, e a saída que volta à tela e ao
//! agente sai sem segredos.

use jayv_base::progress::Stop;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, time::{Duration, Instant}};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Quanto cada etapa pode rodar antes de ser encerrada.
pub const STEP_TIMEOUT:Duration=Duration::from_secs(10*60);
/// Quantas tentativas de reparo automático um build pode ter, no máximo.
pub const MAX_REPAIRS:u32=2;
/// O fim da saída que fica na tela e vai ao agente.
const TAIL_CHARS:usize=3_000;
const TAIL_LINES:usize=60;
/// O quanto da saída o JayV guarda em memória enquanto o comando roda.
const KEEP_BYTES:usize=64*1024;
/// O texto do `npm init` para "sem teste": não é teste nenhum.
const NPM_NO_TEST:&str="no test specified";

/// O que uma etapa confere.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind { Test, Lint, Typecheck, Build }

impl Kind {
    pub const ALL:[Kind;4]=[Kind::Test,Kind::Lint,Kind::Typecheck,Kind::Build];
    pub fn as_str(self)->&'static str { match self { Self::Test=>"test", Self::Lint=>"lint", Self::Typecheck=>"typecheck", Self::Build=>"build" } }
}

/// Uma etapa com o comando que a roda.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct Step { pub kind:Kind, pub command:String }

/// Os comandos que a pessoa escreveu para o projeto. Vazio é o que o JayV
/// detecta; `off` pula a etapa.
#[derive(Debug,Clone,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(default)]
pub struct Manual { pub test:String, pub lint:String, pub typecheck:String, pub build:String }

impl Manual {
    fn of(&self,kind:Kind)->&str { match kind { Kind::Test=>&self.test, Kind::Lint=>&self.lint, Kind::Typecheck=>&self.typecheck, Kind::Build=>&self.build } }
    pub fn is_empty(&self)->bool { Kind::ALL.iter().all(|kind|self.of(*kind).trim().is_empty()) }
    /// Sem espaço nas pontas e sem quebra de linha: o campo é um comando só.
    pub fn cleaned(&self)->Self {
        let one=|value:&str|value.lines().map(str::trim).filter(|line|!line.is_empty()).collect::<Vec<_>>().join(" ");
        Self{test:one(&self.test),lint:one(&self.lint),typecheck:one(&self.typecheck),build:one(&self.build)}
    }
}

/// O que o projeto tem para validar: as etapas com comando e os tipos que
/// ficaram sem (`undefined`).
#[derive(Debug,Clone,Default,PartialEq,Eq,Serialize)]
pub struct Plan { pub steps:Vec<Step>, pub undefined:Vec<Kind> }

impl Plan {
    pub fn has_test(&self)->bool { self.steps.iter().any(|step|step.kind==Kind::Test) }
}

fn read(root:&Path,name:&str)->Option<String> { std::fs::read_to_string(root.join(name)).ok() }

/// O gerenciador do Node pelo arquivo de bloqueio.
fn node_runner(root:&Path)->(&'static str,&'static str) {
    if root.join("pnpm-lock.yaml").is_file() { ("pnpm test","pnpm run") }
    else if root.join("yarn.lock").is_file() { ("yarn test","yarn run") }
    else if root.join("bun.lock").is_file()||root.join("bun.lockb").is_file() { ("bun run test","bun run") }
    else { ("npm test","npm run") }
}

fn node_steps(root:&Path,found:&mut Vec<Step>) {
    let Some(manifest)=read(root,"package.json").and_then(|text|serde_json::from_str::<serde_json::Value>(&text).ok()) else { return };
    let scripts=manifest.get("scripts").and_then(|scripts|scripts.as_object());
    let script=|name:&str|scripts.and_then(|scripts|scripts.get(name)).and_then(|value|value.as_str()).filter(|value|!value.trim().is_empty());
    let (test,run)=node_runner(root);
    if script("test").is_some_and(|body|!body.contains(NPM_NO_TEST)) { found.push(Step{kind:Kind::Test,command:test.into()}); }
    if script("lint").is_some() { found.push(Step{kind:Kind::Lint,command:format!("{run} lint")}); }
    if let Some(name)=["typecheck","type-check","check-types","tsc"].into_iter().find(|name|script(name).is_some()) {
        found.push(Step{kind:Kind::Typecheck,command:format!("{run} {name}")});
    } else if root.join("tsconfig.json").is_file() {
        let typescript=["dependencies","devDependencies"].iter().any(|section|manifest.get(section).and_then(|deps|deps.get("typescript")).is_some());
        if typescript {
            let exec=match run { "pnpm run"=>"pnpm exec", "yarn run"=>"yarn", "bun run"=>"bunx", _=>"npx --no-install" };
            found.push(Step{kind:Kind::Typecheck,command:format!("{exec} tsc --noEmit")});
        }
    }
    if script("build").is_some() { found.push(Step{kind:Kind::Build,command:format!("{run} build")}); }
}

fn cargo_steps(root:&Path,found:&mut Vec<Step>) {
    let manifest=if root.join("Cargo.toml").is_file() { "" } else if root.join("src-tauri/Cargo.toml").is_file() { " --manifest-path src-tauri/Cargo.toml" } else { return };
    found.push(Step{kind:Kind::Test,command:format!("cargo test{manifest}")});
    found.push(Step{kind:Kind::Typecheck,command:format!("cargo check{manifest}")});
    if root.join("clippy.toml").is_file()||root.join(".clippy.toml").is_file() { found.push(Step{kind:Kind::Lint,command:format!("cargo clippy{manifest} -- -D warnings")}); }
    found.push(Step{kind:Kind::Build,command:format!("cargo build{manifest}")});
}

/// `python` onde existe; senão `python3`.
fn python() -> &'static str { if jayv_agents::llm::locate("python").is_some() { "python" } else { "python3" } }

fn python_steps(root:&Path,found:&mut Vec<Step>) {
    let project=read(root,"pyproject.toml").unwrap_or_default();
    let setup=read(root,"setup.cfg").unwrap_or_default();
    let pytest=root.join("pytest.ini").is_file()||root.join("conftest.py").is_file()||project.contains("[tool.pytest")||setup.contains("[tool:pytest]")||(!project.is_empty()&&root.join("tests").is_dir());
    if pytest { found.push(Step{kind:Kind::Test,command:format!("{} -m pytest",python())}); }
    if root.join("ruff.toml").is_file()||root.join(".ruff.toml").is_file()||project.contains("[tool.ruff") { found.push(Step{kind:Kind::Lint,command:"ruff check .".into()}); }
    if root.join("mypy.ini").is_file()||project.contains("[tool.mypy") { found.push(Step{kind:Kind::Typecheck,command:"mypy .".into()}); }
}

fn go_steps(root:&Path,found:&mut Vec<Step>) {
    if !root.join("go.mod").is_file() { return; }
    found.push(Step{kind:Kind::Test,command:"go test ./...".into()});
    found.push(Step{kind:Kind::Lint,command:"go vet ./...".into()});
    found.push(Step{kind:Kind::Build,command:"go build ./...".into()});
}

fn make_steps(root:&Path,found:&mut Vec<Step>) {
    let Some(text)=["Makefile","makefile","GNUmakefile"].into_iter().find_map(|name|read(root,name)) else { return };
    let has=|target:&str|text.lines().any(|line|line.strip_prefix(target).is_some_and(|rest|rest.trim_start().starts_with(':')&&!rest.trim_start().starts_with(":=")));
    for (target,kind) in [("test",Kind::Test),("lint",Kind::Lint),("typecheck",Kind::Typecheck),("build",Kind::Build)] {
        if has(target) { found.push(Step{kind,command:format!("make {target}")}); }
    }
}

/// As etapas que o projeto define, sem o campo manual. De cada tipo vale a
/// primeira que aparece: Node, Cargo, Python, Go, Makefile.
pub fn detect(root:&Path)->Plan {
    let mut found=Vec::new();
    node_steps(root,&mut found);
    cargo_steps(root,&mut found);
    python_steps(root,&mut found);
    go_steps(root,&mut found);
    make_steps(root,&mut found);
    let mut steps:Vec<Step>=Vec::new();
    for kind in Kind::ALL { if let Some(step)=found.iter().find(|step|step.kind==kind) { steps.push(step.clone()); } }
    let undefined=Kind::ALL.into_iter().filter(|kind|!steps.iter().any(|step|step.kind==*kind)).collect();
    Plan{steps,undefined}
}

/// O plano do projeto: o que o JayV detecta, com o campo manual por cima.
pub fn plan(root:&Path,manual:&Manual)->Plan {
    let manual=manual.cleaned();
    let detected=detect(root);
    let mut steps=Vec::new();
    let mut undefined=Vec::new();
    for kind in Kind::ALL {
        let typed=manual.of(kind);
        if typed.eq_ignore_ascii_case("off") { continue; }
        if !typed.is_empty() { steps.push(Step{kind,command:typed.into()}); }
        else if let Some(step)=detected.steps.iter().find(|step|step.kind==kind) { steps.push(step.clone()); }
        else { undefined.push(kind); }
    }
    Plan{steps,undefined}
}

/// Como terminou uma etapa.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Status { Passed, Failed, TimedOut, Blocked, Stopped, CouldNotStart }

impl Status {
    pub fn as_str(self)->&'static str { match self { Self::Passed=>"passed", Self::Failed=>"failed", Self::TimedOut=>"timed_out", Self::Blocked=>"blocked", Self::Stopped=>"stopped", Self::CouldNotStart=>"could_not_start" } }
    /// A etapa provou que algo está errado.
    pub fn broke(self)->bool { matches!(self,Self::Failed|Self::TimedOut|Self::CouldNotStart) }
}

/// O resultado de uma etapa: o comando, como terminou e o fim da saída.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Outcome {
    pub kind:Kind,
    pub command:String,
    pub status:Status,
    pub exit_code:Option<i32>,
    pub millis:u64,
    /// O fim da saída (erro e saída juntos), sem segredos nem cores.
    pub tail:String,
    /// Quem bloqueou o comando (a organização), quando `status` é `blocked`.
    #[serde(skip_serializing_if="Vec::is_empty",default)]
    pub blocked_by:Vec<String>,
}

/// O que a rodada concluiu.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Verdict {
    /// Tudo o que rodou passou, e havia teste.
    Passed,
    /// Alguma etapa falhou.
    Failed,
    /// Nada falhou, mas o teste não existe ou não pôde rodar: o resultado não está provado.
    Unproven,
}

impl Verdict {
    pub fn as_str(self)->&'static str { match self { Self::Passed=>"passed", Self::Failed=>"failed", Self::Unproven=>"unproven" } }
}

/// O veredito de uma rodada de etapas.
pub fn verdict(outcomes:&[Outcome])->Verdict {
    if outcomes.iter().any(|outcome|outcome.status.broke()) { return Verdict::Failed; }
    let tested=outcomes.iter().any(|outcome|outcome.kind==Kind::Test&&outcome.status==Status::Passed);
    let incomplete=outcomes.iter().any(|outcome|matches!(outcome.status,Status::Blocked|Status::Stopped));
    if tested&&!incomplete { Verdict::Passed } else { Verdict::Unproven }
}

/// Quem decide se o comando pode rodar: devolve as organizações que o
/// bloqueiam, ou nada se ele está liberado.
pub trait Judge: Sync { fn blocked_by(&self,command:&str)->Vec<String>; }

/// A política da organização do projeto: bloqueia o comando que ela bloqueia
/// para os agentes (comandos, permissões do chat) e diz quem.
impl Judge for jayv_jev::policy::ProjectPolicy {
    fn blocked_by(&self,command:&str)->Vec<String> {
        if self.policy.blocks_command(command) { self.policy.blocking_orgs(command,&self.org_slug) } else { vec![] }
    }
}

/// Sem política: tudo liberado.
pub struct Open;
impl Judge for Open { fn blocked_by(&self,_:&str)->Vec<String> { vec![] } }

/// Os comandos simples de uma linha de shell (`a && b | c` → `a`, `b`, `c`):
/// a política confere cada um, e não só o primeiro.
pub fn segments(command:&str)->Vec<String> {
    let mut parts=vec![String::new()];
    let mut chars=command.chars().peekable();
    let mut quote:Option<char>=None;
    while let Some(c)=chars.next() {
        match (quote,c) {
            (Some(q),c) if c==q=>{ quote=None; parts.last_mut().map(|part|part.push(c)); }
            (Some(_),c)=>{ parts.last_mut().map(|part|part.push(c)); }
            (None,'"'|'\'')=>{ quote=Some(c); parts.last_mut().map(|part|part.push(c)); }
            // `2>&1` e `&>` são redirecionamento, não separador.
            (None,'&')|(None,'|') if parts.last().is_some_and(|part|part.ends_with('>'))||(c=='&'&&chars.peek()==Some(&'>'))=>{ parts.last_mut().map(|part|part.push(c)); }
            (None,';'|'|'|'&'|'\n')=>{ while chars.peek().is_some_and(|next|matches!(next,';'|'|'|'&')) { chars.next(); } parts.push(String::new()); }
            (None,c)=>{ parts.last_mut().map(|part|part.push(c)); }
        }
    }
    parts.into_iter().map(|part|part.trim().to_string()).filter(|part|!part.is_empty()).collect()
}

/// As organizações que bloqueiam algum dos comandos da linha.
pub fn blockers(judge:&dyn Judge,command:&str)->Vec<String> {
    let mut orgs:Vec<String>=Vec::new();
    for part in segments(command) { for org in judge.blocked_by(&part) { if !orgs.contains(&org) { orgs.push(org); } } }
    orgs
}

/// Tira as cores e os controles de terminal.
pub fn plain(text:&str)->String {
    static ANSI:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let ansi=ANSI.get_or_init(||regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07").expect("expressão fixa"));
    ansi.replace_all(text,"").replace('\r',"")
}

/// O fim da saída: as últimas linhas, dentro do teto de caracteres.
pub fn tail_of(text:&str)->String {
    let text=plain(text);
    let lines:Vec<&str>=text.lines().collect();
    let start=lines.len().saturating_sub(TAIL_LINES);
    let joined=lines[start..].join("\n");
    let count=joined.chars().count();
    if count<=TAIL_CHARS { return joined.trim().to_string(); }
    let cut:String=joined.chars().skip(count-TAIL_CHARS).collect();
    format!("…{}",cut.trim())
}

type Kept=std::sync::Arc<std::sync::Mutex<Vec<u8>>>;

async fn drain(mut pipe:impl AsyncRead+Unpin,kept:Kept) {
    let mut buffer=[0u8;8192];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0)|Err(_)=>break,
            Ok(read)=>{
                let mut kept=kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                kept.extend_from_slice(&buffer[..read]);
                // Só o fim interessa: o começo de uma saída enorme sai.
                if kept.len()>KEEP_BYTES*2 { let drop=kept.len()-KEEP_BYTES; kept.drain(..drop); }
            }
        }
    }
}

/// Quanto se espera pelo fim da saída depois que o shell saiu: um servidor
/// deixado em segundo plano segura a ponta aberta para sempre.
const DRAIN_GRACE:Duration=Duration::from_secs(2);

async fn collected(task:tokio::task::JoinHandle<()>,kept:&Kept)->String {
    let mut task=task;
    if tokio::time::timeout(DRAIN_GRACE,&mut task).await.is_err() { task.abort(); }
    String::from_utf8_lossy(&kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned()
}

/// O PATH do app com as pastas dos instaladores (o Node do nvm, o Cargo): um
/// app aberto pelo menu não as enxerga.
fn search_path()->Option<std::ffi::OsString> { jayv_agents::llm::agent_path(Path::new("")) }

fn shell(command:&str)->tokio::process::Command {
    #[cfg(windows)] {
        let mut process=tokio::process::Command::new("cmd");
        process.arg("/C");
        use std::os::windows::process::CommandExt;
        process.raw_arg(command);
        process.creation_flags(0x0800_0000);
        process
    }
    #[cfg(not(windows))] {
        let mut process=tokio::process::Command::new("sh");
        process.arg("-c").arg(command);
        process
    }
}

/// Roda uma etapa na pasta do projeto. O comando só roda se a política o
/// libera; a saída volta sem segredos (`redact`). Parar o pedido encerra a
/// árvore de processos da etapa.
pub async fn run_step(root:&Path,step:&Step,judge:&dyn Judge,redact:&(dyn Fn(&str)->String+Sync),stop:&Stop,timeout:Duration)->Outcome {
    let outcome=|status,exit_code,started:Instant,tail:String,blocked_by|Outcome{kind:step.kind,command:step.command.clone(),status,exit_code,millis:started.elapsed().as_millis() as u64,tail,blocked_by};
    let started=Instant::now();
    let blocked_by=blockers(judge,&step.command);
    if !blocked_by.is_empty() { return outcome(Status::Blocked,None,started,String::new(),blocked_by); }
    if stop.reason().is_some() { return outcome(Status::Stopped,None,started,String::new(),vec![]); }
    let mut process=shell(&step.command);
    process.current_dir(root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true)
        .env("CI","1").env("NO_COLOR","1").env("FORCE_COLOR","0").env("TERM","dumb");
    if let Some(path)=search_path() { process.env("PATH",path); }
    // Num grupo só dele: parar o pedido derruba o shell e tudo o que ele abriu.
    #[cfg(unix)] process.process_group(0);
    let mut child=match process.spawn() {
        Ok(child)=>child,
        Err(error)=>return outcome(Status::CouldNotStart,None,started,tail_of(&error.to_string()),vec![]),
    };
    let mut tree=jayv_agents::providers::Tree::of(&child);
    let (out_kept,err_kept)=(Kept::default(),Kept::default());
    let out=tokio::spawn(drain(child.stdout.take().expect("saída pedida"),out_kept.clone()));
    let err=tokio::spawn(drain(child.stderr.take().expect("erro pedido"),err_kept.clone()));
    enum End { Exited(std::io::Result<std::process::ExitStatus>), TimedOut, Stopped }
    let end=tokio::select! {
        waited=child.wait()=>End::Exited(waited),
        _=tokio::time::sleep(timeout)=>End::TimedOut,
        _=stopped(stop)=>End::Stopped,
    };
    let exited=matches!(end,End::Exited(_));
    if exited { tree.release(); } else { tree.kill(); let _=child.wait().await; }
    let joined=format!("{}{}",collected(out,&out_kept).await,collected(err,&err_kept).await);
    let tail=tail_of(&redact(&joined));
    match end {
        End::Exited(Ok(status))=>outcome(if status.success() {Status::Passed} else {Status::Failed},status.code(),started,tail,vec![]),
        End::Exited(Err(error))=>outcome(Status::CouldNotStart,None,started,tail_of(&error.to_string()),vec![]),
        End::TimedOut=>outcome(Status::TimedOut,None,started,tail,vec![]),
        End::Stopped=>outcome(Status::Stopped,None,started,tail,vec![]),
    }
}

/// Espera o pedido ser parado.
async fn stopped(stop:&Stop) {
    while stop.reason().is_none() { tokio::time::sleep(Duration::from_millis(250)).await; }
}

/// O pedido de reparo: o que falhou, com o fim da saída, e o que o agente
/// pode e não pode fazer. Em inglês, como toda instrução a modelo.
pub fn repair_prompt(outcomes:&[Outcome],attempt:u32,max:u32)->String {
    let mut text=format!("The checks JayV ran after your last change failed (repair attempt {attempt} of {max}). Find the cause in the project and fix it, then stop: JayV runs the checks again. Fix the code, not the checks: do not skip, delete or weaken tests, do not change the check commands, and do not commit.\n");
    for outcome in outcomes.iter().filter(|outcome|outcome.status.broke()) {
        let how=match outcome.status { Status::TimedOut=>"timed out".to_string(), Status::CouldNotStart=>"could not start".to_string(), _=>match outcome.exit_code { Some(code)=>format!("exit code {code}"), None=>"failed".to_string() } };
        text.push_str(&format!("\n{} - `{}` ({how}):\n<output>\n{}\n</output>\n",outcome.kind.as_str(),outcome.command,outcome.tail));
    }
    text
}

/// "Comando não encontrado" no shell (127) e no `cmd` (9009): falta uma
/// ferramenta na máquina, e o agente não conserta isso editando o projeto.
fn missing_tool(outcome:&Outcome)->bool { matches!(outcome.exit_code,Some(127|9009)) }

/// A rodada tem algo para o agente consertar: etapa que falhou ou estourou o
/// prazo por causa do projeto, não por falta de ferramenta ou de permissão.
pub fn repairable(outcomes:&[Outcome])->bool {
    outcomes.iter().any(|outcome|matches!(outcome.status,Status::Failed|Status::TimedOut)&&!missing_tool(outcome))
}

#[cfg(test)] mod tests {
    use super::*;

    fn dir(files:&[(&str,&str)])->tempfile::TempDir {
        let dir=tempfile::tempdir().expect("pasta");
        for (name,body) in files {
            let path=dir.path().join(name);
            if let Some(parent)=path.parent() { std::fs::create_dir_all(parent).expect("pasta"); }
            std::fs::write(path,body).expect("arquivo");
        }
        dir
    }

    fn commands(plan:&Plan)->Vec<(&'static str,&str)> { plan.steps.iter().map(|step|(step.kind.as_str(),step.command.as_str())).collect() }

    #[test] fn a_node_project_gets_its_scripts_and_the_runner_of_its_lockfile() {
        let project=dir(&[("package.json",r#"{"scripts":{"test":"vitest run","lint":"eslint .","build":"vite build"}}"#),("pnpm-lock.yaml","")]);
        let found=detect(project.path());
        assert_eq!(commands(&found),[("test","pnpm test"),("lint","pnpm run lint"),("build","pnpm run build")]);
        assert_eq!(found.undefined,[Kind::Typecheck]);
    }

    #[test] fn the_default_npm_test_script_is_not_a_test() {
        let project=dir(&[("package.json",r#"{"scripts":{"test":"echo \"Error: no test specified\" && exit 1"}}"#)]);
        let found=detect(project.path());
        assert!(found.steps.is_empty()&&!found.has_test(),"{found:?}");
        assert_eq!(found.undefined.len(),4);
    }

    #[test] fn typescript_without_a_script_still_gets_a_type_check() {
        let project=dir(&[("package.json",r#"{"devDependencies":{"typescript":"5"}}"#),("tsconfig.json","{}")]);
        assert_eq!(commands(&detect(project.path())),[("typecheck","npx --no-install tsc --noEmit")]);
        let scripted=dir(&[("package.json",r#"{"scripts":{"type-check":"tsc -b"},"devDependencies":{"typescript":"5"}}"#),("tsconfig.json","{}")]);
        assert_eq!(commands(&detect(scripted.path())),[("typecheck","npm run type-check")]);
    }

    #[test] fn cargo_python_go_and_make_projects_are_detected() {
        let rust=dir(&[("Cargo.toml","[package]\nname=\"a\"")]);
        assert_eq!(commands(&detect(rust.path())),[("test","cargo test"),("typecheck","cargo check"),("build","cargo build")]);
        let tauri=dir(&[("src-tauri/Cargo.toml","[package]\nname=\"a\""),("clippy.toml","")]);
        let steps=detect(tauri.path());
        assert!(commands(&steps).contains(&("test","cargo test --manifest-path src-tauri/Cargo.toml")));
        assert!(commands(&steps).contains(&("lint","cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings")));
        let python=dir(&[("pyproject.toml","[tool.pytest.ini_options]\n[tool.ruff]\n[tool.mypy]\n")]);
        let steps=commands(&detect(python.path())).into_iter().map(|(kind,_)|kind).collect::<Vec<_>>();
        assert_eq!(steps,["test","lint","typecheck"]);
        let go=dir(&[("go.mod","module a")]);
        assert_eq!(commands(&detect(go.path())),[("test","go test ./..."),("lint","go vet ./..."),("build","go build ./...")]);
        let make=dir(&[("Makefile","build:\n\tcc a.c\n\ntest: build\n\t./a.out\n\nLINT := x\n")]);
        assert_eq!(commands(&detect(make.path())),[("test","make test"),("build","make build")]);
    }

    #[test] fn a_folder_with_nothing_to_run_has_no_steps() {
        let empty=dir(&[("notes.txt","hello")]);
        let found=detect(empty.path());
        assert!(found.steps.is_empty());
        assert!(!found.has_test(),"sem teste definido nunca vira teste");
    }

    #[test] fn the_manual_field_overrides_adds_and_switches_off() {
        let project=dir(&[("package.json",r#"{"scripts":{"test":"vitest","lint":"eslint .","build":"vite build"}}"#)]);
        let manual=Manual{test:"  make check \n".into(),lint:"OFF".into(),typecheck:"mypy src".into(),build:String::new()};
        let found=plan(project.path(),&manual);
        assert_eq!(commands(&found),[("test","make check"),("typecheck","mypy src"),("build","npm run build")]);
        assert!(found.undefined.is_empty(),"o que a pessoa desligou não conta como indefinido: {:?}",found.undefined);
        assert!(plan(project.path(),&Manual::default()).steps.len()==3);
    }

    struct Blocks(&'static str);
    impl Judge for Blocks { fn blocked_by(&self,command:&str)->Vec<String> { if command.split_whitespace().next()==Some(self.0) { vec!["acme".into()] } else { vec![] } } }

    #[test] fn every_command_of_the_line_is_judged() {
        assert_eq!(segments("cd app && npm test | tee out; echo \"a && b\" || true"),["cd app","npm test","tee out","echo \"a && b\"","true"]);
        assert_eq!(segments("npm test 2>&1 | tee out &> log"),["npm test 2>&1","tee out &> log"]);
        assert_eq!(blockers(&Blocks("npm"),"cd app && npm test"),["acme"]);
        assert!(blockers(&Blocks("curl"),"npm test").is_empty());
        assert!(blockers(&Open,"anything").is_empty());
    }

    #[test] fn the_tail_drops_colors_and_keeps_the_end() {
        let colored="\u{1b}[31mFAIL\u{1b}[0m first\r\nsecond";
        assert_eq!(tail_of(colored),"FAIL first\nsecond");
        let long=(0..500).map(|n|format!("line {n}")).collect::<Vec<_>>().join("\n");
        let tail=tail_of(&long);
        assert!(tail.ends_with("line 499")&&!tail.contains("line 100\n")&&tail.lines().count()<=TAIL_LINES,"{tail}");
        let wide="x".repeat(10_000);
        assert!(tail_of(&wide).chars().count()<=TAIL_CHARS+2);
    }

    fn outcome(kind:Kind,status:Status)->Outcome { Outcome{kind,command:format!("run {}",kind.as_str()),status,exit_code:Some(if status==Status::Passed {0} else {1}),millis:1,tail:"output".into(),blocked_by:vec![]} }

    #[test] fn the_verdict_never_calls_an_untested_project_passed() {
        assert_eq!(verdict(&[outcome(Kind::Test,Status::Passed),outcome(Kind::Build,Status::Passed)]),Verdict::Passed);
        assert_eq!(verdict(&[outcome(Kind::Lint,Status::Passed),outcome(Kind::Build,Status::Passed)]),Verdict::Unproven,"sem teste");
        assert_eq!(verdict(&[]),Verdict::Unproven);
        assert_eq!(verdict(&[outcome(Kind::Test,Status::Passed),outcome(Kind::Lint,Status::Failed)]),Verdict::Failed);
        assert_eq!(verdict(&[outcome(Kind::Test,Status::Blocked)]),Verdict::Unproven,"bloqueado não prova nada");
        assert_eq!(verdict(&[outcome(Kind::Test,Status::Passed),outcome(Kind::Build,Status::Blocked)]),Verdict::Unproven);
        assert_eq!(verdict(&[outcome(Kind::Test,Status::TimedOut)]),Verdict::Failed);
    }

    #[test] fn the_repair_prompt_names_only_what_broke_and_forbids_cheating() {
        let mut failed=outcome(Kind::Test,Status::Failed);
        failed.tail="assertion failed: left == right".into();
        let prompt=repair_prompt(&[outcome(Kind::Lint,Status::Passed),failed],1,2);
        assert!(prompt.contains("repair attempt 1 of 2")&&prompt.contains("`run test`")&&prompt.contains("assertion failed"));
        assert!(!prompt.contains("`run lint`"));
        assert!(prompt.contains("do not skip, delete or weaken tests")&&prompt.contains("do not commit"));
    }

    #[cfg(unix)]
    async fn run(root:&Path,kind:Kind,command:&str,judge:&dyn Judge,timeout:Duration)->Outcome {
        run_step(root,&Step{kind,command:command.into()},judge,&|text|text.replace("s3cr3t","[REDACTED]"),&Stop::default(),timeout).await
    }

    #[cfg(unix)] #[tokio::test] async fn a_passing_and_a_failing_command_report_how_they_ended() {
        let project=dir(&[]);
        let ok=run(project.path(),Kind::Test,"echo fine",&Open,STEP_TIMEOUT).await;
        assert_eq!((ok.status,ok.exit_code,ok.tail.as_str()),(Status::Passed,Some(0),"fine"));
        let bad=run(project.path(),Kind::Test,"echo token s3cr3t; echo broken >&2; exit 3",&Open,STEP_TIMEOUT).await;
        assert_eq!((bad.status,bad.exit_code),(Status::Failed,Some(3)));
        assert!(bad.tail.contains("broken")&&bad.tail.contains("token [REDACTED]")&&!bad.tail.contains("s3cr3t"),"{}",bad.tail);
        assert!(repairable(&[ok.clone(),bad.clone()])&&!repairable(&[ok]));
        let mut absent=bad.clone();
        absent.exit_code=Some(127);
        assert!(!repairable(&[absent]),"falta de ferramenta não é defeito do projeto");
    }

    #[cfg(unix)] #[tokio::test] async fn the_command_runs_in_the_project_folder_with_ci_set() {
        let project=dir(&[("marker.txt","here")]);
        let found=run(project.path(),Kind::Build,"cat marker.txt && echo ci=$CI",&Open,STEP_TIMEOUT).await;
        assert_eq!(found.tail,"hereci=1");
    }

    #[cfg(unix)] #[tokio::test] async fn a_blocked_command_never_starts() {
        let project=dir(&[]);
        let found=run(project.path(),Kind::Test,"touch ran.txt && npm test",&Blocks("npm"),STEP_TIMEOUT).await;
        assert_eq!((found.status,found.blocked_by.as_slice()),(Status::Blocked,["acme".to_string()].as_slice()));
        assert!(!project.path().join("ran.txt").exists(),"nem o primeiro comando da linha roda");
    }

    #[cfg(unix)] #[tokio::test] async fn a_slow_command_is_cut_with_its_children() {
        let project=dir(&[]);
        let found=run(project.path(),Kind::Test,"sleep 30 & sleep 30",&Open,Duration::from_millis(300)).await;
        assert_eq!(found.status,Status::TimedOut);
        assert!(found.millis<5_000);
    }

    #[cfg(unix)] #[tokio::test] async fn stopping_the_request_stops_the_step() {
        let project=dir(&[]);
        let stop=Stop::default();
        let step=Step{kind:Kind::Test,command:"sleep 30".into()};
        let (found,())=tokio::join!(run_step(project.path(),&step,&Open,&|text|text.to_string(),&stop,STEP_TIMEOUT),async { tokio::time::sleep(Duration::from_millis(300)).await; stop.stop(jayv_base::progress::StopReason::Asked); });
        assert_eq!(found.status,Status::Stopped);
        assert!(found.millis<5_000);
    }

    #[cfg(unix)] #[tokio::test] async fn a_program_that_does_not_exist_fails_with_the_shell_message() {
        let project=dir(&[]);
        let found=run(project.path(),Kind::Test,"definitely-not-a-program-xyz",&Open,STEP_TIMEOUT).await;
        assert_eq!(found.status,Status::Failed);
        assert_eq!(found.exit_code,Some(127));
    }
}
