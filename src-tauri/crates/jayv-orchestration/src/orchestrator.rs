use crate::{
    agents::AgentRegistry, cache::SemanticCache, config::Config,
    context_engine::{optimize_for_budget, rank_fragments, ContextFragment},
    firewall::ContextFirewall, graph::ExecutionGraph, i18n::{self, Text}, jev, memory::{AgentSession, MemoryManager},
    model::{ChatMessage, Context, ContextSnippet, Decision, IntentAnalysis, ModelSelection, PerformanceRecord, ProcessResult, ProviderResponse, RoutingSignals},
    progress::{Beat, ModeSwitch, Pulse},
    providers::{build_providers, Provider, Workdir}, rag::RepositoryRag, router::{configuration_selection, keep_session_model, rank_models, required_capabilities, PerformanceTracker, Tiebreak},
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, sync::{Arc, Mutex, MutexGuard, PoisonError}, time::Instant};

const SYSTEM_INSTRUCTIONS:&str="You are Jev, a senior software engineering orchestrator. Use supplied repository context only when relevant. Never reveal secrets. State uncertainty explicitly. To ask the developer something, end the reply with the questions, each on its own line ending in '?' with its options right below as a '- ' list.";
const REMOVAL_NOTE:&str="Context was filtered on purpose: [*_REDACTED] replaces secrets, [CONTEXT_TRUNCATED] marks a file cut to fit the token budget, and some files were withheld. Never guess removed content; say it is missing when it matters.";
const TRUNCATION_MARKER:&str="\n[CONTEXT_TRUNCATED]";
const PERFORMANCE_FILE:&str=".jev_performance.json";
const DEFAULT_BUDGET:usize=12_000;
const HISTORY_MESSAGES:usize=6;
/// Uma resposta antiga do modelo volta no histórico só até aqui: o pedido novo
/// precisa do assunto, não do código inteiro que já foi entregue.
const HISTORY_REPLY_CHARS:usize=1_500;
const HISTORY_CUT:&str="\n[EARLIER REPLY SHORTENED]";
/// Quantos pedidos uma sessão de agente atende antes de começar outra: cada
/// volta relê a conversa inteira, e uma sessão longa demais volta a sair cara.
const RESUMED_TURNS:usize=10;
/// Linhas de definição por arquivo no mapa que vai aos agentes.
const MAP_OUTLINE_LINES:usize=8;
const MAP_LINE_CHARS:usize=140;
/// Quantos vizinhos ("usa" / "usado por") cada arquivo mostra no mapa.
const MAP_LINKS:usize=4;
const MAP_NOTE:&str="RELEVANT FILES lists where the task most likely lives, with the definitions found there. Open only the files you need, with your own tools.";
const TITLE_INSTRUCTIONS:&str="Name this conversation from the developer's first request. Answer with the title alone: at most six words, no quotes, no trailing period, no explanation.";
const TITLE_PROMPT_CHARS:usize=600;
/// O agente que cobra por pedido, não por token: não batiza chat.
const PER_REQUEST_AGENT:&str="copilot";
const REQUEST_MARGIN:usize=120;
const SNIPPET_WRAPPER:usize=4;
const NEUTRAL_SIGNAL:f64=0.5;
const CLARIFY_NOTE:&str="Routing confidence was low. If the request is ambiguous, ask one specific clarifying question before assuming an approach.";
/// Sem idioma escolhido no app, o modelo segue o idioma do pedido.
const REQUEST_LANGUAGE_NOTE:&str="Reply to the developer in the language their request is written in.";
const NO_REPOSITORY_NOTE:&str="No repository files were supplied: this request does not depend on them. Answer from general knowledge and never guess this codebase's contents.";
const TOOLS_NOTE:&str="This asks for commands to run or files to change, which this orchestrator cannot execute. Hand back the exact commands or edits for the developer to apply.";
const DESTRUCTIVE_NOTE:&str="This would overwrite or remove existing work. State the exact effect and how to undo it before giving the change.";
/// A nota da nova tentativa depois de "não funcionou".
const RETRY_NOTE:&str="The developer says the previous attempt did not solve this. Before changing anything, check what the previous change actually did and reproduce the failure; then fix the cause instead of repeating the same change, and say how you verified it.";
/// O cabeçalho do plano que segue inteiro para o build.
const PLAN_HANDOFF:&str="PLAN TO IMPLEMENT (written earlier in this chat in planning mode, in full; follow it unless the request above says otherwise):";
const PLAN_NOTE:&str="PLAN mode: the agent runs read-only. Answer with a concrete step-by-step plan (files, changes, how to verify) in the reply, not in a file. Do not claim that any file was changed.";
const BUILD_NOTE:&str="BUILD mode: make the change directly in the project folder within the permissions you were granted, then summarize what changed and how to verify it. You run without a terminal: nobody can answer a permission prompt, change your permission mode or edit your settings files. If a write or command is denied, say exactly what was denied and that the developer can allow it in JayV under Settings > Agents; never offer to approve prompts, switch permission modes or edit .claude/settings.json, and never work around the denial with shell commands.";
/// O relatório do build separa o que o agente rodou do que ele só supõe: o
/// JayV mostra a resposta como não verificada, e o desenvolvedor precisa saber
/// o que o agente diz ter visto acontecer.
const REPORT_NOTE:&str="End with a short report in three labelled parts: what you ran in this session and what it returned (commands, tests, builds), what you assume without having checked it, and what is still not verified. Never say that tests pass, that the project builds or that the change works unless you ran it in this session and saw the result.";
/// Os agentes saem explorando o repositório e replanejando por conta própria;
/// cada volta dessas é sessão gasta. Vai junto em todo pedido a um agente.
const FOCUS_NOTE:&str="Be brief: read only what the task needs, never reread what this conversation already holds, and do not re-plan.";
/// A regra de código enxuto do modo build, nas duas forças do nível. O
/// agente escreve menos — e cada linha que não escreve é token de saída e
/// revisão poupados —, sem nunca tirar o que protege dado e gente.
const LEAN_FULL_NOTE:&str="Write the least code that fully solves the task: check whether it must exist, then reuse the standard library, the platform or an installed dependency, and only then write new code. No speculative abstractions, options or files. Never drop input validation, error handling that prevents data loss, security or accessibility.";
const LEAN_LITE_NOTE:&str="Prefer the simplest change that fully works. When the standard library, the platform or an installed dependency already covers part of the task, say so and use it. Never drop input validation, error handling, security or accessibility.";
const MULTI_REPOSITORY_NOTE:&str="This folder holds several repositories of the same organization; keep each change inside the repository it belongs to and name it in the answer. REPOSITORIES:";
pub use crate::workspace::{work_mode, MODE_AUTO, MODE_BUILD, MODE_PLAN};
/// O chat estava fixo em planejamento e o pedido é para implementar.
pub const SWITCH_ASKED:&str="asked";
/// No automático, o pedido para implementar veio de novo depois de um que
/// ficou em planejamento.
pub const SWITCH_REPEATED:&str="repeated";
const REPOSITORY_CONTEXT_THRESHOLD:f64=0.5;
const TOOLS_THRESHOLD:f64=0.5;
const DESTRUCTIVE_THRESHOLD:f64=0.35;
const COMPLEXITY_MASS:f64=0.15;
const ROUTING_TURNS:usize=4;
const ROUTING_TURN_CHARS:usize=400;
const ROUTING_CANDIDATES:usize=8;
const SOURCE_JEV:&str="jev";
const SOURCE_LOCAL:&str="heuristic";
const SOURCE_FALLBACK:&str="heuristic_after_jev_error";

pub enum RoutingMode { Auto, Local, Fixed(Box<jev::RoutingDecision>), Failed(String) }
#[cfg(test)] fn default_routing_mode()->RoutingMode{RoutingMode::Local}
#[cfg(not(test))] fn default_routing_mode()->RoutingMode{RoutingMode::Auto}

pub struct Orchestrator {
    pub config_path: PathBuf,
    pub config: Config,
    pub memory: MemoryManager,
    pub rag: RepositoryRag,
    pub cache: SemanticCache,
    pub firewall: ContextFirewall,
    pub agents: AgentRegistry,
    pub graph: ExecutionGraph,
    /// O que os pedidos dividem quando correm ao mesmo tempo, cada um no seu
    /// `Orchestrator` (`Shared`).
    shared: Arc<Shared>,
    pub routing_mode: RoutingMode,
    /// A ressalva que o portão de entrada deixou para o próximo pedido, quando
    /// ele foi liberado com semáforo amarelo. `process` a consome uma vez.
    pub pending_gate_note: Option<String>,
    /// O pedido liberado pela portaria, reescrito para o modelo. Toma o lugar
    /// do texto cru só na mensagem enviada: roteamento, contexto e histórico
    /// continuam lendo o que o desenvolvedor escreveu. `process` o consome uma vez.
    pub pending_brief: Option<String>,
    /// Se a portaria liberou o pedido de vez (`pass`) ou com ressalva (`ask`).
    /// Só o liberado de vez pode ir em modo build. Sem portaria — a linha de
    /// comando, os testes — vale como liberado. `process` o consome uma vez.
    pub pending_gate_passed: Option<bool>,
    /// O nível da conta: quanto o Jev confia no pedido e até onde ele constrói.
    pub expertise: crate::expertise::Expertise,
    /// A organização cuja política de LLM vale para o pedido em atendimento
    /// (o `@slug`), ou nenhuma. Quem aplica a política (`use_llm` e
    /// `use_core` com as configurações já restritas) a marca aqui, para a
    /// orientação dizer quem barrou o pedido.
    pub policy_scope: Option<String>,
    /// A leitura de roteamento do Jev pedida junto com a portaria de entrada,
    /// em paralelo, para o pedido não esperar as duas idas uma depois da
    /// outra. `process` a consome uma vez; sem ela, o roteamento é pedido ali.
    pub pending_routing: Option<std::result::Result<jev::RoutingDecision,String>>,
    /// As notas do projeto do chat, prontas para o prompt. Como a memória do
    /// agente, vão só quando uma sessão começa: a sessão retomada já as leu.
    pub project_notes: Option<String>,
    /// Se a regra de código enxuto vai junto no modo build (configuração da
    /// conta, ligada por padrão).
    pub lean_code: bool,
    /// O que vai junto só deste pedido — a receita que casou com ele, a
    /// resposta que o projeto já tinha. `process` o consome uma vez.
    pub pending_context: Vec<String>,
    /// O modo que o desenvolvedor fixou para o chat (`auto`, `plan` ou
    /// `build`). `process` o consome uma vez; sem ele, vale o automático.
    pub pending_work_mode: Option<String>,
    /// A troca de modo que o Jev fez no último `process`, para quem chamou
    /// gravar o modo novo no chat.
    pub mode_switch: Option<ModeSwitch>,
    /// O pedido em atendimento reclama da resposta anterior ("não
    /// funcionou"). `process` o consome uma vez.
    pub pending_retry: bool,
    providers: HashMap<String, Box<dyn Provider>>,
    /// Os mesmos agentes, presos em somente leitura, para o modo planejamento.
    planners: HashMap<String, Box<dyn Provider>>,
    /// A pasta que os agentes de linha de comando enxergam. Anda junto com o
    /// índice: os dois descrevem o projeto do chat que está sendo atendido.
    workdir: Workdir,
    last_decision: Option<Decision>,
    /// A impressão dos agentes em uso (as configurações e os arquivos
    /// protegidos) e quando foram montados: o mesmo conjunto não é remontado a
    /// cada pedido.
    llm_built: Option<(u64,Instant)>,
    /// A revisão do que o pedido mudou, pronta para sair depois da resposta
    /// (`ReviewRequest`): quem atende a fila a leva e a roda fora do
    /// cadeado, e o pedido seguinte não espera a segunda opinião.
    pub pending_review: Option<ReviewRequest>,
}

impl Orchestrator {
    pub fn new(config_path: PathBuf, root: PathBuf) -> Result<Self> {
        let mut orchestrator=Self::unindexed(config_path,root)?;
        orchestrator.rag.index(&orchestrator.firewall)?;
        Ok(orchestrator)
    }

    /// O orquestrador sem ter lido a raiz ainda. O aplicativo de mesa sobe
    /// assim: todo pedido aponta para a pasta do projeto do chat antes de ser
    /// atendido, e ler de saída a pasta de onde o aplicativo foi aberto — o
    /// `$HOME`, pelo menu do sistema — custava tempo e memória à toa.
    pub fn unindexed(config_path: PathBuf, root: PathBuf) -> Result<Self> {
        let config=Config::load(&config_path)?;
        let firewall=ContextFirewall::new(config.privacy.clone());
        let performance_path=root.join(PERFORMANCE_FILE);
        let workdir=Workdir::default();
        workdir.focus(root.clone());
        let rag=RepositoryRag::new(root);
        Ok(Self { config_path, cache:SemanticCache::new(config.jev.context.cache_ttl,1000), providers:build_providers(&config.providers,&workdir), planners:build_planners(&config.providers,&workdir), workdir, config, memory:MemoryManager::default(), rag, firewall, agents:AgentRegistry::default(), graph:ExecutionGraph::default(), shared:Arc::new(Shared::load(performance_path)), routing_mode:default_routing_mode(), pending_gate_note:None, pending_brief:None, pending_gate_passed:None, expertise:Default::default(), policy_scope:None, pending_routing:None, project_notes:None, lean_code:true, pending_context:vec![], pending_work_mode:None, mode_switch:None, pending_retry:false, last_decision:None, llm_built:None, pending_review:None })
    }

    /// Os agentes e modelos que o banco guarda. O arquivo de configuração não
    /// fala deles; sem esta chamada o orquestrador não tem com quem conversar.
    /// Os valores de partida das configurações do Jev e do app: o que o
    /// `config.yaml` diz, ou os padrões.
    pub fn core_defaults(&self)->crate::core_settings::CoreSettings { Self::core_defaults_at(&self.config_path) }
    /// Os mesmos valores de partida, lidos sem o orquestrador: as telas que só
    /// leem não esperam o pedido que está no ar.
    pub fn core_defaults_at(config_path:&Path)->crate::core_settings::CoreSettings {
        Config::load(config_path).map(|config|crate::core_settings::CoreSettings::from_config(&config)).unwrap_or_else(|_|crate::core_settings::CoreSettings::from_config(&Config::default()))
    }

    /// Passa a usar as configurações do Jev e do app. Privacidade nova é
    /// firewall novo, e a pasta é lida de novo no próximo pedido; validade nova
    /// é cache novo.
    /// O que este orquestrador divide com os outros (`Shared`).
    pub fn shared(&self)->Arc<Shared> { self.shared.clone() }

    /// Passa a dividir `shared` com os outros orquestradores: é assim que um
    /// segundo pedido corre em paralelo sem perder o que o primeiro aprendeu.
    pub fn share(&mut self,shared:Arc<Shared>) { self.shared=shared; }

    /// Quantos pedidos o histórico de desempenho guarda.
    pub fn performance_len(&self)->usize { held(&self.shared.performance).len() }

    /// Força a próxima montagem dos agentes (`use_llm`).
    pub fn rebuild_llm(&mut self) { self.llm_built=None; }

    pub fn use_core(&mut self,settings:&crate::core_settings::CoreSettings) {
        let privacy=self.config.privacy.clone();
        let ttl=self.config.jev.context.cache_ttl;
        settings.apply(&mut self.config);
        if self.config.privacy!=privacy { self.firewall=ContextFirewall::new(self.config.privacy.clone()); self.rag.invalidate(); }
        if self.config.jev.context.cache_ttl!=ttl { self.cache=SemanticCache::new(self.config.jev.context.cache_ttl,1000); }
    }

    /// Os agentes das configurações, montados de novo só quando mudam (ou a
    /// cada `LLM_REFRESH`, para achar o agente instalado com o app aberto).
    /// O Claude recebe os arquivos protegidos como regras das ferramentas
    /// dele (`llm::guarding`): por isso a privacidade entra antes, no
    /// `use_core`.
    pub fn use_llm(&mut self,settings:&crate::llm::LlmSettings) {
        let print={
            use std::hash::{Hash, Hasher};
            let mut hasher=std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(settings).unwrap_or_default().hash(&mut hasher);
            self.config.privacy.deny.hash(&mut hasher);
            hasher.finish()
        };
        if self.llm_built.is_some_and(|(built,at)|built==print&&at.elapsed()<LLM_REFRESH) { return; }
        self.llm_built=Some((print,Instant::now()));
        let (mut providers,models)=crate::llm::to_config(settings);
        if let Some(claude)=providers.get_mut(crate::llm::AgentId::Claude.key()) {
            claude.args=crate::llm::guarding(&claude.args,&self.config.privacy.deny);
            claude.plan_args=crate::llm::guarding(&claude.plan_args,&self.config.privacy.deny);
        }
        // O agente sem programa, ou sabidamente sem login, sai da disputa
        // (enquanto houver outro): abri-lo só daria o erro, e o do Codex
        // leva segundos para chegar.
        set_aside_missing(&mut providers,|command|crate::llm::locate(command).is_some()&&!crate::llm::logged_out(command));
        self.providers=build_providers(&providers,&self.workdir);
        self.planners=build_planners(&providers,&self.workdir);
        self.config.providers=providers;
        self.config.models=models;
    }

    pub fn reload(&mut self)->Result<()> { let mut config=Config::load(&self.config_path)?; config.providers=std::mem::take(&mut self.config.providers); config.models=std::mem::take(&mut self.config.models); self.providers=build_providers(&config.providers,&self.workdir); self.planners=build_planners(&config.providers,&self.workdir); self.firewall=ContextFirewall::new(config.privacy.clone()); self.cache=SemanticCache::new(config.jev.context.cache_ttl,1000); self.config=config; self.rag.index(&self.firewall)?; Ok(()) }

    /// Cada chat pertence a um projeto, e é a pasta desse projeto que precisa
    /// entrar no contexto: sem isto o Jev descreveria o diretório de onde o
    /// aplicativo subiu e mandaria os arquivos errados para o modelo. O cache
    /// semântico é chaveado pela raiz, então trocar de projeto não reaproveita
    /// resposta de outro repositório.
    pub fn focus_on(&mut self,root:&Path)->Result<()> {
        anyhow::ensure!(root.is_dir(),crate::i18n::Text::new("project.folderMissing").with("path",root.display().to_string()));
        self.workdir.focus(root.to_path_buf());
        self.rag.focus_on(root.to_path_buf(),&self.firewall)
    }

    /// O mesmo `focus_on`, com a leitura da pasta numa thread de bloqueio: a
    /// varredura do disco não segura a thread que atende a tela e a rede.
    pub async fn focus(&mut self,root:&Path)->Result<()> {
        anyhow::ensure!(root.is_dir(),crate::i18n::Text::new("project.folderMissing").with("path",root.display().to_string()));
        self.workdir.focus(root.to_path_buf());
        if self.rag.root()==root && self.rag.is_fresh() { return Ok(()); }
        let firewall=ContextFirewall::new(self.config.privacy.clone());
        let root=root.to_path_buf();
        self.off_thread(move |rag|rag.focus_on(root,&firewall)).await
    }

    /// Lê de novo a pasta do índice velho (um build mexeu nela, a privacidade
    /// mudou), fora da thread do pedido.
    async fn refresh_index(&mut self) {
        if self.rag.is_fresh() { return; }
        let firewall=ContextFirewall::new(self.config.privacy.clone());
        if let Err(error)=self.off_thread(move |rag|rag.refresh(&firewall)).await { eprintln!("índice: não consegui ler a pasta de novo ({error:#})"); }
    }

    /// Roda `work` no índice numa thread de bloqueio. Se a thread cair, fica
    /// um índice vazio da mesma pasta, por ler no próximo pedido.
    async fn off_thread(&mut self,work:impl FnOnce(&mut RepositoryRag)->Result<()>+Send+'static)->Result<()> {
        let placeholder=self.rag.placeholder();
        let mut rag=std::mem::replace(&mut self.rag,placeholder);
        match tokio::task::spawn_blocking(move ||{ let done=work(&mut rag); (rag,done) }).await {
            Ok((rag,done))=>{ self.rag=rag; done }
            Err(error)=>Err(anyhow!("index thread failed: {error}")),
        }
    }

    /// Os agentes do modo: em build, os que escrevem no projeto — menos quando
    /// a regra de escrita do Jev, ou a da organização por cima dela, é `deny`.
    /// Aí o agente roda como no planejamento, e a regra vale também para o
    /// que ele faria sozinho, não só para o que a resposta conta.
    fn pool(&self,mode:&str)->&HashMap<String,Box<dyn Provider>> {
        if self.writes(mode) {&self.providers} else {&self.planners}
    }
    fn writes(&self,mode:&str)->bool { mode==MODE_BUILD&&self.config.permissions.write!="deny" }

