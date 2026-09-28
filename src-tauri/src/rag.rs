use crate::{firewall::ContextFirewall, model::{ContextSnippet, ProjectInfo}};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashSet}, fs, path::{Path, PathBuf}};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile { pub path: String, pub language: String, pub hash: String, pub content: String, pub tokens: HashSet<String> }

#[derive(Debug, Default)]
pub struct RepositoryRag { root: PathBuf, files: Vec<IndexedFile>, repository_hash: String }
impl RepositoryRag {
    pub fn new(root: PathBuf) -> Self { Self { root, files: vec![], repository_hash:String::new() } }
    pub fn index(&mut self, firewall: &ContextFirewall) -> Result<usize> {
        self.files.clear();
        for entry in WalkDir::new(&self.root).follow_links(false).into_iter().filter_entry(allowed_entry).filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
            let relative = entry.path().strip_prefix(&self.root).unwrap_or(entry.path());
            if firewall.check_file(relative).is_sensitive || entry.metadata().map(|m| m.len()>512_000).unwrap_or(true) { continue; }
            let Some(language) = language_for(entry.path()) else { continue; };
            let Ok(content) = fs::read_to_string(entry.path()) else { continue; };
            let hash = hex::encode(Sha256::digest(content.as_bytes()));
            self.files.push(IndexedFile { path:relative.to_string_lossy().to_string(), language:language.into(), hash, tokens:tokenize(&content), content });
        }
        self.files.sort_by(|a,b| a.path.cmp(&b.path));
        let hashes = self.files.iter().map(|f| format!("{}:{}",f.path,f.hash)).collect::<Vec<_>>().join("|");
        self.repository_hash=hex::encode(Sha256::digest(hashes.as_bytes()));
        Ok(self.files.len())
    }
    pub fn search(&self, query: &str, limit: usize) -> Vec<ContextSnippet> {
        let query_tokens=tokenize(query);
        let mut scored=self.files.iter().map(|file| { let overlap=query_tokens.intersection(&file.tokens).count() as f64; let path_bonus=query_tokens.iter().filter(|t| file.path.to_lowercase().contains(t.as_str())).count() as f64 * 1.5; (file,overlap+path_bonus) }).filter(|(_,s)|*s>0.0).collect::<Vec<_>>();
        scored.sort_by(|a,b| b.1.total_cmp(&a.1));
        scored.into_iter().take(limit).map(|(f,score)| ContextSnippet { path:f.path.clone(), content:truncate(&f.content,12_000), score }).collect()
    }
    pub fn project_info(&self) -> ProjectInfo { let mut languages=self.files.iter().map(|f|f.language.clone()).collect::<Vec<_>>(); languages.sort(); languages.dedup(); ProjectInfo { root:self.root.to_string_lossy().to_string(), name:self.root.file_name().unwrap_or_default().to_string_lossy().to_string(), languages } }
    pub fn root(&self) -> &Path { &self.root }
    /// Aponta o índice para outra pasta — é assim que um chat de projeto faz o
    /// Jev olhar o repositório dele. Reindexar custa uma varredura inteira do
    /// disco, então só acontece quando o caminho muda de verdade.
    pub fn focus_on(&mut self, root: PathBuf, firewall: &ContextFirewall) -> Result<()> { if root==self.root {return Ok(());} self.root=root; self.index(firewall)?; Ok(()) }
    pub fn repository_hash(&self) -> &str { &self.repository_hash }
    pub fn file_hashes(&self) -> BTreeMap<String,String> { self.files.iter().map(|f|(f.path.clone(),f.hash.clone())).collect() }
    pub fn len(&self) -> usize { self.files.len() }
}

fn allowed_entry(entry:&DirEntry)->bool { let name=entry.file_name().to_string_lossy(); !matches!(name.as_ref(),".git"|"target"|"node_modules"|"dist"|"build"|"__pycache__"|".jev"|".jev_cache"|".jev_performance.json") }
fn language_for(path:&Path)->Option<&'static str> { match path.extension()?.to_str()?.to_lowercase().as_str() { "rs"=>Some("Rust"),"py"=>Some("Python"),"js"|"mjs"|"cjs"=>Some("JavaScript"),"ts"|"tsx"=>Some("TypeScript"),"html"=>Some("HTML"),"css"=>Some("CSS"),"json"=>Some("JSON"),"yaml"|"yml"=>Some("YAML"),"toml"=>Some("TOML"),"md"=>Some("Markdown"),"sh"=>Some("Shell"),_=>None } }
fn tokenize(value:&str)->HashSet<String> { value.split(|c:char|!c.is_alphanumeric()).filter(|s|s.len()>2).map(str::to_lowercase).collect() }
fn truncate(value:&str,max:usize)->String { if value.len()<=max { value.into() } else { value.chars().take(max).collect() } }

#[cfg(test)] mod tests { use super::*; use crate::config::PrivacyConfig;
    #[test] fn skips_the_performance_history() { let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn important_router() {}").unwrap(); fs::write(dir.path().join(".jev_performance.json"),"[]").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.len(),1); assert!(!rag.file_hashes().contains_key(".jev_performance.json")); }
    #[test] fn indexes_text_files() { let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn important_router() {}").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.len(),1); assert_eq!(rag.search("router",3)[0].path,"main.rs"); }
    #[test] fn following_the_project_reindexes_the_new_folder() { let firewall=ContextFirewall::new(PrivacyConfig::default()); let first=tempfile::tempdir().unwrap(); let second=tempfile::tempdir().unwrap(); fs::write(first.path().join("main.rs"),"fn old_router() {}").unwrap(); fs::write(second.path().join("lib.rs"),"fn new_router() {}").unwrap(); let mut rag=RepositoryRag::new(first.path().into()); rag.index(&firewall).unwrap(); rag.focus_on(second.path().into(),&firewall).unwrap(); assert_eq!(rag.project_info().root,second.path().to_string_lossy()); assert_eq!(rag.file_hashes().keys().collect::<Vec<_>>(),["lib.rs"]); } }
