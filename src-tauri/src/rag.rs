use crate::{firewall::ContextFirewall, model::{ContextSnippet, ProjectInfo}, symbols::SymbolIndex};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashSet}, fs, path::{Path, PathBuf}};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile { pub path: String, pub language: String, pub hash: String, pub content: String, pub tokens: HashSet<String> }

/// Quanto texto o índice guarda, somando os arquivos. Cada arquivo fica na
/// memória com o conteúdo inteiro e o conjunto de palavras, então uma raiz
/// grande demais — o `$HOME` de quem abriu o aplicativo pelo menu — chegava a
/// vários gigabytes. Um repositório comum cabe folgado.
pub const DEFAULT_BUDGET: u64 = 48 * 1024 * 1024;

#[derive(Debug)]
pub struct RepositoryRag { root: PathBuf, files: Vec<IndexedFile>, repository_hash: String, budget: u64, indexed: bool, repositories: Vec<String>, symbols: SymbolIndex }
impl RepositoryRag {
    pub fn new(root: PathBuf) -> Self { Self { root, files: vec![], repository_hash:String::new(), budget:DEFAULT_BUDGET, indexed:false, repositories:vec![], symbols:SymbolIndex::default() } }
    pub fn with_budget(mut self, budget: u64) -> Self { self.budget=budget; self }
    pub fn index(&mut self, firewall: &ContextFirewall) -> Result<usize> {
        self.files.clear();
        self.indexed=true;
        self.repositories=crate::checkout::nested(&self.root);
        let mut spent=0u64;
        for entry in WalkDir::new(&self.root).follow_links(false).sort_by_file_name().into_iter().filter_entry(allowed_entry).filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
            let relative = entry.path().strip_prefix(&self.root).unwrap_or(entry.path());
            if firewall.check_file(relative).is_sensitive || entry.metadata().map(|m| m.len()>512_000).unwrap_or(true) { continue; }
            let Some(language) = language_for(entry.path()) else { continue; };
            let Ok(content) = fs::read_to_string(entry.path()) else { continue; };
            spent+=content.len() as u64;
            if spent>self.budget { break; }
            let hash = hex::encode(Sha256::digest(content.as_bytes()));
            self.files.push(IndexedFile { path:relative.to_string_lossy().to_string(), language:language.into(), hash, tokens:tokenize(&content), content });
        }
        self.files.sort_by(|a,b| a.path.cmp(&b.path));
        let hashes = self.files.iter().map(|f| format!("{}:{}",f.path,f.hash)).collect::<Vec<_>>().join("|");
        self.repository_hash=hex::encode(Sha256::digest(hashes.as_bytes()));
        self.symbols.update(&self.files);
        Ok(self.files.len())
    }
    /// Os arquivos que mais têm a ver com o pedido. Cada palavra pesa pelo quão
    /// rara ela é no repositório (IDF): `fn` ou `import` aparecem em tudo e não
    /// dizem nada; o nome de uma função diz. Palavras de ligação ficam de fora,
    /// documentação pesa metade de código, e de um arquivo grande vai o trecho
    /// onde as palavras do pedido se concentram, não o começo dele.
    pub fn search(&self, query: &str, limit: usize) -> Vec<ContextSnippet> {
        let query_tokens=tokenize(query).into_iter().filter(|token|!STOPWORDS.contains(&token.as_str())).collect::<Vec<_>>();
        let total=self.files.len() as f64;
        let weights=query_tokens.iter().map(|token|{ let found=self.files.iter().filter(|file|file.tokens.contains(token)).count() as f64; (token.as_str(),(1.0+total/(1.0+found)).ln()) }).collect::<Vec<_>>();
        let mut scored=self.files.iter().map(|file| {
            let overlap=weights.iter().filter(|(token,_)|file.tokens.contains(*token)).map(|(_,weight)|weight).sum::<f64>();
            let path=file.path.to_lowercase();
            let path_bonus=weights.iter().filter(|(token,_)|path.contains(*token)).map(|(_,weight)|weight*1.5).sum::<f64>();
            // Um nome que o arquivo define vale mais do que um que ele só cita.
            let defined=self.symbols.terms(&file.path).map_or(0.0,|terms|weights.iter().filter(|(token,_)|terms.contains(*token)).map(|(_,weight)|weight*DEFINITION_WEIGHT).sum::<f64>());
            let kind=if file.language=="Markdown" {DOCUMENTATION_WEIGHT} else {1.0};
            (file,(overlap+path_bonus+defined)*kind)
        }).filter(|(_,s)|*s>0.0).collect::<Vec<_>>();
        scored.sort_by(|a,b| b.1.total_cmp(&a.1).then_with(||a.0.path.cmp(&b.0.path)));
        scored.into_iter().take(limit).map(|(f,score)| ContextSnippet { path:f.path.clone(), content:best_window(&f.content,&weights,SNIPPET_CHARS), score }).collect()
    }
    pub fn project_info(&self) -> ProjectInfo { let mut languages=self.files.iter().map(|f|f.language.clone()).collect::<Vec<_>>(); languages.sort(); languages.dedup(); ProjectInfo { root:self.root.to_string_lossy().to_string(), name:self.root.file_name().unwrap_or_default().to_string_lossy().to_string(), languages, repositories:self.repositories.clone() } }
    pub fn root(&self) -> &Path { &self.root }
    /// Força a próxima leitura da pasta: as regras de privacidade mudaram.
    pub fn invalidate(&mut self) { self.indexed=false; }
    /// Lê a pasta de novo se o índice foi dado por velho: privacidade nova, ou
    /// um build que mexeu nos arquivos.
    pub fn refresh(&mut self, firewall: &ContextFirewall) -> Result<()> { if !self.indexed { self.index(firewall)?; } Ok(()) }
    /// Aponta o índice para outra pasta — é assim que um chat de projeto faz o
    /// Jev olhar o repositório dele. Reindexar custa uma varredura inteira do
    /// disco, então só acontece quando o caminho muda de verdade — ou na
    /// primeira vez, se a raiz de partida nunca chegou a ser lida.
    pub fn focus_on(&mut self, root: PathBuf, firewall: &ContextFirewall) -> Result<()> { if root==self.root && self.indexed {return Ok(());} self.root=root; self.index(firewall)?; Ok(()) }
    pub fn repository_hash(&self) -> &str { &self.repository_hash }
    pub fn file_hashes(&self) -> BTreeMap<String,String> { self.files.iter().map(|f|(f.path.clone(),f.hash.clone())).collect() }
    pub fn len(&self) -> usize { self.files.len() }
    /// O índice de símbolos dos arquivos lidos.
    pub fn symbols(&self) -> &SymbolIndex { &self.symbols }
    /// Os caminhos indexados, em ordem.
    pub fn paths(&self) -> impl Iterator<Item=&str> { self.files.iter().map(|file|file.path.as_str()) }
}

