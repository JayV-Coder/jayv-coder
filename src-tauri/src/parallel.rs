//! Um pedido grande dividido entre agentes. O planejador quebra o pedido em
//! subtarefas que mexem em arquivos separados; cada uma vai a um agente
//! (agentes diferentes quando há mais de um), numa `git worktree` própria,
//! todas ao mesmo tempo. No fim, as mudanças de cada uma voltam para a pasta
//! do projeto como um patch. A que não encaixa fica guardada para a pessoa
//! aplicar à mão, em vez de bagunçar o projeto.

use crate::model::ModelSelection;
use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::{path::{Path, PathBuf}, process::{Command, Stdio}};

/// Quantas subtarefas no máximo: mais que isso multiplica o custo e as
/// chances de conflito sem acelerar quase nada.
pub const MAX_TASKS:usize=4;

#[derive(Debug,Clone,PartialEq,Deserialize)]
pub struct Subtask {
    pub title:String,
    pub instructions:String,
    #[serde(default)] pub files:Vec<String>,
}

#[derive(Deserialize)]
struct Split { #[serde(default)] tasks:Vec<Subtask> }

/// O pedido ao planejador: a divisão em JSON, ou uma tarefa só quando não dá
/// para dividir sem dois agentes mexerem no mesmo arquivo.
pub fn split_prompt(request:&str)->String {
    format!("Split the request below into at most {MAX_TASKS} subtasks that different coding agents will implement at the same time, each in its own copy of the project. Read what you need from the project, but do not edit any file.

<request>
{request}
</request>

Rules: no two subtasks may change or create the same file; each subtask must make sense on its own, without waiting for another; if the request cannot be split this way, return a single task. Reply with only this JSON, no prose:
{{\"tasks\":[{{\"title\":\"short title in the reply language\",\"instructions\":\"what to do, concrete\",\"files\":[\"relative/path/one\",\"relative/path/two\"]}}]}}")
}

/// A divisão que veio do planejador, se ela vale: de duas a `MAX_TASKS`
/// tarefas, cada uma com instruções e arquivos, sem arquivo repetido entre
/// elas. Qualquer outra coisa é "não divide", e o pedido segue inteiro.
pub fn parse_split(text:&str)->Vec<Subtask> {
    let (Some(start),Some(end))=(text.find('{'),text.rfind('}')) else { return vec![] };
    if end<start { return vec![]; }
    let Ok(split)=serde_json::from_str::<Split>(&text[start..=end]) else { return vec![] };
    let tasks:Vec<Subtask>=split.tasks.into_iter().map(|task|Subtask{
        title:task.title.trim().to_string(),
        instructions:task.instructions.trim().to_string(),
        files:task.files.iter().map(|file|file.trim().trim_start_matches("./").to_string()).filter(|file|!file.is_empty()).collect(),
    }).collect();
    if !(2..=MAX_TASKS).contains(&tasks.len()) { return vec![]; }
    if tasks.iter().any(|task|task.title.is_empty()||task.instructions.is_empty()||task.files.is_empty()) { return vec![]; }
    let mut seen=std::collections::HashSet::new();
    if !tasks.iter().flat_map(|task|task.files.iter()).all(|file|seen.insert(file.clone())) { return vec![]; }
    tasks
}

/// Um agente por subtarefa, na ordem da nota, passando por agentes
/// diferentes antes de repetir um: dividir só vale a pena quando o trabalho
/// corre ao mesmo tempo em mais de um lugar.
pub fn assign(ranked:&[ModelSelection],count:usize)->Vec<ModelSelection> {
    let mut distinct:Vec<&ModelSelection>=Vec::new();
    for candidate in ranked { if !distinct.iter().any(|kept|kept.provider==candidate.provider) { distinct.push(candidate); } }
    if distinct.is_empty() { return vec![]; }
    (0..count).map(|index|distinct[index%distinct.len()].clone()).collect()
}

/// O que cada agente recebe: a parte dele, o pedido inteiro como pano de
/// fundo e os arquivos que são dos outros.
pub fn task_message(request:&str,task:&Subtask,others:&[&Subtask])->String {
    let theirs=others.iter().flat_map(|other|other.files.iter()).map(|file|format!("- {file}")).collect::<Vec<_>>().join("\n");
    format!("You are one of several coding agents working on the same request at the same time, each in its own copy of the project. Do only your part.

The whole request, for context:
<request>
{request}
</request>

Your part: {title}
{instructions}

Files that are yours to change or create:
{mine}

Do not touch these files, other agents are changing them:
{theirs}

When you finish, reply with a short summary of what you changed.",
        title=task.title,instructions=task.instructions,mine=task.files.iter().map(|file|format!("- {file}")).collect::<Vec<_>>().join("\n"),
        theirs=if theirs.is_empty() {"(none)".into()} else {theirs})
}

fn git(dir:&Path,args:&[&str])->Result<String> {
    let mut command=Command::new("git");
    command.current_dir(dir).args(args).env("GIT_TERMINAL_PROMPT","0").env("GIT_OPTIONAL_LOCKS","0").stdin(Stdio::null());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    let output=command.output().with_context(||format!("git {}",args.join(" ")))?;
    if !output.status.success() { bail!("git {}: {}",args.join(" "),String::from_utf8_lossy(&output.stderr).trim()); }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// O repositório da pasta do projeto: a raiz dele e onde a pasta fica dentro
/// dela. Fora de um repositório git não há como dividir.
pub fn repository(folder:&Path)->Option<(PathBuf,String)> {
    let top=git(folder,&["rev-parse","--show-toplevel"]).ok()?;
    let prefix=git(folder,&["rev-parse","--show-prefix"]).ok()?;
    Some((PathBuf::from(top.trim()),prefix.trim().to_string()))
}

/// O ponto de partida das cópias: o projeto como está agora, com as mudanças
/// ainda não gravadas nos arquivos que o git acompanha. Sem nenhuma, o HEAD.
pub fn base(top:&Path)->Result<String> {
    let stash=git(top,&["stash","create"])?;
    let stash=stash.trim();
    if !stash.is_empty() { return Ok(stash.to_string()); }
    Ok(git(top,&["rev-parse","HEAD"])?.trim().to_string())
}

/// Uma cópia do projeto no ponto de partida, sem branch.
pub fn add_worktree(top:&Path,base:&str,dir:&Path)->Result<()> {
    git(top,&["worktree","add","--detach","--quiet",&dir.to_string_lossy(),base]).map(|_|())
}

pub fn remove_worktree(top:&Path,dir:&Path) {
    if git(top,&["worktree","remove","--force",&dir.to_string_lossy()]).is_err() { let _=std::fs::remove_dir_all(dir); let _=git(top,&["worktree","prune"]); }
}

/// O que o agente mudou na cópia, inclusive os arquivos novos, como patch.
pub fn collect_patch(worktree:&Path,base:&str)->Result<String> {
    git(worktree,&["add","-A"])?;
    git(worktree,&["diff","--cached","--binary",base])
}

/// Aplica o patch na pasta do projeto, ou não toca em nada: um patch que não
/// encaixa inteiro não entra pela metade.
pub fn apply_patch(top:&Path,patch:&str)->Result<()> {
    if patch.trim().is_empty() { return Ok(()); }
    let file=tempfile_path(top,"jayv-apply")?;
    std::fs::write(&file,patch)?;
    let path=file.to_string_lossy().into_owned();
    let result=git(top,&["apply","--check","--whitespace=nowarn",&path]).and_then(|_|git(top,&["apply","--whitespace=nowarn",&path]));
    let _=std::fs::remove_file(&file);
    result.map(|_|())
}

/// Guarda, dentro da pasta do git, o patch que não encaixou: a pessoa o
/// aplica com `git apply` depois de resolver o que mudou.
pub fn keep_patch(top:&Path,name:&str,patch:&str)->Result<PathBuf> {
    let dir=PathBuf::from(git(top,&["rev-parse","--absolute-git-dir"])?.trim()).join("jayv-patches");
    std::fs::create_dir_all(&dir)?;
    let path=dir.join(format!("{name}.patch"));
    std::fs::write(&path,patch)?;
    Ok(path)
}

fn tempfile_path(top:&Path,stem:&str)->Result<PathBuf> {
    let dir=PathBuf::from(git(top,&["rev-parse","--absolute-git-dir"])?.trim());
    Ok(dir.join(format!("{stem}-{}.patch",uuid::Uuid::new_v4().simple())))
}

/// Onde mora a cópia de uma subtarefa: fora do projeto, na pasta temporária.
pub fn worktree_dir(index:usize)->PathBuf {
    std::env::temp_dir().join(format!("jayv-task-{}-{index}",uuid::Uuid::new_v4().simple()))
}

#[cfg(test)] mod tests {
    use super::*;

    fn selection(provider:&str)->ModelSelection { ModelSelection{provider:provider.into(),model_name:format!("{provider}-m"),..Default::default()} }

    #[test] fn a_split_needs_two_to_four_tasks_with_separate_files() {
        let good=r#"Aqui está: {"tasks":[{"title":"API","instructions":"rota nova","files":["src/api.rs"]},{"title":"Tela","instructions":"botão","files":["./src/ui.tsx"]}]}"#;
        let tasks=parse_split(good);
        assert_eq!(tasks.len(),2);
        assert_eq!(tasks[1].files,["src/ui.tsx"]);
        assert!(parse_split(r#"{"tasks":[{"title":"Tudo","instructions":"x","files":["a"]}]}"#).is_empty(),"uma tarefa só não divide");
        assert!(parse_split(r#"{"tasks":[{"title":"A","instructions":"x","files":["a"]},{"title":"B","instructions":"y","files":["a"]}]}"#).is_empty(),"dois agentes no mesmo arquivo não");
        assert!(parse_split(r#"{"tasks":[{"title":"A","instructions":"x","files":[]},{"title":"B","instructions":"y","files":["b"]}]}"#).is_empty(),"cada tarefa diz os seus arquivos");
        assert!(parse_split("não sei dividir").is_empty());
    }

    #[test] fn tasks_go_to_different_agents_before_repeating_one() {
        let ranked=[selection("claude"),selection("claude"),selection("codex")];
        let picked=assign(&ranked,3);
        assert_eq!(picked.iter().map(|chosen|chosen.provider.as_str()).collect::<Vec<_>>(),["claude","codex","claude"]);
        assert!(assign(&[],2).is_empty());
    }

    fn run(dir:&Path,args:&[&str]) { assert!(Command::new("git").current_dir(dir).args(args).output().expect("git").status.success(),"git {args:?}"); }

    #[test] fn changes_made_in_worktrees_come_back_as_patches() {
        let project=tempfile::tempdir().expect("pasta");
        let top=project.path();
        run(top,&["init","-q"]);
        std::fs::write(top.join("a.txt"),"um\n").expect("a");
        std::fs::write(top.join("b.txt"),"dois\n").expect("b");
        run(top,&["add","."]);
        run(top,&["-c","user.email=t@t","-c","user.name=t","commit","-qm","base"]);
        // Uma mudança ainda não gravada também vai para as cópias.
        std::fs::write(top.join("a.txt"),"um mudado\n").expect("sujo");
        let start=base(top).expect("base");
        let (first,second)=(worktree_dir(0),worktree_dir(1));
        add_worktree(top,&start,&first).expect("cópia 1");
        add_worktree(top,&start,&second).expect("cópia 2");
        assert_eq!(std::fs::read_to_string(first.join("a.txt")).expect("lido"),"um mudado\n");
        std::fs::write(first.join("novo.txt"),"novo\n").expect("cria");
        std::fs::write(second.join("b.txt"),"dois mudado\n").expect("muda");
        let patches=[collect_patch(&first,&start).expect("patch 1"),collect_patch(&second,&start).expect("patch 2")];
        remove_worktree(top,&first);
        remove_worktree(top,&second);
        assert!(!first.exists()&&!second.exists());
        for patch in &patches { apply_patch(top,patch).expect("aplica"); }
        assert_eq!(std::fs::read_to_string(top.join("novo.txt")).expect("novo"),"novo\n");
        assert_eq!(std::fs::read_to_string(top.join("b.txt")).expect("b"),"dois mudado\n");
        assert_eq!(std::fs::read_to_string(top.join("a.txt")).expect("a"),"um mudado\n","o que já estava mudado fica");
        // O mesmo patch de novo não encaixa: nada muda e ele é guardado.
        assert!(apply_patch(top,&patches[1]).is_err());
        let kept=keep_patch(top,"t-1",&patches[1]).expect("guardado");
        assert!(kept.exists());
    }
}
