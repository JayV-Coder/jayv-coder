use crate::model::Context;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedContext { context: Context, files: BTreeMap<String, String>, created_at: DateTime<Utc>, last_accessed: DateTime<Utc>, hits: usize }
impl CachedContext { fn fresh(&self, current: &BTreeMap<String, String>) -> bool { self.files.iter().all(|(path, hash)| current.get(path) == Some(hash)) } }

#[derive(Debug)]
pub struct SemanticCache { entries: HashMap<String, CachedContext>, ttl: Duration, max_entries: usize, hits: usize, misses: usize }

impl SemanticCache {
    pub fn new(ttl_seconds: u64, max_entries: usize) -> Self { Self { entries: HashMap::new(), ttl: Duration::seconds(ttl_seconds as i64), max_entries, hits: 0, misses: 0 } }
    pub fn request_key(query: &str, repository: &str) -> String {
        let canonical = serde_json::json!({"query":query.trim().to_lowercase(),"repository":repository});
        hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
    }
    #[deprecated(note = "use request_key: mixing every file hash into the key invalidates the whole cache on any edit")]
    pub fn key(query: &str, repository_hash: &str, git_commit: &str, symbols: &[String], file_hashes: &BTreeMap<String, String>) -> String {
        let canonical = serde_json::json!({"query":query.trim().to_lowercase(),"repository":repository_hash,"commit":git_commit,"symbols":symbols,"files":file_hashes});
        hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
    }
    pub fn insert_tracked(&mut self, key: String, context: Context, current: &BTreeMap<String, String>) { let files = referenced(&context, current); self.store(key, context, files); }
    pub fn get_valid(&mut self, key: &str, current: &BTreeMap<String, String>) -> Option<Context> {
        self.remove_expired();
        if self.entries.get(key).map(|entry| !entry.fresh(current)).unwrap_or(false) { self.entries.remove(key); }
        if let Some(entry) = self.entries.get_mut(key) { self.hits += 1; entry.hits += 1; entry.last_accessed = Utc::now(); Some(entry.context.clone()) } else { self.misses += 1; None }
    }
    #[deprecated(note = "use insert_tracked so the entry records the file hashes it depends on")]
    pub fn insert(&mut self, key: String, context: Context) { self.store(key, context, BTreeMap::new()); }
    #[deprecated(note = "use get_valid so only entries touching the changed files are invalidated")]
    pub fn get(&mut self, key: &str) -> Option<Context> { let current = self.entries.get(key).map(|entry| entry.files.clone()).unwrap_or_default(); self.get_valid(key, &current) }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn stats(&self) -> (usize, usize, usize) { (self.entries.len(), self.hits, self.misses) }
    pub fn hit_rate(&self) -> f64 { let total = self.hits + self.misses; if total == 0 { 0.0 } else { self.hits as f64 / total as f64 } }
    fn store(&mut self, key: String, context: Context, files: BTreeMap<String, String>) {
        self.remove_expired();
        if !self.entries.contains_key(&key) && self.entries.len() >= self.max_entries { if let Some(oldest) = self.entries.iter().min_by_key(|(_,v)| v.last_accessed).map(|(k,_)| k.clone()) { self.entries.remove(&oldest); } }
        let now = Utc::now(); self.entries.insert(key, CachedContext { context, files, created_at: now, last_accessed: now, hits: 0 });
    }
    fn remove_expired(&mut self) { let now = Utc::now(); self.entries.retain(|_, entry| now - entry.created_at < self.ttl); }
}

fn referenced(context: &Context, current: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    context.snippets.iter().map(|s| s.path.as_str()).chain(context.relevant_files.iter().map(String::as_str)).filter_map(|path| current.get(path).map(|hash| (path.to_string(), hash.clone()))).collect()
}

