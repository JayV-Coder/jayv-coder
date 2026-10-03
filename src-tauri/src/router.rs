use crate::{config::{Config, ModelConfig}, model::{Context, ModelSelection, PerformanceRecord}};
use anyhow::{Context as _, Result};
use std::{collections::HashSet, fs, path::Path};

const MAX_RECORDS:usize=1000;
const FREE_COST_BONUS:f64=1.5;
const COMPLEX_REASONING_BONUS:f64=1.2;
const FAST_SPEED_BONUS:f64=0.4;
/// O quanto vale o modelo do porte que o pedido pede, e o quanto cada degrau
/// de distância tira. Pesa mais que a velocidade e que o histórico juntos: um
/// pedido grande não vai para o modelo pequeno só porque ele responde rápido.
const TIER_BONUS:f64=2.5;
const TIER_STEP_PENALTY:f64=1.25;
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
    /// A nota do modelo, puxada para o meio enquanto há poucos registros. Com
    /// registros bastantes do mesmo tipo de pedido, vale a nota naquele tipo:
    /// o modelo que acerta revisão não é por isso o que acerta teste.
    fn shrunk_model_score(&self, model:&str, intent:&str)->Option<f64> {
        let all=self.records.iter().filter(|r|r.model_used==model).collect::<Vec<_>>();
        if all.is_empty(){return None;}
        let specific=all.iter().copied().filter(|r|r.task_type==intent).collect::<Vec<_>>();
        let items=if specific.len()>=INTENT_SAMPLES {specific} else {all};
        let successes=items.iter().filter(|r|r.success).count() as f64;
        Some((successes+HISTORICAL_PRIOR*HISTORICAL_PRIOR_SAMPLES)/(items.len() as f64+HISTORICAL_PRIOR_SAMPLES))
    }
    /// O último pedido do chat não resolveu, ainda que a resposta tenha vindo
    /// inteira: o desenvolvedor reclamou dela ou a portaria de saída a segurou.
    /// Devolve se havia um sucesso a desfazer.
    pub fn mark_failed(&mut self, chat:&str)->bool {
        let Some(record)=self.records.iter_mut().rev().find(|record|record.chat.as_deref()==Some(chat)) else { return false };
        std::mem::replace(&mut record.success,false)
    }
}

/// Registros do mesmo tipo de pedido a partir dos quais a nota do tipo vale.
const INTENT_SAMPLES:usize=3;

/// O pedido que diz que a resposta anterior não resolveu: "não funcionou",
/// "ainda dá erro", "that's wrong". Só vale em mensagem curta — um pedido
/// longo que cita um erro é pedido novo, não reclamação.
pub fn is_complaint(text:&str)->bool {
    static COMPLAINT:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    const WORDS:usize=40;
    let pattern=COMPLAINT.get_or_init(||regex::Regex::new(r"(?i)\b(?:n[ãa]o funcionou|n[ãa]o funciona|n[ãa]o deu certo|n[ãa]o era isso|n[ãa]o resolveu|ainda (?:d[áa]|est[áa]|continua|n[ãa]o|falha|quebra)|continua (?:dando|com|quebrado|falhando)|deu erro|est[áa] errado|ficou errado|quebrou|doesn'?t work|didn'?t work|not working|still (?:fails|failing|broken|not|wrong|errors?)|that'?s wrong|is wrong|it broke|no funcion[óa]|sigue (?:fallando|sin)|est[áa] mal)\b").expect("complaint regex"));
    text.split_whitespace().count()<=WORDS&&pattern.is_match(text)
}

fn effective_model_id<'a>(name:&'a str,model:&'a ModelConfig)->&'a str { if model.model.is_empty(){name}else{&model.model} }

pub fn required_capabilities(intent:&str)->Vec<String> { match intent { "code"|"refactor"=>vec!["code".into(),"tools".into()], "test"=>vec!["code".into(),"tools".into()], "security"=>vec!["reasoning".into(),"tools".into()], "analysis"|"review"=>vec!["reasoning".into()], "frontend"=>vec!["code".into()], _=>vec!["chat".into()] } }

/// O porte de um modelo pela classe de custo: `free` (local) 0, `low` 1,
/// `medium` 2, `high` 3. Classe desconhecida não entra na conta de porte.
fn cost_rank(cost_class:&str)->Option<usize> { ["free","low","medium","high"].iter().position(|class|*class==cost_class) }

