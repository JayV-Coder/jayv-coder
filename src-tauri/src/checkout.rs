//! Os repositórios de uma organização neste computador: clonar um deles numa
//! pasta, ou achar os que já estão clonados dentro de uma pasta. O clone usa o
//! `git` instalado, com as credenciais que a pessoa já usa (SSH ou o
//! gerenciador de credenciais): o app não guarda token de provedor nenhum.

use crate::i18n::Text;
use crate::repo_keys;
use anyhow::Result;
use serde::Serialize;
use std::{fs, path::{Path, PathBuf}, process::Stdio};
use tokio::{process::Command, time::{timeout, Duration}};

/// Um clone achado na pasta: a chave do repositório e onde ele está.
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct FoundRepository{pub key:String,pub path:String}

/// Até onde a busca desce: a pasta, as filhas e as netas (`acme/backend/api`).
const SCAN_DEPTH:usize=2;
/// Um clone grande demora; um que não anda em meia hora travou.
const CLONE_LIMIT:Duration=Duration::from_secs(30*60);

/// A chave é o que o servidor guardou: tem de ser uma chave de verdade antes de
/// virar argumento do `git`.
fn checked(key:&str)->Result<String> {
    let key=key.trim().to_lowercase();
    match repo_keys::normalize(&format!("https://{key}")) {
        Some(normal) if normal==key=>Ok(key),
        _=>anyhow::bail!(Text::new("repos.clone.invalid").with("repo",key)),
    }
}

/// O nome da pasta do clone: a última parte do caminho (`acme/api` → `api`).
pub fn folder_name(key:&str)->&str {key.rsplit('/').next().unwrap_or(key)}

/// As URLs que o clone tenta, na ordem: HTTPS (gerenciador de credenciais) e
/// depois SSH (chave do agente).
pub fn clone_urls(key:&str)->[String;2] {
    let (host,path)=key.split_once('/').unwrap_or((key,""));
    [format!("https://{host}/{path}.git"),format!("git@{host}:{path}.git")]
}

/// Onde o clone vai morar. Uma pasta que já existe com coisa dentro não é
/// tocada.
pub fn target(parent:&Path,key:&str)->Result<PathBuf> {
    if !parent.is_dir() {anyhow::bail!(Text::new("repos.folder.missing").with("path",parent.display().to_string()));}
    let target=parent.join(folder_name(key));
    if target.exists() && fs::read_dir(&target).map(|mut entries|entries.next().is_some()).unwrap_or(true) {
        anyhow::bail!(Text::new("repos.clone.exists").with("path",target.display().to_string()));
    }
    Ok(target)
}

/// Sem acesso ao repositório: credencial que falta ou que não pode ler.
fn denied(stderr:&str)->bool {
    let text=stderr.to_lowercase();
    ["authentication failed","could not read username","could not read password","terminal prompts disabled","permission denied","repository not found","not found","host key verification failed","access denied","403","401"]
        .iter().any(|needle|text.contains(needle))
}

/// A última linha do git que diz alguma coisa: o motivo técnico do erro.
fn reason(stderr:&str)->String {
    stderr.lines().map(str::trim).filter(|line|!line.is_empty()).last().unwrap_or("git clone failed").to_string()
}

enum Attempt{Done,Denied(String),Failed(String)}

async fn attempt(url:&str,target:&Path)->Result<Attempt> {
    let mut process=Command::new("git");
    crate::providers::quiet(&mut process)
        .args(["clone","--quiet","--"]).arg(url).arg(target)
        // Ninguém vai responder a um prompt no terminal: sem credencial, falha.
        .env("GIT_TERMINAL_PROMPT","0")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
    let child=match process.spawn() {
        Ok(child)=>child,
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>anyhow::bail!(Text::new("repos.clone.noGit")),
        Err(error)=>return Err(error.into()),
    };
    let output=timeout(CLONE_LIMIT,child.wait_with_output()).await.map_err(|_|Text::new("repos.clone.failed").with("reason","timed out"))??;
    if output.status.success() {return Ok(Attempt::Done);}
    let stderr=String::from_utf8_lossy(&output.stderr);
    Ok(if denied(&stderr) {Attempt::Denied(reason(&stderr))} else {Attempt::Failed(reason(&stderr))})
}

