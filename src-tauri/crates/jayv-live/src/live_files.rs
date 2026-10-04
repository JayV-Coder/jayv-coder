//! Os arquivos que o agente altera durante um pedido, vistos enquanto mudam.
//!
//! Nada de vigia do sistema de arquivos: a cada segundo o `git status` de cada
//! repositório da pasta diz quais arquivos estão diferentes do HEAD, e o
//! carimbo (data e tamanho) de cada um diz se ele mudou desde a última
//! olhada. O git já respeita o `.gitignore`, só lista o que mudou e lê o
//! índice sem travá-lo (`GIT_OPTIONAL_LOCKS=0`), então o custo não cresce com
//! o tamanho do repositório. Pasta sem git é varrida pela data de
//! modificação, com teto de arquivos.
//!
//! O "antes" de cada arquivo é como ele estava quando o pedido começou: o
//! conteúdo guardado na largada para quem já estava alterado, e o do commit
//! em que o HEAD estava na largada para quem estava limpo — o agente pode
//! fazer commit no meio do pedido. Tudo fica neste computador.

use crate::firewall::ContextFirewall;
use serde::Serialize;
use std::{collections::{BTreeSet, HashMap}, fs, path::{Path, PathBuf}, process::{Command, Stdio}, time::{SystemTime, UNIX_EPOCH}};

/// Arquivo maior que isto aparece na lista, mas sem conteúdo.
pub const MAX_BYTES:u64=1024*1024;
/// Quanto conteúdo da largada uma sessão guarda, somando todos os arquivos.
const BASELINE_BUDGET:usize=20*1024*1024;
/// Até quanto tempo depois da gravação a impressão do conteúdo é conferida.
const RECENT:std::time::Duration=std::time::Duration::from_secs(5);
/// O teto da varredura de uma pasta sem git.
const PLAIN_LIMIT:usize=20_000;

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize)]
#[serde(rename_all="lowercase")]
pub enum ChangeKind{Created,Modified,Removed,
    /// Saiu da lista: nasceu e sumiu durante o pedido. Só aparece no aviso à
    /// tela, nunca na lista guardada.
    Discarded}

/// Um arquivo que mudou durante o pedido. `path` é relativo à pasta do chat,
/// com `/`; `at` é a última mudança, em milissegundos desde 1970.
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Change{pub path:String,pub kind:ChangeKind,pub at:i64}

/// Um arquivo como a tela o mostra: o antes e o agora. `before` nulo com
/// `before_known` falso é a pasta sem git, que não tem como saber o antes.
#[derive(Debug,Clone,Default,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct FileView{
    pub path:String,
    pub before:Option<String>,
    pub after:Option<String>,
    pub before_known:bool,
    /// Coberto por `privacy.deny`: só o nome aparece.
    pub hidden:bool,
    pub binary:bool,
    pub too_large:bool,
}

#[derive(Debug,Clone,PartialEq,Eq)]
enum Baseline{Text(String),Missing,Unknown}

/// O carimbo de um arquivo: data e tamanho; nulo quando ele não existe.
type Stamp=Option<(SystemTime,u64)>;

fn stamp(path:&Path)->Stamp {
    let meta=fs::metadata(path).ok().filter(|meta|meta.is_file())?;
    Some((meta.modified().unwrap_or(UNIX_EPOCH),meta.len()))
}

/// A impressão do conteúdo de um arquivo pequeno gravado há pouco. Sistemas de
/// arquivos de relógio grosso (FAT, pastas de rede, o `/mnt/c` do WSL) deixam a
/// data igual entre duas gravações seguidas; com o mesmo tamanho, só o
/// conteúdo diz que o arquivo mudou.
fn fingerprint(path:&Path,stamp:Stamp)->Option<u64> {
    use std::hash::{Hash, Hasher};
    let (modified,size)=stamp?;
    let recent=SystemTime::now().duration_since(modified).map(|age|age<RECENT).unwrap_or(true);
    if !recent || size>MAX_BYTES {return None;}
    let bytes=fs::read(path).ok()?;
    let mut hasher=std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

fn now_ms()->i64 {SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed|elapsed.as_millis() as i64).unwrap_or(0)}

