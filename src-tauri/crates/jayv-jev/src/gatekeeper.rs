//! As duas portarias do Jev. O portão de entrada pontua o pedido do
//! desenvolvedor antes de qualquer modelo ser chamado; o portão de saída
//! confere cada comando ou arquivo que o modelo pediu para mexer contra as
//! regras da casa declaradas em `config.yaml`.

use crate::{config::Config, firewall::ContextFirewall, i18n::{self, Text}, jev::{self, Evaluation, Question}, turns::Turn};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use uuid::Uuid;

pub const ENTRY_QUESTION_IDS:[&str;5]=["bundles_requests","goal_is_clear","says_when_done","says_where","scope"];
pub use crate::i18n::SCOPE_LEVELS;

/// O nível de escopo, aceito também na grafia antiga em português — os checks
/// gravados antes da troca continuam com ela.
pub fn scope_level_of(scope:&str)->usize {
    match scope {"small change"|"ajuste pequeno"=>0,"feature"|"funcionalidade"=>1,_=>2}
}

/// Quanto de clareza cada tamanho de pedido exige para atravessar o portão.
pub const SCOPE_DEMAND:[f64;3]=[0.35,0.55,0.70];
/// Abaixo da exigência o portão pergunta; abaixo dela com esta folga, barra.
pub const BLOCK_MARGIN:f64=0.20;
pub const WEIGHTS:[(&str,f64);4]=[("goal_is_clear",0.40),("says_where",0.25),("says_when_done",0.20),("bundles_requests",0.15)];
/// Por quantos minutos depois de uma resposta um pedido curto do mesmo chat
/// ainda é continuação dela — e herda o veredito do pedido que ela atendeu.
pub const CONTINUATION_MINUTES:f64=30.0;
/// O teto da janela: um dia. Além disso não é continuação, é outra conversa.
const CONTINUATION_MAX_MINUTES:f64=1_440.0;

/// Os números do Jev como vão para o seed de `jev_parameters`. São também o
/// padrão quando o cache não tem um valor válido.
/// Os números da portaria. Cada um vem do cache quando lá está e é válido;
/// faltando ou torto, vale a constante do Rust.
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct JevParameters {
    pub scope_demand:[f64;3],
    pub block_margin:f64,
    pub weights:BTreeMap<String,f64>,
    pub scope_levels:[String;3],
    pub noul_line:f64,
    /// A janela, em minutos, em que um pedido curto ainda continua a
    /// resposta anterior do chat.
    pub continuation_minutes:f64,
}

impl Default for JevParameters {
    fn default()->Self { Self::from_values(&parameters()) }
}

impl JevParameters {
    pub fn from_values(values:&BTreeMap<String,Value>)->Self {
        let defaults=parameters();
        let pick=|key:&str|->Value { values.get(key).cloned().unwrap_or_else(||defaults[key].clone()) };
        let fallback=|key:&str|defaults[key].clone();
        let unit=|value:&Value|value.as_f64().filter(|number|(0.0..=1.0).contains(number));
        let demand=|value:Value|->Option<[f64;3]> { let list:Vec<f64>=serde_json::from_value(value).ok()?; let array:[f64;3]=list.try_into().ok()?; array.iter().all(|number|(0.0..=1.0).contains(number)).then_some(array) };
        let levels=|value:Value|->Option<[String;3]> { let list:Vec<String>=serde_json::from_value(value).ok()?; list.try_into().ok() };
        let weights=|value:Value|->Option<BTreeMap<String,f64>> { let map:BTreeMap<String,f64>=serde_json::from_value(value).ok()?; (!map.is_empty() && map.values().all(|weight|*weight>=0.0)).then_some(map) };
        let minutes=|value:&Value|value.as_f64().filter(|minutes|*minutes>0.0&&*minutes<=CONTINUATION_MAX_MINUTES);
        Self {
            scope_demand:demand(pick("scope_demand")).or_else(||demand(fallback("scope_demand"))).expect("default scope_demand"),
            block_margin:unit(&pick("block_margin")).or_else(||unit(&fallback("block_margin"))).expect("default block_margin"),
            weights:weights(pick("weights")).or_else(||weights(fallback("weights"))).expect("default weights"),
            scope_levels:levels(pick("scope_levels")).or_else(||levels(fallback("scope_levels"))).expect("default scope_levels"),
            noul_line:unit(&pick("noul_line")).or_else(||unit(&fallback("noul_line"))).expect("default noul_line"),
            continuation_minutes:minutes(&pick("continuation_minutes")).or_else(||minutes(&fallback("continuation_minutes"))).expect("default continuation_minutes"),
        }
    }
}

/// Os parâmetros em uso agora. O desktop os troca quando o cache é
/// atualizado; até lá valem as constantes.
static CURRENT:std::sync::RwLock<Option<JevParameters>>=std::sync::RwLock::new(None);

pub fn current_parameters()->JevParameters { CURRENT.read().unwrap_or_else(|poisoned|poisoned.into_inner()).clone().unwrap_or_default() }

pub fn set_current_parameters(parameters:JevParameters) { *CURRENT.write().unwrap_or_else(|poisoned|poisoned.into_inner())=Some(parameters); }

pub fn parameters()->BTreeMap<String,serde_json::Value> {
    BTreeMap::from([
        ("scope_demand".to_string(),json!(SCOPE_DEMAND)),
        ("block_margin".to_string(),json!(BLOCK_MARGIN)),
        ("weights".to_string(),json!(WEIGHTS.iter().map(|(id,weight)|(id.to_string(),json!(weight))).collect::<serde_json::Map<_,_>>())),
        ("scope_levels".to_string(),json!(SCOPE_LEVELS)),
        ("noul_line".to_string(),json!(crate::asking::NOUL_LINE)),
        ("continuation_minutes".to_string(),json!(CONTINUATION_MINUTES)),
    ])
}
const PROMPT_PREVIEW:usize=600;
const SHELL_LANGUAGES:[&str;7]=["bash","sh","shell","zsh","console","terminal","shell-session"];

// ─── portão de entrada ────────────────────────────────────────────────────────

#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum EntryVerdict{Pass,Ask,Block}
impl EntryVerdict {
    pub fn as_str(&self)->&'static str{match self{Self::Pass=>"pass",Self::Ask=>"ask",Self::Block=>"block"}}
    pub fn parse(value:&str)->Result<Self>{Ok(match value{"pass"=>Self::Pass,"ask"=>Self::Ask,"block"=>Self::Block,other=>return Err(anyhow!("unknown entry verdict: `{other}`"))})}
    pub fn lets_through(&self)->bool{!matches!(self,Self::Block)}
    /// A ordem dos vereditos: barrar < perguntar < passar.
    pub fn rank(&self)->u8{match self{Self::Block=>0,Self::Ask=>1,Self::Pass=>2}}
}

/// A nota do pedido liberado pela confirmação do desenvolvedor.
pub const CONFIRMED_NOTE:&str="entry.note.confirmed";
/// A origem da pergunta que a portaria faz no lugar do agente.
pub const GATE_SOURCE:&str="gate";
/// As duas saídas da confirmação; a terceira é completar o pedido em texto.
pub const SEND_AS_IS:&str="send_as_is";
pub const SEND_REWRITTEN:&str="send_rewritten";
pub const CONFIRM_OPTIONS:[&str;2]=[SEND_AS_IS,SEND_REWRITTEN];

/// O que o desenvolvedor respondeu à confirmação da portaria.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum GateChoice{AsIs,Rewritten}

/// A resposta à confirmação, como fica no chat: a escolha gravada como aviso,
/// ou o complemento que o desenvolvedor escreveu, como ele escreveu.
pub fn gate_answer(picked:&[String],text:Option<&str>)->Result<String> {
    if let Some(text)=text.map(str::trim).filter(|text|!text.is_empty()) { return Ok(text.to_string()); }
    let [choice]=picked else { return Err(Text::new("answer.single").into()) };
    if !CONFIRM_OPTIONS.contains(&choice.as_str()) { return Err(Text::new("answer.unknown").with("option",choice).into()); }
    Ok(i18n::notice(&[Text::new("gate.confirm.picked").with("choice",Text::new(&format!("gate.confirm.option.{choice}")))]))
}

