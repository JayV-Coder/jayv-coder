use crate::{config::{Config, ModelConfig}, model::{Context, ModelSelection, PerformanceRecord}};
use anyhow::{Context as _, Result};
use std::{collections::HashSet, fs, path::Path};

const MAX_RECORDS:usize=1000;
const FREE_COST_BONUS:f64=1.5;
const COMPLEX_REASONING_BONUS:f64=1.2;
const FAST_SPEED_BONUS:f64=0.4;
const HISTORICAL_WEIGHT:f64=2.0;
const HISTORICAL_PRIOR:f64=0.5;
const HISTORICAL_PRIOR_SAMPLES:f64=5.0;

#[derive(Debug, Default)]
pub struct PerformanceTracker { records: Vec<PerformanceRecord> }
impl PerformanceTracker {
    pub fn load(path:&Path)->Self { let mut records=fs::read(path).ok().and_then(|bytes|serde_json::from_slice::<Vec<PerformanceRecord>>(&bytes).ok()).unwrap_or_default(); if records.len()>MAX_RECORDS { records.drain(..records.len()-MAX_RECORDS); } Self{records} }
    pub fn save(&self,path:&Path)->Result<()> { if let Some(parent)=path.parent().filter(|p|!p.as_os_str().is_empty()) { fs::create_dir_all(parent).with_context(||format!("could not create {}",parent.display()))?; } let temporary=path.with_extension("json.tmp"); fs::write(&temporary,serde_json::to_vec(&self.records)?).with_context(||format!("could not write {}",temporary.display()))?; fs::rename(&temporary,path).with_context(||format!("could not replace {}",path.display())) }
    pub fn record(&mut self, record:PerformanceRecord) { self.records.push(record); if self.records.len()>MAX_RECORDS { self.records.remove(0); } }
    pub fn model_score(&self, model:&str)->Option<f64> { let items=self.records.iter().filter(|r|r.model_used==model).collect::<Vec<_>>(); if items.is_empty(){None}else{Some(items.iter().filter(|r|r.success).count() as f64/items.len() as f64)} }
    pub fn len(&self)->usize { self.records.len() }
    fn shrunk_model_score(&self, model:&str)->Option<f64> { let items=self.records.iter().filter(|r|r.model_used==model).collect::<Vec<_>>(); if items.is_empty(){return None;} let successes=items.iter().filter(|r|r.success).count() as f64; Some((successes+HISTORICAL_PRIOR*HISTORICAL_PRIOR_SAMPLES)/(items.len() as f64+HISTORICAL_PRIOR_SAMPLES)) }
}

fn effective_model_id<'a>(name:&'a str,model:&'a ModelConfig)->&'a str { if model.model.is_empty(){name}else{&model.model} }

pub fn required_capabilities(intent:&str)->Vec<String> { match intent { "code"|"refactor"=>vec!["code".into(),"tools".into()], "test"=>vec!["code".into(),"tools".into()], "security"=>vec!["reasoning".into(),"tools".into()], "analysis"|"review"=>vec!["reasoning".into()], "frontend"=>vec!["code".into()], _=>vec!["chat".into()] } }

pub fn select_model(config:&Config,intent:&str,complexity:&str,context:&Context,performance:&PerformanceTracker)->ModelSelection {
    let required=required_capabilities(intent); let required_set=required.iter().collect::<HashSet<_>>();
    let budget=*config.budgets.get(complexity).unwrap_or(&12_000);
    let executable=|model:&ModelConfig|model.enabled && config.providers.get(&model.provider).is_some_and(|provider|provider.is_executable());
    let mut choices=config.models.iter().filter(|(_,m)| executable(m) && context.estimated_tokens<=m.context_window && required_set.iter().all(|cap|m.capabilities.contains(cap))).map(|(name,m)|(name,m,score_model(name,m,complexity,config,performance))).collect::<Vec<_>>();
    if choices.is_empty() { choices=config.models.iter().filter(|(_,m)|executable(m) && context.estimated_tokens<=m.context_window).map(|(name,m)|(name,m,score_model(name,m,complexity,config,performance))).collect(); }
    choices.sort_by(|a,b|b.2.total_cmp(&a.2));
    match choices.first() { Some((name,model,score))=>ModelSelection{model_name:effective_model_id(name,model).to_string(),provider:model.provider.clone(),estimated_tokens:context.estimated_tokens.min(budget),score:*score,reason:format!("matched {} capabilities within {budget}-token budget",required.len())}, None=>ModelSelection{model_name:"configuration".into(),provider:"jev".into(),estimated_tokens:context.estimated_tokens.min(budget),score:0.0,reason:"no configured model can satisfy this request".into()} }
}
fn score_model(name:&str,model:&ModelConfig,complexity:&str,config:&Config,performance:&PerformanceTracker)->f64 { let mut score=1.0; if config.jev.optimization.prefer_local && model.cost_class=="free" {score+=FREE_COST_BONUS;} if complexity=="complex" && model.capabilities.contains(&"reasoning".into()){score+=COMPLEX_REASONING_BONUS;} if model.speed=="fast"{score+=FAST_SPEED_BONUS;} if config.jev.adaptive_routing.enabled { if let Some(historical)=performance.shrunk_model_score(effective_model_id(name,model)){score+=historical*HISTORICAL_WEIGHT;} } score }

