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

/// Como desempatar modelos com a mesma nota. Primeiro vale a ordem de
/// preferência dos agentes, quando a pessoa definiu uma
/// (`jev.agent_order`). Sem nada, vale o nome, para a
/// mesma pergunta cair sempre no mesmo modelo. Com o chat, o agente que ele já
/// usa ganha o empate (a sessão dele é retomada) e, num chat novo, a ordem dos
/// empatados sai de uma mistura do chat com o nome: chats diferentes se
/// espalham entre os agentes configurados, e cada chat fica sempre no mesmo.
#[derive(Debug,Default,Clone,Copy)]
pub struct Tiebreak<'a> {
    /// O agente e o modelo da sessão que o chat pode retomar.
    pub sticky:Option<(&'a str,&'a str)>,
    pub seed:&'a str,
}

/// FNV-1a: estável entre execuções e versões, ao contrário do hasher padrão.
fn mix(seed:&str,name:&str)->u64 {
    seed.bytes().chain([0]).chain(name.bytes()).fold(0xcbf2_9ce4_8422_2325,|hash,byte|(hash^u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// Todos os modelos que podem atender o pedido, do melhor para o pior. O
/// primeiro é o escolhido; os de outro agente são o plano B quando o escolhido
/// não consegue nem começar.
pub fn rank_models(config:&Config,intent:&str,complexity:&str,context:&Context,performance:&PerformanceTracker,tiebreak:&Tiebreak<'_>)->Vec<ModelSelection> {
    let required=required_capabilities(intent); let required_set=required.iter().collect::<HashSet<_>>();
    let budget=*config.budgets.get(complexity).unwrap_or(&12_000);
    let executable=|model:&ModelConfig|model.enabled && config.providers.get(&model.provider).is_some_and(|provider|provider.is_executable());
    let mut choices=config.models.iter().filter(|(_,m)| executable(m) && context.estimated_tokens<=m.context_window && required_set.iter().all(|cap|m.capabilities.contains(cap))).map(|(name,m)|(name,m,score_model(name,m,intent,complexity,config,performance))).collect::<Vec<_>>();
    if choices.is_empty() { choices=config.models.iter().filter(|(_,m)|executable(m) && context.estimated_tokens<=m.context_window).map(|(name,m)|(name,m,score_model(name,m,intent,complexity,config,performance))).collect(); }
    // A nota arredondada: somas iguais em ordem diferente não viram desempate.
    let rounded=|score:f64|(score*1e6).round() as i64;
    let stays=|model:&ModelConfig|match tiebreak.sticky { Some((provider,id))=>(model.provider!=provider,effective_model_id("",model)!=id), None=>(true,true) };
    let order=|name:&str|if tiebreak.seed.is_empty() {0} else {mix(tiebreak.seed,name)};
    // Agente fora da lista vem depois dos listados.
    let preferred=|model:&ModelConfig|config.jev.agent_order.iter().position(|agent|*agent==model.provider).unwrap_or(usize::MAX);
    choices.sort_by(|a,b|rounded(b.2).cmp(&rounded(a.2)).then_with(||preferred(a.1).cmp(&preferred(b.1))).then_with(||stays(a.1).cmp(&stays(b.1))).then_with(||order(a.0).cmp(&order(b.0))).then_with(||a.0.cmp(b.0)));
    choices.into_iter().map(|(name,model,score)|ModelSelection{model_name:effective_model_id(name,model).to_string(),provider:model.provider.clone(),estimated_tokens:context.estimated_tokens.min(budget),score,reason:format!("{intent}/{complexity} wants a {} model; matched {} capabilities within {budget}-token budget",["local","small","mid-size","large"][wanted_tier(intent,complexity)],required.len()),..Default::default()}).collect()
}

/// O porte de um modelo já escolhido (1 pequeno, 2 médio, 3 grande; o local
/// conta como pequeno), lido da configuração dele.
pub fn selection_tier(config:&Config,selection:&ModelSelection)->Option<usize> {
    config.models.iter().find(|(name,model)|model.provider==selection.provider&&effective_model_id(name,model)==selection.model_name)
        .and_then(|(_,model)|cost_rank(&model.cost_class)).map(|rank|rank.max(1))
}

/// Dentro de um chat com sessão viva, o modelo da sessão vem primeiro: trocar
/// de modelo abre sessão nova, e o agente relê o projeto do zero — o que custa
/// mais que a diferença entre um modelo pequeno e um médio. O porte escolhe o
/// modelo no primeiro pedido; os seguintes ficam nele, salvo um pedido
/// complexo que peça porte maior que o da sessão. O esforço continua variando
/// por pedido, porque não quebra a sessão.
pub fn keep_session_model(mut ranked:Vec<ModelSelection>,config:&Config,sticky:Option<(&str,&str)>,intent:&str,complexity:&str)->Vec<ModelSelection> {
    let Some((provider,model))=sticky else { return ranked };
    let Some(index)=ranked.iter().position(|candidate|candidate.provider==provider&&candidate.model_name==model) else { return ranked };
    let outgrown=complexity=="complex"&&selection_tier(config,&ranked[index]).is_some_and(|tier|tier<wanted_tier(intent,complexity));
    if !outgrown { let kept=ranked.remove(index); ranked.insert(0,kept); }
    ranked
}

/// O que nenhum modelo configurado atende: o Jev explica como configurar.
pub fn configuration_selection(config:&Config,complexity:&str,context:&Context)->ModelSelection {
    let budget=*config.budgets.get(complexity).unwrap_or(&12_000);
    ModelSelection{model_name:"configuration".into(),provider:"jev".into(),estimated_tokens:context.estimated_tokens.min(budget),score:0.0,reason:"no configured model can satisfy this request".into(),..Default::default()}
}

pub fn select_model(config:&Config,intent:&str,complexity:&str,context:&Context,performance:&PerformanceTracker)->ModelSelection {
    rank_models(config,intent,complexity,context,performance,&Tiebreak::default()).into_iter().next().unwrap_or_else(||configuration_selection(config,complexity,context))
}
/// O modelo sem histórico entra com a nota do meio, a mesma para onde a nota
/// de quem tem poucos registros é puxada: sem isso, quem nunca foi escolhido
/// ficava sempre atrás de quem já acertou uma vez, e nunca ganhava histórico.
fn score_model(name:&str,model:&ModelConfig,intent:&str,complexity:&str,config:&Config,performance:&PerformanceTracker)->f64 { let mut score=1.0+tier_fit(model,wanted_tier(intent,complexity)); if config.jev.optimization.prefer_local && model.cost_class=="free" {score+=FREE_COST_BONUS;} if complexity=="complex" && model.capabilities.contains(&"reasoning".into()){score+=COMPLEX_REASONING_BONUS;} if model.speed=="fast" && matches!(complexity,"trivial"|"simple") {score+=FAST_SPEED_BONUS;} if config.jev.adaptive_routing.enabled { score+=performance.shrunk_model_score(effective_model_id(name,model),intent).unwrap_or(HISTORICAL_PRIOR)*HISTORICAL_WEIGHT; } score }

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

    /// O chat de iniciante do relatório: com o porte decidindo cada pedido, o
    /// modelo troca a cada volta; com o modelo da sessão na frente, só o
    /// pedido complexo que pede porte maior troca.
    #[test]
    fn the_session_model_stays_unless_a_complex_request_outgrows_it() {
        let mut config=bare();
        config.providers.insert("claude".into(),crate::config::ProviderConfig { enabled:true, kind:"openai".into(), api_key:Some("configured".into()), ..Default::default() });
        let all=vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()];
        for (id,cost,speed) in [("haiku","low","fast"),("sonnet","medium","medium"),("opus","high","slow")] {
            config.models.insert(format!("claude:{id}"),ModelConfig{enabled:true,provider:"claude".into(),model:id.into(),capabilities:all.clone(),cost_class:cost.into(),speed:speed.into(),context_window:200_000});
        }
        let chat=[("frontend","medium"),("code","simple"),("analysis","simple"),("code","simple"),("code","simple"),("code","simple"),("refactor","medium"),("test","medium")];
        let run=|keep:bool|{
            let mut previous:Option<String>=None; let mut switches=0;
            for (intent,complexity) in chat {
                let ranked=rank_models(&config,intent,complexity,&Context::default(),&PerformanceTracker::default(),&Tiebreak::default());
                let ranked=if keep { keep_session_model(ranked,&config,previous.as_deref().map(|model|("claude",model)),intent,complexity) } else { ranked };
                let chosen=ranked[0].model_name.clone();
                if previous.as_ref().is_some_and(|before|*before!=chosen) { switches+=1; }
                previous=Some(chosen);
            }
            switches
        };
        assert!(run(false)>=2,"sem a regra, o modelo troca a cada porte: {}",run(false));
        assert_eq!(run(true),0,"com a regra, o chat fica no modelo do primeiro pedido");

        let ranked=rank_models(&config,"refactor","complex",&Context::default(),&PerformanceTracker::default(),&Tiebreak::default());
        assert_eq!(keep_session_model(ranked.clone(),&config,Some(("claude","haiku")),"refactor","complex")[0].model_name,"opus","o complexo que pede porte maior troca");
        assert_eq!(keep_session_model(ranked.clone(),&config,Some(("claude","opus")),"refactor","complex")[0].model_name,"opus");
        assert_eq!(keep_session_model(ranked.clone(),&config,Some(("other","x")),"refactor","complex")[0].model_name,"opus","sessão de modelo fora da lista não muda nada");
        assert_eq!(selection_tier(&config,&ranked[0]),Some(3));
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
        let mut without=config.clone(); without.jev.adaptive_routing.enabled=false;
        let baseline=score_model("coding-premium",&model,"code","simple",&without,&PerformanceTracker::default());
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

    fn four_agents()->Config {
        let mut config=bare();
        for agent in ["claude","codex","copilot","cursor"] { config.providers.insert(agent.into(),crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some(agent.into()),..Default::default()}); }
        let all=vec!["chat".to_string(),"code".into(),"reasoning".into(),"tools".into()];
        for (agent,model) in [("claude","sonnet"),("codex","gpt-5.5"),("copilot","claude-sonnet-4.5"),("cursor","auto")] {
            config.models.insert(format!("{agent}/{model}"),ModelConfig{enabled:true,provider:agent.into(),model:model.into(),capabilities:all.clone(),cost_class:"medium".into(),speed:"medium".into(),context_window:200_000});
        }
        config
    }

    /// Com quatro agentes do mesmo porte, a ordem alfabética dava todo pedido
    /// ao Claude. Agora cada chat cai num deles, e chats diferentes se
    /// espalham.
    #[test] fn tied_agents_are_spread_across_chats_and_stable_per_chat() {
        let config=four_agents();
        let pick=|seed:&str|rank_models(&config,"code","medium",&Context::default(),&PerformanceTracker::default(),&Tiebreak{sticky:None,seed})[0].provider.clone();
        let picked:HashSet<String>=(0..40).map(|chat|pick(&format!("chat-{chat}"))).collect();
        assert!(picked.len()>=3,"os empates continuam caindo quase sempre no mesmo agente: {picked:?}");
        assert_eq!(pick("chat-7"),pick("chat-7"),"o mesmo chat fica no mesmo agente");
    }

    #[test] fn the_agent_the_chat_already_uses_wins_the_tie() {
        let config=four_agents();
        for seed in ["a","b","c","d"] {
            let ranked=rank_models(&config,"code","medium",&Context::default(),&PerformanceTracker::default(),&Tiebreak{sticky:Some(("codex","gpt-5.5")),seed});
            assert_eq!(ranked[0].provider,"codex");
            assert_eq!(ranked.len(),4,"os outros ficam como plano B");
        }
    }

    /// A ordem que a pessoa definiu decide o empate antes da sessão do chat
    /// e da mistura; quem ficou fora da lista vem depois dos listados.
    #[test] fn the_preferred_order_decides_ties() {
        let mut config=four_agents();
        config.jev.agent_order=vec!["cursor".into(),"codex".into()];
        for seed in ["a","b","c","d"] {
            let ranked=rank_models(&config,"code","medium",&Context::default(),&PerformanceTracker::default(),&Tiebreak{sticky:Some(("claude","sonnet")),seed});
            assert_eq!((ranked[0].provider.as_str(),ranked[1].provider.as_str()),("cursor","codex"));
            assert_eq!(ranked[2].provider,"claude","fora da lista, a sessão do chat ainda desempata");
        }
        config.models.get_mut("cursor/auto").expect("auto").cost_class="high".into();
        let ranked=rank_models(&config,"general","trivial",&Context::default(),&PerformanceTracker::default(),&Tiebreak::default());
        assert_eq!(ranked[0].provider,"codex","a preferência não passa por cima do porte");
    }

    /// O desempate nunca passa por cima da nota: o porte certo continua
    /// ganhando do agente da sessão.
    #[test] fn stickiness_does_not_beat_a_better_score() {
        let mut config=four_agents();
        config.models.get_mut("claude/sonnet").expect("sonnet").cost_class="high".into();
        let ranked=rank_models(&config,"general","trivial",&Context::default(),&PerformanceTracker::default(),&Tiebreak{sticky:Some(("claude","sonnet")),seed:"x"});
        assert_ne!(ranked[0].provider,"claude");
    }

    #[test] fn a_model_without_history_is_not_behind_one_lucky_success() {
        let config=four_agents();
        let mut tracker=PerformanceTracker::default();
        tracker.record(sample("sonnet",false));
        let ranked=rank_models(&config,"code","medium",&Context::default(),&tracker,&Tiebreak{sticky:Some(("claude","sonnet")),seed:"x"});
        assert_ne!(ranked[0].provider,"claude","quem falhou cede a vez para quem ainda não foi testado");
    }
}