/// A escolha gravada por `gate_answer`, lida da mensagem crua do turno. Texto
/// livre não é escolha: é o pedido completado, julgado de novo.
pub fn gate_choice(content:&str)->Option<GateChoice> {
    let lines=i18n::read_notice(content)?;
    let [line]=lines.as_slice() else { return None };
    if line.key!="gate.confirm.picked" { return None; }
    match line.params.get("choice") {
        Some(i18n::Param::Text(choice)) if choice.key==format!("gate.confirm.option.{SEND_AS_IS}")=>Some(GateChoice::AsIs),
        Some(i18n::Param::Text(choice)) if choice.key==format!("gate.confirm.option.{SEND_REWRITTEN}")=>Some(GateChoice::Rewritten),
        _=>None,
    }
}

/// Até quantas palavras um pedido pode ser continuação da resposta anterior.
/// O mesmo corte que a heurística usa para "ajuste pequeno".
pub const CONTINUATION_WORDS:usize=12;

/// Se o pedido curto só faz sentido junto com a resposta anterior do chat:
/// confirma ("sim", "pode implementar", "go ahead"), reclama ("não
/// funcionou", "ainda dá erro", "still fails") ou aponta para o que veio
/// antes ("corrige então", "faz isso"). Um pedido curto que nomeia um
/// arquivo ou caminho é assunto novo e não continua nada.
pub fn is_continuation(prompt:&str)->bool {
    static CUES:OnceLock<Regex>=OnceLock::new();
    let trimmed=prompt.trim();
    let words=trimmed.split_whitespace().count();
    if words==0||words>CONTINUATION_WORDS { return false; }
    let (place,..)=regexes();
    if place.find_iter(trimmed).any(|found|found.as_str().contains('/')||found.as_str().contains('.')) { return false; }
    let cues=CUES.get_or_init(||Regex::new(r"(?i)^\s*(?:sim|s|ok|okay|beleza|blz|certo|isso|claro|pode|podes|manda|segue|siga|continua|continue|prossiga|vai|bora|yes|yep|yeah|sure|go|proceed|n[ãa]o|nao|nope|ainda|still|again|de novo|outra vez|mesmo erro|same)\b|\b(?:então|entao|isso|isto|esse|essa|aquilo|assim|ele|ela|it|that|this|them|then|there|anyway|anyways|funcionou|funciona|resolveu|deu certo|deu erro|quebrou|work|works|worked|fixed|broke|broken|fails|failed|error)\b").unwrap());
    cues.is_match(trimmed)
}

