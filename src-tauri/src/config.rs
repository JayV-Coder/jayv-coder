use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, fs, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)] pub jev: JevConfig,
    #[serde(default = "default_providers")] pub providers: HashMap<String, ProviderConfig>,
    #[serde(default = "default_models")] pub models: HashMap<String, ModelConfig>,
    #[serde(default = "default_budgets")] pub budgets: HashMap<String, usize>,
    #[serde(default)] pub permissions: PermissionsConfig,
    #[serde(default)] pub privacy: PrivacyConfig,
}
impl Default for Config { fn default() -> Self { Self { jev:JevConfig::default(), providers:default_providers(), models:default_models(), budgets:default_budgets(), permissions:PermissionsConfig::default(), privacy:PrivacyConfig::default() } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevConfig {
    #[serde(default = "default_strategy")] pub default_strategy: String,
    #[serde(default)] pub optimization: OptimizationConfig,
    #[serde(default)] pub adaptive_routing: AdaptiveConfig,
    #[serde(default)] pub context: ContextConfig,
}
impl Default for JevConfig { fn default() -> Self { Self { default_strategy:default_strategy(), optimization:OptimizationConfig::default(), adaptive_routing:AdaptiveConfig::default(), context:ContextConfig::default() } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    #[serde(default = "yes")] pub minimize_tokens: bool,
    #[serde(default = "yes")] pub prefer_local: bool,
    #[serde(default = "yes")] pub allow_escalation: bool,
}
impl Default for OptimizationConfig { fn default() -> Self { Self { minimize_tokens: true, prefer_local: true, allow_escalation: true } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(default = "default_confidence")] pub confidence_threshold: f64,
}
impl Default for AdaptiveConfig { fn default() -> Self { Self { enabled: true, confidence_threshold: 0.7 } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "default_fork_limit")] pub fork_limit: usize,
    #[serde(default = "default_cache_ttl")] pub cache_ttl: u64,
    #[serde(default = "yes")] pub privacy_filtering: bool,
}
impl Default for ContextConfig { fn default() -> Self { Self { fork_limit: 8_000, cache_ttl: 3_600, privacy_filtering: true } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(rename = "type", default)] pub kind: String,
    #[serde(default)] pub api_key: Option<String>,
    #[serde(default)] pub base_url: Option<String>,
    #[serde(default)] pub command: Option<String>,
    #[serde(default = "default_timeout")] pub timeout: u64,
    #[serde(default)] pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub local: Option<bool>,
}
impl Default for ProviderConfig { fn default() -> Self { Self { enabled:yes(), kind:String::new(), api_key:None, base_url:None, command:None, timeout:default_timeout(), args:vec![], local:None } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(default)] pub provider: String,
    #[serde(default)] pub model: String,
    #[serde(default)] pub capabilities: Vec<String>,
    #[serde(default)] pub cost_class: String,
    #[serde(default)] pub speed: String,
    #[serde(default = "default_context_window")] pub context_window: usize,
}
impl Default for ModelConfig { fn default() -> Self { Self { enabled:yes(), provider:String::new(), model:String::new(), capabilities:vec![], cost_class:String::new(), speed:String::new(), context_window:default_context_window() } } }

impl ProviderConfig {
    pub fn is_executable(&self)->bool {
        if !self.enabled { return false; }
        match self.kind.as_str() {
            "openai"|"anthropic"=>self.api_key.as_deref().is_some_and(|value|!value.trim().is_empty()),
            "openai-compatible"=>self.base_url.as_deref().is_some_and(|value|!value.trim().is_empty()),
            "cli"=>self.command.as_deref().is_some_and(|value|!value.trim().is_empty()),
            _=>false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default = "allow")] pub read: String,
    #[serde(default = "allow")] pub search: String,
    #[serde(default = "ask")] pub write: String,
    #[serde(default = "ask")] pub shell: String,
}
impl Default for PermissionsConfig { fn default() -> Self { Self { read: allow(), search: allow(), write: ask(), shell: ask() } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    #[serde(default = "default_deny")] pub deny: Vec<String>,
    #[serde(default)] pub local_only: Vec<String>,
    #[serde(default = "yes")] pub redact_secrets: bool,
}
impl Default for PrivacyConfig { fn default() -> Self { Self { deny: default_deny(), local_only: vec!["internal/**".into(), "private/**".into()], redact_secrets: true } } }

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        load_dotenv_near(path)?;
        if !path.exists() { return Ok(Self::default()); }
        let raw = fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
        let expanded = expand_env(&raw);
        serde_yaml::from_str(&expanded).with_context(|| format!("invalid YAML in {}", path.display()))
    }

