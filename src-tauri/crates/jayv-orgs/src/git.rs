//! O `git` do sistema, achado mesmo quando o app abre pelo menu. Um app de
//! janela herda um `PATH` curto: no macOS, sem o `/opt/homebrew/bin` e o
//! `/usr/local/bin`; no Windows, às vezes sem a pasta `cmd` do Git for
//! Windows. Sem isso o painel de repositórios e o "Ao vivo" ficavam sem git
//! (e sem dizer por quê) num computador que tem git instalado.

use std::{env, path::{Path, PathBuf}, sync::OnceLock};

/// O nome do programa em cada sistema.
const NAME:&str=if cfg!(windows) {"git.exe"} else {"git"};

/// Onde o git costuma morar além do `PATH`, na ordem de preferência.
fn usual_dirs()->Vec<PathBuf> {
    let mut dirs=Vec::new();
    #[cfg(windows)] {
        for variable in ["ProgramW6432","ProgramFiles","ProgramFiles(x86)"] {
            if let Some(root)=env::var_os(variable) { dirs.push(PathBuf::from(root).join("Git").join("cmd")); }
        }
        if let Some(local)=env::var_os("LOCALAPPDATA") { dirs.push(PathBuf::from(local).join("Programs").join("Git").join("cmd")); }
        if let Some(home)=env::var_os("USERPROFILE") { dirs.push(PathBuf::from(home).join("scoop").join("shims")); }
    }
    #[cfg(not(windows))] {
        dirs.extend(["/opt/homebrew/bin","/usr/local/bin","/usr/bin","/home/linuxbrew/.linuxbrew/bin"].map(PathBuf::from));
        if let Some(home)=env::var_os("HOME") { dirs.push(PathBuf::from(home).join(".linuxbrew").join("bin")); }
    }
    dirs
}

fn is_program(path:&Path)->bool {
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; path.metadata().is_ok_and(|meta|meta.is_file()&&meta.permissions().mode()&0o111!=0) }
    #[cfg(not(unix))] { path.is_file() }
}

/// O primeiro git que existe: o do `PATH` e depois os lugares de costume.
/// `None`: não há git neste computador.
pub fn find(path:Option<&std::ffi::OsStr>,extra:&[PathBuf])->Option<PathBuf> {
    let from_path:Vec<PathBuf>=path.map(|value|env::split_paths(value).collect()).unwrap_or_default();
    from_path.into_iter().chain(extra.iter().cloned()).map(|dir|dir.join(NAME)).find(|candidate|is_program(candidate))
}

/// O git deste computador, procurado uma vez. Sem nenhum, fica o nome puro:
/// a chamada falha com "não encontrado" e quem chamou diz que falta o git.
pub fn program()->&'static Path {
    static FOUND:OnceLock<PathBuf>=OnceLock::new();
    FOUND.get_or_init(||find(env::var_os("PATH").as_deref(),&usual_dirs()).unwrap_or_else(||PathBuf::from(NAME)))
}

/// Se há git neste computador.
pub fn installed()->bool {program().is_absolute()}

/// Um `git` para o tokio, sem janela de console no Windows.
pub fn command()->tokio::process::Command {
    let mut process=tokio::process::Command::new(program());
    crate::providers::quiet(&mut process);
    process
}

/// Um `git` sem tokio, sem janela de console no Windows.
pub fn std_command()->std::process::Command {
    let mut process=std::process::Command::new(program());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; process.creation_flags(0x0800_0000); }
    process
}

/// O que deu errado numa chamada ao git, pela mensagem dele: o git não
/// confia na pasta (dona de outro usuário, `safe.directory`), a pasta não é
/// um repositório, ou outra coisa.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Problem{Unsafe,NotRepository,Other}

pub fn problem_of(stderr:&str)->Problem {
    let text=stderr.to_lowercase();
    if text.contains("dubious ownership")||text.contains("safe.directory") {Problem::Unsafe}
    else if text.contains("not a git repository") {Problem::NotRepository}
    else {Problem::Other}
}

/// A última linha do git que diz alguma coisa.
pub fn reason(stderr:&str)->Option<String> {
    stderr.lines().map(str::trim).filter(|line|!line.is_empty()).last().map(|line|line.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test] fn the_git_outside_the_path_is_found_in_the_usual_places() {
        use std::os::unix::fs::PermissionsExt;
        let empty=tempfile::tempdir().unwrap();
        let brew=tempfile::tempdir().unwrap();
        let program=brew.path().join("git");
        std::fs::write(&program,"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&program,std::fs::Permissions::from_mode(0o755)).unwrap();
        let path=env::join_paths([empty.path()]).unwrap();
        assert_eq!(find(Some(&path),&[brew.path().to_path_buf()]),Some(program.clone()),"fora do PATH, no lugar de costume");
        let path=env::join_paths([brew.path()]).unwrap();
        assert_eq!(find(Some(&path),&[]),Some(program),"no PATH");
        assert_eq!(find(Some(&env::join_paths([empty.path()]).unwrap()),&[]),None,"sem git nenhum");
    }

    #[test] fn the_message_of_git_says_what_went_wrong() {
        assert_eq!(problem_of("fatal: detected dubious ownership in repository at 'D:/acme/api'\nTo add an exception..."),Problem::Unsafe);
        assert_eq!(problem_of("fatal: not a git repository (or any of the parent directories): .git"),Problem::NotRepository);
        assert_eq!(problem_of("fatal: bad object HEAD"),Problem::Other);
        assert_eq!(reason("warning: x\nfatal: bad object HEAD\n\n").as_deref(),Some("fatal: bad object HEAD"));
        assert_eq!(reason("  \n"),None);
    }
}