/// Um critério do portão, já normalizado para a barrinha de 0 a 100% da tela.
/// `band` é a faixa tolerada; fora dela o critério é o que segura o pedido.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Criterion{pub id:String,pub label:String,pub percent:u8,pub band:Option<[u8;2]>,pub reading:String,pub inverted:bool}
impl Criterion {
    fn within_band(&self)->bool{self.band.is_none_or(|[from,to]|(from..=to).contains(&self.percent))}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct EntryCheck {
    /// O turno é a identidade do check: um pedido é pontuado uma vez, e
    /// retentá-lo reescreve esta linha em vez de criar outra.
    pub id:String,
    pub at:DateTime<Utc>,
    pub chat_id:String,
    /// O código que o desenvolvedor lê nos dois lados, `XY4T9B·04`.
    pub turn:String,
    pub prompt:String,
    pub score:u8,
    pub demand:u8,
    pub verdict:EntryVerdict,
    pub scope:String,
    pub criteria:Vec<Criterion>,
    pub source:String,
    pub note:String,
}

impl EntryCheck {
    /// Os critérios que ficaram fora da faixa tolerada, na ordem em que pesam.
    pub fn failing(&self)->Vec<&Criterion>{self.criteria.iter().filter(|criterion|!criterion.within_band()).collect()}
    /// A resposta que a portaria devolve quando barra o pedido, gravada como
    /// aviso: cada um a lê no seu idioma.
    pub fn reply(&self)->String {
        let level=scope_level_of(&self.scope);
        let mut lines=vec![
            Text::new("gate.blocked").with("score",self.score).with("demand",self.demand).with("scope",Text::new(&format!("scope.{level}"))),
            Text::new("gate.missing"),
        ];
        lines.extend(self.failing().iter().map(|criterion|Text::new("gate.missing.item").with("criterion",Text::new(&format!("criterion.{}",criterion.id))).with("reading",Text::new(&format!("criterion.{}.out",criterion.id)))));
        lines.push(Text::new(&format!("gate.asks.{level}")));
        i18n::notice(&lines)
    }
    /// A resposta a uma pergunta do agente herda a passagem do pedido que a
    /// originou. O agente só pergunta depois de a portaria ter liberado esse
    /// pedido, e barrar a resposta é barrar o fluxo que ela própria deixou
    /// abrir — "ainda não, aviso quando sair" não é um pedido novo, é a
    /// conversa continuando. A nota e os critérios continuam os de agora:
    /// muda só o desfecho.
    ///
    /// Vale também para a continuação digitada ("pode implementar", "não
    /// funcionou") logo depois de uma resposta: o veredito sobe para o do
    /// pedido de origem quando ele é melhor — de barrado ou de ressalva para
    /// o que a origem teve —, e nunca desce.
    pub fn inherit(mut self,origin:EntryVerdict)->Self {
        if origin.rank()>self.verdict.rank() {
            self.verdict=origin;
            self.note=format!("entry.note.{}",origin.as_str());
        }
        self
    }
    /// O pedido que o desenvolvedor confirmou depois de a portaria perguntar:
    /// vale como liberado de vez, com os critérios que ela leu.
    pub fn confirmed(mut self)->Self {
        self.verdict=EntryVerdict::Pass;
        self.note=CONFIRMED_NOTE.into();
        self
    }
    /// O que a tela mostra quando a portaria pergunta antes de chamar o
    /// agente: a nota, o que faltou e a escolha. Gravado como aviso.
    pub fn confirmation(&self)->String {
        let level=scope_level_of(&self.scope);
        let mut lines=vec![
            Text::new("gate.confirm").with("score",self.score).with("demand",self.demand).with("scope",Text::new(&format!("scope.{level}"))),
            Text::new("gate.missing"),
        ];
        lines.extend(self.failing().iter().map(|criterion|Text::new("gate.missing.item").with("criterion",Text::new(&format!("criterion.{}",criterion.id))).with("reading",Text::new(&format!("criterion.{}.out",criterion.id)))));
        lines.push(Text::new("gate.confirm.choose"));
        i18n::notice(&lines)
    }
    /// A instrução que acompanha um pedido liberado com ressalva.
    pub fn clarifying_note(&self)->Option<String> {
        if self.verdict!=EntryVerdict::Ask {return None;}
        let gaps=self.failing().iter().map(|criterion|criterion.id.replace('_'," ")).collect::<Vec<_>>().join(", ");
        Some(format!("The JayV entry gate let this request through with reservations ({gaps}). Before starting the work, ask the developer one objective question about the most critical point, and do not assume an approach until it is answered."))
    }
    /// O pedido liberado, reescrito para o modelo com o que a portaria leu nele.
    /// O texto do desenvolvedor vai intacto no topo; embaixo, a leitura de cada
    /// critério vira uma instrução de conduta — o que o pedido já disse é
    /// cobrado como compromisso, o que faltou vira tarefa do modelo. Barrado não
    /// chega a modelo nenhum, então não tem versão melhorada.
    pub fn refined_prompt(&self,request:&str)->Option<String> {
        if !self.verdict.lets_through() {return None;}
        let met=|id:&str|self.criteria.iter().find(|criterion|criterion.id==id).is_some_and(Criterion::within_band);
        let mut steps=vec![match scope_level_of(&self.scope){
            0=>"This is a small adjustment: make the smallest change that satisfies it and leave everything else untouched.",
            1=>"This is one capability delivered end to end: plan the few files it needs, implement them, and keep unrelated code as it is.",
            _=>"This is system-wide work: outline the plan and the parts it touches before changing anything, then deliver it in reviewable steps.",
        }.to_string()];
        steps.push(if met("goal_is_clear"){"Treat the outcome the request states as the goal; do not widen it."}else{"The goal is not explicit: state in one sentence the outcome you are going to deliver before you start."}.into());
        steps.push(if met("says_where"){"Work in the places the request names; if something outside them must change, say which file and why."}else{"No location was given: find where this belongs in the project first and list the files you will touch."}.into());
        steps.push(if met("says_when_done"){"Use the request's own done criterion as your final check and report how it was verified."}else{"No done criterion was given: end by saying how the developer can confirm the work is finished (a test, a command or an observable behaviour)."}.into());
        if !met("bundles_requests") {steps.push("The request bundles independent items: handle them one at a time, in the order given, and report the outcome of each separately.".into());}
        steps.push("Preserve behaviour the request does not mention.".into());
        let steps=steps.iter().map(|step|format!("- {step}")).collect::<Vec<_>>().join("\n");
        Some(format!("REQUEST (verbatim from the developer):\n{}\n\nHOW TO CARRY IT OUT (JayV entry gate: {}/100, scope \"{}\"):\n{steps}",request.trim(),self.score,self.scope))
    }
}

/// As cinco leituras que o portão de entrada precisa, vindas do Jev ou das
/// heurísticas locais.
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct EntryReading{pub scope_score:f64,pub goal_is_clear:f64,pub says_where:f64,pub says_when_done:f64,pub bundles_requests:f64}

impl EntryReading {
    pub fn scope_level(&self)->usize{let score=if self.scope_score.is_finite(){self.scope_score}else{0.0};if score<0.67{0}else if score<1.34{1}else{2}}
    pub fn clarity(&self)->f64 { self.clarity_with(&current_parameters().weights) }
    /// Um peso cujo critério esta versão não conhece não conta.
    pub fn clarity_with(&self,weights:&BTreeMap<String,f64>)->f64 {
        weights.iter().map(|(id,weight)|weight*match id.as_str() {
            "goal_is_clear"=>self.goal_is_clear,
            "says_where"=>self.says_where,
            "says_when_done"=>self.says_when_done,
            "bundles_requests"=>1.0-self.bundles_requests,
            _=>0.0,
        }.clamp(0.0,1.0)).sum()
    }
    pub fn from_evaluation(evaluation:&Evaluation)->Result<Self> {
        let noul=|id:&str|->Result<f64>{let answer=evaluation.answer(id).ok_or_else(||anyhow!("the Jev did not return the `{id}` answer"))?;answer.as_noul().ok_or_else(||anyhow!("the Jev returned `{id}` as {} instead of `noul`",answer.kind()))};
        let scope=evaluation.answer("scope").ok_or_else(||anyhow!("the Jev did not return the `scope` answer"))?;
        Ok(Self{
            scope_score:scope.as_score().ok_or_else(||anyhow!("the Jev returned `scope` as {} instead of `score`",scope.kind()))?,
            goal_is_clear:noul("goal_is_clear")?,
            says_where:noul("says_where")?,
            says_when_done:noul("says_when_done")?,
            bundles_requests:noul("bundles_requests")?,
        })
    }
}

fn percent(value:f64)->u8{(value.clamp(0.0,1.0)*100.0).round() as u8}

fn criterion(id:&str,value:f64,demand:f64,inverted:bool)->Criterion {
    let band=if inverted{[0,percent(1.0-demand)]}else{[percent(demand),100]};
    let reached=percent(value);
    let within=(band[0]..=band[1]).contains(&reached);
    Criterion{id:id.into(),label:format!("criterion.{id}"),percent:reached,band:Some(band),reading:format!("criterion.{id}.{}",if within{"in"}else{"out"}),inverted}
}

/// Passa, pergunta ou bloqueia: a clareza contra a exigência do tamanho, com
/// os números do cache.
pub fn verdict_with(reading:&EntryReading,parameters:&JevParameters)->EntryVerdict {
    let demand=parameters.scope_demand[reading.scope_level()];
    let clarity=reading.clarity_with(&parameters.weights);
    if clarity+parameters.block_margin<demand{EntryVerdict::Block}else if clarity<demand{EntryVerdict::Ask}else{EntryVerdict::Pass}
}

/// Monta o veredito a partir das leituras, aplicando a exigência do tamanho.
pub fn judge(turn:&Turn,prompt:&str,reading:&EntryReading,source:&str)->EntryCheck { judge_for(turn,prompt,reading,source,crate::expertise::Expertise::default()) }

/// O mesmo, com a exigência do nível de quem pediu.
pub fn judge_for(turn:&Turn,prompt:&str,reading:&EntryReading,source:&str,level:crate::expertise::Expertise)->EntryCheck {
    let parameters=level.gate(&current_parameters());
    let level=reading.scope_level();
    let demand=parameters.scope_demand[level];
    let clarity=reading.clarity_with(&parameters.weights);
    let verdict=verdict_with(reading,&parameters);
    let scope=parameters.scope_levels[level].as_str();
    let criteria=vec![
        Criterion{id:"scope".into(),label:"criterion.scope".into(),percent:percent(reading.scope_score/2.0),band:None,reading:scope.into(),inverted:false},
        criterion("goal_is_clear",reading.goal_is_clear,demand,false),
        criterion("says_where",reading.says_where,demand,false),
        criterion("says_when_done",reading.says_when_done,demand,false),
        criterion("bundles_requests",reading.bundles_requests,demand,true),
    ];
    // Nome, leitura e nota vão como chaves do i18n: a tela os diz no idioma de
    // quem lê.
    let note=format!("entry.note.{}",verdict.as_str());
    EntryCheck{id:turn.id.clone(),at:Utc::now(),chat_id:turn.chat_id.clone(),turn:turn.code.clone(),prompt:preview(prompt),score:percent(clarity),demand:percent(demand),verdict,scope:scope.into(),criteria,source:source.into(),note}
}

fn preview(prompt:&str)->String {
    let trimmed=prompt.trim();
    if trimmed.chars().count()<=PROMPT_PREVIEW{return trimmed.into();}
    format!("{}…",trimmed.chars().take(PROMPT_PREVIEW).collect::<String>())
}

/// O estado que o Jev avalia na entrada. `recent_turns` são as últimas falas
/// do chat — as mesmas que o roteamento recebe —, para "pode implementar" ser
/// julgado contra o plano que veio antes. Vazio, a chave nem vai.
pub fn entry_state(prompt:&str,project:&str,languages:&[String],recent_turns:&[String])->serde_json::Value{
    let mut state=json!({"user_request":prompt,"project":{"name":project,"languages":languages}});
    if !recent_turns.is_empty() { state["recent_turns"]=json!(recent_turns); }
    state
}

pub fn entry_questions()->BTreeMap<String,Question> {
    BTreeMap::from([
        ("scope".to_string(),Question::score(
            json!({
                "question":"How much of this codebase does `user_request` ask to be changed or produced?",
                "focus":"Judge the size of the work the request describes, not how long or polite the sentence is. One terse line can ask for a whole system, and a long paragraph can ask for a rename.",
                "background":"`project` names the repository and the languages it is written in."
            }),
            [
                json!({"what":"A small correction inside code that already exists and already works: a rename, a typo, a changed default, one added guard, one adjusted style rule, or a question answered in a line.","examples":["Fix the typo in this log message","Rename `check` to `validate` everywhere","Increase the timeout to 60 seconds"]}),
                json!({"what":"One capability delivered end to end: a new screen, endpoint, command, adapter or test suite, or a defect whose repair touches a handful of files that already exist.","examples":["Add a --json flag to the CLI","Create the paginated table component","Find and fix why the cache returns stale results"]}),
                json!({"what":"A whole system or a change that reaches across the codebase: a new subsystem, a migration, a redesign of how existing parts interact, or work whose extent cannot be stated without exploring the repository first.","examples":["Migrate persistence from JSON files to SQLite","Build the multi-agent orchestration layer","Rewrite the app so every provider streams"]}),
            ])),
        ("goal_is_clear".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` state what the developer wants to be true once the work is finished?","guidance":"Look for the intended outcome, not for politeness or detail. A request can be short and still name its outcome exactly.","conversation":"`recent_turns`, when present, holds the last messages of this conversation. A short follow-up such as \"go ahead\", \"implement it\" or \"it still fails\" takes its outcome from them: judge the request together with what they already established."}),
            json!({"when":"The outcome is stated: the request names the behaviour, artefact or answer it expects to exist afterwards.","examples":["asks for a named capability, file or fix","states the problem to be gone and what working looks like","asks a question whose answer would settle a decision"]}),
            json!({"when":"The outcome has to be guessed.","examples":["\"fix this\", \"improve it here\", \"make it faster\" with nothing to anchor them","names a topic without saying what should change about it","several possible goals with no sign of which one is meant"]}))),
        ("says_where".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` say where in the project the work belongs?","guidance":"A location can be a path, a file, a module, a function, a screen, a layer or a named subsystem. Judge whether someone who knows this project could open the right place without guessing.","conversation":"`recent_turns`, when present, holds the last messages of this conversation. A place named there, or the plan they already laid out, still locates a short follow-up that refers back to it."}),
            json!({"when":"The request points at a place: a path or filename, a named symbol, a module, a screen, a route, or a layer of the system."}),
            json!({"when":"No place is given and the request is not self-locating.","examples":["a change described only by its effect, in a project with many plausible homes for it","\"in the system\", \"in the code\", \"somewhere in the backend\""],"not_a_defect":["a general question that does not touch this project at all"]}))),
        ("says_when_done".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` say how the developer will confirm the work is done?","guidance":"Look for something checkable: a test that should pass, a command whose output is stated, an input and its expected output, an acceptance condition, or a described end state precise enough to compare against."}),
            json!({"when":"A check is stated or clearly implied by a precise end state.","examples":["names a test, a command or an output that must appear","gives an input and the expected result","describes the finished behaviour precisely enough to verify"]}),
            json!({"when":"There is no way to tell the work apart from unfinished work.","examples":["only an adjective as the target: \"better\", \"cleaner\", \"faster\", with no measure","asks for a change with no stated effect to look for"]}))),
        ("bundles_requests".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` bundle work that would be better asked for separately?","guidance":"Judge whether the parts share one outcome. Several files, several steps or a long description of one coherent job is a single request. Separate goals that merely arrived in the same message are a bundle."}),
            json!({"when":"Two or more independent goals are asked for at once, each of which could be delivered, reviewed and verified on its own.","examples":["a refactor plus an unrelated new feature plus a dependency upgrade","\"and while you are at it, also...\"","a list of unrelated defects"]}),
            json!({"when":"Everything asked for serves one outcome, however many files or steps that takes.","also":"Necessary supporting work — the test for the feature, the migration the change needs — belongs to the same request."}))),
    ])
}