/// Clona o repositório em `parent/<nome>` e devolve a pasta. Tenta HTTPS e,
/// sem acesso, SSH; uma falha que não é de acesso (rede, disco) para ali.
pub async fn clone(key:&str,parent:&Path)->Result<PathBuf> {
    let key=checked(key)?;
    let target=target(parent,&key)?;
    let mut last=String::new();
    for url in clone_urls(&key) {
        match attempt(&url,&target).await? {
            Attempt::Done=>return Ok(target),
            Attempt::Denied(why)=>last=why,
            Attempt::Failed(why)=>anyhow::bail!(Text::new("repos.clone.failed").with("reason",why)),
        }
    }
    anyhow::bail!(Text::new("repos.clone.denied").with("repo",key).with("reason",last))
}

fn skipped(name:&str)->bool {name.starts_with('.')||matches!(name,"node_modules"|"target"|"vendor"|"dist"|"build")}

/// As pastas de repositório dentro de `folder` — a própria pasta, as filhas e
/// as netas —, com as chaves dos remotes de cada uma. Não desce dentro de um
/// repositório nem em pasta oculta.
fn repositories(folder:&Path)->Vec<(PathBuf,Vec<String>)> {
    let mut found=Vec::new();
    let mut level=vec![folder.to_path_buf()];
    for depth in 0..=SCAN_DEPTH {
        let mut next=Vec::new();
        for dir in level {
            let keys=repo_keys::of_folder(&dir.to_string_lossy());
            if !keys.is_empty() || dir.join(".git").exists() {
                found.push((dir,keys));
                continue;
            }
            if depth==SCAN_DEPTH {continue;}
            let Ok(entries)=fs::read_dir(&dir) else {continue};
            let mut children:Vec<PathBuf>=entries.filter_map(Result::ok)
                .filter(|entry|entry.file_type().map(|kind|kind.is_dir()).unwrap_or(false))
                .filter(|entry|!skipped(&entry.file_name().to_string_lossy()))
                .map(|entry|entry.path()).collect();
            children.sort();
            next.extend(children);
        }
        level=next;
    }
    found
}

/// Os clones dos repositórios `wanted` dentro de `folder`. Cada chave aparece
/// uma vez, no clone mais raso.
pub fn scan(folder:&Path,wanted:&[String])->Vec<FoundRepository> {
    let wanted:Vec<String>=wanted.iter().map(|key|key.trim().to_lowercase()).collect();
    let mut found:Vec<FoundRepository>=Vec::new();
    for (dir,keys) in repositories(folder) {
        for key in keys {
            if wanted.contains(&key) && !found.iter().any(|item|item.key==key) {
                found.push(FoundRepository{key,path:dir.display().to_string()});
            }
        }
    }
    found.sort_by(|a,b|a.key.cmp(&b.key));
    found
}