    pub fn executable_provider_count(&self)->usize { self.providers.len() }
    pub fn executable_model_count(&self)->usize { self.config.models.values().filter(|model|model.enabled && self.providers.contains_key(&model.provider)).count() }

    /// O `pulse` é por onde o pedido conta o que está fazendo enquanto faz.
    /// `Pulse::silent()` deixa tudo como era: o núcleo não sabe nem precisa
    /// saber se alguém está escutando.
    pub async fn process(&mut self,user_input:&str,session_id:Option<&str>,pulse:&Pulse)->ProcessResult {
        let session_id=session_id.unwrap_or("default");
        let normalized=user_input.trim().to_string();
        let pinned=self.pending_work_mode.take().unwrap_or_else(||MODE_AUTO.into());
        self.mode_switch=None;
        if normalized.starts_with("/why") {
            self.pending_routing=None;
            self.pending_context.clear();
            let result=self.explanation_result(user_input,&normalized);
            if let Some(response)=&result.result { pulse.beat(Beat::Chunk{text:response.response.clone()}); }
            pulse.beat(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0});
            return result;
        }
        self.refresh_index().await;
        self.memory.add_message(session_id,"user",normalized.clone());
        let brief=self.pending_brief.take();
        let mut extras=std::mem::take(&mut self.pending_context);
        let gate_passed=self.pending_gate_passed.take().unwrap_or(true);
        // "Não funcionou": a nova tentativa não sai igual à anterior. O agente
        // confere o que a mudança anterior fez antes de mudar de novo, e da
        // segunda reclamação seguida em diante pensa um degrau a mais.
        let complaints=if std::mem::take(&mut self.pending_retry) { let mut all=held(&self.shared.complaints); let count=all.entry(session_id.to_string()).or_insert(0); *count+=1; *count } else { held(&self.shared.complaints).remove(session_id); 0 };
        if complaints>0 { extras.push(RETRY_NOTE.into()); }
        let (intent,complexity,signals)=self.decide_routing(&normalized,session_id).await;
        let wants_build=asks_to_build(&intent.intent,&signals,gate_passed,self.expertise);
        let stuck=held(&self.shared.stuck_in_plan).contains(session_id);
        // Há um plano deste chat esperando: quem o aprova quer o build, mesmo
        // que o pedido não pareça código ou passe do teto do nível.
        let approved=held(&self.shared.plans).contains_key(session_id)&&(wants_build||(gate_passed&&!destructive(&signals)&&approves_plan(&normalized)));
        let (mode,switched)=resolve_mode(&pinned,select_mode(&intent.intent,&complexity,&signals,gate_passed,self.expertise),wants_build,stuck,approved);
        if pinned==MODE_AUTO&&mode==MODE_PLAN&&wants_build { held(&self.shared.stuck_in_plan).insert(session_id.to_string()); } else { held(&self.shared.stuck_in_plan).remove(session_id); }
        self.mode_switch=switched.clone();
        pulse.beat(Beat::Read{intent:intent.intent.clone(),complexity:complexity.clone(),source:signals.source.clone()});
        let plan=plan_context(&intent.intent,&complexity); let strategy=select_strategy(&intent.intent,&complexity);
        let budget=*self.config.budgets.get(&complexity).unwrap_or(&DEFAULT_BUDGET);
        let notes=format!("{}{}",mode_notes(&signals,mode),self.lean_note(mode).map(|note|format!("\n{note}")).unwrap_or_default());
        let reserved=self.request_overhead(&normalized,session_id)+if notes.is_empty(){0}else{estimate_tokens(&notes)}+self.pending_gate_note.as_deref().map_or(0,estimate_tokens)+brief.as_deref().map_or(0,|brief|estimate_tokens(brief).saturating_sub(estimate_tokens(&normalized)))+extras.iter().map(|extra|estimate_tokens(extra)).sum::<usize>()+self.project_notes.as_deref().map_or(0,estimate_tokens);
        let context=self.assemble_context(&normalized,&plan,budget,reserved,&signals,mode);
        pulse.beat(Beat::Context{files:context.relevant_files.len(),tokens:context.estimated_tokens});
        // O agente que o chat já usa desempata: a sessão dele é retomada.
        let sticky=self.memory.agent_session(session_id).map(|kept|(kept.provider.clone(),kept.model.clone()));
        let sticky=sticky.as_ref().map(|(provider,model)|(provider.as_str(),model.as_str()));
        let ranked=rank_models(&self.config,&intent.intent,&complexity,&context,&held(&self.shared.performance),&Tiebreak{sticky,seed:session_id});
        // O modelo da sessão vem antes do porte: trocar abre sessão nova.
        let ranked=if self.config.jev.keep_session_model { keep_session_model(ranked,&self.config,sticky,&intent.intent,&complexity) } else { ranked };
        let mut selection=ranked.first().cloned().unwrap_or_else(||configuration_selection(&self.config,&complexity,&context));
        selection.mode=mode.into();
        selection.agent=self.agents.for_intent(&intent.intent).map(|agent|agent.name.clone());
        if let Some(guidance)=self.configuration_guidance(&selection) {
            let response=ProviderResponse { response:guidance, input_tokens:0, output_tokens:0, model:"configuration".into(), provider:"jev".into(), latency_ms:0, session:None };
            pulse.beat(Beat::Chunk{text:response.response.clone()});
            pulse.beat(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0});
            self.memory.add_message(session_id,"assistant",i18n::for_model(&response.response));
            let model_selection=ModelSelection { model_name:"configuration".into(), provider:"jev".into(), estimated_tokens:selection.estimated_tokens, score:0.0, reason:"LLM configuration is required before execution".into(), ..Default::default() };
            let decision=Decision { model_provider:"jev".into(), model_name:"configuration".into(), estimated_tokens:model_selection.estimated_tokens, context_files_count:context.relevant_files.len(), rag_files_count:context.snippets.len() };
            self.last_decision=Some(decision.clone());
            return ProcessResult { user_input:user_input.into(), normalized_input:normalized, intent_analysis:intent, complexity, context_plan:plan, context, strategy:"configuration_required".into(), model_selection, result:Some(response), validation:true, decision, routing:signals, error:None };
        }
        pulse.beat(Beat::Route{provider:selection.provider.clone(),model:selection.model_name.clone(),reason:selection.reason.clone(),mode:selection.mode.clone(),agent:selection.agent.clone(),switched:switched.clone()});
        pulse.beat(Beat::Running);
        // A revisão compara o projeto de antes com o de depois: a largada é
        // tirada antes de o agente começar.
        // Só o pedido médio ou complexo ganha a segunda opinião: no simples, ela
        // custava mais que a mudança.
        self.pending_review=None;
        let watch=if self.config.jev.review_changes&&mode==MODE_BUILD&&wants_review(&complexity) { self.start_watch().await } else { None };
        let started=Instant::now();
        // Pedido complexo no build, com a divisão ligada: partes do pedido vão
        // a agentes diferentes ao mesmo tempo. Sem divisão possível, segue
        // inteiro com o agente escolhido.
        // A janela do plano B conta a partir de cada tentativa: o plano e a
        // divisão que vêm antes do construtor não gastam a vez dele.
        let mut attempt=Instant::now();
        let split=if self.config.jev.parallel_tasks&&mode==MODE_BUILD&&crate::split::wants_plan(&complexity)&&self.config.permissions.write!="deny" {
            self.run_parallel(brief.as_deref().unwrap_or(&normalized),&context,&ranked,session_id,pulse).await
        } else { None };
        let effort=if complaints>=2 { raise_effort(effort_for(&complexity)) } else { effort_for(&complexity) };
        // O build que segue um plano do mesmo chat recebe o plano inteiro,
        // fora do corte do histórico, se abrir sessão nova.
        let handoff:Vec<String>=if mode==MODE_BUILD { held(&self.shared.plans).remove(session_id).map(|plan|format!("{PLAN_HANDOFF}\n{plan}")).into_iter().collect() } else { vec![] };
        let mut execution=match split {
            Some((response,first))=>{ selection=ModelSelection{mode:selection.mode.clone(),agent:selection.agent.clone(),..first}; Ok((response,false,0)) }
            None=>{
                // Pedido complexo no build: um modelo de raciocínio planeja
                // antes, em somente leitura, e o plano vai junto ao agente que
                // constrói.
                if self.config.jev.plan_first&&mode==MODE_BUILD&&crate::split::wants_plan(&complexity) {
                    if let Some(plan)=self.plan_first(brief.as_deref().unwrap_or(&normalized),&context,&selection,session_id,pulse).await { extras.push(plan); }
                }
                attempt=Instant::now();
                self.execute(brief.as_deref().unwrap_or(&normalized),&extras,&handoff,effort,&context,&selection,session_id,pulse).await
            }
        };
        // Plano B: o agente que nem conseguiu começar (não instalado, sem
        // login, sem cota) passa a vez ao próximo da lista que é de outro
        // agente. Quem já trabalhou um pouco e falhou não passa: o próximo
        // encontraria o projeto pela metade.
        let mut tried=vec![selection.provider.clone()];
        while let Err(error)=&execution {
            // Parado não é "não conseguiu começar": ninguém mais é chamado.
            if pulse.stop().reason().is_some() || attempt.elapsed()>FALLBACK_WINDOW || !could_not_start(error) { break; }
            let Some(next)=ranked.iter().find(|candidate|!tried.contains(&candidate.provider)&&self.pool(mode).contains_key(&candidate.provider)) else { break };
            pulse.beat(Beat::Fallback{provider:selection.provider.clone(),error:crate::i18n::notice(&[crate::i18n::failure(anyhow!("{error:#}"))])});
            selection=ModelSelection{mode:selection.mode.clone(),agent:selection.agent.clone(),..next.clone()};
            tried.push(selection.provider.clone());
            pulse.beat(Beat::Route{provider:selection.provider.clone(),model:selection.model_name.clone(),reason:selection.reason.clone(),mode:selection.mode.clone(),agent:selection.agent.clone(),switched:switched.clone()});
            attempt=Instant::now();
            execution=self.execute(brief.as_deref().unwrap_or(&normalized),&extras,&handoff,effort,&context,&selection,session_id,pulse).await;
        }
        if let (Some(watch),Ok((response,_,_)))=(watch,&execution) {
            if usable_response(response) {
                self.pending_review=self.review(&normalized,&complexity,&context,&selection,session_id,watch,pulse).await;
            }
        }
        // No build o agente mexe nos arquivos: o índice lido antes dele já não
        // é o projeto. A próxima leitura varre a pasta de novo, e arquivo novo
        // entra na busca, no mapa e nos símbolos.
        if mode==MODE_BUILD { self.rag.invalidate(); }
        let decision=Decision { model_provider:selection.provider.clone(), model_name:selection.model_name.clone(), estimated_tokens:selection.estimated_tokens, context_files_count:context.relevant_files.len(), rag_files_count:context.snippets.len() };
        let resumed=execution.as_ref().is_ok_and(|(_,resumed,_)|*resumed);
        let instructions=execution.as_ref().map_or(0,|(_,_,instructions)|*instructions);
        let execution=execution.map(|(response,_,_)|response);
        self.remember_agent_session(session_id,&selection,execution.as_ref().ok(),resumed,instructions);
        match &execution {
            Ok(response)=>pulse.beat(Beat::Done{input_tokens:response.input_tokens,output_tokens:response.output_tokens,latency_ms:response.latency_ms}),
            Err(error)=>pulse.beat(Beat::Failed{error:crate::i18n::notice(&[crate::i18n::failure(anyhow::anyhow!("{error:#}"))])}),
        }
        let (result,error,valid)=match execution { Ok(response)=>{let usable=usable_response(&response);if !response.response.trim().is_empty(){self.memory.add_message(session_id,"assistant",response.response.clone());}(Some(response),None,usable)}, Err(error)=>(None,Some(crate::i18n::notice(&[crate::i18n::failure(error)])),false) };
        // O plano que o planejamento devolveu fica guardado, inteiro, para o
        // build que vier depois.
        if mode==MODE_PLAN { match result.as_ref().filter(|_|valid) { Some(response)=>{ held(&self.shared.plans).insert(session_id.to_string(),response.response.clone()); } None=>{ held(&self.shared.plans).remove(session_id); } } }
        self.shared.record(PerformanceRecord { task_type:intent.intent.clone(), strategy_used:strategy.clone(), model_used:selection.model_name.clone(), success:valid, response_time_ms:started.elapsed().as_millis(), input_tokens:result.as_ref().map_or(0,|r|r.input_tokens), output_tokens:result.as_ref().map_or(0,|r|r.output_tokens), estimated_cost:0.0, timestamp:Utc::now(), chat:Some(session_id.to_string()) });
        self.last_decision=Some(decision.clone());
        ProcessResult { user_input:user_input.into(), normalized_input:normalized, intent_analysis:intent, complexity, context_plan:plan, context, strategy, model_selection:selection, result, validation:valid, decision, routing:signals, error }
    }

    async fn decide_routing(&mut self,input:&str,session_id:&str)->(IntentAnalysis,String,RoutingSignals) {
        let ahead=self.pending_routing.take();
        match &self.routing_mode {
            RoutingMode::Local=>local_routing(input,None),
            RoutingMode::Fixed(decision)=>self.jev_routing(decision),
            RoutingMode::Failed(error)=>local_routing(input,Some(error.clone())),
            RoutingMode::Auto=>{
                if let Some(ahead)=ahead { return match ahead { Ok(decision)=>self.jev_routing(&decision), Err(error)=>local_routing(input,Some(error)) }; }
                if !jev::reachable() { return local_routing(input,None); }
                match jev::route(&self.routing_input(input,session_id)).await {
                    Ok(decision)=>self.jev_routing(&decision),
                    Err(error)=>local_routing(input,Some(error.to_string())),
                }
            }
        }
    }

    pub fn routing_input(&self,input:&str,session_id:&str)->jev::RoutingInput { self.routing_input_after(input,session_id,1) }

    /// O mesmo estado, montado antes de o pedido entrar na memória do chat —
    /// quando o roteamento é pedido junto com a portaria.
    pub fn routing_input_ahead(&self,input:&str,session_id:&str)->jev::RoutingInput { self.routing_input_after(input,session_id,0) }

    /// Quer o Jev a leitura de roteamento deste pedido? Só no modo automático
    /// e com credencial; nos outros modos ela não é pedida.
    pub fn routes_with_jev(&self)->bool { matches!(self.routing_mode,RoutingMode::Auto)&&jev::reachable() }

    fn routing_input_after(&self,input:&str,session_id:&str,current:usize)->jev::RoutingInput {
        let project=self.rag.project_info();
        jev::RoutingInput::new(input).with_project(project.name,project.languages).with_candidate_files(self.rag.search(input,ROUTING_CANDIDATES).into_iter().map(|snippet|snippet.path).collect()).with_recent_turns(self.recent_turns(session_id,current))
    }

    /// As últimas falas do chat, curtas, antes de o pedido entrar na memória.
    /// A portaria de entrada as recebe junto com o pedido, as mesmas do
    /// roteamento: "pode implementar" é julgado contra o plano que veio antes.
    pub fn recent_turns_ahead(&self,session_id:&str)->Vec<String> { self.recent_turns(session_id,0) }

    fn recent_turns(&self,session_id:&str,current:usize)->Vec<String> {
        self.memory.conversation(session_id).iter().rev().skip(current).take(ROUTING_TURNS).rev().map(|message|format!("{}: {}",message.role,message.content.chars().take(ROUTING_TURN_CHARS).collect::<String>())).collect()
    }

    pub fn jev_routing(&self,decision:&jev::RoutingDecision)->(IntentAnalysis,String,RoutingSignals) {
        let threshold=self.expertise.confidence(self.config.jev.adaptive_routing.confidence_threshold);
        let complexity=if decision.complexity_confidence>=threshold{decision.complexity.clone()}else{widen_complexity(decision)};
        let widened=(complexity!=decision.complexity).then(||decision.complexity.clone());
        let confident=decision.is_confident(threshold);
        let scores=decision.intent_probabilities.iter().map(|(name,probability)|(name.clone(),(probability*100.0).round().clamp(0.0,100.0) as usize)).collect();
        let notice=(!confident).then(||{
            let mut actions=vec![format!("kept the intent `{}`",decision.intent)];
            if let Some(before)=&widened { actions.push(format!("widened the budget from `{before}` to `{complexity}`")); }
            actions.push("asked the model for a clarifying question before assuming".into());
            format!("The Jev routed with low confidence (intent {:.0}%, complexity {:.0}%): {}.",decision.intent_confidence*100.0,decision.complexity_confidence*100.0,actions.join(", "))
        });
        let signals=RoutingSignals{source:SOURCE_JEV.into(),intent_confidence:decision.intent_confidence,complexity_confidence:decision.complexity_confidence,confident,needs_repository_context:Some(decision.needs_repository_context),needs_tools:Some(decision.needs_tools),is_destructive:Some(decision.is_destructive),complexity_before_widening:widened,jev_model:Some(decision.model.clone()),jev_input_tokens:decision.usage.input_tokens,jev_output_tokens:decision.usage.output_tokens,notice};
        (IntentAnalysis{intent:decision.intent.clone(),scores,confidence:decision.intent_confidence},complexity,signals)
    }

    /// A regra de código enxuto deste pedido: só no modo build, só com a
    /// configuração ligada, na força do nível da conta.
    fn lean_note(&self,mode:&str)->Option<&'static str> {
        (self.lean_code&&mode==MODE_BUILD).then(||match self.expertise.lean() { crate::expertise::Lean::Lite=>LEAN_LITE_NOTE, crate::expertise::Lean::Full=>LEAN_FULL_NOTE })
    }

    fn request_overhead(&self,input:&str,session_id:&str)->usize { estimate_tokens(SYSTEM_INSTRUCTIONS)+estimate_tokens(input)+self.history_tokens(session_id)+REQUEST_MARGIN }
    fn history_tokens(&self,session_id:&str)->usize { short_history(self.memory.conversation(session_id)).iter().map(|message|estimate_tokens(&message.content)).sum() }

    fn assemble_context(&mut self,input:&str,plan:&[String],budget:usize,reserved:usize,signals:&RoutingSignals,mode:&str)->Context {
        let mut context=if repository_context_wanted(signals){self.build_context(input,plan,budget,reserved)}else{Context{system_instructions:SYSTEM_INSTRUCTIONS.into(),project:self.rag.project_info(),relevant_files:vec![],snippets:vec![],estimated_tokens:reserved,repository_context_skipped:true}};
        note_project(&mut context);
        note_routing(&mut context,signals,mode);
        if let Some(note)=self.lean_note(mode) { context.system_instructions.push_str(&format!("\n{note}")); }
        if let Some(note)=self.pending_gate_note.take() { context.system_instructions.push_str(&format!("\n{note}")); }
        context
    }

    fn build_context(&mut self,input:&str,plan:&[String],budget:usize,reserved:usize)->Context {
        let mut context=self.retrieve_context(input,plan);
        // O corte do orçamento não conta como economia: o que ele tira é o que
        // a própria busca pôs, e o agente lê do disco o que faltar.
        let pruned=prune_to_budget(&mut context,budget.saturating_sub(reserved));
        context.estimated_tokens=reserved+context.snippets.iter().map(snippet_tokens).sum::<usize>();
        if pruned||context.snippets.iter().any(|snippet|snippet.content.contains("_REDACTED]")) { note_removal(&mut context); }
        context
    }

    fn retrieve_context(&mut self,input:&str,plan:&[String])->Context {
        let hashes=self.rag.file_hashes(); let key=SemanticCache::request_key(input,&self.rag.project_info().root);
        if let Some(context)=self.cache.get_valid(&key,&hashes){crate::usage::mark(crate::usage::JevMark::count("cache_hit",1));return context;}
        crate::usage::mark(crate::usage::JevMark::count("cache_miss",1));
        let snippets=self.rag.search(input,if plan.contains(&"full_repository".to_string()){8}else{4});
        let files=snippets.iter().map(|s|s.path.clone()).collect::<Vec<_>>();
        let retrieved=snippets.len();
        let context=Context { system_instructions:SYSTEM_INSTRUCTIONS.into(), project:self.rag.project_info(), relevant_files:files, estimated_tokens:snippets.iter().map(snippet_tokens).sum(), snippets, repository_context_skipped:false };
        let mut context=self.firewall.filter_context(&context,false);
        mark_firewall(retrieved,&context);
        if context.snippets.len()<retrieved { note_removal(&mut context); }
        self.cache.insert_tracked(key,context.clone(),&hashes); context
    }

    /// Batiza o chat. O modelo recebe o pedido junto com a leitura de intenção
    /// do Jev e devolve só o título; nenhum arquivo do repositório entra nessa
    /// chamada, ela vê apenas o que o desenvolvedor já digitou. Um modelo por
    /// API batiza primeiro; sem nenhum, batiza o agente de linha de comando no
    /// modelo mais barato dele, em somente leitura — uma chamada curta por
    /// chat, uma vez só. Sem modelo nenhum, ou com resposta que não serve como
    /// título, devolve nada e o resumo local que já está no banco continua
    /// valendo.
    ///
    /// Devolve o pedido pronto, com um provedor só dele: a chamada roda fora
    /// do cadeado do orquestrador, e o próximo da fila não espera o batismo.
    pub fn title_request(&self,prompt:&str,intent:&str)->Option<TitleRequest> {
        // O batismo não mexe em nada: vai sempre pelos agentes em somente leitura.
        // O Copilot cobra uma premium request por chamada: com só ele, vale o
        // resumo local que já está no banco.
        let mut usable:Vec<_>=self.config.models.iter().filter(|(_,model)|model.enabled&&model.provider!=PER_REQUEST_AGENT&&self.planners.contains_key(&model.provider)).collect();
        usable.sort_by(|(left_key,left),(right_key,right)|{
            let by_api=|model:&crate::config::ModelConfig|self.planners.get(&model.provider).is_some_and(|provider|provider.explores());
            (by_api(left),cost_rank(&left.cost_class),left_key.as_str()).cmp(&(by_api(right),cost_rank(&right.cost_class),right_key.as_str()))
        });
        let (_,model)=usable.first()?;
        let config=self.config.providers.get(&model.provider)?;
        let provider=build_planners(&HashMap::from([(model.provider.clone(),config.clone())]),&self.workdir).into_values().next()?;
        let request:String=prompt.trim().chars().take(TITLE_PROMPT_CHARS).collect();
        let messages=vec![
            ChatMessage{role:"system".into(),content:format!("{TITLE_INSTRUCTIONS}\n{}",language_note())},
            ChatMessage{role:"user".into(),content:format!("Intent read by Jev: {intent}\n\nRequest:\n{request}")},
        ];
        Some(TitleRequest{provider,model:model.model.clone(),messages})
    }

    /// Manda o pedido ao modelo escolhido. Um agente que explora o repositório
    /// recebe o mapa dos arquivos, não os arquivos: ele os abre por conta
    /// própria, e mandar os dois era pagar a leitura duas vezes. Quando o
    /// agente sabe retomar a sessão do chat, ela é retomada e o histórico não
    /// vai de novo; se a retomada falhar por causa da sessão, o pedido sai do
    /// zero, com o histórico. Devolve também se a sessão foi retomada e a
    /// impressão das instruções de sistema com que a sessão está.
    ///
    /// Na volta retomada as instruções só vão de novo quando mudaram — outra
    /// nota de modo, outro agente — ou quando a sessão trocou de modo; senão
    /// vai só o pedido: a sessão já as tem.
    ///
    /// `handoff` vai só na sessão nova: é o que a sessão retomada já tem
    /// (o plano escrito antes, no mesmo chat).
    #[allow(clippy::too_many_arguments)]
    async fn execute(&self,input:&str,extras:&[String],handoff:&[String],effort:&str,context:&Context,selection:&ModelSelection,session_id:&str,pulse:&Pulse)->Result<(ProviderResponse,bool,u64)> {
        let pool=self.pool(&selection.mode);
        let provider=pool.get(&selection.provider).ok_or_else(||anyhow!("no executable provider named '{}' is configured",selection.provider))?;
        let safe_context=if provider.is_local(){context.clone()}else{self.without_local_only(context)};
        let agent=selection.agent.as_deref().and_then(|name|self.agents.find(name));
        let system=agent.map(|a|format!("{}\n{}",safe_context.system_instructions,a.system_prompt)).unwrap_or_else(||safe_context.system_instructions.clone());
        let system=format!("{system}\n{}",language_note());
        let user=std::iter::once(task_message(input,&safe_context,provider.explores(),self.rag.symbols())).chain(extras.iter().cloned()).collect::<Vec<_>>().join("\n\n");
        let effort=Some(effort);
        let instructions=fingerprint(&system);
        let fresh=match self.resume_check(session_id,provider.as_ref(),selection) {
            Ok(Resume{id,crossed,instructions:kept})=>{
                let mut messages=Vec::new();
                if crossed||kept!=instructions { messages.push(ChatMessage{role:"system".into(),content:system.clone()}); }
                if crossed { messages.push(ChatMessage{role:"system".into(),content:(if self.writes(&selection.mode) {MODE_TO_BUILD_NOTE} else {MODE_TO_PLAN_NOTE}).into()}); }
                messages.push(ChatMessage{role:"user".into(),content:user.clone()});
                match provider.chat_turn(&messages,&selection.model_name,effort,Some(&id),pulse).await {
                    Ok(response)=>return Ok((response,true,instructions)),
                    Err(error) if !lost_session(&error)=>return Err(error),
                    Err(error)=>{ eprintln!("sessão do agente: não retomou, começando outra ({error:#})"); NewSession::Lost }
                }
            }
            Err(reason)=>reason,
        };
        crate::usage::mark(crate::usage::JevMark::count(fresh.mark(),1));
        // Sessão nova: as notas do projeto entram uma vez, no começo dela, e o
        // agente que explora recebe também a planta do projeto.
        let system=match &self.project_notes { Some(notes)=>format!("{system}\n{notes}"), None=>system };
        let system=match provider.explores().then(||crate::project_map::render(&self.rag)).flatten() { Some(map)=>format!("{system}\n\n{map}"), None=>system };
        let mut messages=vec![ChatMessage{role:"system".into(),content:system}];
        messages.extend(short_history(self.memory.conversation(session_id)));
        let user=std::iter::once(user).chain(handoff.iter().cloned()).collect::<Vec<_>>().join("\n\n");
        messages.push(ChatMessage{role:"user".into(),content:user});
        Ok((provider.chat_turn(&messages,&selection.model_name,effort,None,pulse).await?,false,instructions))
    }

    /// O plano escrito por um modelo de raciocínio, em somente leitura, para
    /// o agente que constrói seguir. Sem planejador ou com ele falhando, nada:
    /// o agente constrói como antes.
    async fn plan_first(&self,request:&str,context:&Context,builder:&ModelSelection,session_id:&str,pulse:&Pulse)->Option<String> {
        let ranked=rank_models(&self.config,"analysis","complex",context,&held(&self.shared.performance),&Tiebreak{sticky:None,seed:session_id});
        let planner=ranked.iter().find(|candidate|self.planners.contains_key(&candidate.provider))?;
        let provider=self.planners.get(&planner.provider)?;
        let system=format!("{}\n{}",context.system_instructions,language_note());
        let messages=[ChatMessage{role:"system".into(),content:system},ChatMessage{role:"user".into(),content:crate::split::prompt(request,&agent_name(&builder.provider))}];
        pulse.beat(Beat::Plan{provider:planner.provider.clone(),model:planner.model_name.clone()});
        // O plano aparece na faixa linha a linha enquanto é escrito: um plano
        // de pedido complexo leva minutos, e a faixa ficava parada nele.
        let told=pulse.as_lines();
        let planned=provider.chat_once(&messages,&planner.model_name,Some("high"),&told).await;
        told.finish_lines();
        match planned {
            Ok(response)=>crate::split::handoff(&response.response,&agent_name(&planner.provider)),
            Err(error)=>{ eprintln!("plano: {} não planejou ({error:#})",planner.provider); None }
        }
    }

    /// O pedido dividido entre agentes. O planejador quebra o pedido em partes
    /// com arquivos separados; cada parte roda numa cópia do projeto (`git
    /// worktree`), todas ao mesmo tempo, e as mudanças voltam como patch. A
    /// resposta junta o que cada agente disse. Devolve também quem fez a
    /// primeira parte, que fica como o agente do pedido. Nada quando o
    /// projeto não é um repositório git, quando não há planejador ou quando a
    /// divisão não vale: aí o pedido segue inteiro.
    async fn run_parallel(&self,request:&str,context:&Context,ranked:&[ModelSelection],session_id:&str,pulse:&Pulse)->Option<(ProviderResponse,ModelSelection)> {
        let folder=PathBuf::from(self.rag.project_info().root);
        let (top,prefix)=tokio::task::spawn_blocking({let folder=folder.clone(); move ||crate::parallel::repository(&folder)}).await.ok().flatten()?;
        let planners=rank_models(&self.config,"analysis","complex",context,&held(&self.shared.performance),&Tiebreak{sticky:None,seed:session_id});
        let planner=planners.iter().find(|candidate|self.planners.contains_key(&candidate.provider))?;
        let system=format!("{}\n{}",context.system_instructions,language_note());
        let asked=[ChatMessage{role:"system".into(),content:system.clone()},ChatMessage{role:"user".into(),content:crate::parallel::split_prompt(request)}];
        let answer=match self.planners.get(&planner.provider)?.chat_once(&asked,&planner.model_name,Some("high"),&pulse.quiet()).await {
            Ok(answer)=>answer,
            Err(error)=>{ eprintln!("divisão: {} não dividiu ({error:#})",planner.provider); return None; }
        };
        let tasks=crate::parallel::parse_split(&answer.response);
        let workers=crate::parallel::assign(&ranked.iter().filter(|candidate|self.providers.contains_key(&candidate.provider)).cloned().collect::<Vec<_>>(),tasks.len());
        if tasks.is_empty()||workers.len()!=tasks.len() { return None; }
        let start=tokio::task::spawn_blocking({let top=top.clone(); move ||crate::parallel::base(&top)}).await.ok()?.map_err(|error|eprintln!("divisão: sem ponto de partida ({error:#})")).ok()?;
        // Cada parte numa cópia própria; os agentes de cada uma enxergam a
        // pasta do projeto dentro da cópia.
        let mut copies:Vec<PathBuf>=Vec::new();
        for index in 0..tasks.len() {
            let dir=crate::parallel::worktree_dir(index);
            let created=tokio::task::spawn_blocking({let (top,start,dir)=(top.clone(),start.clone(),dir.clone()); move ||crate::parallel::add_worktree(&top,&start,&dir)}).await;
            if !matches!(created,Ok(Ok(()))) {
                eprintln!("divisão: a cópia {index} não foi criada");
                for made in &copies { crate::parallel::remove_worktree(&top,made); }
                return None;
            }
            copies.push(dir);
        }
        pulse.beat(Beat::Split{tasks:tasks.iter().zip(&workers).map(|(task,worker)|crate::progress::SplitTask{title:task.title.clone(),provider:worker.provider.clone(),model:worker.model_name.clone()}).collect()});
        let pools:Vec<HashMap<String,Box<dyn Provider>>>=copies.iter().map(|dir|{let workdir=Workdir::default(); workdir.focus(dir.join(&prefix)); build_providers(&self.config.providers,&workdir)}).collect();
        let runs=tasks.iter().enumerate().map(|(index,task)|{
            let others:Vec<&crate::parallel::Subtask>=tasks.iter().enumerate().filter(|(other,_)|*other!=index).map(|(_,other)|other).collect();
            let messages=vec![ChatMessage{role:"system".into(),content:system.clone()},ChatMessage{role:"user".into(),content:crate::parallel::task_message(request,task,&others)}];
            let (worker,pool)=(&workers[index],&pools[index]);
            let quiet=pulse.quiet();
            async move {
                match pool.get(&worker.provider) {
                    Some(provider)=>provider.chat_once(&messages,&worker.model_name,Some("medium"),&quiet).await,
                    None=>Err(anyhow!("no executable provider named '{}' is configured",worker.provider)),
                }
            }
        });
        let results=futures_util::future::join_all(runs).await;
        // Parado no meio: as cópias saem e nada volta ao projeto. O pedido
        // segue para o `execute`, que devolve a parada sem abrir agente.
        if pulse.stop().reason().is_some() {
            for dir in &copies { let (top,dir)=(top.clone(),dir.clone()); let _=tokio::task::spawn_blocking(move ||crate::parallel::remove_worktree(&top,&dir)).await; }
            return None;
        }
        let mut text=String::new();
        let (mut input_tokens,mut output_tokens)=(0,0);
        for (index,((task,worker),result)) in tasks.iter().zip(&workers).zip(results).enumerate() {
            let (outcome,patch_path,said)=match result {
                Err(error)=>("failed",None,crate::i18n::notice(&[crate::i18n::failure(error)])),
                Ok(response)=>{
                    input_tokens+=response.input_tokens; output_tokens+=response.output_tokens;
                    let patch=tokio::task::spawn_blocking({let (dir,start)=(copies[index].clone(),start.clone()); move ||crate::parallel::collect_patch(&dir,&start)}).await.ok().and_then(Result::ok).unwrap_or_default();
                    let applied=tokio::task::spawn_blocking({let (top,patch)=(top.clone(),patch.clone()); move ||crate::parallel::apply_patch(&top,&patch)}).await.ok().map(|applied|applied.is_ok()).unwrap_or(false);
                    if patch.trim().is_empty() { ("empty",None,response.response) }
                    else if applied { ("applied",None,response.response) }
                    else {
                        let kept=crate::parallel::keep_patch(&top,&format!("{session_id}-{}",index+1),&patch).ok().map(|path|path.display().to_string());
                        ("conflict",kept,response.response)
                    }
                }
            };
            pulse.beat(Beat::Subtask{index,title:task.title.clone(),provider:worker.provider.clone(),outcome:outcome.into(),patch:patch_path.clone()});
            let mark=match outcome { "applied"|"empty"=>"✓", _=>"⚠" };
            let said=crate::i18n::for_model(&said);
            let apply=patch_path.map(|path|format!("\n\n```\ngit apply \"{path}\"\n```")).unwrap_or_default();
            text.push_str(&format!("{}### {mark} {} · {}\n\n{}{apply}",if text.is_empty() {""} else {"\n\n"},task.title,agent_name(&worker.provider),said.trim()));
        }
        for dir in &copies { let (top,dir)=(top.clone(),dir.clone()); let _=tokio::task::spawn_blocking(move ||crate::parallel::remove_worktree(&top,&dir)).await; }
        pulse.beat(Beat::Chunk{text:text.clone()});
        let first=workers[0].clone();
        Some((ProviderResponse{response:text,input_tokens,output_tokens,model:first.model_name.clone(),provider:first.provider.clone(),latency_ms:0,session:None},first))
    }

    /// A foto do projeto antes do pedido, para a revisão saber o que ele
    /// mudou. Sem pasta legível, sem revisão.
    async fn start_watch(&self)->Option<crate::live_files::Session> {
        let root=PathBuf::from(self.rag.project_info().root);
        let firewall=ContextFirewall::new(self.config.privacy.clone());
        tokio::task::spawn_blocking(move ||root.is_dir().then(||crate::live_files::Session::start("review",&root,firewall))).await.ok().flatten()
    }

    /// A segunda opinião sobre o que o pedido mudou: um agente de outro
    /// provedor, em somente leitura, lê o diff e aponta problemas. Aqui sai só
    /// o pedido pronto — o diff tirado agora, antes de outro pedido mexer na
    /// pasta —; a conversa com o revisor corre depois da resposta. Sem
    /// mudança ou sem outro provedor, nada.
    #[allow(clippy::too_many_arguments)]
    async fn review(&self,request:&str,complexity:&str,context:&Context,executor:&ModelSelection,session_id:&str,mut watch:crate::live_files::Session,pulse:&Pulse)->Option<ReviewRequest> {
        let (watch,diff)=tokio::task::spawn_blocking(move ||{ watch.poll(); let diff=crate::review::diff(&watch); (watch,diff) }).await.ok()?;
        let diff=diff?;
        let ranked=rank_models(&self.config,"review",complexity,context,&held(&self.shared.performance),&Tiebreak{sticky:None,seed:session_id});
        let reviewer=crate::review::pick(&ranked,&executor.provider,|provider|self.planners.contains_key(provider))?;
        pulse.beat(Beat::Review{provider:reviewer.provider.clone(),model:reviewer.model_name.clone(),files:watch.changes().len()});
        let messages=vec![ChatMessage{role:"system".into(),content:language_note()},ChatMessage{role:"user".into(),content:crate::review::prompt(request,&diff,&agent_name(&executor.provider),&agent_name(&reviewer.provider))}];
        // Um provedor só dela, preso à pasta deste pedido: a revisão roda fora
        // do cadeado do orquestrador, e o pedido seguinte pode ser de outro
        // projeto.
        let config=self.config.providers.get(&reviewer.provider)?;
        let pinned=Workdir::default();
        pinned.focus(PathBuf::from(self.rag.project_info().root));
        let own=build_planners(&HashMap::from([(reviewer.provider.clone(),config.clone())]),&pinned).into_values().next();
        Some(ReviewRequest{provider:own?,name:reviewer.provider.clone(),model:reviewer.model_name.clone(),messages,effort:effort_for(complexity)})
    }

    /// O pedido anterior deste chat não resolveu: a nota do modelo que o
    /// atendeu cai, e o roteamento aprende com isso.
    pub fn mark_last_failed(&mut self,session_id:&str) {
        let mut performance=held(&self.shared.performance);
        if performance.mark_failed(session_id) { let _=performance.save(&self.shared.performance_path); }
    }

    /// O mesmo pedido mandado cru ao agente que o Jev escolheu, sem nada do
    /// Jev no caminho: nem contexto, nem nota, nem esforço, nem histórico
    /// enxuto. É como o desenvolvedor usaria o agente sozinho — com a sessão
    /// dele seguindo de um pedido para o outro —, e é contra isso que o
    /// `jayv bench` mede o JayV.
    pub async fn direct(&mut self,input:&str,session_id:&str,selection:&ModelSelection)->Result<ProviderResponse> {
        let pool=self.pool(&selection.mode);
        let provider=pool.get(&selection.provider).ok_or_else(||anyhow!("no executable provider named '{}' is configured",selection.provider))?;
        let resume=self.resumable_session(session_id,provider.as_ref(),selection);
        let mut messages=if resume.is_some() { vec![] } else { self.memory.conversation(session_id).to_vec() };
        messages.push(ChatMessage{role:"user".into(),content:input.into()});
        let response=provider.chat_turn(&messages,&selection.model_name,None,resume.as_deref(),&Pulse::silent()).await?;
        self.memory.add_message(session_id,"user",input);
        self.memory.add_message(session_id,"assistant",response.response.clone());
        self.remember_agent_session(session_id,selection,Some(&response),resume.is_some(),0);
        Ok(response)
    }

    /// O agente e o modelo que um pedido de código médio recebe, em build,
    /// sem leitura do Jev: é com ele que o `jayv bench` roda o lado direto,
    /// do primeiro ao último pedido da tarefa.
    pub fn build_selection(&self)->ModelSelection {
        let ranked=rank_models(&self.config,"code","medium",&Context::default(),&held(&self.shared.performance),&Tiebreak::default());
        let selection=ranked.into_iter().next().unwrap_or_else(||configuration_selection(&self.config,"medium",&Context::default()));
        ModelSelection{mode:MODE_BUILD.into(),..selection}
    }

    /// A sessão do agente que este chat pode retomar: a do mesmo agente, do
    /// mesmo modelo, da mesma pasta e do mesmo lado da escrita, enquanto não
    /// chegou ao teto de pedidos. A sessão aberta no planejamento guarda o
    /// `--permission-mode plan` e a nota de somente leitura; retomada no
    /// build, o agente seguia dizendo que a sessão permanecia somente leitura.
    fn resumable_session(&self,session_id:&str,provider:&dyn Provider,selection:&ModelSelection)->Option<String> {
        self.resume_check(session_id,provider,selection).ok().map(|resume|resume.id)
    }

    /// A sessão a retomar ou, quando não há, o motivo. O motivo vai para as
    /// marcas do Jev (`session_new:<motivo>`): é com ele que se mede por que
    /// o agente relê o projeto do zero.
    ///
    /// Com `resume_across_modes`, a sessão do outro lado da escrita também é
    /// retomada, e o pedido leva uma nota dizendo que o modo mudou.
    fn resume_check(&self,session_id:&str,provider:&dyn Provider,selection:&ModelSelection)->std::result::Result<Resume,NewSession> {
        if !provider.resumes() { return Err(NewSession::CannotResume); }
        let kept=self.memory.agent_session(session_id).ok_or(NewSession::First)?;
        if kept.provider!=selection.provider { return Err(NewSession::OtherAgent); }
        if kept.model!=selection.model_name { return Err(NewSession::OtherModel); }
        if kept.root!=self.rag.project_info().root { return Err(NewSession::OtherFolder); }
        let crossed=kept.writes!=self.writes(&selection.mode);
        if crossed&&!self.config.jev.resume_across_modes { return Err(NewSession::OtherMode); }
        if kept.turns>=RESUMED_TURNS { return Err(NewSession::TurnCeiling); }
        Ok(Resume{id:kept.id.clone(),crossed,instructions:kept.instructions})
    }

    /// Guarda (ou esquece) a sessão que o agente acabou de usar, para o pedido
    /// seguinte do chat retomá-la.
    fn remember_agent_session(&mut self,session_id:&str,selection:&ModelSelection,response:Option<&ProviderResponse>,resumed:bool,instructions:u64) {
        let resumes=self.providers.get(&selection.provider).is_some_and(|provider|provider.resumes());
        let Some(id)=response.and_then(|response|response.session.clone()).filter(|_|resumes) else { self.memory.forget_agent_session(session_id); return };
        let turns=if resumed { self.memory.agent_session(session_id).map_or(0,|kept|kept.turns)+1 } else { 1 };
        if resumed { crate::usage::mark(crate::usage::JevMark::count("session_resumed",1)); }
        self.memory.keep_agent_session(session_id,AgentSession{provider:selection.provider.clone(),model:selection.model_name.clone(),root:self.rag.project_info().root,id,turns,writes:self.writes(&selection.mode),instructions});
    }

    fn without_local_only(&self,context:&Context)->Context {
        let mut filtered=context.clone();
        filtered.relevant_files.retain(|path|!self.firewall.check_file(path).local_only);
        filtered.snippets.retain(|snippet|filtered.relevant_files.contains(&snippet.path));
        let withheld=context.snippets.len()-filtered.snippets.len();
        if withheld>0 { crate::usage::mark(crate::usage::JevMark::count("file_withheld",withheld as u64)); }
        if filtered.snippets.len()<context.snippets.len() { filtered.estimated_tokens=filtered.estimated_tokens.saturating_sub(context.snippets.iter().filter(|s|!filtered.snippets.iter().any(|kept|kept.path==s.path)).map(snippet_tokens).sum()); note_removal(&mut filtered); }
        filtered
    }

    /// O que falta configurar, gravado como aviso: cada um o lê no seu idioma.
    fn configuration_guidance(&self, selection:&ModelSelection)->Option<String> {
        // Com política, o que sobrou sem agente ou sem modelo foi ela que
        // desligou — ou ela junto com quem usa. Mexer só na configuração de
        // quem usa não resolve, então a orientação diz de quem é a regra.
        let blocked_by_policy=!self.config.models.is_empty() && (self.providers.is_empty()||selection.provider=="jev");
        let problem=if let Some(org)=self.policy_scope.as_deref().filter(|_|blocked_by_policy) {
            Text::new("guidance.policyBlocked").with("org",org)
        } else if self.config.models.is_empty() && self.providers.is_empty() {
            Text::new("guidance.nothingConfigured")
        } else if self.config.models.is_empty() {
            Text::new("guidance.noModels")
        } else if self.providers.is_empty() {
            Text::new("guidance.noAgent")
        } else if selection.provider=="jev" {
            Text::new("guidance.noFittingModel")
        } else if !self.providers.contains_key(&selection.provider) {
            Text::new("guidance.unknownProvider").with("provider",&selection.provider)
        } else {
            return None;
        };
        let fix=match problem.key.as_str() { "guidance.policyBlocked"=>Text::new("guidance.policyFix").with("org",self.policy_scope.as_deref().unwrap_or_default()), _=>Text::new("guidance.fix") };
        Some(i18n::notice(&[Text::new("guidance.failed").with("problem",problem),fix]))
    }

    fn explanation_result(&self,user_input:&str,normalized:&str)->ProcessResult {
        let explained=self.last_decision.as_ref().map(|d|Text::new("explain.last").with("model",&d.model_name).with("provider",&d.model_provider).with("files",d.context_files_count).with("tokens",d.estimated_tokens)).unwrap_or_else(||Text::new("explain.none"));
        let response=i18n::notice(&[explained]);
        let result=ProviderResponse{response,input_tokens:0,output_tokens:0,model:"internal".into(),provider:"jev".into(),latency_ms:0,session:None}; let decision=Decision{model_provider:"jev".into(),model_name:"internal".into(),estimated_tokens:0,context_files_count:0,rag_files_count:0};
        ProcessResult{user_input:user_input.into(),normalized_input:normalized.into(),intent_analysis:analyze_intent(normalized),complexity:"trivial".into(),context_plan:vec![],context:Context::default(),strategy:"explanation".into(),model_selection:ModelSelection{model_name:"internal".into(),provider:"jev".into(),estimated_tokens:0,score:1.0,reason:"local explanation".into(),..Default::default()},result:Some(result),validation:true,decision,routing:RoutingSignals{source:SOURCE_LOCAL.into(),..Default::default()},error:None}
    }
}