fn git(dir:&Path)->Command {
    let mut command=Command::new("git");
    command.current_dir(dir).env("GIT_OPTIONAL_LOCKS","0").env("GIT_TERMINAL_PROMPT","0").stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    command
}

/// Lê `git status --porcelain=v1 -z`: os caminhos (relativos ao repositório)
/// que estão diferentes do HEAD, inclusive os novos. Num rename vale o nome
/// novo e o antigo, que sumiu.
pub fn parse_porcelain(output:&[u8])->Vec<String> {
    let mut paths=Vec::new();
    let mut parts=output.split(|byte|*byte==0).filter(|part|!part.is_empty());
    while let Some(entry)=parts.next() {
        if entry.len()<4 {continue;}
        let status=&entry[..2];
        paths.push(String::from_utf8_lossy(&entry[3..]).into_owned());
        if status.contains(&b'R')||status.contains(&b'C') {
            if let Some(origin)=parts.next() {paths.push(String::from_utf8_lossy(origin).into_owned());}
        }
    }
    paths
}

/// Um repositório olhado. `within` é a pasta do chat quando ela fica dentro
/// do repositório (um projeto aberto numa subpasta): só o que está nela conta.
struct Repo{dir:PathBuf,head:Option<String>,within:Option<PathBuf>}

impl Repo {
    fn dirty(&self)->Vec<PathBuf> {
        let Ok(output)=git(&self.dir).args(["status","--porcelain=v1","-z","--untracked-files=all"]).output() else {return vec![]};
        if !output.status.success() {return vec![];}
        parse_porcelain(&output.stdout).into_iter().map(|path|self.dir.join(path))
            .filter(|path|self.within.as_ref().is_none_or(|within|path.starts_with(within))).collect()
    }

    /// O conteúdo do arquivo no commit da largada; `None` quando ele não
    /// existia lá (ou o repositório ainda não tinha commit).
    fn at_start(&self,path:&Path)->Option<Vec<u8>> {
        let head=self.head.as_ref()?;
        let relative=path.strip_prefix(&self.dir).ok()?.to_string_lossy().replace('\\',"/");
        let output=git(&self.dir).arg("show").arg(format!("{head}:{relative}")).stdout(Stdio::piped()).output().ok()?;
        output.status.success().then_some(output.stdout)
    }

    fn existed(&self,path:&Path)->bool {
        let (Some(head),Ok(relative))=(&self.head,path.strip_prefix(&self.dir)) else {return false};
        let relative=relative.to_string_lossy().replace('\\',"/");
        git(&self.dir).args(["cat-file","-e"]).arg(format!("{head}:{relative}")).stdout(Stdio::null()).status().is_ok_and(|status|status.success())
    }
}

fn head_of(dir:&Path)->Option<String> {
    let output=git(dir).args(["rev-parse","--verify","--quiet","HEAD"]).stdout(Stdio::piped()).output().ok()?;
    output.status.success().then(||String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|head|!head.is_empty())
}

/// O repositório em volta de uma pasta que não é a raiz dele: o projeto
/// aberto numa subpasta (`apps/web` de um monorepo). O `git status` lista os
/// caminhos a partir da raiz do repositório, então a raiz é a pasta do chat
/// subindo tantos níveis quantos o `--show-prefix` disser, escrita do mesmo
/// jeito que a pasta do chat.
fn enclosing(folder:&Path)->Option<Repo> {
    let output=git(folder).args(["rev-parse","--show-prefix"]).stdout(Stdio::piped()).output().ok()?;
    if !output.status.success() {return None;}
    let prefix=String::from_utf8_lossy(&output.stdout).trim().to_string();
    let depth=prefix.split('/').filter(|part|!part.is_empty()).count();
    if depth==0 {return None;}
    let dir=folder.ancestors().nth(depth)?.to_path_buf();
    Some(Repo{head:head_of(&dir),dir,within:Some(folder.to_path_buf())})
}

fn skipped(name:&str)->bool {name.starts_with('.')||matches!(name,"node_modules"|"target"|"vendor"|"dist"|"build"|"__pycache__")}