#[cfg(test)] mod tests {
    use super::*; use crate::model::ContextSnippet;
    fn context(paths: &[&str]) -> Context { Context { relevant_files: paths.iter().map(|p| (*p).into()).collect(), snippets: paths.iter().map(|p| ContextSnippet { path: (*p).into(), content: "body".into(), score: 1.0 }).collect(), ..Default::default() } }
    fn hashes(pairs: &[(&str, &str)]) -> BTreeMap<String, String> { pairs.iter().map(|(p, h)| ((*p).to_string(), (*h).to_string())).collect() }
    #[test] #[allow(deprecated)] fn caches_context() { let mut c = SemanticCache::new(60, 2); c.insert("x".into(), Context::default()); assert!(c.get("x").is_some()); assert_eq!(c.stats(), (1,1,0)); }
    #[test] fn editing_an_unrelated_file_keeps_the_cached_context() {
        let mut c = SemanticCache::new(60, 10); let before = hashes(&[("src/a.rs","a1"),("src/b.rs","b1"),("src/c.rs","c1")]);
        c.insert_tracked(SemanticCache::request_key("explain a","/repo"), context(&["src/a.rs"]), &before);
        let after = hashes(&[("src/a.rs","a1"),("src/b.rs","b2"),("src/c.rs","c9")]);
        assert!(c.get_valid(&SemanticCache::request_key("explain a","/repo"), &after).is_some());
        assert_eq!(c.stats(), (1,1,0)); assert_eq!(c.hit_rate(), 1.0);
    }
    #[test] fn editing_a_referenced_file_invalidates_the_cached_context() {
        let mut c = SemanticCache::new(60, 10); let key = SemanticCache::request_key("explain a","/repo");
        c.insert_tracked(key.clone(), context(&["src/a.rs"]), &hashes(&[("src/a.rs","a1"),("src/b.rs","b1")]));
        assert!(c.get_valid(&key, &hashes(&[("src/a.rs","a2"),("src/b.rs","b1")])).is_none()); assert_eq!(c.len(), 0);
    }
    #[test] fn deleting_a_referenced_file_invalidates_the_cached_context() {
        let mut c = SemanticCache::new(60, 10); let key = SemanticCache::request_key("explain a","/repo");
        c.insert_tracked(key.clone(), context(&["src/a.rs"]), &hashes(&[("src/a.rs","a1"),("src/b.rs","b1")]));
        assert!(c.get_valid(&key, &hashes(&[("src/b.rs","b1")])).is_none()); assert_eq!(c.len(), 0);
    }
    #[test] fn a_stale_entry_counts_as_a_miss() {
        let mut c = SemanticCache::new(60, 10); let key = SemanticCache::request_key("explain a","/repo");
        c.insert_tracked(key.clone(), context(&["src/a.rs"]), &hashes(&[("src/a.rs","a1")]));
        assert!(c.get_valid(&key, &hashes(&[("src/a.rs","a2")])).is_none()); assert_eq!(c.stats(), (0,0,1)); assert_eq!(c.hit_rate(), 0.0);
        c.insert_tracked(key.clone(), context(&["src/a.rs"]), &hashes(&[("src/a.rs","a2")]));
        assert!(c.get_valid(&key, &hashes(&[("src/a.rs","a2")])).is_some()); assert_eq!(c.stats(), (1,1,1)); assert_eq!(c.hit_rate(), 0.5);
    }
    #[test] fn expires_entries_after_the_ttl() {
        let mut c = SemanticCache::new(0, 10); let key = SemanticCache::request_key("explain a","/repo"); let current = hashes(&[("src/a.rs","a1")]);
        c.insert_tracked(key.clone(), context(&["src/a.rs"]), &current);
        assert!(c.get_valid(&key, &current).is_none()); assert_eq!(c.stats(), (0,0,1));
    }
    #[test] fn evicts_the_least_recently_used_entry_when_full() {
        let mut c = SemanticCache::new(60, 2); let current = hashes(&[("src/a.rs","a1")]);
        for query in ["one","two","three"] { c.insert_tracked(SemanticCache::request_key(query,"/repo"), context(&["src/a.rs"]), &current); }
        assert_eq!(c.len(), 2);
        assert!(c.get_valid(&SemanticCache::request_key("three","/repo"), &current).is_some());
        assert!(c.get_valid(&SemanticCache::request_key("one","/repo"), &current).is_none());
    }
    #[test] fn the_key_ignores_repository_file_contents() { assert_eq!(SemanticCache::request_key("  Explain A  ","/repo"), SemanticCache::request_key("explain a","/repo")); assert_ne!(SemanticCache::request_key("explain a","/repo"), SemanticCache::request_key("explain a","/other")); }
}