/// A sessão que o pedido retoma: o id, se ela atravessa a troca de modo e a
/// impressão das instruções com que ela está.
struct Resume { id:String, crossed:bool, instructions:u64 }

/// A nota da volta retomada que trocou de lado da escrita. O que ficou na
/// conversa da sessão dizia o contrário; a nota diz o que vale agora.
const MODE_TO_BUILD_NOTE:&str="MODE CHANGE: this session continues in BUILD mode. Any earlier instruction in this conversation that the session is read-only, or that you must only plan, no longer applies: make the change within the permissions you have now.";
const MODE_TO_PLAN_NOTE:&str="MODE CHANGE: this session continues in PLAN mode. Do not change any file from now on; answer with the plan.";

/// FNV-1a das instruções de sistema: estável entre execuções, para a sessão
/// guardada no disco saber se as instruções mudaram.
fn fingerprint(text:&str)->u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325,|hash,byte|(hash^u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// Por que o pedido abriu uma sessão nova do agente em vez de retomar a do
/// chat. Cada sessão nova é um agente relendo o projeto do zero; contar o
/// motivo diz qual regra está quebrando a retomada.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum NewSession {
    /// O chat ainda não tinha sessão (ou a anterior não foi guardada).
    First,
    /// O agente não sabe retomar (sem o lugar da sessão na linha de comando,
    /// ou um provedor por API).
    CannotResume,
    OtherAgent,
    OtherModel,
    OtherFolder,
    /// A sessão nasceu do outro lado da escrita (planejamento × build).
    OtherMode,
    /// A sessão chegou ao teto de pedidos.
    TurnCeiling,
    /// A retomada foi tentada e o agente não achou a sessão.
    Lost,
}