/// O porte que o pedido pede: o pequeno para o trivial, o grande para o
/// complexo. Análise, revisão e segurança pensam mais que escrevem e sobem um
/// degrau a partir do médio.
pub fn wanted_tier(intent:&str,complexity:&str)->usize {
    let base=match complexity { "trivial"|"simple"=>1, "medium"=>2, "complex"=>3, _=>2 };
    if base>=2 && matches!(intent,"analysis"|"review"|"security") { (base+1).min(3) } else { base }
}

fn tier_fit(model:&ModelConfig,wanted:usize)->f64 {
    match cost_rank(&model.cost_class) {
        // O modelo local é do porte pequeno: o bônus de local é à parte.
        Some(rank)=>TIER_BONUS-TIER_STEP_PENALTY*(rank.max(1).abs_diff(wanted) as f64),
        None=>0.0,
    }
}

pub fn select_model(config:&Config,intent:&str,complexity:&str,context:&Context,performance:&PerformanceTracker)->ModelSelection {
    let required=required_capabilities(intent); let required_set=required.iter().collect::<HashSet<_>>();
    let budget=*config.budgets.get(complexity).unwrap_or(&12_000);
    let executable=|model:&ModelConfig|model.enabled && config.providers.get(&model.provider).is_some_and(|provider|provider.is_executable());
    let mut choices=config.models.iter().filter(|(_,m)| executable(m) && context.estimated_tokens<=m.context_window && required_set.iter().all(|cap|m.capabilities.contains(cap))).map(|(name,m)|(name,m,score_model(name,m,intent,complexity,config,performance))).collect::<Vec<_>>();
    if choices.is_empty() { choices=config.models.iter().filter(|(_,m)|executable(m) && context.estimated_tokens<=m.context_window).map(|(name,m)|(name,m,score_model(name,m,intent,complexity,config,performance))).collect(); }
    // Empate desfeito pelo nome: a mesma pergunta cai sempre no mesmo modelo.
    choices.sort_by(|a,b|b.2.total_cmp(&a.2).then_with(||a.0.cmp(b.0)));
    match choices.first() { Some((name,model,score))=>ModelSelection{model_name:effective_model_id(name,model).to_string(),provider:model.provider.clone(),estimated_tokens:context.estimated_tokens.min(budget),score:*score,reason:format!("{intent}/{complexity} wants a {} model; matched {} capabilities within {budget}-token budget",["local","small","mid-size","large"][wanted_tier(intent,complexity)],required.len()),..Default::default()}, None=>ModelSelection{model_name:"configuration".into(),provider:"jev".into(),estimated_tokens:context.estimated_tokens.min(budget),score:0.0,reason:"no configured model can satisfy this request".into(),..Default::default()} }
}
fn score_model(name:&str,model:&ModelConfig,intent:&str,complexity:&str,config:&Config,performance:&PerformanceTracker)->f64 { let mut score=1.0+tier_fit(model,wanted_tier(intent,complexity)); if config.jev.optimization.prefer_local && model.cost_class=="free" {score+=FREE_COST_BONUS;} if complexity=="complex" && model.capabilities.contains(&"reasoning".into()){score+=COMPLEX_REASONING_BONUS;} if model.speed=="fast" && matches!(complexity,"trivial"|"simple") {score+=FAST_SPEED_BONUS;} if config.jev.adaptive_routing.enabled { if let Some(historical)=performance.shrunk_model_score(effective_model_id(name,model),intent){score+=historical*HISTORICAL_WEIGHT;} } score }