    pub fn discover(requested: Option<PathBuf>) -> PathBuf {
        let cwd=env::current_dir().unwrap_or_else(|_|PathBuf::from("."));
        let executable=env::current_exe().ok();
        discover_from(requested,&cwd,executable.as_deref())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path=path.as_ref();
        if let Some(parent)=path.parent(){fs::create_dir_all(parent).with_context(||format!("could not create {}",parent.display()))?;}
        let temporary=path.with_extension("yaml.tmp");
        fs::write(&temporary,serde_yaml::to_string(self)?).with_context(||format!("could not write {}",temporary.display()))?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary,fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&temporary,path).with_context(||format!("could not replace {}",path.display()))
    }
}

const CONFIG_CANDIDATES:[&str;4]=["config.yaml","config_complete.yaml","bkp/config_complete.yaml","bkp/config.yaml"];

fn discover_from(requested:Option<PathBuf>,cwd:&Path,executable:Option<&Path>)->PathBuf {
    if let Some(path)=requested {
        return if path.is_absolute(){path}else{cwd.join(path)};
    }
    let cwd_candidates=CONFIG_CANDIDATES.iter().map(|name|cwd.join(name));
    let executable_candidates=executable.into_iter()
        .filter_map(Path::parent)
        .flat_map(Path::ancestors)
        .flat_map(|directory|CONFIG_CANDIDATES.iter().map(move |name|directory.join(name)));
    cwd_candidates.chain(executable_candidates).find(|path|path.is_file()).unwrap_or_else(||cwd.join("config.yaml"))
}

fn load_dotenv_near(config_path:&Path)->Result<()> {
    let directory=config_path.parent().unwrap_or_else(||Path::new("."));
    let dotenv_path=directory.join(".env");
    if dotenv_path.is_file() {
        dotenvy::from_path(&dotenv_path).with_context(||format!("could not load {}",dotenv_path.display()))?;
    }
    Ok(())
}

fn expand_env(input: &str) -> String {
    let regex = regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}").expect("valid regex");
    regex.replace_all(input, |caps: &regex::Captures| env::var(&caps[1]).unwrap_or_default()).into_owned()
}
fn default_budgets() -> HashMap<String, usize> { [("trivial", 2_000), ("simple", 5_000), ("medium", 12_000), ("complex", 30_000)].into_iter().map(|(k,v)|(k.into(),v)).collect() }
/// Sem `config.yaml`, o Jev já sobe com os dois CLIs que o desenvolvedor
/// provavelmente tem instalados. Nenhum dos dois é local: os dois mandam o
/// código para a nuvem, então não recebem `local: true`.
fn default_providers() -> HashMap<String, ProviderConfig> {
    [("claude","claude",vec!["--model","{model}","--print"]),("codex","codex",vec!["exec","--model","{model}","-"])].into_iter()
        .map(|(name,command,args)|(name.to_string(),ProviderConfig{kind:"cli".into(),command:Some(command.into()),args:args.into_iter().map(String::from).collect(),timeout:120,..ProviderConfig::default()}))
        .collect()
}

fn default_models() -> HashMap<String, ModelConfig> {
    [("claude","claude","claude-sonnet-4-5",200_000),("codex","codex","gpt-5-codex",128_000)].into_iter()
        .map(|(name,provider,model,window)|(name.to_string(),ModelConfig{provider:provider.into(),model:model.into(),capabilities:["chat","code","reasoning","tools"].into_iter().map(String::from).collect(),cost_class:"high".into(),speed:"medium".into(),context_window:window,..ModelConfig::default()}))
        .collect()
}

fn default_deny() -> Vec<String> { vec![".env".into(), "*.pem".into(), ".ssh/**".into(), "secrets/**".into(), "*.key".into(), "*.secret".into()] }
fn yes() -> bool { true }
fn default_strategy() -> String { "auto".into() }
fn default_confidence() -> f64 { 0.7 }
fn default_fork_limit() -> usize { 8_000 }
fn default_cache_ttl() -> u64 { 3_600 }
fn default_timeout() -> u64 { 30 }
fn default_context_window() -> usize { 8_192 }
fn allow() -> String { "allow".into() }
fn ask() -> String { "ask".into() }

