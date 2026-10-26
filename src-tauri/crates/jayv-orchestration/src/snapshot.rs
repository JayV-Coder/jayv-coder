//! O ponto de volta de um build: antes de o agente mexer no projeto, o JayV
//! grava como a pasta está agora (arquivos que o git acompanha e arquivos
//! novos que o `.gitignore` não cobre) num commit guardado em
//! `refs/jayv/checkpoints/`. Ele não toca no HEAD, na branch, no índice nem no
//! stash; só some do `git log` normal. Se o build sai pior do que entrou, a
//! pasta volta a esse ponto com um comando.
//!
//! A foto sai por um índice temporário (cópia do índice de verdade), então o
//! `git add -A` só relê os arquivos que mudaram. Fora de um repositório git
//! não há ponto de volta.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{path::{Path, PathBuf}, process::{Command, Stdio}};

/// Onde ficam os pontos de volta.
const REF_PREFIX:&str="refs/jayv/checkpoints/";
/// Quantos pontos de volta o repositório guarda; os mais velhos são apagados.
const KEEP:usize=30;

/// Um ponto de volta gravado.
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Checkpoint {
    /// O nome do ponto (o fim da ref).
    pub id:String,
    /// O commit que guarda a foto.
    pub commit:String,
    /// O HEAD no momento da foto, se o repositório já tinha um commit.
    pub head:Option<String>,
}

/// O que a volta fez.
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Rollback {
    /// Quantos arquivos voltaram ao que eram (restaurados ou apagados).
    pub files:usize,
    /// O agente fez commits depois da foto: os arquivos voltaram, os commits
    /// continuam no histórico.
    pub head_moved:bool,
    /// A ref que guarda a pasta como estava antes da volta.
    pub safety:String,
}

fn git(top:&Path,index:Option<&Path>,args:&[&str],stdin:Option<&[u8]>)->Result<Vec<u8>> {
    use std::io::Write;
    let mut command=Command::new("git");
    command.current_dir(top).args(args)
        .env("GIT_TERMINAL_PROMPT","0").env("GIT_OPTIONAL_LOCKS","0")
        .env("GIT_AUTHOR_NAME","JayV").env("GIT_AUTHOR_EMAIL","jayv@localhost")
        .env("GIT_COMMITTER_NAME","JayV").env("GIT_COMMITTER_EMAIL","jayv@localhost")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(if stdin.is_some() {Stdio::piped()} else {Stdio::null()});
    if let Some(index)=index { command.env("GIT_INDEX_FILE",index); }
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    let mut child=command.spawn().with_context(||format!("git {}",args.join(" ")))?;
    if let (Some(bytes),Some(mut pipe))=(stdin,child.stdin.take()) {
        // Escrito de uma thread: um git que enche a saída antes de ler tudo não trava.
        let bytes=bytes.to_vec();
        std::thread::spawn(move||{ let _=pipe.write_all(&bytes); });
    }
    let output=child.wait_with_output().with_context(||format!("git {}",args.join(" ")))?;
    if !output.status.success() { bail!("git {}: {}",args.join(" "),String::from_utf8_lossy(&output.stderr).trim()); }
    Ok(output.stdout)
}

fn text(bytes:Vec<u8>)->String { String::from_utf8_lossy(&bytes).trim().to_string() }

/// A raiz do repositório da pasta, ou nada se ela não estiver num.
pub fn top_of(folder:&Path)->Option<PathBuf> {
    git(folder,None,&["rev-parse","--show-toplevel"],None).ok().map(|out|PathBuf::from(text(out))).filter(|top|top.is_dir())
}

fn head_of(top:&Path)->Option<String> {
    git(top,None,&["rev-parse","-q","--verify","HEAD"],None).ok().map(text).filter(|head|!head.is_empty())
}

/// Um índice temporário que some ao sair de escopo.
struct TempIndex(PathBuf);

impl TempIndex {
    fn open(top:&Path)->Result<Self> {
        let git_dir=PathBuf::from(text(git(top,None,&["rev-parse","--absolute-git-dir"],None)?));
        let nanos=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|elapsed|elapsed.as_nanos());
        let path=git_dir.join(format!("jayv-index-{}-{nanos}",std::process::id()));
        let real=git_dir.join("index");
        if real.is_file() { std::fs::copy(&real,&path).context("copiando o índice")?; }
        Ok(Self(path))
    }
}