/// O que um pedido mudou na pasta do chat, desde a largada.
pub struct Session{
    pub turn_id:String,
    folder:PathBuf,
    repos:Vec<Repo>,
    /// A largada com um segundo de folga: o relógio do sistema de arquivos
    /// anda em passos mais grossos que o do processo.
    started:SystemTime,
    baseline:HashMap<PathBuf,Baseline>,
    seen:HashMap<PathBuf,Stamp>,
    /// A impressão do conteúdo dos arquivos da lista gravados há pouco.
    prints:HashMap<PathBuf,u64>,
    changes:Vec<Change>,
    pub running:bool,
    firewall:ContextFirewall,
}

impl Session {
    /// A largada: os repositórios da pasta, o HEAD de cada um e o conteúdo dos
    /// arquivos que já estavam alterados antes do pedido.
    pub fn start(turn_id:&str,folder:&Path,firewall:ContextFirewall)->Self {
        let mut repos:Vec<Repo>=crate::checkout::repository_dirs(folder).into_iter().map(|dir|{let head=head_of(&dir);Repo{dir,head,within:None}}).collect();
        if repos.is_empty() {repos.extend(enclosing(folder));}
        let mut session=Session{turn_id:turn_id.into(),folder:folder.to_path_buf(),repos,started:SystemTime::now()-std::time::Duration::from_secs(1),baseline:HashMap::new(),seen:HashMap::new(),prints:HashMap::new(),changes:vec![],running:true,firewall};
        let mut budget=BASELINE_BUDGET;
        for path in session.repos.iter().flat_map(Repo::dirty) {
            let found=stamp(&path);
            let baseline=match found {
                None=>Baseline::Missing,
                Some((_,size)) if size<=MAX_BYTES && (size as usize)<=budget=>match fs::read(&path) {
                    Ok(bytes)=>{budget-=bytes.len();String::from_utf8(bytes).map(Baseline::Text).unwrap_or(Baseline::Unknown)}
                    Err(_)=>Baseline::Unknown,
                },
                Some(_)=>Baseline::Unknown,
            };
            session.baseline.insert(path.clone(),baseline);
            session.seen.insert(path,found);
        }
        session
    }

    pub fn changes(&self)->&[Change] {&self.changes}

    fn relative(&self,path:&Path)->String {
        path.strip_prefix(&self.folder).unwrap_or(path).to_string_lossy().replace('\\',"/")
    }

    fn repo_of(&self,path:&Path)->Option<&Repo> {
        self.repos.iter().filter(|repo|path.starts_with(&repo.dir)).max_by_key(|repo|repo.dir.components().count())
    }

    /// Os arquivos que podem ter mudado: os que o git lista agora e os que já
    /// mudaram neste pedido (podem ter voltado a ficar iguais ao HEAD).
    fn candidates(&self)->BTreeSet<PathBuf> {
        let mut paths:BTreeSet<PathBuf>=self.changes.iter().map(|change|self.folder.join(&change.path)).collect();
        if self.repos.is_empty() {
            let walker=walkdir::WalkDir::new(&self.folder).into_iter()
                .filter_entry(|entry|entry.depth()==0||!skipped(&entry.file_name().to_string_lossy()));
            for entry in walker.filter_map(Result::ok).take(PLAIN_LIMIT) {
                if !entry.file_type().is_file() {continue;}
                let fresh=entry.metadata().ok().and_then(|meta|meta.modified().ok()).is_some_and(|modified|modified>=self.started);
                if fresh {paths.insert(entry.into_path());}
            }
        } else {
            for repo in &self.repos {paths.extend(repo.dirty());}
        }
        paths
    }

