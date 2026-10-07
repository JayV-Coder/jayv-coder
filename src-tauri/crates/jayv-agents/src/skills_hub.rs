//! O skills.sh: o diretório aberto de skills. A busca é a mesma da CLI
//! (`GET https://skills.sh/api/search?q=…`, que devolve `{skills:[{id,name,
//! installs,source}]}`) e a instalação lê a skill do repositório do GitHub que
//! o resultado aponta (`source` = `dono/repo`): acha a pasta com o `SKILL.md`
//! da skill pela árvore do repositório e baixa os arquivos dela. A skill
//! instalada é uma skill comum (`skills::install_folder`).
//!
//! A rede fica atrás de `Web`, para os testes rodarem sem ela.

use crate::i18n::Text;
use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

pub const SEARCH_URL:&str="https://skills.sh/api/search";
pub const TREE_URL:&str="https://api.github.com/repos";
pub const RAW_URL:&str="https://raw.githubusercontent.com";
/// Quantos resultados a busca traz.
pub const LIMIT:usize=20;
/// Quantas pastas candidatas se abrem à procura do nome no cabeçalho.
const CANDIDATES_MAX:usize=12;
const FILES_MAX:usize=300;
const BYTES_MAX:u64=10*1024*1024;
const SKIPPED:[&str;3]=[".git","node_modules","target"];

/// Um resultado da busca: `source` é o repositório (`dono/repo`) e `name` o
/// nome da skill nele.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Hit { pub id:String, pub name:String, pub source:String, pub installs:u64 }

/// A rede que a busca e a instalação usam.
#[async_trait]
pub trait Web: Send + Sync {
    async fn get(&self,url:&str)->Result<Vec<u8>>;
}

/// A rede de verdade: HTTPS, com prazo e sem páginas enormes.
pub struct Live { client:reqwest::Client }

impl Default for Live {
    fn default()->Self { Self{client:reqwest::Client::builder().user_agent("JayV").timeout(std::time::Duration::from_secs(25)).build().unwrap_or_default()} }
}

#[async_trait]
impl Web for Live {
    async fn get(&self,url:&str)->Result<Vec<u8>> {
        let response=self.client.get(url).send().await.map_err(|error|anyhow::anyhow!(Text::new("skills.hub.network").with("reason",error.to_string())))?;
        if !response.status().is_success() { bail!(Text::new("skills.hub.network").with("reason",format!("{} {url}",response.status()))); }
        if response.content_length().is_some_and(|length|length>BYTES_MAX) { bail!(Text::new("skills.invalid.size")); }
        Ok(response.bytes().await.map_err(|error|anyhow::anyhow!(Text::new("skills.hub.network").with("reason",error.to_string())))?.to_vec())
    }
}

fn text(value:&Value,key:&str)->String { value.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string() }

/// Lê a resposta da busca. Aceita só o que dá para instalar: um `source` no
/// formato `dono/repo` e um nome de skill válido (o `name`, ou o último pedaço
/// do `id` quando falta).
pub fn parse_hits(body:&str)->Vec<Hit> {
    let Ok(value)=serde_json::from_str::<Value>(body) else { return vec![] };
    let list=value.get("skills").cloned().unwrap_or(value);
    let Some(items)=list.as_array() else { return vec![] };
    items.iter().filter_map(|item|{
        let id=text(item,"id");
        let mut source=text(item,"source");
        let mut name=text(item,"name");
        if source.is_empty() { source=id.rsplitn(2,'/').nth(1).unwrap_or_default().to_string(); }
        if name.is_empty() { name=id.rsplit('/').next().unwrap_or_default().to_string(); }
        valid_source(&source)?;
        valid_name(&name).then_some(())?;
        let installs=item.get("installs").and_then(Value::as_u64).unwrap_or(0);
        Some(Hit{id:if id.is_empty() { format!("{source}/{name}") } else { id },name,source,installs})
    }).collect()
}

/// `dono/repo`, só com o que o GitHub aceita num nome: nada de `..` nem de
/// barra a mais, para o endereço montado ser sempre o do repositório.
pub fn valid_source(source:&str)->Option<(&str,&str)> {
    let (owner,repo)=source.split_once('/')?;
    let part=|part:&str|!part.is_empty()&&part.len()<=100&&part!="."&&part!=".."&&part.chars().all(|char|char.is_ascii_alphanumeric()||matches!(char,'-'|'_'|'.'));
    (part(owner)&&part(repo)).then_some((owner,repo))
}

/// O mesmo nome que `skills::parse` aceita: sem ponto, sem barra.
fn valid_name(name:&str)->bool { !name.is_empty()&&name.len()<=64&&name.chars().all(|char|char.is_ascii_alphanumeric()||matches!(char,'-'|'_')) }

fn encode(query:&str)->String {
    query.bytes().map(|byte|if byte.is_ascii_alphanumeric()||matches!(byte,b'-'|b'_'|b'.'|b'~') { (byte as char).to_string() } else { format!("%{byte:02X}") }).collect()
}