impl Drop for TempIndex { fn drop(&mut self) { let _=std::fs::remove_file(&self.0); } }

/// A árvore da pasta como está agora.
fn tree_now(top:&Path)->Result<String> {
    let index=TempIndex::open(top)?;
    git(top,Some(&index.0),&["add","-A"],None)?;
    Ok(text(git(top,Some(&index.0),&["write-tree"],None)?))
}

fn commit_of(top:&Path,tree:&str,head:Option<&str>,id:&str)->Result<String> {
    let message=format!("jayv checkpoint {id}");
    let mut args=vec!["commit-tree",tree,"-m",&message];
    if let Some(head)=head { args.extend(["-p",head]); }
    Ok(text(git(top,None,&args,None)?))
}

fn save(top:&Path,id:&str)->Result<Checkpoint> {
    let tree=tree_now(top)?;
    let head=head_of(top);
    let commit=commit_of(top,&tree,head.as_deref(),id)?;
    git(top,None,&["update-ref",&format!("{REF_PREFIX}{id}"),&commit],None)?;
    Ok(Checkpoint{id:id.into(),commit,head})
}

/// Grava o ponto de volta `id` da pasta. Fora de um repositório, ou se o git
/// falhar, não há ponto de volta e o build segue sem ele.
pub fn take(folder:&Path,id:&str)->Option<Checkpoint> {
    let top=top_of(folder)?;
    let saved=match save(&top,id) {
        Ok(saved)=>saved,
        Err(error)=>{ eprintln!("ponto de volta: não consegui gravar ({error:#})"); return None; }
    };
    prune(&top);
    Some(saved)
}

/// Apaga os pontos mais velhos além do limite.
fn prune(top:&Path) {
    let Ok(listed)=git(top,None,&["for-each-ref","--sort=-creatordate","--format=%(refname)",REF_PREFIX],None) else { return };
    for name in text(listed).lines().skip(KEEP) { let _=git(top,None,&["update-ref","-d",name],None); }
}

fn commit_exists(top:&Path,commit:&str)->bool {
    git(top,None,&["cat-file","-e",&format!("{commit}^{{commit}}")],None).is_ok()
}

/// Os arquivos que diferem entre o ponto de volta e a pasta de agora, com a
/// letra do git: `A` só existe agora, `M` mudou, `D` sumiu.
fn differences(top:&Path,commit:&str,now_tree:&str)->Result<Vec<(char,String)>> {
    let out=git(top,None,&["diff-tree","-r","--name-status","--no-renames","-z",&format!("{commit}^{{tree}}"),now_tree],None)?;
    let text=String::from_utf8_lossy(&out).into_owned();
    let mut fields=text.split('\0').filter(|field|!field.is_empty());
    let mut found=Vec::new();
    while let (Some(letter),Some(path))=(fields.next(),fields.next()) {
        if let Some(letter)=letter.chars().next() { found.push((letter,path.to_string())); }
    }
    Ok(found)
}

/// Os arquivos que o build mudou: o que difere entre o ponto de volta e a
/// pasta de agora. `None` quando não dá para saber (a foto sumiu, o git falhou).
pub fn changed_since(folder:&Path,commit:&str)->Option<Vec<String>> {
    let top=top_of(folder)?;
    if !commit_exists(&top,commit) { return None; }
    let now=tree_now(&top).ok()?;
    differences(&top,commit,&now).ok().map(|found|found.into_iter().map(|(_,path)|path).collect())
}

/// O ponto de volta ainda existe no repositório.
pub fn exists(folder:&Path,commit:&str)->bool {
    top_of(folder).is_some_and(|top|commit_exists(&top,commit))
}