pub async fn evaluate_entry(prompt:&str,project:&str,languages:&[String],recent_turns:&[String])->Result<EntryReading> {
    let evaluation=jev::evaluate("entry",entry_state(prompt,project,languages,recent_turns),None).await?;
    EntryReading::from_evaluation(&evaluation)
}

// ─── heurísticas locais, para quando o Jev não está configurado ───────────────

fn regexes()->&'static (Regex,Regex,Regex,Regex,Regex) {
    static COMPILED:OnceLock<(Regex,Regex,Regex,Regex,Regex)>=OnceLock::new();
    COMPILED.get_or_init(||(
        Regex::new(r"(?:^|[\s`(])(?:[\w.-]+/)+[\w.-]+|\b[\w-]+\.(?:rs|js|ts|tsx|jsx|css|html|json|yaml|yml|toml|py|go|java|sql|md|sh)\b|\bsrc-tauri\b|\b(?:módulo|modulo|module|função|funcao|function|componente|component|tela|screen|endpoint|rota|route|arquivo|file|classe|class|struct|tabela|table)\b").unwrap(),
        Regex::new(r"(?i)\b(?:pronto quando|feito quando|done when|critério|criterio|aceitação|aceitacao|acceptance|deve retornar|deve responder|should return|espero que|espera-se|até que|ate que|sem erro|passe[m]? nos? teste|testes? (?:passa|verde)|cargo test|npm test|npm run|exit code|retorne|output|saída esperada|saida esperada)\b").unwrap(),
        Regex::new(r"(?i)^\s*(?:por favor\s+)?(?:me\s+)?(?:adicione|adiciona|crie|cria|implemente|implementa|escreva|escreve|corrija|corrige|conserte|conserta|refatore|refatora|renomeie|renomeia|remova|remove|extraia|extrai|explique|explica|revise|revisa|analise|analisa|migre|migra|atualize|atualiza|configure|configura|ajuste|ajusta|mostre|mostra|documente|documenta|teste|testa|otimize|otimiza|add|create|implement|write|fix|refactor|rename|remove|extract|explain|review|analyse|analyze|migrate|update|configure|adjust|show|document|test|optimi[sz]e)\b").unwrap(),
        Regex::new(r"(?i)\b(?:e também|e tambem|além disso|alem disso|de quebra|já que|ja que|aproveitando|and also|also,|plus,|on top of that)\b|(?:^|\n)\s*(?:\d+[.)]|[-*])\s+").unwrap(),
        Regex::new(r"(?i)\b(?:isso|aquilo|isto|essa coisa|esse negócio|esse negocio|tudo|geral|melhor|melhora|mais limpo|mais rápido|mais rapido|arruma|arrumar|dá um jeito|da um jeito|that|this thing|everything|better|cleaner|faster|clean ?up)\b").unwrap(),
    ))
}

/// Os critérios da portaria que uma nota do projeto pode cobrir: dizem do
/// projeto, não de um pedido só.
pub const LEARNABLE_CRITERIA:[&str;2]=["says_when_done","says_where"];

/// A frase do pedido que atende a um critério aprendível — onde fica, como
/// conferir —, se houver uma. É o que vira nota do projeto quando o
/// desenvolvedor a escreve depois de a portaria ter cobrado.
pub fn evidence(prompt:&str,criterion:&str)->Option<String> {
    static SENTENCES:OnceLock<Regex>=OnceLock::new();
    let (place,done,..)=regexes();
    let pattern=match criterion {"says_where"=>place,"says_when_done"=>done,_=>return None};
    let sentences=SENTENCES.get_or_init(||Regex::new(r"[.!?](?:\s+|$)|\n").unwrap());
    sentences.split(prompt).map(str::trim).find(|sentence|sentence.split_whitespace().count()>=2&&pattern.is_match(sentence)).map(|sentence|sentence.chars().take(EVIDENCE_CHARS).collect())
}
const EVIDENCE_CHARS:usize=300;

/// A leitura com o que as notas do projeto já respondem: o critério coberto
/// conta como dito, e a portaria não cobra de novo o que o projeto ensinou.
pub fn with_notes(mut reading:EntryReading,covered:&[String])->EntryReading {
    const COVERED:f64=0.82;
    for criterion in covered {
        match criterion.as_str() {
            "says_when_done"=>reading.says_when_done=reading.says_when_done.max(COVERED),
            "says_where"=>reading.says_where=reading.says_where.max(COVERED),
            _=>{}
        }
    }
    reading
}

/// Uma leitura só com o texto do pedido, para quando não há sessão
/// está definida. Deliberadamente generosa: a portaria local não deve barrar
/// mais que o Jev.
pub fn heuristic_entry(prompt:&str)->EntryReading {
    let (place,done,action,bundle,vague)=regexes();
    let trimmed=prompt.trim();
    let words=trimmed.split_whitespace().count();
    let scope_score=match words {0..=12=>0.3,13..=45=>1.0,_=>1.7};
    let places=place.find_iter(trimmed).count().min(3) as f64;
    let vagueness=vague.find_iter(trimmed).count().min(3) as f64;
    let bundles=bundle.find_iter(trimmed).count();
    EntryReading {
        scope_score,
        goal_is_clear:(0.30+if action.is_match(trimmed){0.35}else{0.0}+(words.min(24) as f64/24.0)*0.35-vagueness*0.18).clamp(0.05,0.95),
        says_where:(0.15+places*0.27).clamp(0.05,0.95),
        says_when_done:if done.is_match(trimmed){0.82}else{0.18},
        bundles_requests:(bundles as f64*0.3).clamp(0.05,0.95),
    }
}