/// Procura skills no skills.sh. Busca vazia ou de uma letra só não vai à rede.
pub async fn search(web:&dyn Web,query:&str)->Result<Vec<Hit>> {
    let query=query.trim();
    if query.chars().count()<2 { return Ok(vec![]); }
    let body=web.get(&format!("{SEARCH_URL}?q={}&limit={LIMIT}",encode(query))).await?;
    Ok(parse_hits(&String::from_utf8_lossy(&body)))
}

#[derive(Debug,Deserialize)]
struct Tree { #[serde(default)] tree:Vec<Entry>, #[serde(default)] truncated:bool }
#[derive(Debug,Clone,Deserialize)]
struct Entry { path:String, #[serde(rename="type")] kind:String, #[serde(default)] size:u64 }

/// As pastas do repositório que têm um `SKILL.md` (a raiz é `""`), com a
/// pasta que casa com o nome pelo último pedaço primeiro.
fn skill_dirs(tree:&[Entry],name:&str)->Vec<String> {
    let mut dirs:Vec<String>=tree.iter().filter(|entry|entry.kind=="blob").filter_map(|entry|{
        if entry.path=="SKILL.md" { return Some(String::new()); }
        entry.path.strip_suffix("/SKILL.md").map(str::to_string)
    }).filter(|dir|!dir.split('/').any(|part|SKIPPED.contains(&part))).collect();
    dirs.sort_by_key(|dir|{
        let leaf=dir.rsplit('/').next().unwrap_or_default();
        (if leaf==name {0} else if leaf.eq_ignore_ascii_case(name) {1} else {2},dir.matches('/').count(),dir.clone())
    });
    dirs.dedup();
    dirs
}

fn raw_url(owner:&str,repo:&str,path:&str)->String {
    let path=path.split('/').map(encode).collect::<Vec<_>>().join("/");
    format!("{RAW_URL}/{owner}/{repo}/HEAD/{path}")
}

/// Baixa a skill `name` do repositório `source` para uma pasta nova dentro de
/// `staging` e devolve o caminho dela, pronto para `skills::install_folder`.
/// A pasta é a que casa com o nome (pelo nome da pasta ou pelo `name` do
/// cabeçalho do `SKILL.md`).
pub async fn download(web:&dyn Web,source:&str,name:&str,staging:&Path)->Result<PathBuf> {
    let Some((owner,repo))=valid_source(source) else { bail!(Text::new("skills.hub.badSource")) };
    if !valid_name(name) { bail!(Text::new("skills.hub.badSource")); }
    let listing=web.get(&format!("{TREE_URL}/{owner}/{repo}/git/trees/HEAD?recursive=1")).await?;
    let tree:Tree=serde_json::from_slice(&listing).map_err(|_|anyhow::anyhow!(Text::new("skills.hub.notFound").with("name",name).with("source",source)))?;
    let dirs=skill_dirs(&tree.tree,name);
    let mut chosen=None;
    for dir in dirs.iter().take(CANDIDATES_MAX) {
        let leaf=dir.rsplit('/').next().unwrap_or_default();
        let file=if dir.is_empty() { "SKILL.md".to_string() } else { format!("{dir}/SKILL.md") };
        let body=String::from_utf8_lossy(&web.get(&raw_url(owner,repo,&file)).await?).to_string();
        let parsed=crate::skills::parse(&body).ok();
        if parsed.as_ref().map_or(leaf==name,|document|document.name==name) { chosen=Some(dir.clone()); break; }
    }
    let Some(dir)=chosen else { bail!(Text::new("skills.hub.notFound").with("name",name).with("source",source)) };
    let prefix=if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let files:Vec<&Entry>=tree.tree.iter().filter(|entry|entry.kind=="blob"&&entry.path.starts_with(&prefix)&&!entry.path[prefix.len()..].split('/').any(|part|SKIPPED.contains(&part))).collect();
    // A raiz do repositório inteira não é uma skill: só o que fica na pasta dela, e dentro do limite.
    if files.len()>FILES_MAX||files.iter().map(|entry|entry.size).sum::<u64>()>BYTES_MAX||(tree.truncated&&files.is_empty()) { bail!(Text::new("skills.invalid.size")); }
    let target=staging.join(name);
    let _=std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target)?;
    for entry in files {
        let relative=Path::new(&entry.path[prefix.len()..]);
        // O caminho vem da rede: nada de sair da pasta da skill.
        if relative.components().any(|part|!matches!(part,Component::Normal(_))) { continue; }
        let bytes=web.get(&raw_url(owner,repo,&entry.path)).await?;
        let destination=target.join(relative);
        if let Some(parent)=destination.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(destination,bytes)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake(HashMap<String,Vec<u8>>);
    #[async_trait]
    impl Web for Fake {
        async fn get(&self,url:&str)->Result<Vec<u8>> { self.0.get(url).cloned().ok_or_else(||anyhow::anyhow!("404 {url}")) }
    }

    fn skill(name:&str)->String { format!("---\nname: {name}\ndescription: Does {name}.\n---\nSteps for {name}") }

    fn repo()->Fake {
        let tree=serde_json::json!({"tree":[
            {"path":"README.md","type":"blob","size":10},
            {"path":"skills","type":"tree"},
            {"path":"skills/pdf/SKILL.md","type":"blob","size":50},
            {"path":"skills/pdf/scripts/run.py","type":"blob","size":9},
            {"path":"skills/pdf/node_modules/x.js","type":"blob","size":9},
            {"path":"skills/other/SKILL.md","type":"blob","size":50},
            {"path":"docs/renamed/SKILL.md","type":"blob","size":50},
        ],"truncated":false});
        let mut pages:HashMap<String,Vec<u8>>=HashMap::new();
        pages.insert(format!("{TREE_URL}/acme/skills/git/trees/HEAD?recursive=1"),tree.to_string().into_bytes());
        for (path,body) in [("skills/pdf/SKILL.md",skill("pdf")),("skills/pdf/scripts/run.py","print(1)".into()),("skills/other/SKILL.md",skill("other")),("docs/renamed/SKILL.md",skill("nice-name"))] {
            pages.insert(raw_url("acme","skills",path),body.into_bytes());
        }
        Fake(pages)
    }

    #[test] fn the_search_answer_keeps_only_what_can_be_installed() {
        let body=r#"{"skills":[{"id":"acme/skills/pdf","name":"pdf","installs":1200,"source":"acme/skills"},{"id":"x/y/z","installs":3},{"id":"bad","name":"a b","source":"../etc"},{"name":"ok","source":"a/b.c"},{"name":"dotted.name","source":"a/b"}]}"#;
        let hits=parse_hits(body);
        assert_eq!(hits.iter().map(|hit|(hit.source.as_str(),hit.name.as_str(),hit.installs)).collect::<Vec<_>>(),[("acme/skills","pdf",1200),("x/y","z",3),("a/b.c","ok",0)]);
        assert!(parse_hits("not json").is_empty()&&parse_hits("{}").is_empty());
        assert_eq!(valid_source("../x"),None);
        assert_eq!(valid_source("a/b/c"),None);
        assert_eq!(valid_source("a/b"),Some(("a","b")));
    }