pub(crate) fn allowed_entry(entry:&DirEntry)->bool { let name=entry.file_name().to_string_lossy(); !matches!(name.as_ref(),".git"|"target"|"node_modules"|"dist"|"build"|"__pycache__"|".jev"|".jev_cache"|".jev_performance.json") }
fn language_for(path:&Path)->Option<&'static str> { match path.extension()?.to_str()?.to_lowercase().as_str() { "rs"=>Some("Rust"),"py"=>Some("Python"),"js"|"mjs"|"cjs"=>Some("JavaScript"),"ts"|"tsx"=>Some("TypeScript"),"go"=>Some("Go"),"html"=>Some("HTML"),"css"=>Some("CSS"),"json"=>Some("JSON"),"yaml"|"yml"=>Some("YAML"),"toml"=>Some("TOML"),"md"=>Some("Markdown"),"sh"=>Some("Shell"),_=>None } }
const SNIPPET_CHARS:usize=12_000;
const DOCUMENTATION_WEIGHT:f64=0.5;
/// Peso de uma palavra do pedido que está no nome de algo que o arquivo define.
const DEFINITION_WEIGHT:f64=2.0;
const WINDOW_LINES:usize=40;
/// Palavras de ligação, em inglês e em português, que casam com qualquer arquivo.
const STOPWORDS:[&str;64]=["the","and","for","that","this","with","from","are","was","not","but","you","have","has","can","will","what","when","where","which","how","why","all","any","into","use","make","need","want","should","would","please",
    "como","para","que","uma","com","por","não","nao","mais","dos","das","isso","esse","essa","este","esta","quando","onde","qual","quais","ser","tem","está","sobre","fazer","faça","preciso","quero","todo","toda","pelo","pela"];