// ─── portão de saída ─────────────────────────────────────────────────────────

#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ExitVerdict{Cleared,Held}
impl ExitVerdict {
    pub fn as_str(&self)->&'static str{match self{Self::Cleared=>"cleared",Self::Held=>"held"}}
    pub fn parse(value:&str)->Result<Self>{Ok(match value{"cleared"=>Self::Cleared,"held"=>Self::Held,other=>return Err(anyhow!("unknown exit verdict: `{other}`"))})}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ExitCheck {
    pub id:String,
    pub at:DateTime<Utc>,
    pub chat_id:String,
    pub turn_id:String,
    /// O pedido que originou esta saída, como o desenvolvedor o lê.
    pub turn:String,
    /// `command` ou `file`; a tela traduz. Checks antigos guardam `comando` e
    /// `arquivo`, e a tela também os reconhece.
    pub kind:String,
    pub target:String,
    pub rule:Option<String>,
    pub verdict:ExitVerdict,
}

impl ExitCheck {
    pub(crate) fn new(turn:&Turn,kind:&str,target:&str,rule:Option<String>)->Self {
        Self{id:Uuid::new_v4().to_string(),at:Utc::now(),chat_id:turn.chat_id.clone(),turn_id:turn.id.clone(),turn:turn.code.clone(),kind:kind.into(),target:target.into(),rule:rule.clone(),verdict:if rule.is_some(){ExitVerdict::Held}else{ExitVerdict::Cleared}}
    }
}

/// A regra que segura um comando, se alguma segurar.
pub fn command_rule(config:&Config,line:&str)->Option<String> {
    let program=line.split_whitespace().next().unwrap_or_default();
    if program.is_empty(){return None;}
    match config.permissions.shell.as_str() {
        "allow"=>None,
        other=>Some(format!("permissions.shell · {other}")),
    }
}

/// O que a resposta faz com um arquivo: entregar o conteúdo dele é escrever;
/// citar o caminho é só ler.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Access{Read,Write}

/// A regra que segura um arquivo, se alguma segurar.
pub fn file_rule(config:&Config,firewall:&ContextFirewall,root:&Path,path:&str,access:Access)->Option<String> {
    let info=firewall.check_file(path);
    if let Some(rule)=info.matched_rule {return Some(rule);}
    if escapes_root(root,path){return Some("workspace.root · outside".into());}
    let (name,permission)=match access {Access::Read=>("read",&config.permissions.read),Access::Write=>("write",&config.permissions.write)};
    match permission.as_str() {
        "allow"=>None,
        other=>Some(format!("permissions.{name} · {other}")),
    }
}

fn escapes_root(root:&Path,path:&str)->bool {
    let candidate=Path::new(path);
    if candidate.components().any(|component|component.as_os_str()=="..") {return true;}
    candidate.is_absolute() && !candidate.starts_with(root)
}

/// As pastas onde cada agente guarda o próprio estado, na home do usuário.
const AGENT_HOMES:[&str;6]=[".claude",".codex",".cursor",".copilot",".agent",".agents"];

/// Um arquivo que o próprio agente grava para si — o plano do modo
/// planejamento do Claude em `~/.claude/plans/`, a sessão do Codex em
/// `~/.codex/` — e cita na resposta. Não é mudança no projeto nem pedido ao
/// desenvolvedor: segurá-lo como "fora do projeto" só enchia a portaria. O
/// `.claude/` de dentro do projeto continua sob as regras da casa.
fn agent_state(path:&str,home:Option<&Path>)->bool {
    let inside=path.strip_prefix("~/").map(Path::new)
        .or_else(||home.and_then(|home|Path::new(path).strip_prefix(home).ok()));
    inside.and_then(|inside|inside.components().next()).is_some_and(|first|AGENT_HOMES.iter().any(|name|first.as_os_str()==*name))
}

/// Lê a resposta do modelo e devolve tudo que ele pediu para rodar ou mexer,
/// já confrontado com as regras da casa.
pub fn scan_answer(turn:&Turn,answer:&str,config:&Config,firewall:&ContextFirewall,root:&Path)->Vec<ExitCheck> {
    let mut checks=Vec::new();
    let home=dirs::home_dir();
    let mut seen=Vec::new();
    for line in shell_lines(answer) {
        if seen.contains(&line){continue;}
        seen.push(line.clone());
        checks.push(ExitCheck::new(turn,"command",&line,command_rule(config,&line)));
    }
    for path in written_paths(answer) {
        if seen.contains(&path)||agent_state(&path,home.as_deref()){continue;}
        seen.push(path.clone());
        checks.push(ExitCheck::new(turn,"file",&path,file_rule(config,firewall,root,&path,Access::Write)));
    }
    // Um caminho só citado não sai do projeto: entra no feed apenas quando
    // bate numa regra — um `.env` lembrado na resposta continua segurado.
    for path in cited_paths(answer) {
        if seen.contains(&path)||agent_state(&path,home.as_deref()){continue;}
        seen.push(path.clone());
        if let Some(rule)=file_rule(config,firewall,root,&path,Access::Read) {checks.push(ExitCheck::new(turn,"file",&path,Some(rule)));}
    }
    checks
}

/// As linhas executáveis dos blocos de shell da resposta.
pub fn shell_lines(answer:&str)->Vec<String> {
    let mut lines=Vec::new();
    let mut language:Option<String>=None;
    for raw in answer.lines() {
        if let Some(rest)=raw.trim_start().strip_prefix("```") {
            language=match language {Some(_)=>None,None=>Some(rest.trim().to_lowercase())};
            continue;
        }
        let Some(current)=language.as_deref() else {continue};
        if !SHELL_LANGUAGES.contains(&current){continue;}
        let line=raw.trim();
        if line.is_empty()||line.starts_with('#')||line.starts_with("//"){continue;}
        let line=line.trim_start_matches("$ ").trim();
        lines.push(line.to_string());
    }
    lines
}

/// Os arquivos cujo conteúdo a resposta entrega, em `FILE:` ou como rótulo de
/// um bloco de código: é o que o modelo pede para escrever.
pub fn written_paths(answer:&str)->Vec<String> {
    static MARKER:OnceLock<Regex>=OnceLock::new();
    let marker=MARKER.get_or_init(||Regex::new(r"(?im)^\s*(?:FILE:\s*|```[a-z]*\s+)([\w./-]+)\s*$").unwrap());
    collect_paths(marker.captures_iter(answer).map(|capture|capture.get(1).map_or("",|path|path.as_str())))
}

/// Os caminhos que a resposta só cita em `código`.
pub fn cited_paths(answer:&str)->Vec<String> {
    static INLINE:OnceLock<Regex>=OnceLock::new();
    let inline=INLINE.get_or_init(||Regex::new(r"`([^`\n]{1,160})`").unwrap());
    collect_paths(inline.captures_iter(answer).map(|capture|capture.get(1).map_or("",|path|path.as_str())))
}

fn collect_paths<'a>(candidates:impl Iterator<Item=&'a str>)->Vec<String> {
    let mut paths:Vec<String>=Vec::new();
    for candidate in candidates {
        let candidate=candidate.trim().trim_start_matches("./");
        if looks_like_path(candidate)&&!paths.iter().any(|kept|kept==candidate){paths.push(candidate.to_string());}
    }
    paths
}