/// Devolve a pasta ao ponto de volta: o que o build mudou volta ao que era, o
/// que ele criou é apagado e o que ele apagou reaparece. O que o `.gitignore`
/// cobre não é tocado. Antes de mexer em qualquer arquivo, a pasta de agora é
/// gravada em `safety`, e a volta pode ser desfeita a partir dela.
pub fn rollback(folder:&Path,checkpoint:&Checkpoint,safety:&str)->Result<Rollback> {
    let Some(top)=top_of(folder) else { bail!("not a git repository") };
    if !commit_exists(&top,&checkpoint.commit) { bail!("the checkpoint is gone from the repository"); }
    let before=save(&top,safety)?;
    let now=tree_now(&top)?;
    let found=differences(&top,&checkpoint.commit,&now)?;
    let mut restore=Vec::new();
    let mut removed=0;
    for (letter,path) in &found {
        if *letter=='A' {
            let target=top.join(path);
            if target.symlink_metadata().is_ok() && std::fs::remove_file(&target).is_ok() {
                removed+=1;
                remove_empty_parents(&top,&target);
            }
        } else {
            restore.push(path.as_str());
        }
    }
    if !restore.is_empty() {
        let index=TempIndex::open(&top)?;
        git(&top,Some(&index.0),&["read-tree",&format!("{}^{{tree}}",checkpoint.commit)],None)?;
        let mut listed=Vec::new();
        for path in &restore { listed.extend_from_slice(path.as_bytes()); listed.push(0); }
        git(&top,Some(&index.0),&["checkout-index","-f","-z","--stdin"],Some(&listed))?;
    }
    let head_moved=checkpoint.head!=head_of(&top);
    Ok(Rollback{files:removed+restore.len(),head_moved,safety:before.id})
}

/// Sobe a partir do arquivo apagado removendo as pastas que ficaram vazias,
/// sem sair da raiz do repositório.
fn remove_empty_parents(top:&Path,removed:&Path) {
    let mut dir=removed.parent();
    while let Some(current)=dir {
        if current==top || !current.starts_with(top) || std::fs::remove_dir(current).is_err() { break; }
        dir=current.parent();
    }
}