#[cfg(test)] mod tests {
    use super::*;
    use chrono::Utc;

    /// Catálogo vazio: estes testes montam os modelos que querem avaliar.
    fn bare()->Config { Config{providers:Default::default(),models:Default::default(),..Config::default()} }

    fn sample(model:&str,success:bool)->PerformanceRecord { PerformanceRecord{task_type:"code".into(),strategy_used:"direct".into(),model_used:model.into(),success,response_time_ms:12,input_tokens:8,output_tokens:16,estimated_cost:0.0,timestamp:Utc::now(),chat:None} }
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

    /// Três portes no mesmo agente: o pedido escolhe o porte, não a pressa.
    #[test]
    fn the_size_of_the_request_picks_the_size_of_the_model() {
        let mut config=bare();
        config.providers.insert("claude".into(),crate::config::ProviderConfig { enabled:true, kind:"openai".into(), api_key:Some("configured".into()), ..Default::default() });
        let all=vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()];
        for (id,cost,speed) in [("small","low","fast"),("mid","medium","medium"),("large","high","slow")] {
            config.models.insert(format!("claude:{id}"),ModelConfig{enabled:true,provider:"claude".into(),model:id.into(),capabilities:all.clone(),cost_class:cost.into(),speed:speed.into(),context_window:200_000});
        }
        let pick=|config:&Config,intent:&str,complexity:&str|select_model(config,intent,complexity,&Context::default(),&PerformanceTracker::default()).model_name;
        assert_eq!(pick(&config,"general","trivial"),"small");
        assert_eq!(pick(&config,"code","simple"),"small");
        assert_eq!(pick(&config,"code","medium"),"mid");
        assert_eq!(pick(&config,"analysis","medium"),"large","análise média pensa como pedido grande");
        assert_eq!(pick(&config,"refactor","complex"),"large");

        // Um disabled sai da conta, e o porte vizinho assume.
        config.models.get_mut("claude:large").expect("large").enabled=false;
        assert_eq!(pick(&config,"refactor","complex"),"mid");
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
        let baseline=score_model("coding-premium",&model,"code","simple",&config,&PerformanceTracker::default());
        let learned=score_model("coding-premium",&model,"code","simple",&config,&tracker);
        assert!((learned-baseline-0.5*HISTORICAL_WEIGHT).abs()<1e-9,"expected historical bonus, got {learned} vs {baseline}");
    }

    #[test]
    fn historical_bonus_falls_back_to_the_alias_when_the_model_id_is_empty() {
        let mut config=bare();
        let model=ModelConfig{enabled:true,provider:"local".into(),model:String::new(),capabilities:vec!["chat".into()],context_window:8_192,..Default::default()};
        config.models.insert("local-fast".into(),model.clone());
        let mut tracker=PerformanceTracker::default(); tracker.record(sample("local-fast",true));
        assert!(score_model("local-fast",&model,"general","simple",&config,&tracker)>score_model("local-fast",&model,"general","simple",&config,&PerformanceTracker::default()));
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
        assert!(tracker.shrunk_model_score("newcomer-1","code")<tracker.shrunk_model_score("veteran-1","code"));
        assert_eq!(select_model(&config,"general","trivial",&Context::default(),&tracker).model_name,"veteran-1");
        let newcomer=ModelConfig{model:"newcomer-1".into(),..template};
        let baseline=score_model("newcomer",&newcomer,"general","simple",&config,&PerformanceTracker::default());
        assert!(score_model("newcomer",&newcomer,"general","simple",&config,&tracker)-baseline<0.6*HISTORICAL_WEIGHT);
    }

    #[test]
    fn disabled_adaptive_routing_drops_the_historical_term() {
        let (mut config,model)=premium(); config.jev.adaptive_routing.enabled=false;
        let mut tracker=PerformanceTracker::default(); tracker.record(sample("claude-sonnet-4-5",true));
        assert_eq!(score_model("coding-premium",&model,"code","simple",&config,&tracker),score_model("coding-premium",&model,"code","simple",&config,&PerformanceTracker::default()));
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

    #[test]
    fn a_complaint_in_the_chat_turns_the_last_success_into_a_failure() {
        let mut tracker=PerformanceTracker::default();
        tracker.record(PerformanceRecord{chat:Some("chat-a".into()),..sample("model-a",true)});
        tracker.record(PerformanceRecord{chat:Some("chat-b".into()),..sample("model-a",true)});
        assert!(tracker.mark_failed("chat-a"));
        assert!(!tracker.mark_failed("chat-a"),"já era falha");
        assert!(!tracker.mark_failed("chat-c"));
        assert_eq!(tracker.model_score("model-a"),Some(0.5));
        assert!(is_complaint("não funcionou, ainda dá erro no build"));
        assert!(is_complaint("That's wrong, it still fails"));
        assert!(!is_complaint("adicione um teste para o roteador"));
        assert!(!is_complaint(&format!("{} não funcionou",["palavra";50].join(" "))),"pedido longo é pedido novo");
    }

    #[test]
    fn with_enough_history_of_one_kind_the_kind_decides() {
        let mut tracker=PerformanceTracker::default();
        for _ in 0..3 { tracker.record(PerformanceRecord{task_type:"test".into(),..sample("model-a",false)}); }
        for _ in 0..6 { tracker.record(sample("model-a",true)); }
        assert!(tracker.shrunk_model_score("model-a","test").unwrap()<0.5,"erra os testes, mesmo acertando o resto");
        assert!(tracker.shrunk_model_score("model-a","code").unwrap()>0.5);
        assert_eq!(tracker.shrunk_model_score("model-a","review"),tracker.shrunk_model_score("model-a","docs"),"sem histórico do tipo, vale o geral");
    }
}