/// As palavras que dizem do que um texto trata: as de [`tokenize`] sem as de
/// ligação. É com elas que dois pedidos são comparados.
pub fn terms(value:&str)->HashSet<String> { tokenize(value).into_iter().filter(|token|!STOPWORDS.contains(&token.as_str())).collect() }

/// O quanto dois textos falam da mesma coisa, de 0 a 1 (Jaccard das palavras).
pub fn similarity(left:&HashSet<String>,right:&HashSet<String>)->f64 {
    let union=left.union(right).count();
    if union==0 { 0.0 } else { left.intersection(right).count() as f64/union as f64 }
}

/// As palavras de um texto, em minúsculas. Um identificador entra inteiro e
/// também partido (`fetchUserName` → `fetch`, `user`, `name`), para o pedido
/// em palavras soltas achar o código que as junta.
fn tokenize(value:&str)->HashSet<String> {
    let mut tokens=HashSet::new();
    for word in value.split(|c:char|!c.is_alphanumeric()).filter(|s|!s.is_empty()) {
        if word.chars().count()>2 { tokens.insert(word.to_lowercase()); }
        let mut part=String::new();
        let mut previous_lower=false;
        for c in word.chars() {
            if c.is_uppercase()&&previous_lower { if part.chars().count()>2 { tokens.insert(part.to_lowercase()); } part.clear(); }
            previous_lower=c.is_lowercase()||c.is_ascii_digit();
            part.push(c);
        }
        if part.chars().count()>2 { tokens.insert(part.to_lowercase()); }
    }
    tokens
}

/// O trecho de até `max` caracteres onde as palavras do pedido se concentram,
/// começando numa linha inteira. Arquivo que cabe vai inteiro.
fn best_window(content:&str,weights:&[(&str,f64)],max:usize)->String {
    if content.chars().count()<=max { return content.into(); }
    let lines=content.lines().collect::<Vec<_>>();
    let score=|line:&str|{ let tokens=tokenize(line); weights.iter().filter(|(token,_)|tokens.contains(*token)).map(|(_,weight)|weight).sum::<f64>() };
    let scores=lines.iter().map(|line|score(line)).collect::<Vec<_>>();
    let mut best=(0usize,0.0f64);
    let mut running=scores.iter().take(WINDOW_LINES).sum::<f64>();
    if running>best.1 { best=(0,running); }
    for start in 1..=lines.len().saturating_sub(WINDOW_LINES) {
        running+=scores.get(start+WINDOW_LINES-1).copied().unwrap_or(0.0)-scores[start-1];
        if running>best.1+f64::EPSILON { best=(start,running); }
    }
    let start=best.0.saturating_sub(WINDOW_LINES/4);
    let window=lines[start..].join("\n");
    let window:String=window.chars().take(max).collect();
    if start==0 { window } else { format!("[… {start} lines above omitted]\n{window}") }
}