#[cfg(test)] mod tests {
    use super::*;

    fn run(dir:&Path,args:&[&str]) { assert!(Command::new("git").current_dir(dir).args(args).output().expect("git").status.success(),"git {args:?}"); }

    fn repository()->tempfile::TempDir {
        let dir=tempfile::tempdir().expect("pasta");
        run(dir.path(),&["init","-q"]);
        run(dir.path(),&["config","user.email","t@t"]);
        run(dir.path(),&["config","user.name","t"]);
        std::fs::write(dir.path().join(".gitignore"),"target/\n").expect("ignore");
        std::fs::write(dir.path().join("lib.rs"),"fn one() {}\n").expect("lib");
        std::fs::write(dir.path().join("keep.rs"),"fn keep() {}\n").expect("keep");
        run(dir.path(),&["add","."]);
        run(dir.path(),&["commit","-qm","base"]);
        dir
    }

    fn read(dir:&Path,name:&str)->String { std::fs::read_to_string(dir.join(name)).unwrap_or_default() }

    #[test] fn a_folder_outside_git_has_no_checkpoint() {
        let dir=tempfile::tempdir().expect("pasta");
        assert!(take(dir.path(),"one").is_none());
    }

    /// A pasta volta ao que era: arquivo mudado, criado e apagado, inclusive o
    /// trabalho ainda não gravado de quem pediu; o ignorado fica como está.
    #[test] fn the_rollback_restores_changed_created_and_removed_files() {
        let dir=repository();
        let path=dir.path();
        std::fs::write(path.join("lib.rs"),"fn one() {}\nfn mine_unsaved() {}\n").expect("trabalho da pessoa");
        std::fs::write(path.join("draft.rs"),"fn draft() {}\n").expect("rascunho");
        let checkpoint=take(path,"t1").expect("ponto de volta");
        assert_eq!(checkpoint.head,head_of(path));
        assert_eq!(changed_since(path,&checkpoint.commit),Some(vec![]),"nada mudou ainda");

        std::fs::write(path.join("lib.rs"),"fn one() { broken }\n").expect("agente muda");
        std::fs::remove_file(path.join("keep.rs")).expect("agente apaga");
        std::fs::create_dir_all(path.join("src/deep")).expect("pasta");
        std::fs::write(path.join("src/deep/new.rs"),"fn new() {}\n").expect("agente cria");
        std::fs::create_dir_all(path.join("target")).expect("target");
        std::fs::write(path.join("target/out.bin"),"artifact").expect("ignorado");
        let mut changed=changed_since(path,&checkpoint.commit).expect("mudou");
        changed.sort();
        assert_eq!(changed,["keep.rs","lib.rs","src/deep/new.rs"],"o ignorado não conta");

        let back=rollback(path,&checkpoint,"t1-before").expect("volta");
        assert_eq!(back.files,3);
        assert!(!back.head_moved);
        assert_eq!(read(path,"lib.rs"),"fn one() {}\nfn mine_unsaved() {}\n");
        assert_eq!(read(path,"keep.rs"),"fn keep() {}\n");
        assert_eq!(read(path,"draft.rs"),"fn draft() {}\n");
        assert!(!path.join("src/deep/new.rs").exists()&&!path.join("src").exists(),"a pasta vazia também sai");
        assert_eq!(read(path,"target/out.bin"),"artifact","o ignorado não é tocado");
        assert!(exists(path,&checkpoint.commit));
        assert_eq!(changed_since(path,&checkpoint.commit),Some(vec![]));
    }

    /// A volta pode ser desfeita: a pasta de antes dela fica guardada.
    #[test] fn the_folder_before_the_rollback_is_kept() {
        let dir=repository();
        let path=dir.path();
        let checkpoint=take(path,"t2").expect("ponto de volta");
        std::fs::write(path.join("lib.rs"),"fn changed() {}\n").expect("muda");
        let back=rollback(path,&checkpoint,"t2-before").expect("volta");
        let kept=Checkpoint{id:back.safety.clone(),commit:text(git(path,None,&["rev-parse",&format!("{REF_PREFIX}{}",back.safety)],None).expect("ref")),head:head_of(path)};
        assert_eq!(read(path,"lib.rs"),"fn one() {}\n");
        rollback(path,&kept,"t2-again").expect("desfaz a volta");
        assert_eq!(read(path,"lib.rs"),"fn changed() {}\n");
    }

    /// O agente que fez commit: os arquivos voltam e a tela é avisada.
    #[test] fn commits_made_by_the_agent_stay_and_the_rollback_says_so() {
        let dir=repository();
        let path=dir.path();
        let checkpoint=take(path,"t3").expect("ponto de volta");
        std::fs::write(path.join("lib.rs"),"fn committed() {}\n").expect("muda");
        run(path,&["commit","-qam","agent"]);
        let back=rollback(path,&checkpoint,"t3-before").expect("volta");
        assert!(back.head_moved);
        assert_eq!(read(path,"lib.rs"),"fn one() {}\n");
    }

    #[test] fn the_checkpoint_does_not_touch_head_the_index_or_the_stash() {
        let dir=repository();
        let path=dir.path();
        std::fs::write(path.join("lib.rs"),"fn staged() {}\n").expect("muda");
        run(path,&["add","lib.rs"]);
        std::fs::write(path.join("draft.rs"),"fn draft() {}\n").expect("novo");
        let before=(head_of(path),text(git(path,None,&["status","--porcelain"],None).expect("status")),text(git(path,None,&["stash","list"],None).expect("stash")));
        take(path,"t4").expect("ponto de volta");
        let after=(head_of(path),text(git(path,None,&["status","--porcelain"],None).expect("status")),text(git(path,None,&["stash","list"],None).expect("stash")));
        assert_eq!(before,after);
        assert!(!text(git(path,None,&["branch","--list"],None).expect("branches")).contains("jayv"));
    }

    #[test] fn only_the_latest_checkpoints_are_kept() {
        let dir=repository();
        let path=dir.path();
        for number in 0..KEEP+3 { take(path,&format!("n{number}")).expect("ponto de volta"); }
        let listed=text(git(path,None,&["for-each-ref","--format=%(refname)",REF_PREFIX],None).expect("refs"));
        assert_eq!(listed.lines().count(),KEEP);
    }

    #[test] fn a_repository_without_commits_still_gets_a_checkpoint() {
        let dir=tempfile::tempdir().expect("pasta");
        run(dir.path(),&["init","-q"]);
        std::fs::write(dir.path().join("a.rs"),"a").expect("a");
        let checkpoint=take(dir.path(),"t5").expect("ponto de volta");
        assert!(checkpoint.head.is_none());
        std::fs::write(dir.path().join("a.rs"),"b").expect("muda");
        rollback(dir.path(),&checkpoint,"t5-before").expect("volta");
        assert_eq!(read(dir.path(),"a.rs"),"a");
    }
}