    /// Uma olhada: devolve o que mudou desde a anterior.
    pub fn poll(&mut self)->Vec<Change> {
        let mut fresh=Vec::new();
        for path in self.candidates() {
            let found=stamp(&path);
            let before=match self.seen.get(&path) {
                Some(previous)=>Some(*previous),
                // Um arquivo limpo na largada: ele existia como está no HEAD.
                None if !self.repos.is_empty()=>None,
                None=>Some(None),
            };
            let print=self.prints.contains_key(&path).then(||fingerprint(&path,found)).flatten();
            if before==Some(found) && (print.is_none() || self.prints.get(&path)==print.as_ref()) {continue;}
            if before.is_none() && self.repo_of(&path).is_none() {continue;}
            self.seen.insert(path.clone(),found);
            let existed=self.existed_at_start(&path);
            let relative=self.relative(&path);
            // Nasceu e sumiu durante o pedido (o arquivo temporário de uma
            // gravação atômica, um rascunho apagado): no fim, nada mudou.
            if found.is_none() && !existed {
                if self.changes.iter().any(|item|item.path==relative) {
                    self.changes.retain(|item|item.path!=relative);
                    fresh.push(Change{path:relative,kind:ChangeKind::Discarded,at:now_ms()});
                }
                continue;
            }
            let kind=if found.is_none() {ChangeKind::Removed} else if existed {ChangeKind::Modified} else {ChangeKind::Created};
            let change=Change{path:relative,kind,at:now_ms()};
            match print.or_else(||fingerprint(&path,found)) {Some(print)=>{self.prints.insert(path.clone(),print);} None=>{self.prints.remove(&path);}}
            self.changes.retain(|item|item.path!=change.path);
            self.changes.insert(0,change.clone());
            fresh.push(change);
        }
        fresh
    }

    fn existed_at_start(&self,path:&Path)->bool {
        match self.baseline.get(path) {
            Some(Baseline::Missing)=>false,
            Some(_)=>true,
            None=>match self.repo_of(path) {Some(repo)=>repo.existed(path), None=>true},
        }
    }

    /// O antes e o agora de um arquivo da lista. Fora da pasta, nada.
    pub fn view(&self,relative:&str)->Option<FileView> {
        let relative=relative.trim().trim_start_matches("./");
        if relative.split(['/','\\']).any(|part|part==".."||part.is_empty()) {return None;}
        // Um caminho absoluto (`/etc/x`, `C:\x`) troca a pasta inteira no
        // `join`: só caminhos relativos, sem raiz nem letra de unidade.
        if std::path::Path::new(relative).components().any(|part|!matches!(part,std::path::Component::Normal(_))) || relative.contains(':') {return None;}
        let path=self.folder.join(relative);
        let mut view=FileView{path:relative.to_string(),before_known:true,..Default::default()};
        if self.firewall.check_file(relative).is_sensitive {view.hidden=true;return Some(view);}
        let decode=|bytes:Vec<u8>,view:&mut FileView|->Option<String> {
            if bytes.len() as u64>MAX_BYTES {view.too_large=true;return None;}
            if bytes.contains(&0) {view.binary=true;return None;}
            Some(String::from_utf8_lossy(&bytes).into_owned())
        };
        let after=match stamp(&path) {
            Some((_,size)) if size>MAX_BYTES=>{view.too_large=true;None}
            Some(_)=>fs::read(&path).ok(),
            None=>None,
        };
        view.after=after.and_then(|bytes|decode(bytes,&mut view));
        view.before=match self.baseline.get(&path) {
            Some(Baseline::Text(text))=>Some(text.clone()),
            Some(Baseline::Missing)=>None,
            Some(Baseline::Unknown)=>{view.before_known=false;None}
            None=>match self.repo_of(&path) {
                Some(repo)=>repo.at_start(&path).and_then(|bytes|decode(bytes,&mut view)),
                None=>{view.before_known=false;None}
            },
        };
        Some(view)
    }

    /// O caminho de verdade de um arquivo da lista, para abrir no editor.
    pub fn absolute(&self,relative:&str)->Option<PathBuf> {
        self.changes.iter().any(|change|change.path==relative).then(||self.folder.join(relative))
    }
}

/// Os editores com linha de comando que este computador tem, na ordem de
/// preferência: os que entendem `-r -g arquivo:linha` (VS Code e os
/// derivados dele).
pub const EDITORS:[&str;5]=["code","cursor","windsurf","code-insiders","codium"];