#[cfg(test)] mod default_tests {
    use super::*;
    #[test] fn code_built_defaults_match_the_deserialized_ones() {
        let parsed:Config=serde_yaml::from_str("{}").expect("empty config");
        let built=Config::default();
        assert_eq!(built.budgets,parsed.budgets);
        assert_eq!(built.budgets.get("trivial"),Some(&2_000));
        assert_eq!(built.jev.default_strategy,parsed.jev.default_strategy);
        assert!(ProviderConfig::default().enabled && ModelConfig::default().enabled);
        assert_eq!(ProviderConfig::default().timeout,default_timeout());
        assert_eq!(ModelConfig::default().context_window,default_context_window());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn defaults_are_safe() { let c = Config::default(); assert_eq!(c.permissions.write, "ask"); assert!(c.privacy.deny.contains(&".env".to_string())); }

    #[test] fn ships_claude_and_codex_ready_to_run() {
        let config=Config::default();
        let mut providers:Vec<_>=config.providers.keys().cloned().collect(); providers.sort();
        let mut models:Vec<_>=config.models.keys().cloned().collect(); models.sort();
        assert_eq!(providers,["claude","codex"]);
        assert_eq!(models,["claude","codex"]);
        for (name,provider) in &config.providers {
            assert_eq!(provider.kind,"cli","{name} precisa ser CLI");
            assert!(provider.is_executable(),"{name} precisa subir sem chave nenhuma");
            assert_eq!(provider.local,None,"{name} manda o código para a nuvem, não pode ser local");
        }
        for (name,model) in &config.models { assert!(model.enabled && config.providers.contains_key(&model.provider),"{name} aponta para um provedor que não existe"); }
    }

    /// A tela Configuracao mostra exatamente o que o arquivo tem. O preset que
    /// acompanha o projeto traz só os dois CLIs; qualquer outro provedor ou
    /// modelo é o desenvolvedor que adiciona, e voltar a listar exemplos
    /// desligados aqui enche a tela de cartões que ninguém pediu.
    #[test] fn the_shipped_config_lists_only_the_two_cli_presets() {
        let shipped:Config=serde_yaml::from_str(&expand_env(include_str!("../../config.yaml"))).expect("config.yaml do projeto");
        let mut providers:Vec<_>=shipped.providers.keys().cloned().collect(); providers.sort();
        let mut models:Vec<_>=shipped.models.keys().cloned().collect(); models.sort();
        assert_eq!(providers,["claude","codex"]);
        assert_eq!(models,["claude","codex"]);
        for (name,provider) in &shipped.providers {
            assert!(provider.enabled,"{name} vem ligado");
            assert_eq!(provider.local,None,"{name} manda o código para a nuvem, não pode ser local");
        }
    }

    #[test] fn an_empty_file_lands_on_the_same_preset() {
        let parsed:Config=serde_yaml::from_str("{}").expect("empty config");
        assert_eq!(parsed.providers.len(),2);
        assert_eq!(parsed.models.len(),2);
    }
    #[test] fn provider_local_flag_is_optional_and_opt_in() { let implicit:ProviderConfig=serde_yaml::from_str("type: cli\ncommand: ollama").expect("cli provider"); assert_eq!(implicit.local,None); let explicit:ProviderConfig=serde_yaml::from_str("type: cli\ncommand: ollama\nlocal: true").expect("local cli provider"); assert_eq!(explicit.local,Some(true)); assert!(!serde_yaml::to_string(&implicit).expect("yaml").contains("local")); }
    #[test] fn expands_missing_env_to_empty() { assert_eq!(expand_env("api_key: ${JEV_TEST_MISSING}"), "api_key: "); }
    #[test]
    fn discovers_config_from_executable_ancestors() {
        let root=tempfile::tempdir().expect("temporary root");
        let elsewhere=tempfile::tempdir().expect("different working directory");
        let config=root.path().join("config.yaml");
        fs::write(&config,"models: {}").expect("config fixture");
        let executable=root.path().join("src-tauri/target/debug/jev");

        assert_eq!(discover_from(None,elsewhere.path(),Some(&executable)),config);
    }
    #[test]
    fn resolves_relative_explicit_config_from_working_directory() {
        let cwd=Path::new("/tmp/jev-test-cwd");
        assert_eq!(discover_from(Some(PathBuf::from("custom.yaml")),cwd,None),cwd.join("custom.yaml"));
    }
    #[test]
    fn loads_dotenv_next_to_config_before_expanding_yaml() {
        let root=tempfile::tempdir().expect("temporary config directory");
        let config=root.path().join("config.yaml");
        fs::write(root.path().join(".env"),"JEV_CONFIG_DOTENV_TEST_KEY=loaded-from-adjacent-dotenv\n").expect("dotenv fixture");
        fs::write(&config,"providers:\n  test:\n    type: openai\n    api_key: ${JEV_CONFIG_DOTENV_TEST_KEY}\n").expect("config fixture");

        let loaded=Config::load(&config).expect("config with adjacent dotenv");

        assert_eq!(loaded.providers["test"].api_key.as_deref(),Some("loaded-from-adjacent-dotenv"));
    }
}