#[cfg(test)] mod tests { use super::*; use crate::config::PrivacyConfig;
    #[test] fn a_rare_identifier_outweighs_words_every_file_shares() {
        let dir=tempfile::tempdir().unwrap();
        for name in ["a.rs","b.rs","c.rs"] { fs::write(dir.path().join(name),"fn handle_request() { let value = config(); }").unwrap(); }
        fs::write(dir.path().join("d.rs"),"fn handle_request() { refresh_token(); }").unwrap();
        fs::write(dir.path().join("notes.md"),"how to handle the refresh token request").unwrap();
        let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap();
        let found=rag.search("how should the request handle a refresh token?",2);
        assert_eq!(found[0].path,"d.rs","o código com o identificador raro vem antes da documentação que fala dele");
    }
    #[test] fn identifiers_are_split_into_their_words() { assert!(tokenize("fetchUserName").is_superset(&["fetchusername","fetch","user","name"].map(String::from).into_iter().collect())); }
    #[test] fn a_large_file_sends_the_part_that_matches() {
        let mut body=String::new(); for index in 0..2_000 { body.push_str(&format!("let filler_{index} = {index};\n")); } body.push_str("fn compute_shipping_rate() {}\n"); for index in 0..200 { body.push_str(&format!("let tail_{index} = {index};\n")); }
        let window=best_window(&body,&[("shipping",2.0)],2_000);
        assert!(window.contains("compute_shipping_rate"),"o trecho que casa com o pedido vai junto");
        assert!(window.starts_with("[… "),"e o modelo sabe que o começo ficou de fora");
    }
    #[test] fn skips_the_performance_history() { let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn important_router() {}").unwrap(); fs::write(dir.path().join(".jev_performance.json"),"[]").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.len(),1); assert!(!rag.file_hashes().contains_key(".jev_performance.json")); }
    #[test] fn indexes_text_files() { let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn important_router() {}").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.len(),1); assert_eq!(rag.search("router",3)[0].path,"main.rs"); }
    #[test] fn stops_reading_once_the_budget_is_spent() { let dir=tempfile::tempdir().unwrap(); for name in ["a.md","b.md","c.md"] { fs::write(dir.path().join(name),"x".repeat(400)).unwrap(); } let mut rag=RepositoryRag::new(dir.path().into()).with_budget(1_000); rag.index(&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.file_hashes().keys().collect::<Vec<_>>(),["a.md","b.md"]); }
    #[test] fn focusing_on_the_starting_root_indexes_it_the_first_time() { let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn router() {}").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); assert_eq!(rag.len(),0); rag.focus_on(dir.path().into(),&ContextFirewall::new(PrivacyConfig::default())).unwrap(); assert_eq!(rag.len(),1); }
    #[test] fn a_stale_index_reads_the_folder_again() { let firewall=ContextFirewall::new(PrivacyConfig::default()); let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("main.rs"),"fn router() {}").unwrap(); let mut rag=RepositoryRag::new(dir.path().into()); rag.index(&firewall).unwrap(); fs::write(dir.path().join("added.rs"),"fn added_by_agent() {}").unwrap(); rag.refresh(&firewall).unwrap(); assert_eq!(rag.len(),1,"índice em dia não varre de novo"); rag.invalidate(); rag.refresh(&firewall).unwrap(); assert_eq!(rag.len(),2,"o arquivo que o agente criou entra"); assert!(rag.search("added_by_agent",1).first().is_some_and(|hit|hit.path=="added.rs")); rag.focus_on(dir.path().into(),&firewall).unwrap(); assert_eq!(rag.len(),2); }
    #[test] fn following_the_project_reindexes_the_new_folder() { let firewall=ContextFirewall::new(PrivacyConfig::default()); let first=tempfile::tempdir().unwrap(); let second=tempfile::tempdir().unwrap(); fs::write(first.path().join("main.rs"),"fn old_router() {}").unwrap(); fs::write(second.path().join("lib.rs"),"fn new_router() {}").unwrap(); let mut rag=RepositoryRag::new(first.path().into()); rag.index(&firewall).unwrap(); rag.focus_on(second.path().into(),&firewall).unwrap(); assert_eq!(rag.project_info().root,second.path().to_string_lossy()); assert_eq!(rag.file_hashes().keys().collect::<Vec<_>>(),["lib.rs"]); } }