pub fn find_editor(name:&str)->Option<PathBuf> {
    let paths=std::env::var_os("PATH")?;
    let names:Vec<String>=if cfg!(windows) {vec![format!("{name}.cmd"),format!("{name}.exe"),name.to_string()]} else {vec![name.to_string()]};
    std::env::split_paths(&paths).flat_map(|dir|names.iter().map(move |file|dir.join(file))).find(|path|path.is_file())
}

pub fn editors()->Vec<String> {EDITORS.iter().filter(|name|find_editor(name).is_some()).map(|name|name.to_string()).collect()}

/// Abre o arquivo na janela do editor que já está aberta, na linha pedida.
pub fn open_in_editor(editor:&str,file:&Path,line:u32)->std::io::Result<()> {
    if !EDITORS.contains(&editor) {return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,"unknown editor"));}
    let program=find_editor(editor).ok_or_else(||std::io::Error::new(std::io::ErrorKind::NotFound,"editor not found"))?;
    let mut command=Command::new(program);
    command.args(["-r","-g"]).arg(format!("{}:{}",file.display(),line.max(1))).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    command.spawn().map(|_|())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PrivacyConfig;

    fn run(dir:&Path,args:&[&str]) {assert!(Command::new("git").args(args).current_dir(dir).output().unwrap().status.success(),"{args:?}");}

    fn repo()->tempfile::TempDir {
        let root=tempfile::tempdir().unwrap();
        run(root.path(),&["init","--quiet","-b","main"]);
        fs::write(root.path().join("lib.rs"),"fn a() {}\n").unwrap();
        fs::write(root.path().join("old.rs"),"x\n").unwrap();
        fs::write(root.path().join(".gitignore"),"target/\n").unwrap();
        run(root.path(),&["add","."]);
        run(root.path(),&["-c","user.email=a@b.c","-c","user.name=a","commit","--quiet","-m","x"]);
        root
    }

    fn open(root:&Path)->Session {Session::start("t",root,ContextFirewall::new(PrivacyConfig{deny:vec!["*.env".into()],..Default::default()}))}

    #[test] fn porcelain_paths_include_both_sides_of_a_rename() {
        assert_eq!(parse_porcelain(b" M src/lib.rs\0?? new.txt\0R  b.rs\0a.rs\0"),["src/lib.rs","new.txt","b.rs","a.rs"]);
    }

    #[test] fn changes_are_seen_with_their_kind_and_before() {
        let root=repo();
        let mut session=open(root.path());
        assert!(session.poll().is_empty());
        fs::write(root.path().join("lib.rs"),"fn a() {}\nfn b() {}\n").unwrap();
        fs::write(root.path().join("new.rs"),"fn c() {}\n").unwrap();
        fs::remove_file(root.path().join("old.rs")).unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        fs::write(root.path().join("target/out.o"),"bin").unwrap();
        let mut kinds:Vec<(String,ChangeKind)>=session.poll().into_iter().map(|change|(change.path,change.kind)).collect();
        kinds.sort();
        assert_eq!(kinds,[("lib.rs".into(),ChangeKind::Modified),("new.rs".into(),ChangeKind::Created),("old.rs".into(),ChangeKind::Removed)]);
        assert!(session.poll().is_empty(),"nada mudou desde a última olhada");
        let view=session.view("lib.rs").unwrap();
        assert_eq!((view.before.as_deref(),view.after.as_deref()),(Some("fn a() {}\n"),Some("fn a() {}\nfn b() {}\n")));
        assert_eq!(session.view("new.rs").unwrap().before,None);
        assert!(session.view("../outside").is_none());
    }

    #[test] fn a_file_already_dirty_keeps_its_state_from_the_start() {
        let root=repo();
        fs::write(root.path().join("lib.rs"),"fn mine() {}\n").unwrap();
        let mut session=open(root.path());
        assert!(session.poll().is_empty(),"o que já estava alterado antes do pedido não conta");
        fs::write(root.path().join("lib.rs"),"fn mine() {}\nfn agent() {}\n").unwrap();
        assert_eq!(session.poll().len(),1);
        assert_eq!(session.view("lib.rs").unwrap().before.as_deref(),Some("fn mine() {}\n"));
    }

    #[test] fn a_commit_in_the_middle_still_compares_with_the_start() {
        let root=repo();
        let mut session=open(root.path());
        fs::write(root.path().join("lib.rs"),"fn changed() {}\n").unwrap();
        assert_eq!(session.poll().len(),1);
        run(root.path(),&["-c","user.email=a@b.c","-c","user.name=a","commit","--quiet","-am","y"]);
        assert!(session.poll().is_empty(),"o commit não muda o arquivo");
        assert_eq!(session.view("lib.rs").unwrap().before.as_deref(),Some("fn a() {}\n"));
    }

    #[test] fn denied_files_show_only_the_name() {
        let root=repo();
        let mut session=open(root.path());
        fs::write(root.path().join("prod.env"),"KEY=1\n").unwrap();
        assert_eq!(session.poll().len(),1);
        let view=session.view("prod.env").unwrap();
        assert!(view.hidden && view.after.is_none());
    }

    #[test] fn a_project_in_a_subfolder_of_the_repository_uses_git() {
        let root=repo();
        fs::create_dir_all(root.path().join("apps/web")).unwrap();
        fs::write(root.path().join("apps/web/page.ts"),"old\n").unwrap();
        run(root.path(),&["add","."]);
        run(root.path(),&["-c","user.email=a@b.c","-c","user.name=a","commit","--quiet","-m","web"]);
        let folder=root.path().join("apps/web");
        let mut session=open(&folder);
        fs::write(folder.join("page.ts"),"new\n").unwrap();
        fs::write(root.path().join("lib.rs"),"fora da pasta\n").unwrap();
        let changes=session.poll();
        assert_eq!(changes.iter().map(|change|(change.path.as_str(),change.kind)).collect::<Vec<_>>(),[("page.ts",ChangeKind::Modified)]);
        let view=session.view("page.ts").unwrap();
        assert!(view.before_known,"dentro do repositório o antes vem do git");
        assert_eq!(view.before.as_deref(),Some("old\n"));
    }

    #[test] fn a_file_created_and_removed_during_the_request_leaves_the_list() {
        let root=repo();
        let mut session=open(root.path());
        fs::write(root.path().join("lib.rs.tmp"),"rascunho").unwrap();
        assert_eq!(session.poll().len(),1);
        fs::rename(root.path().join("lib.rs.tmp"),root.path().join("lib.rs")).unwrap();
        let mut seen:Vec<(String,ChangeKind)>=session.poll().into_iter().map(|change|(change.path,change.kind)).collect();
        seen.sort();
        assert_eq!(seen,[("lib.rs".into(),ChangeKind::Modified),("lib.rs.tmp".into(),ChangeKind::Discarded)]);
        assert_eq!(session.changes().iter().map(|change|change.path.as_str()).collect::<Vec<_>>(),["lib.rs"]);
    }

    #[test] fn a_rewrite_with_the_same_date_and_size_is_seen() {
        let root=repo();
        let mut session=open(root.path());
        let path=root.path().join("lib.rs");
        fs::write(&path,"fn b() {}\n").unwrap();
        let first=fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(session.poll().len(),1);
        // Um sistema de arquivos de relógio grosso: a data não anda.
        fs::write(&path,"fn c() {}\n").unwrap();
        fs::File::options().write(true).open(&path).unwrap().set_modified(first).unwrap();
        assert_eq!(session.poll().len(),1,"o conteúdo mudou mesmo com data e tamanho iguais");
        assert!(session.poll().is_empty());
    }

    #[test] fn a_plain_folder_is_scanned_by_date() {
        let root=tempfile::tempdir().unwrap();
        let mut session=open(root.path());
        fs::create_dir(root.path().join("node_modules")).unwrap();
        fs::write(root.path().join("node_modules/x.js"),"x").unwrap();
        fs::write(root.path().join("notes.md"),"hi").unwrap();
        let changes=session.poll();
        assert_eq!(changes.iter().map(|change|change.path.as_str()).collect::<Vec<_>>(),["notes.md"]);
        assert!(!session.view("notes.md").unwrap().before_known);
    }
}