impl NewSession {
    pub fn as_str(&self)->&'static str {
        match self { Self::First=>"first", Self::CannotResume=>"cannot_resume", Self::OtherAgent=>"other_agent", Self::OtherModel=>"other_model", Self::OtherFolder=>"other_folder", Self::OtherMode=>"other_mode", Self::TurnCeiling=>"turn_ceiling", Self::Lost=>"lost" }
    }
    /// A marca do Jev que conta esta sessão nova.
    pub fn mark(&self)->String { format!("session_new:{}",self.as_str()) }
}

/// O que o firewall fez num contexto recém-lido: arquivos retidos e valores
/// trocados por `[…_REDACTED]`. Só na leitura nova — o contexto que sai do
/// cache já foi contado quando entrou nele.
/// O pedido como o modelo o lê. Um modelo por API não enxerga o disco e
/// recebe os arquivos; um agente que explora o repositório recebe só o mapa —
/// caminho e definições — e abre o que precisar.
fn task_message(input:&str,context:&Context,explores:bool,symbols:&crate::symbols::SymbolIndex)->String {
    if context.snippets.is_empty() { return input.into(); }
    if explores {
        let map=context.snippets.iter().map(|snippet|map_entry(snippet,symbols)).collect::<Vec<_>>().join("\n");
        return format!("TASK:\n{input}\n\n{MAP_NOTE}\nRELEVANT FILES:\n{map}");
    }
    let files=context.snippets.iter().map(|s|format!("FILE: {}\n{}",s.path,s.content)).collect::<Vec<_>>().join("\n\n");
    format!("TASK:\n{input}\n\nREPOSITORY CONTEXT:\n{files}")
}

/// Um arquivo no mapa: o caminho, as definições com a linha onde estão e,
/// quando o índice de símbolos conhece o arquivo, com quem ele se liga. Sem
/// gramática para a linguagem, as definições saem das linhas do trecho.
fn map_entry(snippet:&ContextSnippet,symbols:&crate::symbols::SymbolIndex)->String {
    let mut lines=match symbols.definitions(&snippet.path) {
        Some(definitions)=>definitions.iter().take(MAP_OUTLINE_LINES).map(|definition|format!("L{} {}",definition.line,definition.signature)).collect::<Vec<_>>(),
        None=>outline(&snippet.content),
    };
    let linked=|label:&str,paths:Vec<&str>|(!paths.is_empty()).then(||{
        let shown=paths.iter().take(MAP_LINKS).copied().collect::<Vec<_>>().join(", ");
        let more=paths.len().saturating_sub(MAP_LINKS);
        if more>0 { format!("{label}: {shown} (+{more})") } else { format!("{label}: {shown}") }
    });
    lines.extend(linked("uses",symbols.uses(&snippet.path)));
    lines.extend(linked("used by",symbols.used_by(&snippet.path)));
    if lines.is_empty() { format!("- {}",snippet.path) } else { format!("- {}\n{}",snippet.path,lines.iter().map(|line|format!("    {line}")).collect::<Vec<_>>().join("\n")) }
}

/// As linhas que declaram algo — função, tipo, classe, constante exportada —,
/// em qualquer das linguagens comuns. É a reserva das linguagens que o índice
/// de símbolos não lê.
fn outline(content:&str)->Vec<String> {
    const STARTS:[&str;24]=["fn ","pub fn ","pub(crate) fn ","async fn ","pub async fn ","struct ","pub struct ","enum ","pub enum ","trait ","pub trait ","impl ","def ","async def ","class ","function ","export ","interface ","type ","func ","public ","module ","const ","pub const "];
    content.lines().map(str::trim).filter(|line|STARTS.iter().any(|start|line.starts_with(start))).take(MAP_OUTLINE_LINES).map(|line|{
        let line=line.trim_end_matches(['{',' ']);
        if line.chars().count()>MAP_LINE_CHARS { format!("{}…",line.chars().take(MAP_LINE_CHARS).collect::<String>()) } else { line.to_string() }
    }).collect()
}

/// As últimas mensagens antes do pedido atual (que é a última da conversa),
/// com as respostas antigas do modelo encurtadas.
fn short_history(conversation:&[ChatMessage])->Vec<ChatMessage> {
    conversation.iter().rev().skip(1).take(HISTORY_MESSAGES).rev().map(|message|{
        if message.role!="assistant"||message.content.chars().count()<=HISTORY_REPLY_CHARS { return message.clone(); }
        ChatMessage{role:message.role.clone(),content:format!("{}{HISTORY_CUT}",message.content.chars().take(HISTORY_REPLY_CHARS).collect::<String>())}
    }).collect()
}

/// A falha que vem da sessão retomada — apagada, de outra pasta, expirada —, e
/// não do pedido: com ela, vale começar outra sessão.
/// Até quando depois da largada uma falha ainda conta como "não começou".
/// O que os pedidos de chats diferentes dividem quando correm ao mesmo tempo,
/// cada um no seu `Orchestrator` (com a sua pasta, os seus agentes e o seu
/// índice): o desempenho dos modelos, que o roteamento aprende, e o estado de
/// cada chat que passa de um pedido para o seguinte — o plano guardado para o
/// build, as reclamações seguidas, o "preso no planejamento". Os pedidos de um
/// mesmo chat continuam em ordem, então cada chat só é tocado por um pedido de
/// cada vez; os cadeados são curtos e nunca atravessam um `await`.
pub struct Shared {
    performance: Mutex<PerformanceTracker>,
    performance_path: PathBuf,
    plans: Mutex<HashMap<String,String>>,
    complaints: Mutex<HashMap<String,u32>>,
    stuck_in_plan: Mutex<HashSet<String>>,
}

impl Shared {
    fn load(performance_path:PathBuf)->Self {
        Self{performance:Mutex::new(PerformanceTracker::load(&performance_path)),performance_path,plans:Mutex::default(),complaints:Mutex::default(),stuck_in_plan:Mutex::default()}
    }

    /// Anota o pedido e grava o histórico no disco, sob o mesmo cadeado: dois
    /// pedidos terminando juntos não gravam um por cima do outro.
    fn record(&self,record:PerformanceRecord) {
        let mut performance=held(&self.performance);
        performance.record(record);
        let _=performance.save(&self.performance_path);
    }
}

fn held<T>(lock:&Mutex<T>)->MutexGuard<'_,T> { lock.lock().unwrap_or_else(PoisonError::into_inner) }

/// De quanto em quanto tempo os agentes são montados de novo mesmo sem mudar
/// nada: é assim que o agente instalado com o app aberto aparece.
const LLM_REFRESH:std::time::Duration=std::time::Duration::from_secs(60);
#[cfg(not(test))] const FALLBACK_WINDOW:std::time::Duration=std::time::Duration::from_secs(30);
/// Nos testes a janela é curta: o que se prova é que ela conta a partir de
/// cada tentativa, não o tamanho dela.
#[cfg(test)] const FALLBACK_WINDOW:std::time::Duration=std::time::Duration::from_secs(1);

/// O agente falhou antes de trabalhar: não está instalado, não abriu, ou
/// recusou de cara por login, chave ou cota.
/// O nome do agente como a pessoa o conhece, para o prompt da revisão.
fn agent_name(provider:&str)->String {
    match provider { "claude"=>"Claude Code", "codex"=>"Codex", "copilot"=>"GitHub Copilot", "cursor"=>"Cursor", other=>other }.to_string()
}

pub fn could_not_start(error:&anyhow::Error)->bool {
    static REFUSAL:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let refusal=REFUSAL.get_or_init(||regex::Regex::new(r"(?i)not logged in|log ?in|sign ?in|authenticat|unauthori[sz]ed|api key|credential|quota|rate.?limit|usage limit|credit|billing|subscription|\b(?:401|402|403|429)\b").expect("refusal regex"));
    // O motivo que veio só do fim da resposta não conta: um pedido sobre a tela
    // de login que falhou no meio fala de login sem ser recusa nenhuma, e o
    // próximo agente encontraria o projeto pela metade.
    let from_output=|text:&Text|matches!(text.params.get("origin"),Some(crate::i18n::Param::Plain(origin)) if origin==crate::providers::FAILURE_OUTPUT);
    error.chain().filter_map(|cause|cause.downcast_ref::<Text>()).any(|text|match text.key.as_str() {
        "provider.notInstalled"|"provider.start"=>true,
        "provider.failed"=>!from_output(text)&&matches!(text.params.get("reason"),Some(crate::i18n::Param::Plain(reason)) if refusal.is_match(reason)),
        _=>false,
    })
}

/// O agente não achou a sessão que pedimos para retomar. Só as frases de erro
/// de retomada contam: a falha de um pedido que *fala* de sessão (o texto da
/// resposta vai junto do erro) não pode fazer o pedido rodar de novo do zero
/// numa árvore já meio mexida.
fn lost_session(error:&anyhow::Error)->bool {
    static LOST:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    // O Codex 0.160 diz `thread/resume failed: no rollout found for thread id …`.
    let lost=LOST.get_or_init(||regex::Regex::new(r"(?i)no conversation found|conversation (?:id )?not found|session (?:id )?not found|no such session|unknown session|invalid session(?: id)?|session (?:\S+ )?(?:does not exist|has expired|expired)|could not (?:find|resume|load) (?:the )?(?:session|conversation)|failed to resume|no rollout found|thread/resume failed").expect("lost session regex"));
    lost.is_match(&format!("{error:#}"))
}

fn mark_firewall(retrieved:usize,filtered:&Context) {
    let withheld=retrieved.saturating_sub(filtered.snippets.len());
    if withheld>0 { crate::usage::mark(crate::usage::JevMark::count("file_withheld",withheld as u64)); }
    let redacted=redactions(filtered);
    if redacted>0 { crate::usage::mark(crate::usage::JevMark::count("secret_redacted",redacted as u64)); }
}

/// Quantos valores o firewall trocou por marcador no contexto.
fn redactions(context:&Context)->usize {
    context.snippets.iter().map(|snippet|snippet.content.matches("_REDACTED]").count()).sum()
}

/// A língua das respostas: a que o desenvolvedor escolheu no app ou, sem
/// escolha, a do pedido. Código, comandos e caminhos ficam como estão.
pub fn language_note()->String {
    match i18n::reply_language() {
        Some(language)=>format!("Write every reply to the developer in {} (BCP 47 tag `{}`), whatever language the request, the history or these instructions are written in. Keep code, identifiers, commands and file paths exactly as they are.",language.name,language.tag),
        None=>REQUEST_LANGUAGE_NOTE.into(),
    }
}