fn looks_like_path(candidate:&str)->bool {
    const EXTENSIONS:[&str;22]=["rs","js","mjs","ts","tsx","jsx","css","html","json","yaml","yml","toml","py","go","java","sql","md","sh","lock","env","pem","key"];
    if candidate.is_empty()||candidate.len()>160||candidate.contains(char::is_whitespace){return false;}
    if candidate.contains("::")||candidate.contains('(')||candidate.ends_with('/'){return false;}
    let named=candidate.rsplit('/').next().unwrap_or(candidate);
    let dotted=named.starts_with('.')&&named.len()>1&&!named[1..].contains('.');
    dotted||named.rsplit_once('.').is_some_and(|(stem,extension)|!stem.is_empty()&&EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

// ─── o feed e o placar ───────────────────────────────────────────────────────

#[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Tally{pub passed:u32,pub asked:u32,pub blocked:u32,pub held:u32}

#[derive(Debug,Clone,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct GateFeed{pub entries:Vec<EntryCheck>,pub exits:Vec<ExitCheck>,pub tally:Tally}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn without_cached_parameters_the_constants_apply() {
        let cache=crate::local::global::GlobalCache::in_memory().expect("cache");
        let parameters=JevParameters::from_values(&cache.jev_parameter_values().expect("parâmetros"));
        assert_eq!(parameters.scope_demand,SCOPE_DEMAND);
        assert_eq!(parameters.block_margin,BLOCK_MARGIN);
        assert_eq!(parameters.noul_line,crate::asking::NOUL_LINE);
        assert_eq!(parameters.weights.len(),WEIGHTS.len());
    }

    /// Um valor torto vindo do painel não derruba a portaria: só ele volta ao
    /// padrão, os outros valem.
    #[test] fn a_valid_value_replaces_and_a_malformed_one_falls_back_to_default() {
        let mut cache=crate::local::global::GlobalCache::in_memory().expect("cache");
        cache.save_jev_parameters(&BTreeMap::from([
            ("block_margin".to_string(),json!(0.3)),
            ("scope_demand".to_string(),json!([0.5,2.0])),
            ("noul_line".to_string(),json!("alto")),
        ])).expect("grava");
        let parameters=JevParameters::from_values(&cache.jev_parameter_values().expect("parâmetros"));
        assert_eq!(parameters.block_margin,0.3);
        assert_eq!(parameters.scope_demand,SCOPE_DEMAND);
        assert_eq!(parameters.noul_line,crate::asking::NOUL_LINE);
    }


    /// Os níveis são identificadores em inglês; os checks antigos, gravados em
    /// português, continuam lidos no nível certo.
    #[test]
    fn the_scope_is_english_and_accepts_the_legacy_spelling() {
        assert_eq!(SCOPE_LEVELS,["small change","feature","whole system"]);
        for (level,(english,portuguese)) in SCOPE_LEVELS.iter().zip(["ajuste pequeno","funcionalidade","sistema inteiro"]).enumerate() {
            assert_eq!((scope_level_of(english),scope_level_of(portuguese)),(level,level));
        }
    }

    /// Os números do painel valem: uma margem maior bloqueia o que a padrão
    /// só deixaria passar com ressalva.
    #[test]
    fn cached_parameters_change_the_verdict() {
        let reading=EntryReading{scope_score:0.0,goal_is_clear:0.0,says_where:0.0,says_when_done:0.0,bundles_requests:0.6};
        let defaults=JevParameters::default();
        assert_eq!(verdict_with(&reading,&defaults),EntryVerdict::Block);
        let mut lenient=defaults.clone();
        lenient.block_margin=0.5;
        assert_eq!(verdict_with(&reading,&lenient),EntryVerdict::Ask);
        lenient.weights=std::collections::BTreeMap::from([("bundles_requests".to_string(),1.0)]);
        assert_eq!(verdict_with(&reading,&lenient),EntryVerdict::Pass,"só o peso que veio do cache conta");
    }
    use crate::{config::{Config,PermissionsConfig,PrivacyConfig},turns::TurnStatus};

    /// Um turno de mentira, do tamanho que o portão precisa: ele só lê o id, o
    /// chat e o código.
    fn turn_at(chat:&str)->Turn{Turn{id:format!("turno-de-{chat}"),chat_id:chat.into(),code:format!("{}·01",chat.to_uppercase()),ordinal:1,status:TurnStatus::Flying,created_at:Utc::now()}}

    fn reading(scope:f64,goal:f64,where_:f64,done:f64,bundles:f64)->EntryReading{EntryReading{scope_score:scope,goal_is_clear:goal,says_where:where_,says_when_done:done,bundles_requests:bundles}}
    fn firewall()->ContextFirewall{ContextFirewall::new(PrivacyConfig::default())}
    fn config(shell:&str,write:&str)->Config{Config{permissions:PermissionsConfig{read:"allow".into(),search:"allow".into(),write:write.into(),shell:shell.into()},..Default::default()}}

    #[test]
    fn the_entry_questions_match_the_five_criteria_the_screen_shows() {
        let questions=entry_questions();
        let mut ids=questions.keys().map(String::as_str).collect::<Vec<_>>(); ids.sort();
        assert_eq!(ids,ENTRY_QUESTION_IDS.to_vec());
        for (id,question) in &questions {question.validate().unwrap_or_else(|error|panic!("{id}: {error}"));}
        let scope=questions.get("scope").expect("scope");
        assert_eq!(scope.kind(),"score");
        assert_eq!(scope.levels(),SCOPE_LEVELS.len());
        for id in ["goal_is_clear","says_where","says_when_done","bundles_requests"] {assert_eq!(questions[id].kind(),"noul","{id}");}
        let wire=serde_json::to_string(&questions).expect("json");
        assert!(wire.contains("`user_request`") && wire.contains("`project`"));
        assert!(entry_state("pedido","JayV",&["Rust".into()],&[]).pointer("/project/name").is_some());
        assert!(entry_state("pedido","JayV",&[],&[]).get("recent_turns").is_none(),"sem conversa, a chave nem vai");
        let turns=vec!["user: planeje o cache".to_string(),"assistant: 1. criar o cache".to_string()];
        assert_eq!(entry_state("pode implementar","JayV",&[],&turns)["recent_turns"],json!(turns));
        for id in ["goal_is_clear","says_where"] {assert!(serde_json::to_string(&questions[id]).expect("json").contains("`recent_turns`"),"{id} julga a continuação contra a conversa");}
    }

    /// As continuações típicas de quem já está numa conversa são reconhecidas;
    /// pedido novo, pedido longo e pedido que nomeia um arquivo não.
    #[test]
    fn a_short_follow_up_is_read_as_a_continuation() {
        for follow in ["sim","pode implementar","não funcionou","Não funcionou, dá erro ao salvar","corrige então","faz isso","ainda dá erro","go ahead","it still fails","ok, segue","implementa isso"] {
            assert!(is_continuation(follow),"{follow}");
        }
        for fresh in ["faz o login","cria uma tela de login com email e senha","corrige o bug em src/lib.rs então","explica o roteamento","",
            "adicione validação de email no formulário e depois escreva os testes do login com todos os casos de erro"] {
            assert!(!is_continuation(fresh),"{fresh}");
        }
    }

    /// A continuação sobe para o veredito melhor da origem e nunca desce.
    #[test]
    fn the_inherited_verdict_only_ever_goes_up() {
        let asked=judge(&turn_at("c"),"pode implementar",&heuristic_entry("pode implementar"),"local");
        assert_eq!(asked.verdict,EntryVerdict::Ask,"sozinho, \"pode implementar\" pergunta: {}",asked.score);
        let followed=asked.clone().inherit(EntryVerdict::Pass);
        assert_eq!((followed.verdict,followed.note.as_str()),(EntryVerdict::Pass,"entry.note.pass"));
        assert_eq!((followed.score,&followed.criteria),(asked.score,&asked.criteria),"o que a portaria leu fica");
        assert_eq!(asked.clone().inherit(EntryVerdict::Block).verdict,EntryVerdict::Ask,"herdar não piora");
        let confirmed=asked.confirmed();
        assert_eq!((confirmed.verdict,confirmed.note.as_str()),(EntryVerdict::Pass,CONFIRMED_NOTE));
    }

    /// A confirmação no lugar do agente: a pergunta lista o que faltou, e a
    /// resposta volta como escolha ou como o pedido completado.
    #[test]
    fn the_gate_asks_and_reads_back_the_choice() {
        let asked=judge(&turn_at("c"),"melhora o desempenho do app",&heuristic_entry("melhora o desempenho do app"),"local");
        assert_eq!(asked.verdict,EntryVerdict::Ask);
        let lines=i18n::read_notice(&asked.confirmation()).expect("aviso");
        assert_eq!(lines.first().map(|line|line.key.as_str()),Some("gate.confirm"));
        assert_eq!(lines.last().map(|line|line.key.as_str()),Some("gate.confirm.choose"));
        assert!(lines.iter().any(|line|line.key=="gate.missing.item"),"diz o que faltou");
        assert!(i18n::for_model(&asked.confirmation()).contains("held this request before calling any agent"));

        let as_is=gate_answer(&[SEND_AS_IS.into()],None).expect("escolha");
        let rewritten=gate_answer(&[SEND_REWRITTEN.into()],None).expect("escolha");
        assert_eq!((gate_choice(&as_is),gate_choice(&rewritten)),(Some(GateChoice::AsIs),Some(GateChoice::Rewritten)));
        assert_eq!(i18n::for_model(&as_is),"Send it as written.");
        let completed=gate_answer(&[],Some("  no arquivo src/app.rs, até o teste passar  ")).expect("texto");
        assert_eq!((completed.as_str(),gate_choice(&completed)),("no arquivo src/app.rs, até o teste passar",None),"texto livre é o pedido completado");
        assert!(gate_answer(&["outra".into()],None).is_err());
        assert!(gate_answer(&[],None).is_err());
    }

    #[test]
    fn the_continuation_window_comes_from_the_cache_within_a_day() {
        assert_eq!(JevParameters::default().continuation_minutes,CONTINUATION_MINUTES);
        let read=|value:Value|JevParameters::from_values(&BTreeMap::from([("continuation_minutes".to_string(),value)])).continuation_minutes;
        assert_eq!(read(json!(45)),45.0);
        assert_eq!(read(json!(0)),CONTINUATION_MINUTES,"zero desligaria a continuação por engano");
        assert_eq!(read(json!(5000)),CONTINUATION_MINUTES,"mais de um dia não é continuação");
    }

    #[test]
    fn a_bigger_request_has_to_say_more_to_get_through() {
        let precise=reading(0.2,0.9,0.9,0.9,0.05);
        assert_eq!(judge(&turn_at("c"),"x",&precise,"jev").verdict,EntryVerdict::Pass);
        // A mesma clareza fraca atravessa como ajuste pequeno e barra como sistema inteiro.
        let small=judge(&turn_at("c"),"x",&reading(0.2,0.5,0.2,0.1,0.1),"jev");
        let whole=judge(&turn_at("c"),"x",&reading(1.9,0.5,0.2,0.1,0.1),"jev");
        assert_eq!(small.score,whole.score);
        assert_eq!((small.verdict,whole.verdict),(EntryVerdict::Pass,EntryVerdict::Block));
        assert_eq!((small.demand,whole.demand),(35,70));
    }

    #[test]
    fn the_three_verdicts_follow_the_demand_and_the_margin() {
        let level=|score:f64|reading(score,0.0,0.0,0.0,0.0).scope_level();
        assert_eq!((level(0.0),level(0.66),level(0.67),level(1.33),level(1.34),level(2.0)),(0,0,1,1,2,2));
        // Uma funcionalidade exige 55: 70 passa, 50 pergunta, 30 barra.
        let at=|clarity:f64|{let mut r=reading(1.0,clarity,clarity,clarity,1.0-clarity);r.bundles_requests=1.0-clarity;r};
        assert_eq!(judge(&turn_at("c"),"x",&at(0.70),"jev").verdict,EntryVerdict::Pass);
        assert_eq!(judge(&turn_at("c"),"x",&at(0.50),"jev").verdict,EntryVerdict::Ask);
        assert_eq!(judge(&turn_at("c"),"x",&at(0.30),"jev").verdict,EntryVerdict::Block);
        assert_eq!(judge(&turn_at("c"),"x",&at(0.70),"jev").demand,55);
    }

    #[test]
    fn every_criterion_reaches_the_screen_as_a_percentage_with_its_band() {
        let check=judge(&turn_at("chat"),"Refatore o roteador",&reading(1.0,0.9,0.2,0.8,0.4),"jev");
        let ids=check.criteria.iter().map(|criterion|criterion.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids,vec!["scope","goal_is_clear","says_where","says_when_done","bundles_requests"]);
        assert!(check.criteria.iter().all(|criterion|criterion.percent<=100 && !criterion.label.is_empty() && !criterion.reading.is_empty()));
        let scope=&check.criteria[0];
        assert_eq!((scope.band,scope.reading.as_str()),(None,"feature"));
        let bundles=check.criteria.last().expect("bundles");
        assert!(bundles.inverted && bundles.band==Some([0,45]));
        let says_where=&check.criteria[2];
        assert_eq!(says_where.band,Some([55,100]));
        assert!(check.failing().iter().any(|criterion|criterion.id=="says_where"));
        assert_eq!(check.score,percent(reading(1.0,0.9,0.2,0.8,0.4).clarity()));
    }

    #[test]
    fn a_blocked_request_explains_itself_and_an_asked_one_carries_an_instruction() {
        let blocked=judge(&turn_at("chat"),"arruma tudo ai",&reading(1.9,0.2,0.1,0.05,0.6),"heuristica");
        assert_eq!(blocked.verdict,EntryVerdict::Block);
        let reply=blocked.reply();
        let lines=i18n::read_notice(&reply).expect("o barrado vai como aviso traduzível");
        assert_eq!(lines.first().map(|line|line.key.as_str()),Some("gate.blocked"));
        assert!(lines.iter().any(|line|line.key=="gate.missing.item"),"{lines:?}");
        assert_eq!(lines.last().map(|line|line.key.as_str()),Some("gate.asks.2"));
        let english=i18n::for_model(&reply);
        assert!(english.contains("whole system") && english.contains("70") && english.contains("says where"),"{english}");
        assert!(blocked.clarifying_note().is_none());
        let asked=judge(&turn_at("chat"),"Adicione paginação na listagem",&reading(1.0,0.7,0.4,0.05,0.05),"jev");
        assert_eq!(asked.verdict,EntryVerdict::Ask);
        let note=asked.clarifying_note().expect("ressalva");
        assert!(note.contains("one objective question") && note.contains("says when done"),"{note}");
        assert!(judge(&turn_at("chat"),"x",&reading(0.2,0.95,0.95,0.95,0.0),"jev").clarifying_note().is_none());
    }

    #[test]
    fn a_released_request_reaches_the_model_rewritten_with_what_the_gate_read() {
        let request="Adicione paginação em src/components/Table.tsx; pronto quando `npm test` passar";
        let passed=judge(&turn_at("chat"),request,&reading(1.0,0.9,0.9,0.9,0.05),"jev");
        assert_eq!(passed.verdict,EntryVerdict::Pass);
        let refined=passed.refined_prompt(&format!("  {request}\n")).expect("liberado tem prompt melhorado");
        assert!(refined.starts_with(&format!("REQUEST (verbatim from the developer):\n{request}\n")),"{refined}");
        assert!(refined.contains("one capability") && refined.contains("places the request names") && refined.contains("own done criterion"),"{refined}");
        assert!(!refined.contains("bundles independent items"),"{refined}");
        let asked=judge(&turn_at("chat"),"Adicione paginação na listagem e também troque o tema",&reading(1.0,0.7,0.4,0.05,0.8),"jev");
        assert_eq!(asked.verdict,EntryVerdict::Ask);
        let refined=asked.refined_prompt("Adicione paginação na listagem e também troque o tema").expect("ressalva também é melhorada");
        assert!(refined.contains("No location was given") && refined.contains("No done criterion") && refined.contains("bundles independent items"),"{refined}");
        let blocked=judge(&turn_at("chat"),"arruma tudo ai",&reading(1.9,0.2,0.1,0.05,0.6),"heuristica");
        assert!(blocked.refined_prompt("arruma tudo ai").is_none());
    }

    #[test]
    fn the_local_heuristic_reads_the_same_shape_as_the_jev_answer() {
        let sloppy=heuristic_entry("arruma isso ai");
        assert_eq!(sloppy.scope_level(),0);
        assert!(sloppy.says_when_done<0.3 && sloppy.says_where<0.5);
        let precise=heuristic_entry("Corrija o parser em `src-tauri/src/config.rs` para aceitar chaves ausentes; pronto quando `cargo test` passar sem erro no módulo de configuração.");
        assert!(precise.says_when_done>0.7,"{precise:?}");
        assert!(precise.says_where>0.5,"{precise:?}");
        assert!(precise.goal_is_clear>sloppy.goal_is_clear);
        assert!(precise.clarity()>sloppy.clarity());
        let bundled=heuristic_entry("Refatore o roteador e também atualize as dependências, e já que você está aí conserta o CSS do sidebar");
        assert!(bundled.bundles_requests>0.25,"{bundled:?}");
        assert!(heuristic_entry("").clarity().is_finite());
    }

    #[test]
    fn the_exit_gate_names_the_rule_each_command_and_file_hit() {
        let answer="Rode isto:\n\n```bash\n$ cargo test --lib\nrm -rf target\n```\n\nDepois troque o roteador:\n\n```rust src/router.rs\npub fn select_model() {}\n```\n\nVeja também `src/lib.rs` e nunca toque em `.env`.";
        let ask=config("ask","ask");
        let checks=scan_answer(&turn_at("chat"),answer,&ask,&firewall(),Path::new("/projeto"));
        let commands=checks.iter().filter(|check|check.kind=="command").collect::<Vec<_>>();
        assert_eq!(commands.len(),2);
        assert_eq!(commands[0].target,"cargo test --lib");
        assert!(commands.iter().all(|check|check.verdict==ExitVerdict::Held && check.rule.as_deref()==Some("permissions.shell · ask")));
        let files=checks.iter().filter(|check|check.kind=="file").collect::<Vec<_>>();
        assert_eq!(files.iter().map(|check|check.target.as_str()).collect::<Vec<_>>(),vec!["src/router.rs",".env"]);
        assert_eq!(files[0].rule.as_deref(),Some("permissions.write · ask"));
        assert_eq!(files[1].rule.as_deref(),Some("privacy.deny · .env"));
        let open=config("allow","allow");
        let relaxed=scan_answer(&turn_at("chat"),answer,&open,&firewall(),Path::new("/projeto"));
        assert!(relaxed.iter().filter(|check|check.target=="cargo test --lib").all(|check|check.verdict==ExitVerdict::Cleared));
        assert_eq!(relaxed.iter().find(|check|check.target==".env").expect("env").verdict,ExitVerdict::Held);
    }

    /// Pedir só uma análise já pôs dez arquivos na coluna de saída como
    /// `permissions.write · ask`: a resposta apenas citava os caminhos. Citar
    /// é ler; só escreve quem entrega o conteúdo do arquivo.
    #[test]
    fn citing_a_file_in_an_analysis_is_not_asking_to_write_it() {
        let answer="## Arquivos envolvidos\n\n- `src/app/Main.tsx` (ponto de entrada)\n- `lib/service.py` ou módulo equivalente\n- `helpers.go`";
        let ask=config("ask","ask");
        assert!(scan_answer(&turn_at("chat"),answer,&ask,&firewall(),Path::new("/projeto")).is_empty());
        let mut closed=config("ask","ask");
        closed.permissions.read="ask".into();
        let held=scan_answer(&turn_at("chat"),answer,&closed,&firewall(),Path::new("/projeto"));
        assert_eq!(held.len(),3);
        assert!(held.iter().all(|check|check.rule.as_deref()==Some("permissions.read · ask")));
        let leaving=scan_answer(&turn_at("chat"),"compare com `../outro/lib.rs`",&ask,&firewall(),Path::new("/projeto"));
        assert_eq!(leaving[0].rule.as_deref(),Some("workspace.root · outside"));
    }

    #[test]
    fn only_real_commands_and_real_paths_reach_the_feed() {
        assert_eq!(shell_lines("```bash\n# comenta\n\n$ npm run build\n```\ntexto `npm test` solto\n```rust\nlet x=1;\n```"),vec!["npm run build"]);
        let paths=cited_paths("veja `src/lib.rs`, a função `Config::load`, o valor `42`, `handler()`, `config.yaml` e `.env`");
        assert_eq!(paths,vec!["src/lib.rs","config.yaml",".env"]);
        assert!(cited_paths("nada aqui").is_empty());
        assert_eq!(written_paths("FILE: src/a.rs\nfn a(){}\n\n```ts src/b.ts\nexport {}\n```\ne `src/c.rs` só citado"),vec!["src/a.rs","src/b.ts"]);
        assert!(!looks_like_path("src/") && !looks_like_path("a b.rs") && looks_like_path("src-tauri/src/jev.rs"));
    }

    #[test]
    fn the_agents_own_state_files_are_not_held() {
        let home=Path::new("/home/dev");
        assert!(agent_state("/home/dev/.claude/plans/plano.md",Some(home)));
        assert!(agent_state("~/.codex/sessions/s.json",Some(home)));
        assert!(!agent_state(".claude/settings.json",Some(home)),"o .claude do projeto segue as regras");
        assert!(!agent_state("/home/dev/.ssh/id.key",Some(home)));
        assert!(!agent_state("/etc/.claude/x.md",Some(home)));
    }

    #[test]
    fn a_path_leaving_the_project_is_held_by_the_workspace_rule() {
        let open=config("allow","allow");
        let root=Path::new("/projeto");
        assert_eq!(file_rule(&open,&firewall(),root,"../outro/lib.rs",Access::Write).as_deref(),Some("workspace.root · outside"));
        assert_eq!(file_rule(&open,&firewall(),root,"/etc/hosts.md",Access::Read).as_deref(),Some("workspace.root · outside"));
        assert_eq!(file_rule(&open,&firewall(),root,"src/lib.rs",Access::Write),None);
        assert_eq!(command_rule(&open,""),None);
    }

    #[test]
    fn reads_a_batched_entry_evaluation_and_reports_what_is_missing() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "scope":{"type":"score","score":1.8,"confidence":0.8,"probabilities":{"0":0.0,"1":0.2,"2":0.8}},
            "goal_is_clear":{"type":"noul","noul":0.91},
            "says_where":{"type":"noul","noul":0.12},
            "says_when_done":{"type":"noul","noul":0.20},
            "bundles_requests":{"type":"noul","noul":0.40}},
            "usage":{"input_tokens":420,"output_tokens":30}}"#).unwrap();
        let entry=EntryReading::from_evaluation(&evaluation).expect("leitura");
        assert_eq!(entry.scope_level(),2);
        // Objetivo claríssimo, mas num sistema inteiro sem dizer onde nem como
        // conferir: a portaria libera e manda perguntar antes de começar.
        let check=judge(&turn_at("chat"),"Migre a persistência",&entry,"jev");
        assert_eq!((check.verdict,check.scope.as_str(),check.demand),(EntryVerdict::Ask,"whole system",70));
        assert_eq!(check.failing().iter().map(|criterion|criterion.id.as_str()).collect::<Vec<_>>(),vec!["says_where","says_when_done","bundles_requests"]);
        let partial:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"goal_is_clear":{"type":"noul","noul":0.9}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        assert!(EntryReading::from_evaluation(&partial).unwrap_err().to_string().contains("scope"));
        let mistyped:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "scope":{"type":"noul","noul":0.5},
            "goal_is_clear":{"type":"noul","noul":0.9},
            "says_where":{"type":"noul","noul":0.9},
            "says_when_done":{"type":"noul","noul":0.9},
            "bundles_requests":{"type":"noul","noul":0.1}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        assert!(EntryReading::from_evaluation(&mistyped).unwrap_err().to_string().contains("score"));
    }
}