/// Os repositórios de uma pasta que junta vários (a pasta da organização),
/// como o modelo os lê: `api/ (github.com/acme/api)`. Pasta que é ela mesma um
/// repositório não junta nada e devolve lista vazia.
pub fn nested(folder:&Path)->Vec<String> {
    let found=repositories(folder);
    if found.iter().any(|(dir,_)|dir==folder) {return vec![];}
    let mut listed:Vec<String>=found.into_iter().map(|(dir,keys)|{
        let relative=dir.strip_prefix(folder).unwrap_or(&dir).to_string_lossy().replace('\\',"/");
        match keys.first() {Some(key)=>format!("{relative}/ ({key})"),None=>format!("{relative}/")}
    }).collect();
    listed.sort();
    listed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clone_of(root:&Path,relative:&str,url:&str)->PathBuf {
        let dir=root.join(relative);
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/config"),format!("[remote \"origin\"]\n\turl = {url}\n")).unwrap();
        dir
    }

    #[test] fn the_scan_finds_clones_of_wanted_repositories_only() {
        let root=tempfile::tempdir().unwrap();
        clone_of(root.path(),"api","git@github.com:acme/api.git");
        clone_of(root.path(),"backend/worker","https://gitlab.com/acme/worker");
        clone_of(root.path(),"other","https://github.com/someone/else");
        clone_of(root.path(),".hidden/web","https://github.com/acme/web");
        let wanted=["github.com/acme/api".to_string(),"gitlab.com/acme/worker".into(),"github.com/acme/web".into()];
        let found=scan(root.path(),&wanted);
        let keys:Vec<&str>=found.iter().map(|item|item.key.as_str()).collect();
        assert_eq!(keys,["github.com/acme/api","gitlab.com/acme/worker"]);
        assert!(found[1].path.ends_with("worker"));
    }

    #[test] fn the_scan_stops_inside_a_repository_and_at_its_depth() {
        let root=tempfile::tempdir().unwrap();
        let outer=clone_of(root.path(),"mono","https://github.com/acme/mono");
        clone_of(&outer,"nested","https://github.com/acme/nested");
        clone_of(root.path(),"a/b/c","https://github.com/acme/deep");
        let wanted=["github.com/acme/mono".to_string(),"github.com/acme/nested".into(),"github.com/acme/deep".into()];
        let keys:Vec<String>=scan(root.path(),&wanted).into_iter().map(|item|item.key).collect();
        assert_eq!(keys,["github.com/acme/mono"]);
    }

    #[test] fn the_folder_itself_can_be_the_clone() {
        let root=tempfile::tempdir().unwrap();
        clone_of(root.path(),"","https://bitbucket.org/acme/site.git");
        assert_eq!(scan(root.path(),&["bitbucket.org/acme/site".into()]).len(),1);
    }

    #[test] fn the_organization_folder_lists_its_repositories() {
        let root=tempfile::tempdir().unwrap();
        clone_of(root.path(),"api","git@github.com:acme/api.git");
        clone_of(root.path(),"backend/worker","https://gitlab.com/acme/worker");
        fs::create_dir_all(root.path().join("scratch/.git")).unwrap();
        assert_eq!(nested(root.path()),["api/ (github.com/acme/api)","backend/worker/ (gitlab.com/acme/worker)","scratch/"]);
        assert!(nested(&root.path().join("api")).is_empty(),"um repositório não junta outros");
    }

    #[test] fn urls_and_folder_come_from_the_key() {
        assert_eq!(clone_urls("gitlab.com/group/sub/app"),["https://gitlab.com/group/sub/app.git".to_string(),"git@gitlab.com:group/sub/app.git".into()]);
        assert_eq!(folder_name("gitlab.com/group/sub/app"),"app");
    }

    #[test] fn only_real_keys_reach_git() {
        assert!(checked("github.com/acme/api").is_ok());
        for bad in ["github.com/-upload-pack=x/api","example.com/acme/api","github.com/acme","github.com/acme/a b"] {
            assert!(checked(bad).is_err(),"{bad}");
        }
    }

    #[test] fn a_folder_with_files_is_not_overwritten() {
        let root=tempfile::tempdir().unwrap();
        assert_eq!(target(root.path(),"github.com/acme/api").unwrap(),root.path().join("api"));
        fs::create_dir(root.path().join("api")).unwrap();
        assert!(target(root.path(),"github.com/acme/api").is_ok(),"pasta vazia serve");
        fs::write(root.path().join("api/README.md"),"x").unwrap();
        assert_eq!(Text::from(target(root.path(),"github.com/acme/api").unwrap_err()).key,"repos.clone.exists");
        assert_eq!(Text::from(target(&root.path().join("missing"),"github.com/acme/api").unwrap_err()).key,"repos.folder.missing");
    }

    #[test] fn access_errors_are_told_apart() {
        assert!(denied("fatal: could not read Username for 'https://github.com': terminal prompts disabled"));
        assert!(denied("git@github.com: Permission denied (publickey)."));
        assert!(!denied("fatal: unable to access 'https://github.com/': Could not resolve host: github.com"));
        assert_eq!(reason("Cloning...\nfatal: boom\n\n"),"fatal: boom");
    }

    #[tokio::test] async fn a_local_clone_lands_in_the_folder() {
        // O caminho inteiro do clone, sem rede: um repositório local faz o
        // papel do servidor.
        let root=tempfile::tempdir().unwrap();
        let origin=root.path().join("origin");
        fs::create_dir(&origin).unwrap();
        let git=|args:&[&str]|assert!(std::process::Command::new("git").args(args).current_dir(&origin).output().unwrap().status.success());
        git(&["init","--quiet"]);
        git(&["-c","user.email=a@b.c","-c","user.name=a","commit","--allow-empty","--quiet","-m","x"]);
        let parent=root.path().join("code");
        fs::create_dir(&parent).unwrap();
        let target=target(&parent,"github.com/acme/api").unwrap();
        assert!(matches!(attempt(&origin.display().to_string(),&target).await.unwrap(),Attempt::Done));
        assert!(target.join(".git").is_dir());
    }
}