/// O que volta do modelo raramente é só o título: vem entre aspas, com marca
/// de lista, às vezes com um parágrafo de justificativa embaixo. Fica a
/// primeira linha limpa, e só se ela couber numa aba da barra lateral.
/// A revisão pronta para sair depois da resposta, já sem depender do
/// orquestrador. Devolve o texto que entra no chat, no mesmo turno, como uma
/// mensagem própria — ou nada, se o revisor falhar.
pub struct ReviewRequest { provider:Box<dyn Provider>, name:String, model:String, messages:Vec<ChatMessage>, effort:&'static str }

impl ReviewRequest {
    pub async fn run(self)->Option<String> {
        match self.provider.chat_once(&self.messages,&self.model,Some(self.effort),&Pulse::silent()).await {
            Ok(response)=>crate::review::section(&response.response),
            Err(error)=>{ eprintln!("revisão: {} não revisou ({error:#})",self.name); None }
        }
    }
}

/// Os portes que pedem segunda opinião.
fn wants_review(complexity:&str)->bool { matches!(complexity,"medium"|"complex") }

/// O batismo pronto para sair, já sem depender do orquestrador.
pub struct TitleRequest { provider:Box<dyn Provider>, model:String, messages:Vec<ChatMessage> }

impl TitleRequest {
    pub async fn run(self)->Option<String> {
        clean_title(&self.provider.chat_with_effort(&self.messages,&self.model,Some(effort_for("trivial")),&Pulse::silent()).await.ok()?.response)
    }
}

/// `low` < `medium` < `high`; classe desconhecida vai por último.
fn cost_rank(class:&str)->u8 { match class { "low"=>0, "medium"=>1, "high"=>2, _=>3 } }

fn clean_title(response:&str)->Option<String> {
    let line=response.lines().map(str::trim).find(|line|!line.is_empty())?;
    let line=line.trim_start_matches(['#','-','*','>']).trim();
    let line=line.trim_matches(['"','\'','`','«','»']).trim();
    let line=line.trim_end_matches(['.',':']).trim();
    if line.is_empty()||line.chars().count()>80||line.lines().count()>1 {return None;}
    Some(line.to_string())
}

pub fn analyze_intent(input:&str)->IntentAnalysis {
    let lower=input.to_lowercase();
    let groups:[(&str,&[&str]);8]=[
        ("code", &["code","fix","implement","write","create","build","implemente","crie","corrija","código"]),
        ("analysis", &["analyze","explain","investigate","analise","explique","investigue"]),
        ("test", &["test","pytest","coverage","teste"]),
        ("refactor", &["refactor","rewrite","optimize","cleanup","refatore","reescreva","otimize"]),
        ("security", &["security","vulnerability","secure","segurança","vulnerabilidade"]),
        ("frontend", &["react","vue","html","css","frontend","ui","ux","interface"]),
        ("review", &["review","audit","critique","revise","audite"]),
        ("general", &["help","what","how","why","ajuda","como","por que"]),
    ];
    let counted=groups.iter().map(|(name,words)|(*name,words.iter().filter(|word|lower.contains(**word)).count())).collect::<Vec<_>>();
    let (intent,score)=counted.iter().fold(("general",0),|(best,top),(name,score)|if *score>top{(*name,*score)}else{(best,top)});
    let scores=counted.into_iter().map(|(name,score)|(name.into(),score)).collect::<HashMap<String,usize>>();
    IntentAnalysis{intent:if score==0{"general".into()}else{intent.into()},scores,confidence:(score as f64/8.0).min(1.0)}
}
pub fn analyze_complexity(input:&str,intent:&IntentAnalysis)->String { let words=input.split_whitespace().count(); let matched=intent.scores.values().sum::<usize>(); if words<8&&matched<=1{"trivial"}else if words<30&&matched<=3{"simple"}else if words<100{"medium"}else{"complex"}.into() }
pub fn plan_context(intent:&str,complexity:&str)->Vec<String>{let mut p=match intent{"code"|"refactor"|"test"=>vec!["code_files".into(),"dependencies".into(),"tests".into()],"security"=>vec!["code_files".into(),"configuration".into(),"dependencies".into()],"frontend"=>vec!["frontend_files".into(),"styles".into()],_=>vec!["documentation".into()]};if complexity=="complex"{p.push("full_repository".into());}p}
/// Quanto o agente pode pensar, pelo tamanho do pedido que o Jev leu. Os
/// agentes pensam o máximo do plano deles quando ninguém diz nada, e é isso
/// que consumia a sessão até em pedido pequeno.
pub fn effort_for(complexity:&str)->&'static str { match complexity { "trivial"|"simple"=>"low", "complex"=>"high", _=>"medium" } }
/// Um degrau de esforço acima: a segunda reclamação seguida pede que o
/// agente pense mais, não que tente igual.
pub fn raise_effort(effort:&str)->&'static str { match effort { "low"=>"medium", _=>"high" } }
pub fn select_strategy(intent:&str,complexity:&str)->String { if complexity=="complex"{"execution_graph"}else if matches!(intent,"code"|"refactor"|"test"|"security"){"rag_first"}else{"single_model"}.into() }
pub fn local_routing(input:&str,error:Option<String>)->(IntentAnalysis,String,RoutingSignals) {
    let intent=analyze_intent(input); let complexity=analyze_complexity(input,&intent);
    let signals=RoutingSignals{source:if error.is_some(){SOURCE_FALLBACK}else{SOURCE_LOCAL}.into(),intent_confidence:intent.confidence,notice:error.map(|error|format!("The Jev could not route this request and the local heuristics took over: {error}")),..Default::default()};
    (intent,complexity,signals)
}
fn widen_complexity(decision:&jev::RoutingDecision)->String {
    let base=jev::COMPLEXITY_BUCKETS.iter().position(|bucket|*bucket==decision.complexity).unwrap_or(0);
    let supported=decision.complexity_probabilities.iter().filter(|(_,probability)|**probability>=COMPLEXITY_MASS).filter_map(|(level,_)|level.parse::<usize>().ok()).max().unwrap_or(base+1);
    jev::COMPLEXITY_BUCKETS[supported.clamp(base,(base+1).min(jev::COMPLEXITY_BUCKETS.len()-1))].into()
}
fn jev_routed(signals:&RoutingSignals)->bool{signals.source==SOURCE_JEV}
fn repository_context_wanted(signals:&RoutingSignals)->bool{signals.needs_repository_context.is_none_or(|probability|jev::RoutingDecision::holds(probability,REPOSITORY_CONTEXT_THRESHOLD))}
fn tools_wanted(signals:&RoutingSignals)->bool{signals.needs_tools.is_none_or(|probability|jev::RoutingDecision::holds(probability,TOOLS_THRESHOLD))}
fn destructive(signals:&RoutingSignals)->bool{signals.is_destructive.is_some_and(|probability|jev::RoutingDecision::holds(probability,DESTRUCTIVE_THRESHOLD))}
pub fn routing_capabilities(intent:&str,signals:&RoutingSignals)->Vec<String>{let mut capabilities=required_capabilities(intent);if !tools_wanted(signals){capabilities.retain(|capability|capability!="tools");}capabilities}
pub fn routing_notes(signals:&RoutingSignals)->String {
    let mut notes=vec![];
    if jev_routed(signals)&&!signals.confident { notes.push(CLARIFY_NOTE); }
    if !repository_context_wanted(signals) { notes.push(NO_REPOSITORY_NOTE); }
    if signals.needs_tools.is_some()&&tools_wanted(signals) { notes.push(TOOLS_NOTE); }
    if destructive(signals) { notes.push(DESTRUCTIVE_NOTE); }
    if notes.is_empty(){String::new()}else{format!("\n{}",notes.join("\n"))}
}
/// Em que repositório o pedido está sendo atendido. Vai no prompt porque sem
/// ele um caminho de arquivo na resposta não quer dizer nada: nem o modelo sabe
/// de onde partir, nem o portão de saída tem contra o que medir o que voltou.
fn note_project(context:&mut Context){
    let mut line=format!("\nPROJECT: {} at {}",context.project.name,context.project.root);
    // A pasta da organização junta vários repositórios: o modelo precisa saber
    // onde cada um está para não misturar um com outro.
    if !context.project.repositories.is_empty() { line.push_str(&format!("\n{MULTI_REPOSITORY_NOTE} {}",context.project.repositories.join(", "))); }
    if !context.system_instructions.contains(&line) { context.system_instructions.push_str(&line); }
}

/// Planejamento ou build. Só vai em build o pedido que a portaria liberou de
/// vez, que pede para escrever código (código, refatoração, teste ou tela),
/// que precisa de ferramentas, que não apaga trabalho — pela régua do nível —
/// e cuja complexidade cabe no teto do nível. Todo o resto é planejamento:
/// análise, revisão, segurança, conversa, pedido com ressalva.
pub fn select_mode(intent:&str,complexity:&str,signals:&RoutingSignals,gate_passed:bool,level:crate::expertise::Expertise)->&'static str {
    if asks_to_build(intent,signals,gate_passed,level) && level.builds(complexity) { MODE_BUILD } else { MODE_PLAN }
}
/// Se o pedido é para implementar: a portaria liberou, a intenção é escrever
/// código, ele precisa agir na máquina e não apaga trabalho. É o que o build
/// pede antes do teto do nível.
pub fn asks_to_build(intent:&str,signals:&RoutingSignals,gate_passed:bool,level:crate::expertise::Expertise)->bool {
    let writes=matches!(intent,"code"|"refactor"|"test"|"frontend");
    let erases=signals.is_destructive.is_some_and(|probability|jev::RoutingDecision::holds(probability,level.destructive_threshold()));
    gate_passed && writes && tools_wanted(signals) && !erases
}
/// O modo do pedido diante do que o chat fixou. Fixo em build, é build: foi o
/// desenvolvedor quem escolheu. Fixo em planejamento, só um pedido para
/// implementar tira o chat de lá, e quem tira é o Jev, avisando. No
/// automático vale a escolha do Jev, menos quando o desenvolvedor insiste:
/// o pedido anterior queria implementar e ficou em planejamento, este quer de
/// novo, e ele sai em build em vez de prender o chat no plano.
pub fn resolve_mode(pinned:&str,chosen:&'static str,wants_build:bool,stuck_before:bool,plan_approved:bool)->(&'static str,Option<ModeSwitch>) {
    let switch=|from:&str,reason:&str|Some(ModeSwitch{from:from.into(),reason:reason.into()});
    match pinned {
        MODE_BUILD=>(MODE_BUILD,None),
        MODE_PLAN if wants_build=>(MODE_BUILD,switch(MODE_PLAN,SWITCH_ASKED)),
        MODE_PLAN=>(MODE_PLAN,None),
        MODE_AUTO if chosen==MODE_PLAN&&plan_approved=>(MODE_BUILD,switch(MODE_AUTO,SWITCH_ASKED)),
        _ if chosen==MODE_PLAN&&wants_build&&stuck_before=>(MODE_BUILD,switch(MODE_AUTO,SWITCH_REPEATED)),
        _=>(chosen,None),
    }
}
/// Se o pedido aprova o plano que o chat acabou de receber ("aprovado,
/// implemente", "pode seguir"). Mensagens curtas de sim também valem: só
/// são lidas quando há um plano esperando.
pub fn approves_plan(request:&str)->bool {
    let text=request.to_lowercase();
    const STRONG:[&str;14]=["aprov","approv","implement","pode seguir","pode fazer","pode executar","siga o plano","segue o plano","execute","go ahead","proceed","do it","vai em frente","manda ver"];
    if STRONG.iter().any(|cue|text.contains(cue)) { return true; }
    const YES:[&str;7]=["sim","ok","yes","isso","bora","vamos","faça"];
    text.split_whitespace().count()<=5&&text.split(|c:char|!c.is_alphanumeric()).any(|word|YES.contains(&word))
}
/// As notas do roteamento mais a do modo. Em build o agente executa, então a
/// nota de "devolva os comandos para o desenvolvedor" não vale.
pub fn mode_notes(signals:&RoutingSignals,mode:&str)->String {
    let notes=routing_notes(signals);
    let notes=if mode==MODE_BUILD { notes.replace(&format!("\n{TOOLS_NOTE}"),"") } else { notes };
    format!("{notes}\n{}\n{FOCUS_NOTE}",if mode==MODE_BUILD {format!("{BUILD_NOTE}\n{REPORT_NOTE}")} else {PLAN_NOTE.to_string()})
}
/// O agente ligado cujo programa não está neste computador sai da disputa,
/// desde que outro ligado esteja: assim o Jev não o escolhe para depois falhar.
/// Sozinho, ele fica, e o pedido diz que falta instalá-lo.
fn set_aside_missing(providers:&mut HashMap<String,crate::config::ProviderConfig>,installed:impl Fn(&str)->bool) {
    let missing:Vec<String>=providers.iter().filter(|(_,provider)|provider.enabled&&provider.kind=="cli"&&provider.command.as_deref().is_some_and(|command|!installed(command))).map(|(name,_)|name.clone()).collect();
    if providers.values().filter(|provider|provider.enabled).count()>missing.len() {
        for name in missing { if let Some(provider)=providers.get_mut(&name) { provider.enabled=false; } }
    }
}

fn build_planners(configs:&HashMap<String,crate::config::ProviderConfig>,workdir:&Workdir)->HashMap<String,Box<dyn Provider>> {
    build_providers(&configs.iter().map(|(name,config)|(name.clone(),config.for_planning())).collect(),workdir)
}
fn note_routing(context:&mut Context,signals:&RoutingSignals,mode:&str){ let notes=mode_notes(signals,mode); if !notes.is_empty()&&!context.system_instructions.contains(notes.trim_start()) { context.system_instructions.push_str(&notes); } }
fn estimate_tokens(value:&str)->usize{(value.chars().count()/4).max(1)}
fn snippet_tokens(snippet:&ContextSnippet)->usize{estimate_tokens(&snippet.content)+estimate_tokens(&snippet.path)+SNIPPET_WRAPPER}
fn note_removal(context:&mut Context){ if !context.system_instructions.contains(REMOVAL_NOTE) { context.system_instructions=format!("{}\n{REMOVAL_NOTE}",context.system_instructions); } }
fn fragment(index:usize,snippet:&ContextSnippet,top:f64)->ContextFragment{ContextFragment{id:index.to_string(),content:String::new(),relevance:(snippet.score/top).clamp(0.0,1.0),confidence:NEUTRAL_SIGNAL,freshness:NEUTRAL_SIGNAL,tokens:snippet_tokens(snippet),dependency_importance:NEUTRAL_SIGNAL}}

fn prune_to_budget(context:&mut Context,budget:usize)->bool {
    let retrieved=context.snippets.len();
    if retrieved==0 { return false; }
    if budget==0 { context.snippets.clear(); context.relevant_files.clear(); return true; }
    let top=context.snippets.iter().map(|s|s.score).fold(0.0,f64::max).max(1.0);
    let fragments=context.snippets.iter().enumerate().map(|(index,snippet)|fragment(index,snippet,top)).collect::<Vec<_>>();
    let mut kept=optimize_for_budget(fragments.clone(),budget).iter().filter_map(|f|f.id.parse::<usize>().ok()).collect::<HashSet<_>>();
    if kept.is_empty() {
        let best=rank_fragments(fragments).first().and_then(|f|f.id.parse::<usize>().ok()).unwrap_or(0);
        let room=budget.saturating_sub(estimate_tokens(&context.snippets[best].path)+estimate_tokens(TRUNCATION_MARKER)+SNIPPET_WRAPPER);
        if room==0 { context.snippets.clear(); context.relevant_files.clear(); return true; }
        let head=context.snippets[best].content.chars().take(room*4).collect::<String>();
        context.snippets[best].content=format!("{head}{TRUNCATION_MARKER}");
        kept.insert(best);
    }
    let mut index=0; context.snippets.retain(|_|{let keep=kept.contains(&index);index+=1;keep});
    context.relevant_files=context.snippets.iter().map(|s|s.path.clone()).collect();
    context.snippets.len()<retrieved||context.snippets.iter().any(|s|s.content.ends_with(TRUNCATION_MARKER))
}