    #[tokio::test] async fn a_short_query_never_goes_to_the_network() {
        let none=Fake(HashMap::new());
        assert!(search(&none,"a").await.expect("busca").is_empty());
        assert!(search(&none,"  ").await.expect("busca").is_empty());
        let mut pages=HashMap::new();
        pages.insert(format!("{SEARCH_URL}?q=pdf%20forms&limit={LIMIT}"),br#"{"skills":[{"id":"acme/skills/pdf","name":"pdf","installs":5,"source":"acme/skills"}]}"#.to_vec());
        assert_eq!(search(&Fake(pages),"pdf forms").await.expect("busca").len(),1);
    }

    #[tokio::test] async fn the_skill_folder_comes_down_without_dependencies_and_installs_like_any_other() {
        let staging=tempfile::tempdir().expect("pasta");
        let folder=download(&repo(),"acme/skills","pdf",staging.path()).await.expect("baixada");
        assert!(folder.join("SKILL.md").is_file()&&folder.join("scripts/run.py").is_file());
        assert!(!folder.join("node_modules").exists());
        let (connection,data)=(rusqlite::Connection::open_in_memory().expect("banco"),tempfile::tempdir().expect("dados"));
        connection.execute_batch(crate::skills::SCHEMA).expect("esquema");
        let root=crate::skills::root(data.path());
        std::fs::create_dir_all(&root).expect("raiz");
        let installed=crate::skills::install_folder(&connection,&root,&folder).expect("instalada");
        assert_eq!(installed[0].name,"pdf");
        assert!(root.join("pdf/scripts/run.py").is_file());
    }

    #[tokio::test] async fn the_header_name_finds_a_skill_whose_folder_has_another_name() {
        let staging=tempfile::tempdir().expect("pasta");
        let folder=download(&repo(),"acme/skills","nice-name",staging.path()).await.expect("baixada");
        assert!(std::fs::read_to_string(folder.join("SKILL.md")).expect("arquivo").contains("nice-name"));
    }

    #[tokio::test] async fn an_unknown_skill_or_a_bad_repository_is_refused_before_any_download() {
        let staging=tempfile::tempdir().expect("pasta");
        assert!(download(&repo(),"acme/skills","missing",staging.path()).await.is_err());
        assert!(download(&repo(),"../skills","pdf",staging.path()).await.is_err());
        assert!(download(&repo(),"acme/skills","../pdf",staging.path()).await.is_err());
        assert!(std::fs::read_dir(staging.path()).expect("lista").next().is_none(),"nada foi gravado");
    }
}