#[cfg(test)] mod tests {
    use super::*;
    use chrono::Utc;

    /// Catálogo vazio: estes testes montam os modelos que querem avaliar.
    fn bare()->Config { Config{providers:Default::default(),models:Default::default(),..Config::default()} }

    fn sample(model:&str,success:bool)->PerformanceRecord { PerformanceRecord{task_type:"code".into(),strategy_used:"direct".into(),model_used:model.into(),success,response_time_ms:12,input_tokens:8,output_tokens:16,estimated_cost:0.0,timestamp:Utc::now()} }
    fn premium()->(Config,ModelConfig) { let mut config=bare(); let model=ModelConfig{enabled:true,provider:"anthropic".into(),model:"claude-sonnet-4-5".into(),capabilities:vec!["code".into(),"tools".into()],context_window:200_000,..Default::default()}; config.models.insert("coding-premium".into(),model.clone()); (config,model) }

    #[test]
    fn maps_code_capabilities() {
        assert!(required_capabilities("code").contains(&"tools".into()));
    }

    #[test]
    fn missing_models_route_to_jev_configuration_help() {
        let selection = select_model(
            &bare(),
            "general",
            "trivial",
            &Context::default(),
            &PerformanceTracker::default(),
        );

        assert_eq!(selection.provider, "jev");
        assert_eq!(selection.model_name, "configuration");
        assert_ne!(selection.provider, "none");
    }

    #[test]
    fn ignores_disabled_local_model() {
        let mut config=bare();
        config.providers.insert("local".into(),crate::config::ProviderConfig { enabled:false, kind:"openai-compatible".into(), base_url:Some("http://127.0.0.1:1234/v1".into()), ..Default::default() });
        config.providers.insert("cloud".into(),crate::config::ProviderConfig { enabled:true, kind:"openai".into(), api_key:Some("configured".into()), ..Default::default() });
        config.models.insert("local-fast".into(),ModelConfig { enabled:false, provider:"local".into(), model:"local".into(), capabilities:vec!["chat".into()], context_window:8_192, ..Default::default() });
        config.models.insert("cloud-model".into(),ModelConfig { enabled:true, provider:"cloud".into(), model:"cloud".into(), capabilities:vec!["chat".into()], context_window:8_192, ..Default::default() });

        let selection=select_model(&config,"general","trivial",&Context::default(),&PerformanceTracker::default());

        assert_eq!(selection.provider,"cloud");
        assert_eq!(selection.model_name,"cloud");
    }

    #[test]
    fn historical_bonus_uses_the_recorded_model_id_not_the_registry_alias() {
        let (config,model)=premium();
        let mut tracker=PerformanceTracker::default();
        tracker.record(sample("claude-sonnet-4-5",true)); tracker.record(sample("claude-sonnet-4-5",true)); tracker.record(sample("claude-sonnet-4-5",false)); tracker.record(sample("claude-sonnet-4-5",false));
        assert!(tracker.model_score("coding-premium").is_none());
        let baseline=score_model("coding-premium",&model,"simple",&config,&PerformanceTracker::default());
        let learned=score_model("coding-premium",&model,"simple",&config,&tracker);
        assert!((learned-baseline-0.5*HISTORICAL_WEIGHT).abs()<1e-9,"expected historical bonus, got {learned} vs {baseline}");
    }

    #[test]
    fn historical_bonus_falls_back_to_the_alias_when_the_model_id_is_empty() {
        let mut config=bare();
        let model=ModelConfig{enabled:true,provider:"local".into(),model:String::new(),capabilities:vec!["chat".into()],context_window:8_192,..Default::default()};
        config.models.insert("local-fast".into(),model.clone());
        let mut tracker=PerformanceTracker::default(); tracker.record(sample("local-fast",true));
        assert!(score_model("local-fast",&model,"simple",&config,&tracker)>score_model("local-fast",&model,"simple",&config,&PerformanceTracker::default()));
    }

    #[test]
    fn one_lucky_success_does_not_outrank_a_long_good_record() {
        let mut config=bare();
        config.providers.insert("cloud".into(),crate::config::ProviderConfig { enabled:true, kind:"openai".into(), api_key:Some("configured".into()), ..Default::default() });
        let template=ModelConfig{enabled:true,provider:"cloud".into(),capabilities:vec!["chat".into()],context_window:8_192,..Default::default()};
        config.models.insert("newcomer".into(),ModelConfig{model:"newcomer-1".into(),..template.clone()});
        config.models.insert("veteran".into(),ModelConfig{model:"veteran-1".into(),..template.clone()});
        let mut tracker=PerformanceTracker::default();
        tracker.record(sample("newcomer-1",true));
        for index in 0..20 { tracker.record(sample("veteran-1",index<18)); }
        assert_eq!(tracker.model_score("newcomer-1"),Some(1.0));
        assert!(tracker.model_score("veteran-1").expect("history")<1.0);
        assert!(tracker.shrunk_model_score("newcomer-1")<tracker.shrunk_model_score("veteran-1"));
        assert_eq!(select_model(&config,"general","trivial",&Context::default(),&tracker).model_name,"veteran-1");
        let newcomer=ModelConfig{model:"newcomer-1".into(),..template};
        let baseline=score_model("newcomer",&newcomer,"simple",&config,&PerformanceTracker::default());
        assert!(score_model("newcomer",&newcomer,"simple",&config,&tracker)-baseline<0.6*HISTORICAL_WEIGHT);
    }

    #[test]
    fn disabled_adaptive_routing_drops_the_historical_term() {
        let (mut config,model)=premium(); config.jev.adaptive_routing.enabled=false;
        let mut tracker=PerformanceTracker::default(); tracker.record(sample("claude-sonnet-4-5",true));
        assert_eq!(score_model("coding-premium",&model,"simple",&config,&tracker),score_model("coding-premium",&model,"simple",&config,&PerformanceTracker::default()));
    }

    #[test]
    fn load_tolerates_missing_empty_and_corrupt_files() {
        let dir=tempfile::tempdir().expect("temporary directory");
        assert_eq!(PerformanceTracker::load(&dir.path().join("absent.json")).len(),0);
        let empty=dir.path().join("empty.json"); std::fs::write(&empty,"").expect("fixture");
        assert_eq!(PerformanceTracker::load(&empty).len(),0);
        let corrupt=dir.path().join("corrupt.json"); std::fs::write(&corrupt,"{not json at all[[").expect("fixture");
        assert_eq!(PerformanceTracker::load(&corrupt).len(),0);
        let foreign=dir.path().join("legacy.json"); std::fs::write(&foreign,r#"{"model_performance":{"local-fast":{"total_executions":3}}}"#).expect("fixture");
        assert_eq!(PerformanceTracker::load(&foreign).len(),0);
        assert_eq!(PerformanceTracker::load(dir.path()).len(),0);
    }

    #[test]
    fn save_then_load_round_trips_records() {
        let dir=tempfile::tempdir().expect("temporary directory");
        let path=dir.path().join("nested").join(".jev_performance.json");
        let mut tracker=PerformanceTracker::default(); tracker.record(sample("claude-sonnet-4-5",true)); tracker.record(sample("claude-sonnet-4-5",false));
        tracker.save(&path).expect("save");
        assert!(!path.with_extension("json.tmp").exists());
        let loaded=PerformanceTracker::load(&path);
        assert_eq!(loaded.len(),2);
        assert_eq!(loaded.model_score("claude-sonnet-4-5"),Some(0.5));
        assert!(loaded.model_score("gpt-5").is_none());
    }

    #[test]
    fn record_and_load_cap_history_at_one_thousand() {
        let dir=tempfile::tempdir().expect("temporary directory");
        let path=dir.path().join(".jev_performance.json");
        let oversized=(0..1_500).map(|i|sample(&format!("model-{i}"),i%2==0)).collect::<Vec<_>>();
        std::fs::write(&path,serde_json::to_vec(&oversized).expect("json")).expect("fixture");
        let loaded=PerformanceTracker::load(&path);
        assert_eq!(loaded.len(),MAX_RECORDS);
        assert!(loaded.model_score("model-499").is_none());
        assert!(loaded.model_score("model-500").is_some());
        assert!(loaded.model_score("model-1499").is_some());
        let mut tracker=PerformanceTracker::default();
        for record in oversized { tracker.record(record); }
        assert_eq!(tracker.len(),MAX_RECORDS);
        assert!(tracker.model_score("model-499").is_none());
    }
}