const FAILURE_PREFIXES:[&str;6]=["error:","erro:","error ","http 4","http 5","traceback (most recent call last)"];
const FAILURE_MARKERS:[&str;10]=["rate limit","quota","unauthorized","forbidden","timed out","request timeout","service unavailable","context length","internal server error","overloaded"];
pub fn usable_response(response:&ProviderResponse)->bool {
    let body=response.response.trim();
    if body.chars().count()<2 { return false; }
    if body.matches("```").count()%2==1 { return false; }
    let lower=body.to_lowercase();
    if FAILURE_PREFIXES.iter().any(|prefix|lower.starts_with(prefix)) { return false; }
    if body.chars().count()<=200 && FAILURE_MARKERS.iter().any(|marker|lower.contains(marker)) { return false; }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn the_reply_language_follows_the_app_and_falls_back_to_the_request() {
        crate::i18n::set_reply_language(Some(crate::i18n::ReplyLanguage{tag:"ja".into(),name:"日本語".into()}));
        let note=language_note();
        assert!(note.contains("日本語") && note.contains("`ja`"),"{note}");
        crate::i18n::set_reply_language(Some(crate::i18n::ReplyLanguage{tag:"  ".into(),name:"".into()}));
        assert!(crate::i18n::reply_language().is_none(),"an empty tag is no choice");
        assert!(language_note().contains("language their request is written in"));
    }

    fn repository(files:&[(&str,String)])->tempfile::TempDir { let dir=tempfile::tempdir().expect("temporary repository"); for (name,body) in files { std::fs::write(dir.path().join(name),body).expect("fixture"); } dir }
    fn orchestrator(dir:&tempfile::TempDir)->Orchestrator { Orchestrator::new(dir.path().join("missing-config.yaml"),dir.path().to_path_buf()).expect("orchestrator") }
    fn filler(word:&str,chars:usize)->String { let mut body=format!("fn {word}() {{}}\n"); while body.len()<chars { body.push_str("let padding_value = 1;\n"); } body }
    fn answer(text:&str)->ProviderResponse { ProviderResponse{response:text.into(),..Default::default()} }
    fn sent_tokens(context:&Context)->usize { context.snippets.iter().map(snippet_tokens).sum() }
    fn routed(intent:&str,intent_confidence:f64,complexity:&str,complexity_confidence:f64)->jev::RoutingDecision {
        jev::RoutingDecision{intent:intent.into(),intent_confidence,intent_probabilities:HashMap::from([(intent.to_string(),intent_confidence)]),complexity:complexity.into(),complexity_score:0.0,complexity_confidence,complexity_probabilities:HashMap::new(),needs_repository_context:1.0,needs_tools:1.0,is_destructive:0.0,model:"jev-1.13.0".into(),usage:jev::Usage{input_tokens:1_200,output_tokens:64}}
    }
    fn signals_of(orchestrator:&Orchestrator,decision:&jev::RoutingDecision)->RoutingSignals { orchestrator.jev_routing(decision).2 }
    fn repository_root()->PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().expect("repository root") }

    /// O Jev conta o que fez no contexto: a leitura nova, o acerto do cache
    /// na segunda vez e os segredos trocados por marcador.
    #[tokio::test] async fn the_context_work_is_counted() {
        let dir=repository(&[("util.py",format!("def connect():\n    api_key = \"sk-live-01234567890abcdef\"\n{}",filler("connect",200)))]);
        let mut orchestrator=orchestrator(&dir);
        let (sink,mut entries)=tokio::sync::mpsc::unbounded_channel();
        crate::usage::within_sink(crate::usage::Scope::default(),sink,async {
            orchestrator.retrieve_context("connect api key",&["full_repository".into()]);
            orchestrator.retrieve_context("connect api key",&["full_repository".into()]);
        }).await;
        let mut kinds=vec![];
        while let Ok(crate::usage::Entry::Jev(_,mark))=entries.try_recv() { kinds.push((mark.kind,mark.amount)); }
        assert!(kinds.contains(&("cache_miss".into(),1.0)),"{kinds:?}");
        assert!(kinds.contains(&("cache_hit".into(),1.0)),"{kinds:?}");
        assert!(kinds.iter().any(|(kind,amount)|kind=="secret_redacted"&&*amount>=1.0),"{kinds:?}");
    }

    #[test]
    fn the_title_survives_the_decoration_the_model_puts_around_it() {
        assert_eq!(clean_title("Cálculo do frete no checkout").as_deref(),Some("Cálculo do frete no checkout"));
        assert_eq!(clean_title("\n  \"Revisão do cálculo do frete.\"  \n").as_deref(),Some("Revisão do cálculo do frete"));
        assert_eq!(clean_title("- **Frete** do checkout\n\nEscolhi esse título porque o pedido trata do cálculo.").as_deref(),Some("**Frete** do checkout"));
        assert_eq!(clean_title("Title: ").as_deref(),Some("Title"));
        assert_eq!(clean_title("   \n  "),None,"resposta vazia deixa o resumo local de pé");
        assert_eq!(clean_title(&"palavra ".repeat(20)),None,"um parágrafo não cabe na lateral");
    }

    #[test]
    fn jev_replaces_the_keyword_counter_with_its_own_calibrated_confidence() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let orchestrator=orchestrator(&dir);
        let request="fix the auth bypass in the payment handler";
        assert_eq!(analyze_intent(request).intent,"code");
        assert_eq!(analyze_complexity(request,&analyze_intent(request)),"simple");

        let mut decision=routed("security",0.91,"complex",0.88); decision.needs_tools=0.06;
        let (intent,complexity,signals)=orchestrator.jev_routing(&decision);

        assert_eq!(intent.intent,"security");
        assert_eq!(intent.confidence,0.91);
        assert_eq!(intent.scores.get("security"),Some(&91));
        assert_eq!(complexity,"complex");
        assert!(signals.confident);
        assert_eq!(signals.source,SOURCE_JEV);
        assert_eq!(signals.jev_input_tokens,1_200);
        assert!(signals.notice.is_none());
        assert!(routing_notes(&signals).is_empty());
    }

    #[test]
    fn a_low_complexity_confidence_widens_the_budget_by_at_most_one_bucket() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let orchestrator=orchestrator(&dir);
        let mut decision=routed("code",0.95,"simple",0.41);
        decision.complexity_probabilities=HashMap::from([("1".into(),0.5),("2".into(),0.3),("3".into(),0.2)]);

        let (_,complexity,signals)=orchestrator.jev_routing(&decision);

        assert_eq!(complexity,"medium");
        assert_eq!(signals.complexity_before_widening.as_deref(),Some("simple"));
        assert_eq!(orchestrator.jev_routing(&routed("code",0.95,"trivial",0.1)).1,"simple");
        assert_eq!(orchestrator.jev_routing(&routed("code",0.95,"complex",0.1)).1,"complex");
        let mut noisy=routed("code",0.95,"medium",0.2); noisy.complexity_probabilities=HashMap::from([("0".into(),0.4),("2".into(),0.6)]);
        assert_eq!(orchestrator.jev_routing(&noisy).1,"medium");
    }

    #[test]
    fn a_low_intent_confidence_keeps_the_jev_intent_and_asks_the_model_to_clarify() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let orchestrator=orchestrator(&dir);

        let (intent,complexity,signals)=orchestrator.jev_routing(&routed("refactor",0.38,"medium",0.95));

        assert_eq!(intent.intent,"refactor");
        assert_eq!(complexity,"medium");
        assert!(!signals.confident);
        assert!(routing_notes(&signals).contains(CLARIFY_NOTE));
        let notice=signals.notice.expect("aviso de confiança baixa");
        assert!(notice.contains("low confidence") && notice.contains("refactor"),"{notice}");
    }

    #[test]
    fn a_request_that_needs_no_repository_context_ships_no_snippets() {
        let mut orchestrator=Orchestrator::new(repository_root().join("config.yaml"),repository_root()).expect("orchestrator");
        let request="what does this error mean";
        let mut decision=routed("general",0.94,"trivial",0.91);
        decision.needs_repository_context=0.04; decision.needs_tools=0.05;
        let heuristic=RoutingSignals{source:SOURCE_LOCAL.into(),..Default::default()};
        let signals=signals_of(&orchestrator,&decision);
        let plan=plan_context("general","trivial");
        let retrieved=orchestrator.retrieve_context(request,&plan);

        let before=orchestrator.assemble_context(request,&plan,2_000,120,&heuristic,MODE_PLAN);
        let after=orchestrator.assemble_context(request,&plan,2_000,120,&signals,MODE_PLAN);

        assert!(!before.snippets.is_empty(),"the fixture must retrieve something to save");
        assert!(sent_tokens(&before)>0);
        assert!(after.snippets.is_empty() && after.relevant_files.is_empty());
        assert!(after.repository_context_skipped && !before.repository_context_skipped);
        assert_eq!(sent_tokens(&after),0);
        assert!(after.system_instructions.contains(NO_REPOSITORY_NOTE));
        assert!(estimate_tokens(&routing_notes(&signals))<sent_tokens(&before),"the instruction must cost less than the context it replaces");
        println!("`{request}` on this repository | retrieved: {} files / {} tokens | before (pruned to the trivial budget): {} files / {} tokens | after: {} files / {} tokens + {} instruction tokens",retrieved.snippets.len(),sent_tokens(&retrieved),before.snippets.len(),sent_tokens(&before),after.snippets.len(),sent_tokens(&after),estimate_tokens(&routing_notes(&signals)));
    }

    /// O modelo precisa saber em que repositório ele está mexendo. Sem a pasta
    /// no prompt, um caminho de arquivo devolvido pela resposta não quer dizer
    /// nada, e o portão de saída não tem contra o que medi-lo.
    #[test]
    fn the_prompt_says_which_folder_the_project_is_in() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let plan=plan_context("general","trivial");

        let context=orchestrator.assemble_context("onde estou",&plan,2_000,120,&RoutingSignals::default(),MODE_PLAN);

        assert!(context.system_instructions.contains(&context.project.root),"o caminho do projeto vai no prompt: {}",context.system_instructions);
        assert!(context.system_instructions.contains(&context.project.name),"o nome do projeto vai junto");
    }

    #[test]
    fn the_prompt_lists_the_repositories_of_an_organization_folder() {
        let dir=repository(&[]);
        for name in ["api","web"] {
            std::fs::create_dir_all(dir.path().join(name).join(".git")).unwrap();
            std::fs::write(dir.path().join(name).join("lib.rs"),filler(name,200)).unwrap();
            std::fs::write(dir.path().join(name).join(".git/config"),format!("[remote \"origin\"]\n\turl = https://github.com/acme/{name}\n")).unwrap();
        }
        let mut orchestrator=orchestrator(&dir);
        orchestrator.rag.invalidate();
        orchestrator.focus_on(dir.path()).unwrap();
        let plan=plan_context("general","trivial");

        let context=orchestrator.assemble_context("onde estou",&plan,2_000,120,&RoutingSignals::default(),MODE_PLAN);

        assert!(context.system_instructions.contains("REPOSITORIES: api/ (github.com/acme/api), web/ (github.com/acme/web)"),"cada repositório da pasta vai no prompt: {}",context.system_instructions);
    }

    #[test]
    fn tools_that_are_not_needed_leave_the_tools_capability_out() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let orchestrator=orchestrator(&dir);
        let mut quiet=routed("code",0.9,"simple",0.9); quiet.needs_tools=0.08;
        let quiet=signals_of(&orchestrator,&quiet);
        let noisy=signals_of(&orchestrator,&routed("code",0.9,"simple",0.9));
        let heuristic=RoutingSignals{source:SOURCE_LOCAL.into(),..Default::default()};

        assert_eq!(routing_capabilities("code",&quiet),vec!["code".to_string()]);
        assert!(routing_capabilities("code",&noisy).contains(&"tools".to_string()));
        assert_eq!(routing_capabilities("code",&heuristic),required_capabilities("code"));
        assert!(!routing_notes(&quiet).contains(TOOLS_NOTE));
        assert!(routing_notes(&noisy).contains(TOOLS_NOTE));
    }

    #[tokio::test]
    async fn a_destructive_request_is_surfaced_and_stated_to_the_model() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let mut decision=routed("refactor",0.93,"medium",0.9); decision.is_destructive=0.94;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(decision));

        let result=orchestrator.process("apague o roteador antigo e reescreva",Some("test"),&Pulse::silent()).await;

        assert_eq!(result.routing.is_destructive,Some(0.94));
        assert_eq!(result.routing.source,SOURCE_JEV);
        assert!(result.context.system_instructions.contains(DESTRUCTIVE_NOTE));
        assert!(result.error.is_none(),"a safety signal must never block the request");
        assert!(result.result.is_some());
    }

    #[test]
    fn only_a_cleared_request_to_write_code_goes_to_build() {
        use crate::expertise::Expertise::{Architect, Mid, Starter};
        let jev=|tools:f64,destructive:f64|RoutingSignals{source:SOURCE_JEV.into(),needs_tools:Some(tools),is_destructive:Some(destructive),..Default::default()};
        assert_eq!(select_mode("code","simple",&jev(0.9,0.0),true,Mid),MODE_BUILD);
        assert_eq!(select_mode("frontend","medium",&jev(0.9,0.0),true,Mid),MODE_BUILD);
        assert_eq!(select_mode("code","simple",&jev(0.9,0.0),false,Mid),MODE_PLAN,"a ressalva da portaria pede plano");
        assert_eq!(select_mode("code","simple",&jev(0.1,0.0),true,Mid),MODE_PLAN,"sem ferramentas não há o que construir");
        assert_eq!(select_mode("refactor","medium",&jev(0.9,0.8),true,Mid),MODE_PLAN,"apagar trabalho começa por um plano");
        assert_eq!(select_mode("code","complex",&jev(0.9,0.0),true,Mid),MODE_PLAN,"o sistema inteiro começa por um plano");
        for intent in ["analysis","review","security","general"] { assert_eq!(select_mode(intent,"simple",&jev(0.9,0.0),true,Mid),MODE_PLAN,"{intent}"); }
        assert_eq!(select_mode("code","simple",&RoutingSignals::default(),true,Mid),MODE_BUILD,"a heurística local não sabe das ferramentas e não trava o build");
        // O nível move o teto do build e a régua do que apaga trabalho.
        assert_eq!(select_mode("code","medium",&jev(0.9,0.0),true,Starter),MODE_PLAN,"quem começa recebe plano no médio");
        assert_eq!(select_mode("code","complex",&jev(0.9,0.0),true,Architect),MODE_BUILD,"quem projeta constrói o sistema");
        assert_eq!(select_mode("refactor","simple",&jev(0.9,0.25),true,Starter),MODE_PLAN);
        assert_eq!(select_mode("refactor","simple",&jev(0.9,0.25),true,Mid),MODE_BUILD);

        let build=mode_notes(&jev(0.9,0.0),MODE_BUILD);
        assert!(build.contains(BUILD_NOTE)&&build.contains(REPORT_NOTE)&&!build.contains(TOOLS_NOTE)&&!build.contains(PLAN_NOTE));
        let plan=mode_notes(&jev(0.9,0.0),MODE_PLAN);
        assert!(plan.contains(PLAN_NOTE)&&plan.contains(TOOLS_NOTE)&&!plan.contains(REPORT_NOTE),"o planejamento não roda nada para relatar");
    }

    #[test]
    fn the_lean_rule_goes_only_with_build_and_follows_the_level() {
        use crate::expertise::Expertise::{Architect, Starter};
        let dir=repository(&[("lib.rs","pub fn run() {}".into())]);
        let mut orchestrator=orchestrator(&dir);
        assert_eq!(orchestrator.lean_note(MODE_BUILD),Some(LEAN_FULL_NOTE),"ligada por padrão, na força do nível médio");
        assert_eq!(orchestrator.lean_note(MODE_PLAN),None,"plano não escreve código");
        orchestrator.expertise=Starter;
        assert_eq!(orchestrator.lean_note(MODE_BUILD),Some(LEAN_LITE_NOTE));
        orchestrator.expertise=Architect;
        orchestrator.lean_code=false;
        assert_eq!(orchestrator.lean_note(MODE_BUILD),None);
        assert!(estimate_tokens(LEAN_FULL_NOTE)<=90,"a regra custa {} tokens por pedido de build",estimate_tokens(LEAN_FULL_NOTE));
    }

    /// Sem modelo por API, o agente de linha de comando batiza o chat — no
    /// modelo mais barato, em somente leitura, e fora do orquestrador.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_line_agent_names_the_chat_when_no_api_model_exists() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        assert!(orchestrator.title_request("corrige o frete","code").is_none(),"sem agente nenhum, fica o resumo local");
        let script=|title:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),"cat >/dev/null; echo construiu".into()],plan_args:vec!["-c".into(),format!("cat >/dev/null; echo '\"{title}.\"'")],..Default::default()};
        let model=|provider:&str,cost:&str|crate::config::ModelConfig{enabled:true,provider:provider.into(),model:format!("{provider}-model"),capabilities:vec!["chat".into()],cost_class:cost.into(),speed:"medium".into(),context_window:200_000};
        let providers=HashMap::from([("caro".to_string(),script("Título do caro")),("barato".to_string(),script("Frete do checkout"))]);
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([("caro/m".to_string(),model("caro","high")),("barato/m".to_string(),model("barato","low"))]);

        let request=orchestrator.title_request("corrige o cálculo do frete no checkout","code").expect("há quem batize");
        drop(orchestrator);

        assert_eq!(request.run().await.as_deref(),Some("Frete do checkout"),"o mais barato batiza, e o título sai limpo");
    }

    /// Com só o Copilot, que cobra por pedido, o chat fica com o resumo local.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_per_request_agent_does_not_name_the_chat() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let agent=crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),"cat >/dev/null; echo x".into()],plan_args:vec!["-c".into(),"cat >/dev/null; echo '\"Título\"'".into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"copilot".into(),model:"claude-sonnet-4.5".into(),capabilities:vec!["chat".into()],cost_class:"low".into(),speed:"medium".into(),context_window:200_000};
        let providers=HashMap::from([("copilot".to_string(),agent)]);
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([("copilot/m".to_string(),model)]);
        assert!(orchestrator.title_request("corrige o frete","code").is_none(),"uma premium request só para o título não vale");
    }

    /// O plano do planejamento segue inteiro para o build que abre sessão nova
    /// (o histórico cortaria em 1.500 caracteres), e "não funcionou" leva a
    /// nota de conferir antes de mudar, com um degrau de esforço a mais na
    /// segunda reclamação seguida.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_build_gets_the_whole_plan_and_a_retry_does_not_repeat_itself() {
        let dir=repository(&[("router.rs",filler("route_request",200))]);
        let calls=tempfile::tempdir().expect("chamadas");
        let mut orchestrator=orchestrator(&dir);
        let plan=format!("1. {}FIM-DO-PLANO",["passo longo do plano"; 200].join(" "));
        let build=format!(r#"cat > "{dir}/build-$(ls {dir} | wc -l)"; echo "$@" > "{dir}/args-$(ls {dir} | wc -l)"; echo pronto"#,dir=calls.path().display());
        let planner=format!(r#"cat >/dev/null; echo "{plan}""#);
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),build,"agent".into(),"--effort".into(),crate::llm::EFFORT.into()],plan_args:vec!["-c".into(),planner],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        let read=|name:String|std::fs::read_to_string(calls.path().join(&name)).unwrap_or_else(|_|panic!("{name}"));

        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("analysis",0.93,"simple",0.9)));
        orchestrator.pending_work_mode=Some(MODE_PLAN.into());
        orchestrator.process("planeje o cache do roteador",Some("chat"),&Pulse::silent()).await;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.process("pode implementar",Some("chat"),&Pulse::silent()).await;
        let built=read("build-0".into());
        assert!(built.contains(PLAN_HANDOFF)&&built.contains("FIM-DO-PLANO"),"o plano vai inteiro, até o fim");
        assert!(read("args-1".into()).contains("--effort low"));

        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.pending_retry=true;
        orchestrator.process("não funcionou",Some("chat"),&Pulse::silent()).await;
        assert!(read("build-2".into()).contains(RETRY_NOTE),"a nova tentativa confere antes de mudar");
        assert!(!read("build-2".into()).contains(PLAN_HANDOFF),"o plano vai uma vez");
        assert!(read("args-3".into()).contains("--effort low"),"a primeira reclamação ainda pensa igual");
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.pending_retry=true;
        orchestrator.process("ainda dá erro",Some("chat"),&Pulse::silent()).await;
        assert!(read("args-5".into()).contains("--effort medium"),"a segunda seguida pensa um degrau a mais: {}",read("args-5".into()));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.process("agora adicione um teste",Some("chat"),&Pulse::silent()).await;
        assert!(!read("build-6".into()).contains(RETRY_NOTE)&&read("args-7".into()).contains("--effort low"),"pedido novo zera a conta");
    }

    /// O agente escolhido que não consegue começar passa a vez ao próximo de
    /// outro agente; o que falha trabalhando não passa.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_agent_that_cannot_start_hands_the_request_to_the_next() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let script=|body:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),format!("cat >/dev/null; {body}")],plan_args:vec!["-c".into(),format!("cat >/dev/null; {body}")],..Default::default()};
        let model=|provider:&str,cost:&str|crate::config::ModelConfig{enabled:true,provider:provider.into(),model:format!("{provider}-model"),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:cost.into(),speed:"medium".into(),context_window:200_000};
        let use_agents=|orchestrator:&mut Orchestrator,first:crate::config::ProviderConfig|{
            let providers=HashMap::from([("first".to_string(),first),("second".to_string(),script("echo do segundo"))]);
            // O primeiro é do porte que o pedido simples pede: ganha na nota.
            let models=HashMap::from([("first/m".to_string(),model("first","low")),("second/m".to_string(),model("second","medium"))]);
            orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
            orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
            orchestrator.config.providers=providers;
            orchestrator.config.models=models;
            orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        };

        use_agents(&mut orchestrator,crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("jayv-agent-that-is-not-installed".into()),..Default::default()});
        let missing=orchestrator.process("adicione um teste ao roteador",Some("missing"),&Pulse::silent()).await;
        assert_eq!(missing.result.as_ref().map(|answer|answer.response.trim()),Some("do segundo"));
        assert_eq!(missing.model_selection.provider,"second");
        assert_eq!(missing.decision.model_provider,"second");

        use_agents(&mut orchestrator,script("echo 'Error: not logged in. Run /login' >&2; exit 1"));
        let logged_out=orchestrator.process("adicione um teste ao roteador",Some("logged-out"),&Pulse::silent()).await;
        assert_eq!(logged_out.model_selection.provider,"second","sem login o agente nem começa");

        use_agents(&mut orchestrator,script("echo 'error[E0425]: cannot find value' >&2; exit 1"));
        let broken=orchestrator.process("adicione um teste ao roteador",Some("broken"),&Pulse::silent()).await;
        assert!(broken.error.is_some(),"a falha de quem trabalhou fica com ele");
        assert_eq!(broken.model_selection.provider,"first");
    }

    /// O plano demorado de um pedido complexo não tira o plano B do
    /// construtor: a janela conta a partir da tentativa dele.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_fallback_window_starts_at_the_build() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let agent=|build:&str,plan:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),build.into()],plan_args:vec!["-c".into(),plan.into()],..Default::default()};
        let providers=HashMap::from([
            ("thinker".to_string(),agent("cat >/dev/null; echo thinker","sleep 1.5; cat >/dev/null; echo '1. PLANO-X'")),
            ("maker".to_string(),agent("cat >/dev/null; echo 'Error: not logged in. Run /login' >&2; exit 1","cat >/dev/null; echo plano")),
            ("spare".to_string(),agent("cat >/dev/null; echo do-reserva","cat >/dev/null; echo plano")),
        ]);
        let caps=|list:&[&str]|list.iter().map(|cap|cap.to_string()).collect::<Vec<_>>();
        let builder=|provider:&str|crate::config::ModelConfig{enabled:true,provider:provider.into(),model:"coder".into(),capabilities:caps(&["chat","code","tools"]),cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([
            ("thinker/m".to_string(),crate::config::ModelConfig{enabled:true,provider:"thinker".into(),model:"deep".into(),capabilities:caps(&["chat","reasoning"]),cost_class:"high".into(),speed:"slow".into(),context_window:200_000}),
            ("maker/m".to_string(),builder("maker")),
            ("spare/m".to_string(),builder("spare")),
        ]);
        orchestrator.config.jev.agent_order=vec!["maker".into(),"spare".into()];
        orchestrator.config.jev.plan_first=true;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"complex",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let result=orchestrator.process("reescreva o roteador inteiro",Some("slow-plan"),&Pulse::silent()).await;
        assert_eq!(result.model_selection.provider,"spare","o construtor sem login passou a vez mesmo depois do plano demorado");
        assert_eq!(result.result.map(|answer|answer.response.trim().to_string()).as_deref(),Some("do-reserva"));
    }

    /// "Parar" no meio do plano: o planejador cai, o construtor nem abre e
    /// nenhum outro agente é chamado no lugar.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_stop_during_the_plan_ends_the_request_without_building() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let built=dir.path().join("construiu");
        let agent=|build:String,plan:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),build],plan_args:vec!["-c".into(),plan.into()],..Default::default()};
        let providers=HashMap::from([
            ("thinker".to_string(),agent("cat >/dev/null; echo thinker".into(),"cat >/dev/null; sleep 30; echo '1. PLANO'")),
            ("maker".to_string(),agent(format!("cat >/dev/null; touch '{}'; echo feito",built.display()),"cat >/dev/null; echo plano")),
        ]);
        let caps=|list:&[&str]|list.iter().map(|cap|cap.to_string()).collect::<Vec<_>>();
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([
            ("thinker/m".to_string(),crate::config::ModelConfig{enabled:true,provider:"thinker".into(),model:"deep".into(),capabilities:caps(&["chat","reasoning"]),cost_class:"high".into(),speed:"slow".into(),context_window:200_000}),
            ("maker/m".to_string(),crate::config::ModelConfig{enabled:true,provider:"maker".into(),model:"coder".into(),capabilities:caps(&["chat","code","tools"]),cost_class:"medium".into(),speed:"medium".into(),context_window:200_000}),
        ]);
        orchestrator.config.jev.agent_order=vec!["maker".into()];
        orchestrator.config.jev.plan_first=true;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"complex",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let pulse=Pulse::silent();
        let stop=pulse.stop().clone();
        tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(300)).await; stop.stop(crate::progress::StopReason::Asked); });
        let started=Instant::now();
        let result=orchestrator.process("reescreva o roteador inteiro",Some("stopped"),&pulse).await;
        assert!(started.elapsed()<std::time::Duration::from_secs(10),"não esperou o planejador terminar");
        assert!(result.result.is_none());
        assert!(result.error.as_deref().is_some_and(|error|error.contains("turn.cancelled")),"{:?}",result.error);
        assert!(!built.exists(),"o construtor não foi aberto");
        assert_eq!(result.model_selection.provider,"maker","ninguém entrou no lugar");
    }

    /// Dois orquestradores que dividem o `Shared` veem o mesmo desempenho e o
    /// mesmo plano guardado: o pedido de um chat atendido pelo segundo
    /// atendente segue o que o primeiro aprendeu.
    #[test] fn orchestrators_that_share_see_the_same_learning() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let first=orchestrator(&dir);
        let mut second=orchestrator(&dir);
        second.share(first.shared());
        let before=second.performance_len();
        first.shared.record(PerformanceRecord{task_type:"code".into(),strategy_used:"auto".into(),model_used:"m".into(),success:true,response_time_ms:1,input_tokens:1,output_tokens:1,estimated_cost:0.0,timestamp:Utc::now(),chat:Some("chat".into())});
        assert_eq!(second.performance_len(),before+1);
        held(&first.shared.plans).insert("chat".into(),"1. PLANO".into());
        assert_eq!(held(&second.shared.plans).get("chat").map(String::as_str),Some("1. PLANO"));
    }

    #[test] fn only_medium_and_complex_requests_get_a_second_opinion() {
        assert!(!wants_review("trivial")&&!wants_review("simple"));
        assert!(wants_review("medium")&&wants_review("complex"));
    }

    /// O Codex 0.160 diz que não achou a sessão com estas palavras; o pedido
    /// recomeça do zero em vez de falhar.
    #[test] fn a_lost_codex_thread_starts_a_new_session() {
        let codex=anyhow!("thread/resume: thread/resume failed: no rollout found for thread id 0199a213-81c0-7800-8aa1-bbab2a035a53 (code -32600)");
        assert!(lost_session(&codex));
        assert!(!lost_session(&anyhow!("error[E0425]: cannot find value `session`")));
    }

    /// Só o que o agente anunciou ou escreveu no canal de erro decide que ele
    /// nem começou; o fim da resposta falando de login não decide.
    #[test] fn only_an_announced_refusal_means_the_agent_could_not_start() {
        let failed=|origin:&str|anyhow::Error::new(Text::new("provider.failed").with("provider","claude").with("reason","please log in again").with("origin",origin));
        assert!(could_not_start(&failed(crate::providers::FAILURE_STDERR)));
        assert!(could_not_start(&failed(crate::providers::FAILURE_ANNOUNCED)));
        assert!(!could_not_start(&failed(crate::providers::FAILURE_OUTPUT)),"o texto da resposta não é recusa");
        let legacy=anyhow::Error::new(Text::new("provider.failed").with("reason","not logged in"));
        assert!(could_not_start(&legacy),"sem origem, vale como antes");
    }

    /// Com a revisão ligada, o que o modo build mudou vai a um agente de
    /// outro provedor, em somente leitura, e a revisão entra no fim da
    /// resposta. Sem mudança no projeto, ninguém revisa.
    #[cfg(unix)]
    #[tokio::test]
    async fn another_provider_reviews_what_the_build_changed() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        for args in [&["init","-q"][..],&["-c","user.email=t@t","-c","user.name=t","commit","-qm","base","--allow-empty"],&["add","."],&["-c","user.email=t@t","-c","user.name=t","commit","-qm","fixture"]] {
            assert!(std::process::Command::new("git").current_dir(dir.path()).args(args).output().expect("git").status.success());
        }
        let mut orchestrator=orchestrator(&dir);
        let script=|build:&str,plan:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),format!("cat >/dev/null; {build}")],plan_args:vec!["-c".into(),plan.into()],..Default::default()};
        let model=|provider:&str,cost:&str|crate::config::ModelConfig{enabled:true,provider:provider.into(),model:format!("{provider}-model"),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:cost.into(),speed:"medium".into(),context_window:200_000};
        let providers=HashMap::from([
            ("maker".to_string(),script("echo 'fn added() {}' > added.rs; echo feito","cat >/dev/null; echo maker-planejou")),
            // Quem revisa recebe o diff pela entrada: a resposta prova que ele chegou.
            ("checker".to_string(),script("echo checker-construiu","grep -q 'fn added' && printf '### Revisão\\nnada a corrigir'")),
        ]);
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([("maker/m".to_string(),model("maker","medium")),("checker/m".to_string(),model("checker","high"))]);
        orchestrator.config.jev.review_changes=true;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"medium",0.9)));
        let (pulse,mut beats)=Pulse::channel();
        let result=orchestrator.process("adicione um teste ao roteador",Some("review"),&pulse).await;
        drop(pulse);
        let answer=result.result.expect("resposta").response;
        assert_eq!(answer.trim(),"feito","a resposta sai sem esperar a revisão");
        let review=orchestrator.pending_review.take().expect("revisão pronta").run().await.expect("revisou");
        assert!(review.trim_end().ends_with("nada a corrigir"),"{review}");
        let mut reviewed=None;
        while let Some(beat)=beats.recv().await { if let Beat::Review{provider,files,..}=beat { reviewed=Some((provider,files)); } }
        assert_eq!(reviewed,Some(("checker".to_string(),1)));

        // O pedido seguinte não muda nada: a resposta vem sem revisão.
        orchestrator.providers=build_providers(&HashMap::from([("maker".to_string(),script("echo só-li","cat >/dev/null; echo maker-planejou")),("checker".to_string(),script("echo checker-construiu","cat >/dev/null; echo revisou"))]),&orchestrator.workdir);
        let quiet=orchestrator.process("adicione um teste ao roteador",Some("quiet"),&Pulse::silent()).await;
        assert_eq!(quiet.result.map(|answer|answer.response.trim().to_string()).as_deref(),Some("só-li"));
        assert!(orchestrator.rag.paths().any(|path|path=="added.rs"),"o arquivo que o build criou entra no índice do pedido seguinte");
    }

    /// Com o plano ligado, o pedido complexo do build passa antes por um
    /// modelo de raciocínio, em somente leitura, e o plano chega ao agente
    /// que constrói. O pedido que não é complexo vai direto.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_reasoning_model_plans_before_another_agent_builds() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let agent=|build:&str,plan:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),build.into()],plan_args:vec!["-c".into(),plan.into()],..Default::default()};
        let providers=HashMap::from([
            ("thinker".to_string(),agent("cat >/dev/null; echo thinker-construiu","cat >/dev/null; echo '1. PLANO-X'")),
            ("maker".to_string(),agent("grep -q 'PLANO-X' && echo seguiu-o-plano || echo sem-plano","cat >/dev/null; echo maker-planejou")),
        ]);
        let caps=|list:&[&str]|list.iter().map(|cap|cap.to_string()).collect::<Vec<_>>();
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([
            // Só pensa: não entra na disputa de quem constrói.
            ("thinker/m".to_string(),crate::config::ModelConfig{enabled:true,provider:"thinker".into(),model:"deep".into(),capabilities:caps(&["chat","reasoning"]),cost_class:"high".into(),speed:"slow".into(),context_window:200_000}),
            ("maker/m".to_string(),crate::config::ModelConfig{enabled:true,provider:"maker".into(),model:"coder".into(),capabilities:caps(&["chat","code","tools"]),cost_class:"medium".into(),speed:"medium".into(),context_window:200_000}),
        ]);
        orchestrator.config.jev.plan_first=true;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"complex",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let (pulse,mut beats)=Pulse::channel();
        let complex=orchestrator.process("reescreva o roteador inteiro",Some("complex"),&pulse).await;
        drop(pulse);
        assert_eq!(complex.result.map(|answer|answer.response.trim().to_string()).as_deref(),Some("seguiu-o-plano"));
        assert_eq!(complex.model_selection.provider,"maker");
        let mut planned=None;
        while let Some(beat)=beats.recv().await { if let Beat::Plan{provider,model}=beat { planned=Some((provider,model)); } }
        assert_eq!(planned,Some(("thinker".to_string(),"deep".to_string())));

        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let simple=orchestrator.process("adicione um teste ao roteador",Some("simple"),&Pulse::silent()).await;
        assert_eq!(simple.result.map(|answer|answer.response.trim().to_string()).as_deref(),Some("sem-plano"));
    }

    /// Com a divisão ligada, o pedido complexo do build vira partes que
    /// agentes diferentes fazem ao mesmo tempo, cada um na sua cópia do
    /// projeto; as mudanças voltam para a pasta do projeto e as cópias somem.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_complex_build_is_split_between_agents_in_worktrees() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        for args in [&["init","-q"][..],&["add","."],&["-c","user.email=t@t","-c","user.name=t","commit","-qm","base"]] {
            assert!(std::process::Command::new("git").current_dir(dir.path()).args(args).output().expect("git").status.success());
        }
        let mut orchestrator=orchestrator(&dir);
        let agent=|build:&str,plan:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),build.into()],plan_args:vec!["-c".into(),plan.into()],..Default::default()};
        let worker=r#"input=$(cat); case "$input" in *"Your part: Parte A"*) echo A > a.txt;; *"Your part: Parte B"*) echo B > b.txt;; esac; echo pronto"#;
        let split=r#"cat >/dev/null; printf '%s' '{"tasks":[{"title":"Parte A","instructions":"crie a","files":["a.txt"]},{"title":"Parte B","instructions":"crie b","files":["b.txt"]}]}'"#;
        let providers=HashMap::from([
            ("thinker".to_string(),agent("cat >/dev/null; echo thinker-construiu",split)),
            ("maker".to_string(),agent(worker,"cat >/dev/null; echo maker-planejou")),
            ("helper".to_string(),agent(worker,"cat >/dev/null; echo helper-planejou")),
        ]);
        let caps=|list:&[&str]|list.iter().map(|cap|cap.to_string()).collect::<Vec<_>>();
        let model=|provider:&str,capabilities:Vec<String>,cost:&str|crate::config::ModelConfig{enabled:true,provider:provider.into(),model:format!("{provider}-m"),capabilities,cost_class:cost.into(),speed:"medium".into(),context_window:200_000};
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=HashMap::from([
            ("thinker/m".to_string(),model("thinker",caps(&["chat","reasoning"]),"high")),
            ("maker/m".to_string(),model("maker",caps(&["chat","code","tools"]),"medium")),
            ("helper/m".to_string(),model("helper",caps(&["chat","code","tools"]),"medium")),
        ]);
        orchestrator.config.jev.parallel_tasks=true;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"complex",0.9)));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let (pulse,mut beats)=Pulse::channel();
        let result=orchestrator.process("crie os arquivos a e b",Some("split"),&pulse).await;
        drop(pulse);
        let answer=result.result.expect("resposta").response;
        assert!(answer.contains("✓ Parte A")&&answer.contains("✓ Parte B"),"{answer}");
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).expect("a").trim(),"A");
        assert_eq!(std::fs::read_to_string(dir.path().join("b.txt")).expect("b").trim(),"B");
        let listed=std::process::Command::new("git").current_dir(dir.path()).args(["worktree","list"]).output().expect("git");
        assert_eq!(String::from_utf8_lossy(&listed.stdout).lines().count(),1,"as cópias foram removidas");
        let mut split_into=vec![];
        let mut finished=vec![];
        while let Some(beat)=beats.recv().await {
            match beat { Beat::Split{tasks}=>split_into=tasks.into_iter().map(|task|task.provider).collect(), Beat::Subtask{outcome,..}=>finished.push(outcome), _=>{} }
        }
        split_into.sort();
        assert_eq!(split_into,["helper","maker"],"cada parte foi para um agente");
        assert_eq!(finished,["applied","applied"]);
    }

    #[test] fn a_missing_agent_steps_aside_only_when_another_is_there() {
        let cli=|command:&str|crate::config::ProviderConfig{enabled:true,kind:"cli".into(),command:Some(command.into()),..Default::default()};
        let mut both=HashMap::from([("claude".to_string(),cli("claude")),("codex".to_string(),cli("codex"))]);
        set_aside_missing(&mut both,|command|command=="claude");
        assert!(both["claude"].enabled && !both["codex"].enabled);
        let mut alone=HashMap::from([("codex".to_string(),cli("codex"))]);
        set_aside_missing(&mut alone,|_|false);
        assert!(alone["codex"].enabled,"sozinho ele fica, e o pedido diz que falta instalá-lo");
    }

    /// O mesmo agente, duas linhas de comando: a do modo escolhido é a que roda,
    /// e o balão sabe qual agente, modelo, modo e papel atenderam.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_chosen_mode_runs_the_matching_command_line() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),"cat >/dev/null; echo ran-build".into()],plan_args:vec!["-c".into(),"cat >/dev/null; echo ran-plan".into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;

        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        let built=orchestrator.process("adicione um teste ao roteador",Some("build"),&Pulse::silent()).await;
        assert_eq!(built.result.as_ref().map(|answer|answer.response.trim()),Some("ran-build"));
        assert_eq!((built.model_selection.provider.as_str(),built.model_selection.model_name.as_str(),built.model_selection.mode.as_str(),built.model_selection.agent.as_deref()),("cli","modelo",MODE_BUILD,Some("developer")));

        // A regra de escrita em `deny` vale para o agente também: ele roda com a
        // linha que só lê, mesmo em build.
        orchestrator.config.permissions.write="deny".into();
        let denied=orchestrator.process("adicione outro teste ao roteador",Some("denied"),&Pulse::silent()).await;
        assert_eq!(denied.result.as_ref().map(|answer|answer.response.trim()),Some("ran-plan"),"sem escrita, o agente não ganha a linha que escreve");
        orchestrator.config.permissions.write="ask".into();

        orchestrator.pending_gate_passed=Some(false);
        let held=orchestrator.process("adicione um teste ao roteador",Some("held"),&Pulse::silent()).await;
        assert_eq!(held.result.as_ref().map(|answer|answer.response.trim()),Some("ran-plan"),"liberado com ressalva não escreve");
        assert_eq!(held.model_selection.mode,MODE_PLAN);

        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));
        let reviewed=orchestrator.process("revise o roteador",Some("review"),&Pulse::silent()).await;
        assert_eq!(reviewed.result.as_ref().map(|answer|answer.response.trim()),Some("ran-plan"));
        assert_eq!(reviewed.model_selection.agent.as_deref(),Some("reviewer"));

        // Fixo em build, a revisão também roda com a linha de build; fixo em
        // planejamento, o pedido para implementar tira o chat de lá.
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        let pinned=orchestrator.process("revise o roteador",Some("pinned"),&Pulse::silent()).await;
        assert_eq!((pinned.model_selection.mode.as_str(),orchestrator.mode_switch.as_ref()),(MODE_BUILD,None));
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"simple",0.9)));
        orchestrator.pending_work_mode=Some(MODE_PLAN.into());
        let freed=orchestrator.process("adicione um teste ao roteador",Some("planning"),&Pulse::silent()).await;
        assert_eq!(freed.result.as_ref().map(|answer|answer.response.trim()),Some("ran-build"));
        assert_eq!(orchestrator.mode_switch,Some(ModeSwitch{from:MODE_PLAN.into(),reason:SWITCH_ASKED.into()}));
        let next=orchestrator.process("revise o roteador",Some("planning"),&Pulse::silent()).await;
        assert_eq!((next.model_selection.mode.as_str(),orchestrator.mode_switch.as_ref()),(MODE_BUILD,None),"sem modo fixo no pedido, vale o do Jev");
    }

    #[test]
    fn the_jev_takes_the_chat_out_of_planning_only_when_it_is_stuck() {
        let asked=Some(ModeSwitch{from:MODE_PLAN.into(),reason:SWITCH_ASKED.into()});
        let asked_from_auto=Some(ModeSwitch{from:MODE_AUTO.into(),reason:SWITCH_ASKED.into()});
        let repeated=Some(ModeSwitch{from:MODE_AUTO.into(),reason:SWITCH_REPEATED.into()});
        assert_eq!(resolve_mode(MODE_BUILD,MODE_PLAN,false,false,false),(MODE_BUILD,None),"build fixo é escolha do desenvolvedor");
        assert_eq!(resolve_mode(MODE_PLAN,MODE_BUILD,false,false,false),(MODE_PLAN,None),"planejar não sai do plano");
        assert_eq!(resolve_mode(MODE_PLAN,MODE_PLAN,true,false,false),(MODE_BUILD,asked),"pedir para implementar sai, mesmo acima do teto do nível");
        assert_eq!(resolve_mode(MODE_AUTO,MODE_BUILD,true,false,false),(MODE_BUILD,None));
        assert_eq!(resolve_mode(MODE_AUTO,MODE_PLAN,true,false,false),(MODE_PLAN,None),"a primeira vez fica com o Jev");
        assert_eq!(resolve_mode(MODE_AUTO,MODE_PLAN,true,true,false),(MODE_BUILD,repeated),"a segunda vez não prende o chat");
        assert_eq!(resolve_mode(MODE_AUTO,MODE_PLAN,false,true,false),(MODE_PLAN,None));
        assert_eq!(resolve_mode(MODE_AUTO,MODE_PLAN,false,false,true),(MODE_BUILD,asked_from_auto),"plano aprovado no automático vai ao build");
        assert_eq!(resolve_mode(MODE_PLAN,MODE_PLAN,false,false,true),(MODE_PLAN,None),"planejamento fixo só sai com pedido de implementar");
        assert!(approves_plan("Aprovado, pode implementar")&&approves_plan("sim")&&!approves_plan("e se usarmos outro banco de dados para isso?"));
        assert_eq!((work_mode("plan"),work_mode("build"),work_mode("auto"),work_mode("x")),(Some(MODE_PLAN),Some(MODE_BUILD),Some(MODE_AUTO),None));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn asking_to_build_twice_in_auto_leaves_planning() {
        use crate::expertise::Expertise::Starter;
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),"cat >/dev/null; echo ran-build".into()],plan_args:vec!["-c".into(),"cat >/dev/null; echo ran-plan".into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.expertise=Starter;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("code",0.93,"medium",0.9)));
        let first=orchestrator.process("implemente o cache do roteador",Some("chat"),&Pulse::silent()).await;
        assert_eq!((first.model_selection.mode.as_str(),orchestrator.mode_switch.as_ref()),(MODE_PLAN,None),"o médio passa do teto de quem começa");
        orchestrator.process("implemente o cache do roteador",Some("other"),&Pulse::silent()).await;
        assert_eq!(orchestrator.mode_switch,None,"a insistência é por chat");
        let second=orchestrator.process("pode implementar",Some("chat"),&Pulse::silent()).await;
        assert_eq!(second.result.as_ref().map(|answer|answer.response.trim()),Some("ran-build"));
        assert_eq!(orchestrator.mode_switch,Some(ModeSwitch{from:MODE_AUTO.into(),reason:SWITCH_ASKED.into()}),"o plano do chat estava esperando: o pedido o aprova");
    }

    /// O agente explora o repositório sozinho: recebe o mapa, não os arquivos,
    /// e o segundo pedido do chat retoma a sessão do primeiro sem reenviar a
    /// conversa.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_exploring_agent_gets_the_file_map_and_resumes_the_chat_session() {
        let dir=repository(&[("router.rs",filler("route_request",3_000))]);
        let prompts=tempfile::tempdir().expect("prompts");
        let mut orchestrator=orchestrator(&dir);
        let script=format!(r#"cat > "{}/stdin-$#"; echo '{{"type":"system","subtype":"init","session_id":"s-1"}}'; echo resposta"#,prompts.path().display());
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),script.clone(),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],plan_args:vec!["-c".into(),script,"agent".into(),"--resume".into(),crate::llm::RESUME.into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));

        orchestrator.process("explain how route_request works",Some("chat"),&Pulse::silent()).await;
        let first=std::fs::read_to_string(prompts.path().join("stdin-0")).expect("o primeiro pedido abre uma sessão nova");
        assert!(first.contains("RELEVANT FILES:\n- router.rs\n    L1 fn route_request()"),"vai o mapa, com a linha de cada definição: {first}");
        assert!(!first.contains("padding_value"),"o corpo do arquivo fica no disco, para o agente abrir se precisar");
        assert_eq!(orchestrator.memory.agent_session("chat").map(|kept|(kept.id.as_str(),kept.turns)),Some(("s-1",1)));

        orchestrator.process("and where is it called from?",Some("chat"),&Pulse::silent()).await;
        let second=std::fs::read_to_string(prompts.path().join("stdin-2")).expect("o segundo pedido retoma a sessão (--resume s-1)");
        assert!(!second.contains("explain how route_request works"),"a sessão já tem a conversa: {second}");
        assert_eq!(orchestrator.memory.agent_session("chat").map(|kept|kept.turns),Some(2));

        orchestrator.process("and the tests?",Some("other-chat"),&Pulse::silent()).await;
        assert_eq!(orchestrator.memory.agent_session("other-chat").map(|kept|kept.turns),Some(1),"outro chat, outra sessão");
    }

    /// A sessão aberta no planejamento não é retomada no build: ela guarda a
    /// permissão de só ler, e o agente seguia dizendo que a sessão
    /// permanecia somente leitura. No mesmo modo, retoma como sempre.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_plan_session_is_not_resumed_in_build_mode() {
        let dir=repository(&[("router.rs",filler("route_request",200))]);
        let calls=tempfile::tempdir().expect("chamadas");
        let mut orchestrator=orchestrator(&dir);
        let script=|side:&str|format!(r#"cat >/dev/null; echo "$@" > "{}/{side}-$(ls {} | wc -l)"; echo '{{"type":"system","subtype":"init","session_id":"s-{side}"}}'; echo resposta"#,calls.path().display(),calls.path().display());
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),script("build"),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],plan_args:vec!["-c".into(),script("plan"),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));
        let call=|name:&str|std::fs::read_to_string(calls.path().join(name)).unwrap_or_else(|_|panic!("{name} não rodou"));

        orchestrator.pending_work_mode=Some(MODE_PLAN.into());
        orchestrator.process("explain how route_request works",Some("chat"),&Pulse::silent()).await;
        assert!(!call("plan-0").contains("--resume"),"sessão nova");
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.process("explain route_request again",Some("chat"),&Pulse::silent()).await;
        assert!(!call("build-1").contains("--resume"),"a sessão do planejamento fica para trás: {}",call("build-1"));
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.process("and where is it called from?",Some("chat"),&Pulse::silent()).await;
        assert!(call("build-2").contains("--resume s-build"),"no mesmo modo, retoma: {}",call("build-2"));
    }

    /// Cada sessão nova sai com o motivo de não ter retomado a anterior; a
    /// retomada conta à parte. É isso que a Fase 0 mede.
    #[cfg(unix)]
    #[tokio::test]
    async fn every_new_agent_session_is_counted_with_its_reason() {
        let dir=repository(&[("router.rs",filler("route_request",200))]);
        let mut orchestrator=orchestrator(&dir);
        let script=r#"cat >/dev/null; echo '{"type":"system","subtype":"init","session_id":"s-1"}'; echo resposta"#.to_string();
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),script.clone(),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],plan_args:vec!["-c".into(),script,"agent".into(),"--resume".into(),crate::llm::RESUME.into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));
        let (sink,mut entries)=tokio::sync::mpsc::unbounded_channel();
        crate::usage::within_sink(crate::usage::Scope::default(),sink,async {
            orchestrator.pending_work_mode=Some(MODE_PLAN.into());
            orchestrator.process("explain how route_request works",Some("chat"),&Pulse::silent()).await;
            orchestrator.pending_work_mode=Some(MODE_BUILD.into());
            orchestrator.process("explain route_request again",Some("chat"),&Pulse::silent()).await;
            orchestrator.pending_work_mode=Some(MODE_BUILD.into());
            orchestrator.process("and where is it called from?",Some("chat"),&Pulse::silent()).await;
        }).await;
        let mut kinds=vec![];
        while let Ok(entry)=entries.try_recv() { if let crate::usage::Entry::Jev(_,mark)=entry { if mark.kind.starts_with("session_") { kinds.push(mark.kind); } } }
        assert_eq!(kinds,["session_new:first","session_new:other_mode","session_resumed"]);
    }

    /// A volta retomada leva só o pedido quando as instruções não mudaram; com
    /// a retomada entre modos ligada, a sessão do planejamento continua no
    /// build com a nota da troca e as instruções novas.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_resumed_turn_sends_only_what_changed() {
        let dir=repository(&[("router.rs",filler("route_request",200))]);
        let prompts=tempfile::tempdir().expect("prompts");
        let mut orchestrator=orchestrator(&dir);
        let script=format!(r#"cat > "{}/stdin-$(ls {} | wc -l)"; echo '{{"type":"system","subtype":"init","session_id":"s-1"}}'; echo resposta"#,prompts.path().display(),prompts.path().display());
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),script.clone(),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],plan_args:vec!["-c".into(),script,"agent".into(),"--resume".into(),crate::llm::RESUME.into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));
        let sent=|index:usize|std::fs::read_to_string(prompts.path().join(format!("stdin-{index}"))).unwrap_or_else(|_|panic!("pedido {index}"));

        orchestrator.pending_work_mode=Some(MODE_PLAN.into());
        orchestrator.process("explain how route_request works",Some("chat"),&Pulse::silent()).await;
        assert!(sent(0).starts_with("system: "),"a sessão nova leva as instruções");
        orchestrator.pending_work_mode=Some(MODE_PLAN.into());
        orchestrator.process("and where is it called from?",Some("chat"),&Pulse::silent()).await;
        assert!(sent(1).starts_with("user: ")&&!sent(1).contains("system: "),"retomada sem mudança leva só o pedido: {}",sent(1));

        orchestrator.config.jev.resume_across_modes=true;
        orchestrator.pending_work_mode=Some(MODE_BUILD.into());
        orchestrator.process("now add a test for route_request",Some("chat"),&Pulse::silent()).await;
        assert!(sent(2).contains("MODE CHANGE: this session continues in BUILD mode"),"a troca de modo vai dita: {}",sent(2));
        assert!(sent(2).contains("BUILD mode:"),"e as instruções do modo novo vão junto");
        assert_eq!(orchestrator.memory.agent_session("chat").map(|kept|(kept.turns,kept.writes)),Some((3,true)),"a mesma sessão, agora do lado que escreve");
    }

    #[test]
    fn the_session_reasons_are_stable_identifiers() {
        let all=[NewSession::First,NewSession::CannotResume,NewSession::OtherAgent,NewSession::OtherModel,NewSession::OtherFolder,NewSession::OtherMode,NewSession::TurnCeiling,NewSession::Lost];
        let names=all.iter().map(NewSession::mark).collect::<Vec<_>>();
        assert_eq!(names,["session_new:first","session_new:cannot_resume","session_new:other_agent","session_new:other_model","session_new:other_folder","session_new:other_mode","session_new:turn_ceiling","session_new:lost"],"as consultas do Supabase leem estes nomes");
    }

    /// A planta do projeto vai no começo da sessão do agente, e a sessão
    /// retomada já a tem.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_project_map_goes_once_per_agent_session() {
        let mut files=vec![("router.rs".to_string(),filler("route_request",3_000))];
        files.extend((0..crate::project_map::MIN_FILES).map(|index|(format!("handler_{index}.rs"),format!("fn handle_{index}() {{ route_request(); }}"))));
        let borrowed=files.iter().map(|(path,body)|(path.as_str(),body.clone())).collect::<Vec<_>>();
        let dir=repository(&borrowed);
        let prompts=tempfile::tempdir().expect("prompts");
        let mut orchestrator=orchestrator(&dir);
        let script=format!(r#"cat > "{}/stdin-$#"; echo '{{"type":"system","subtype":"init","session_id":"s-1"}}'; echo resposta"#,prompts.path().display());
        let agent=crate::config::ProviderConfig{kind:"cli".into(),command:Some("sh".into()),args:vec!["-c".into(),script.clone(),"agent".into(),"--resume".into(),crate::llm::RESUME.into()],plan_args:vec!["-c".into(),script,"agent".into(),"--resume".into(),crate::llm::RESUME.into()],..Default::default()};
        let model=crate::config::ModelConfig{enabled:true,provider:"cli".into(),model:"modelo".into(),capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000};
        let (providers,models)=(HashMap::from([("cli".to_string(),agent)]),HashMap::from([("cli/modelo".to_string(),model)]));
        orchestrator.providers=build_providers(&providers,&orchestrator.workdir);
        orchestrator.planners=build_planners(&providers,&orchestrator.workdir);
        orchestrator.config.providers=providers;
        orchestrator.config.models=models;
        orchestrator.routing_mode=RoutingMode::Fixed(Box::new(routed("review",0.93,"medium",0.9)));

        orchestrator.process("explain how route_request works",Some("chat"),&Pulse::silent()).await;
        let first=std::fs::read_to_string(prompts.path().join("stdin-0")).expect("sessão nova");
        assert!(first.contains("PROJECT MAP")&&first.contains("- router.rs (used by 80)"),"a planta vai no começo da sessão: {first}");
        orchestrator.process("and where is it called from?",Some("chat"),&Pulse::silent()).await;
        let second=std::fs::read_to_string(prompts.path().join("stdin-2")).expect("sessão retomada");
        assert!(!second.contains("PROJECT MAP"),"a sessão retomada já tem a planta: {second}");
    }

    /// A leitura pedida junto com a portaria vale para o pedido, sem outra ida
    /// ao Jev, e só para ele.
    #[tokio::test]
    async fn the_routing_read_alongside_the_gate_is_used_once() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        orchestrator.routing_mode=RoutingMode::Auto;
        orchestrator.memory.add_message("chat","user","earlier");
        let ahead=orchestrator.routing_input_ahead("now",  "chat");
        orchestrator.memory.add_message("chat","user","now");
        assert_eq!(ahead.recent_turns,orchestrator.routing_input("now","chat").recent_turns,"o mesmo estado, antes de o pedido entrar na memória");
        orchestrator.pending_routing=Some(Ok(routed("test",0.93,"simple",0.9)));
        let (intent,_,signals)=orchestrator.decide_routing("now","chat").await;
        assert_eq!((intent.intent.as_str(),signals.source.as_str()),("test",SOURCE_JEV));
        assert!(orchestrator.pending_routing.is_none());
        orchestrator.pending_routing=Some(Err("offline".into()));
        assert_eq!(orchestrator.decide_routing("now","chat").await.2.source,SOURCE_FALLBACK);
    }

    #[test]
    fn the_agent_map_lists_definitions_of_the_whole_file_and_its_neighbours() {
        let filler="// filler\n".repeat(2_000);
        let dir=repository(&[("cache.rs",format!("pub struct Cache;\n{filler}pub fn invalidate_entries() {{}}\n")),("orchestrator.rs","fn run() { invalidate_entries(); }\n".into())]);
        let orchestrator=orchestrator(&dir);
        let context=Context{snippets:vec![ContextSnippet{path:"cache.rs".into(),content:"pub struct Cache;".into(),score:1.0}],..Default::default()};
        let message=task_message("why?",&context,true,orchestrator.rag.symbols());
        assert!(message.contains("    L1 pub struct Cache;"),"{message}");
        assert!(message.contains("    L2002 pub fn invalidate_entries()"),"a definição depois do trecho entra: {message}");
        assert!(message.contains("    used by: orchestrator.rs"),"{message}");
    }

    #[test]
    fn a_model_without_the_disk_gets_the_files_and_old_replies_come_back_short() {
        let context=Context{snippets:vec![ContextSnippet{path:"src/lib.rs".into(),content:"pub fn total() -> u32 { 1 }".into(),score:1.0}],..Default::default()};
        let none=crate::symbols::SymbolIndex::default();
        assert!(task_message("why?",&context,false,&none).contains("FILE: src/lib.rs\npub fn total() -> u32 { 1 }"));
        assert!(task_message("why?",&context,true,&none).contains("- src/lib.rs\n    pub fn total() -> u32"));
        let conversation=[ChatMessage{role:"user".into(),content:"q".repeat(3_000)},ChatMessage{role:"assistant".into(),content:"a".repeat(3_000)},ChatMessage{role:"user".into(),content:"now".into()}];
        let history=short_history(&conversation);
        assert_eq!(history.len(),2,"o pedido atual não entra no histórico");
        assert_eq!(history[0].content.len(),3_000,"o que o desenvolvedor escreveu vai inteiro");
        assert!(history[1].content.ends_with(HISTORY_CUT)&&history[1].content.len()<HISTORY_REPLY_CHARS+HISTORY_CUT.len()+1);
    }

    #[tokio::test]
    async fn a_routing_failure_falls_back_to_the_heuristics_without_failing_the_request() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        orchestrator.routing_mode=RoutingMode::Failed("a TypeSafe aplicou limite de requisições (429)".into());

        let result=orchestrator.process("Implemente um cliente HTTP",Some("test"),&Pulse::silent()).await;

        assert_eq!(result.routing.source,SOURCE_FALLBACK);
        assert_eq!(result.intent_analysis.intent,analyze_intent("Implemente um cliente HTTP").intent);
        assert!(result.routing.needs_repository_context.is_none());
        assert!(result.error.is_none());
        let notice=result.routing.notice.expect("aviso de fallback");
        assert!(notice.contains("local heuristics") && notice.contains("429"),"{notice}");
        assert!(!result.context.system_instructions.contains(CLARIFY_NOTE));
    }

    #[tokio::test]
    async fn without_jev_the_routing_and_the_prompt_are_exactly_what_they_are_today() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);

        let result=orchestrator.process("Explique o roteador",Some("test"),&Pulse::silent()).await;

        assert_eq!(result.routing.source,SOURCE_LOCAL);
        assert!(result.routing.notice.is_none() && result.routing.jev_model.is_none());
        assert!(result.routing.needs_repository_context.is_none() && result.routing.needs_tools.is_none() && result.routing.is_destructive.is_none());
        assert_eq!(result.intent_analysis.intent,analyze_intent("Explique o roteador").intent);
        assert_eq!(result.complexity,analyze_complexity("Explique o roteador",&analyze_intent("Explique o roteador")));
        // Sem o Jev, o prompt é o de sempre mais a linha do projeto — ela não vem
        // do roteador, vem de onde o pedido está sendo atendido, e vai sempre —
        // e a linha do modo, que todo pedido leva.
        assert_eq!(result.context.system_instructions,format!("{SYSTEM_INSTRUCTIONS}\nPROJECT: {} at {}\n{PLAN_NOTE}\n{FOCUS_NOTE}",result.context.project.name,result.context.project.root));
        assert!(!result.context.repository_context_skipped);
        assert!(routing_notes(&result.routing).is_empty());
    }

    #[test]
    fn routing_notes_are_idempotent_and_stay_within_a_measured_budget() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let orchestrator=orchestrator(&dir);
        let mut decision=routed("code",0.3,"medium",0.3);
        decision.needs_repository_context=0.02; decision.needs_tools=0.99; decision.is_destructive=0.99;
        let signals=signals_of(&orchestrator,&decision);
        let notes=routing_notes(&signals);

        assert!(notes.contains(CLARIFY_NOTE) && notes.contains(NO_REPOSITORY_NOTE) && notes.contains(TOOLS_NOTE) && notes.contains(DESTRUCTIVE_NOTE));
        assert!(estimate_tokens(&notes)<=140,"worst-case routing instructions cost {} tokens",estimate_tokens(&notes));
        let with_mode=mode_notes(&signals,MODE_PLAN);
        // O teto inclui o FOCUS_NOTE: poucos tokens que evitam o agente reler o contexto.
        assert!(estimate_tokens(&with_mode)<=210,"worst-case routing and mode instructions cost {} tokens",estimate_tokens(&with_mode));
        let mut context=Context{system_instructions:SYSTEM_INSTRUCTIONS.into(),..Default::default()};
        note_routing(&mut context,&signals,MODE_PLAN); let once=context.system_instructions.clone();
        note_routing(&mut context,&signals,MODE_PLAN);
        assert_eq!(context.system_instructions,once);
        assert_eq!(estimate_tokens(&routing_notes(&RoutingSignals{source:SOURCE_LOCAL.into(),..Default::default()})),1);
    }

    #[test]
    fn the_state_sent_to_jev_carries_the_project_the_candidates_and_the_recent_turns() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        orchestrator.memory.add_message("test","user","primeira pergunta");
        orchestrator.memory.add_message("test","assistant","primeira resposta");
        orchestrator.memory.add_message("test","user","router");

        let input=orchestrator.routing_input("router","test");

        assert_eq!(input.request,"router");
        assert_eq!(input.project_name,orchestrator.rag.project_info().name);
        assert_eq!(input.languages,vec!["Rust".to_string()]);
        assert_eq!(input.candidate_files,vec!["router.rs".to_string()]);
        assert_eq!(input.recent_turns,vec!["user: primeira pergunta".to_string(),"assistant: primeira resposta".to_string()]);
        assert!(input.candidate_files.iter().map(|path|estimate_tokens(path)).sum::<usize>()<=ROUTING_CANDIDATES*8);
    }

    #[test]
    fn keyword_ties_resolve_deterministically() {
        let analysis=analyze_intent("review the coverage");
        assert_eq!(analysis.scores["test"],1);
        assert_eq!(analysis.scores["review"],1);
        for _ in 0..64 { assert_eq!(analyze_intent("review the coverage").intent,analysis.intent); }
        assert_eq!(analysis.intent,"test");
    }

    #[test]
    fn identifies_code_intent() {
        assert_eq!(analyze_intent("Implement a Rust function").intent,"code");
    }

    #[test]
    fn plans_code_context() {
        let p=plan_context("code","simple");
        assert!(p.contains(&"dependencies".into()));
    }

    #[test]
    fn a_trivial_request_never_ships_more_than_its_budget() {
        let dir=repository(&[("router.rs",filler("router",40_000)),("cache.rs",filler("router_cache",40_000)),("graph.rs",filler("router_graph",40_000)),("memory.rs",filler("router_memory",40_000))]);
        let mut orchestrator=orchestrator(&dir);
        let reserved=orchestrator.request_overhead("router","test");
        let unpruned=sent_tokens(&orchestrator.retrieve_context("router",&plan_context("code","trivial")));
        let context=orchestrator.build_context("router",&plan_context("code","trivial"),2_000,reserved);
        assert!(unpruned>2_000,"fixture must exceed the trivial budget, got {unpruned}");
        assert!(context.estimated_tokens<=2_000,"estimated {} above budget",context.estimated_tokens);
        assert!(reserved+sent_tokens(&context)<=2_000,"actually sent {} above budget",reserved+sent_tokens(&context));
        assert!(!context.snippets.is_empty(),"budget pruning must not drop all context silently");
        assert!(context.system_instructions.contains("Never guess removed content"));
    }

    #[test]
    fn pruning_keeps_the_highest_value_fragments() {
        let dir=repository(&[("small.rs",filler("router",200)),("huge.rs",filler("router",40_000))]);
        let mut orchestrator=orchestrator(&dir);
        let context=orchestrator.build_context("router",&plan_context("code","trivial"),2_000,120);
        assert_eq!(context.relevant_files,vec!["small.rs".to_string()]);
        assert_eq!(context.snippets.len(),1);
    }

    #[test]
    fn a_single_oversized_snippet_is_truncated_instead_of_dropped() {
        let dir=repository(&[("router.rs",filler("router",40_000))]);
        let mut orchestrator=orchestrator(&dir);
        let context=orchestrator.build_context("router",&plan_context("code","trivial"),2_000,120);
        assert_eq!(context.snippets.len(),1);
        assert!(context.snippets[0].content.ends_with(TRUNCATION_MARKER));
        assert!(120+sent_tokens(&context)<=2_000);
        assert!(context.system_instructions.contains("[CONTEXT_TRUNCATED]"));
    }

    #[test]
    fn untouched_context_carries_no_removal_note() {
        let dir=repository(&[("router.rs",filler("router",200))]);
        let mut orchestrator=orchestrator(&dir);
        let context=orchestrator.build_context("router",&plan_context("code","simple"),5_000,120);
        assert!(!context.system_instructions.contains(REMOVAL_NOTE));
        assert_eq!(context.system_instructions,SYSTEM_INSTRUCTIONS);
    }

    #[test]
    fn the_cached_context_is_reused_after_an_unrelated_edit() {
        let dir=repository(&[("router.rs",filler("router",200)),("notes.md","unrelated".into())]);
        let mut orchestrator=orchestrator(&dir);
        let plan=plan_context("code","simple");
        orchestrator.build_context("router",&plan,5_000,120);
        std::fs::write(dir.path().join("notes.md"),"changed").expect("fixture");
        orchestrator.rag.index(&orchestrator.firewall).expect("reindex");
        orchestrator.build_context("router",&plan,5_000,120);
        assert!(orchestrator.cache.hit_rate()>0.0,"an unrelated edit must not invalidate the cached context");
    }

    #[test]
    fn rejects_provider_failures_masquerading_as_answers() {
        assert!(!usable_response(&answer("   ")));
        assert!(!usable_response(&answer("Error: 429 rate limit reached")));
        assert!(!usable_response(&answer("HTTP 503 service unavailable")));
        assert!(!usable_response(&answer("```rust\nfn truncated() {")));
        assert!(usable_response(&answer("The router selects a model and then\n```rust\nfn ok() {}\n```\ndone.")));
        assert!(usable_response(&answer(&format!("{} the quota handling code lives in router.rs and is exercised by tests.",filler("explain",260)))));
    }

    #[tokio::test]
    async fn explains_how_to_configure_an_llm_when_none_is_available() {
        let root=tempfile::tempdir().expect("temporary repository");
        let config_path=root.path().join("missing-config.yaml");
        let mut orchestrator=Orchestrator::new(config_path.clone(),root.path().to_path_buf()).expect("orchestrator");

        let result=orchestrator.process("Explique este projeto",Some("test"),&Pulse::silent()).await;

        assert!(result.error.is_none());
        assert!(result.validation);
        assert_eq!(result.decision.model_provider,"jev");
        assert_eq!(result.decision.model_name,"configuration");
        let response=result.result.expect("Jev configuration guidance").response;
        let lines=crate::i18n::read_notice(&response).expect("a guidance notice the screen translates");
        assert_eq!(lines.iter().map(|line|line.key.as_str()).collect::<Vec<_>>(),["guidance.failed","guidance.fix"]);
        assert_eq!(lines[0].params.get("problem"),Some(&crate::i18n::Param::Text(Text::new("guidance.nothingConfigured"))));
        assert!(!response.contains("provider named 'none'"));
    }

    /// Quem desligou o único agente foi a política da organização: a
    /// orientação diz isso, e não para ligar um agente que a política barra.
    #[tokio::test]
    async fn a_request_the_policy_leaves_without_a_model_says_whose_rule_it_is() {
        let root=tempfile::tempdir().expect("temporary repository");
        let mut orchestrator=Orchestrator::new(root.path().join("missing-config.yaml"),root.path().to_path_buf()).expect("orchestrator");
        let settings=crate::llm::LlmSettings{
            agents:vec![crate::llm::AgentSettings{id:crate::llm::AgentId::Claude,enabled:true,command:"claude".into(),timeout:300,options:serde_json::Value::Null}],
            models:vec![crate::llm::AgentModel{agent:crate::llm::AgentId::Claude,model:"sonnet".into(),enabled:true,capabilities:vec!["chat".into(),"code".into(),"reasoning".into(),"tools".into()],cost_class:"medium".into(),speed:"medium".into(),context_window:200_000}],
        };
        let policy=crate::policy::LlmPolicy{agents:Some(vec!["codex".into()]),..Default::default()};
        orchestrator.use_llm(&policy.restrict_llm(&settings));
        orchestrator.policy_scope=Some("acme".into());

        let result=orchestrator.process("Explique este projeto",Some("test"),&Pulse::silent()).await;

        let response=result.result.expect("orientação").response;
        let lines=crate::i18n::read_notice(&response).expect("aviso");
        assert_eq!(lines.iter().map(|line|line.key.as_str()).collect::<Vec<_>>(),["guidance.failed","guidance.policyFix"]);
        assert_eq!(lines[0].params.get("problem"),Some(&crate::i18n::Param::Text(Text::new("guidance.policyBlocked").with("org","acme"))));
        assert!(crate::i18n::for_model(&response).contains("@acme"));
    }
}
