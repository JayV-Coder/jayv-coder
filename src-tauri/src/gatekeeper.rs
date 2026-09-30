//! As duas portarias do Jev. O portão de entrada pontua o pedido do
//! desenvolvedor antes de qualquer modelo ser chamado; o portão de saída
//! confere cada comando ou arquivo que o modelo pediu para mexer contra as
//! regras da casa declaradas em `config.yaml`.

use crate::{config::Config, firewall::ContextFirewall, jev::{self, Evaluation, Question}, turns::Turn};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use uuid::Uuid;

pub const ENTRY_QUESTION_IDS:[&str;5]=["bundles_requests","goal_is_clear","says_when_done","says_where","scope"];
pub const SCOPE_LEVELS:[&str;3]=["ajuste pequeno","funcionalidade","sistema inteiro"];
/// Quanto de clareza cada tamanho de pedido exige para atravessar o portão.
pub const SCOPE_DEMAND:[f64;3]=[0.35,0.55,0.70];
/// Abaixo da exigência o portão pergunta; abaixo dela com esta folga, barra.
pub const BLOCK_MARGIN:f64=0.20;
pub const WEIGHTS:[(&str,f64);4]=[("goal_is_clear",0.40),("says_where",0.25),("says_when_done",0.20),("bundles_requests",0.15)];

/// Os números do Jev como vão para o seed de `jev_parameters`. São também o
/// padrão quando o cache não tem um valor válido.
pub fn parameters()->BTreeMap<String,serde_json::Value> {
    BTreeMap::from([
        ("scope_demand".to_string(),json!(SCOPE_DEMAND)),
        ("block_margin".to_string(),json!(BLOCK_MARGIN)),
        ("weights".to_string(),json!(WEIGHTS.iter().map(|(id,weight)|(id.to_string(),json!(weight))).collect::<serde_json::Map<_,_>>())),
        ("scope_levels".to_string(),json!(SCOPE_LEVELS)),
        ("noul_line".to_string(),json!(crate::asking::NOUL_LINE)),
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
    pub fn parse(value:&str)->Result<Self>{Ok(match value{"pass"=>Self::Pass,"ask"=>Self::Ask,"block"=>Self::Block,other=>return Err(anyhow!("veredito de entrada desconhecido: `{other}`"))})}
    pub fn lets_through(&self)->bool{!matches!(self,Self::Block)}
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
    /// A resposta que a portaria devolve quando barra o pedido.
    pub fn reply(&self)->String {
        let missing=self.failing().iter().map(|criterion|format!("- {}: {}",criterion.label.to_lowercase(),criterion.reading)).collect::<Vec<_>>();
        let asks=match self.scope.as_str(){
            "ajuste pequeno"=>"Diga o que deve mudar e como você confirma que mudou.",
            "funcionalidade"=>"Diga o objetivo, onde mexer e como você confirma que ficou pronto.",
            _=>"Um pedido desse tamanho precisa do objetivo, dos arquivos ou módulos envolvidos e do critério de pronto. Se der, quebre em partes.",
        };
        format!("A portaria barrou este pedido com {} de 100 (o mínimo para um {} é {}).\n\nO que está faltando:\n{}\n\n{}",self.score,self.scope,self.demand,missing.join("\n"),asks)
    }
    /// A instrução que acompanha um pedido liberado com ressalva.
    pub fn clarifying_note(&self)->Option<String> {
        if self.verdict!=EntryVerdict::Ask {return None;}
        let gaps=self.failing().iter().map(|criterion|criterion.label.to_lowercase()).collect::<Vec<_>>().join(", ");
        Some(format!("A portaria liberou este pedido com ressalva: {gaps}. Faça uma única pergunta objetiva sobre o ponto mais crítico antes de começar o trabalho, e não presuma uma abordagem enquanto ela não for respondida."))
    }
    /// O pedido liberado, reescrito para o modelo com o que a portaria leu nele.
    /// O texto do desenvolvedor vai intacto no topo; embaixo, a leitura de cada
    /// critério vira uma instrução de conduta — o que o pedido já disse é
    /// cobrado como compromisso, o que faltou vira tarefa do modelo. Barrado não
    /// chega a modelo nenhum, então não tem versão melhorada.
    pub fn refined_prompt(&self,request:&str)->Option<String> {
        if !self.verdict.lets_through() {return None;}
        let met=|id:&str|self.criteria.iter().find(|criterion|criterion.id==id).is_some_and(Criterion::within_band);
        let mut steps=vec![match self.scope.as_str(){
            "ajuste pequeno"=>"This is a small adjustment: make the smallest change that satisfies it and leave everything else untouched.",
            "funcionalidade"=>"This is one capability delivered end to end: plan the few files it needs, implement them, and keep unrelated code as it is.",
            _=>"This is system-wide work: outline the plan and the parts it touches before changing anything, then deliver it in reviewable steps.",
        }.to_string()];
        steps.push(if met("goal_is_clear"){"Treat the outcome the request states as the goal; do not widen it."}else{"The goal is not explicit: state in one sentence the outcome you are going to deliver before you start."}.into());
        steps.push(if met("says_where"){"Work in the places the request names; if something outside them must change, say which file and why."}else{"No location was given: find where this belongs in the project first and list the files you will touch."}.into());
        steps.push(if met("says_when_done"){"Use the request's own done criterion as your final check and report how it was verified."}else{"No done criterion was given: end by saying how the developer can confirm the work is finished (a test, a command or an observable behaviour)."}.into());
        if !met("bundles_requests") {steps.push("The request bundles independent items: handle them one at a time, in the order given, and report the outcome of each separately.".into());}
        steps.push("Preserve behaviour the request does not mention, and answer in the language the request was written in.".into());
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
    pub fn clarity(&self)->f64 {
        WEIGHTS.iter().map(|(id,weight)|weight*match *id {
            "goal_is_clear"=>self.goal_is_clear,
            "says_where"=>self.says_where,
            "says_when_done"=>self.says_when_done,
            _=>1.0-self.bundles_requests,
        }.clamp(0.0,1.0)).sum()
    }
    pub fn from_evaluation(evaluation:&Evaluation)->Result<Self> {
        let noul=|id:&str|->Result<f64>{let answer=evaluation.answer(id).ok_or_else(||anyhow!("o Jev não devolveu a resposta `{id}`"))?;answer.as_noul().ok_or_else(||anyhow!("o Jev devolveu `{id}` como {} em vez de `noul`",answer.kind()))};
        let scope=evaluation.answer("scope").ok_or_else(||anyhow!("o Jev não devolveu a resposta `scope`"))?;
        Ok(Self{
            scope_score:scope.as_score().ok_or_else(||anyhow!("o Jev devolveu `scope` como {} em vez de `score`",scope.kind()))?,
            goal_is_clear:noul("goal_is_clear")?,
            says_where:noul("says_where")?,
            says_when_done:noul("says_when_done")?,
            bundles_requests:noul("bundles_requests")?,
        })
    }
}

fn percent(value:f64)->u8{(value.clamp(0.0,1.0)*100.0).round() as u8}

fn criterion(id:&str,label:&str,value:f64,demand:f64,inverted:bool,reading:(&str,&str))->Criterion {
    let band=if inverted{[0,percent(1.0-demand)]}else{[percent(demand),100]};
    let reached=percent(value);
    let within=(band[0]..=band[1]).contains(&reached);
    Criterion{id:id.into(),label:label.into(),percent:reached,band:Some(band),reading:if within{reading.0.into()}else{reading.1.into()},inverted}
}

/// Monta o veredito a partir das leituras, aplicando a exigência do tamanho.
pub fn judge(turn:&Turn,prompt:&str,reading:&EntryReading,source:&str)->EntryCheck {
    let level=reading.scope_level();
    let demand=SCOPE_DEMAND[level];
    let clarity=reading.clarity();
    let verdict=if clarity+BLOCK_MARGIN<demand{EntryVerdict::Block}else if clarity<demand{EntryVerdict::Ask}else{EntryVerdict::Pass};
    let scope=SCOPE_LEVELS[level];
    let criteria=vec![
        Criterion{id:"scope".into(),label:"Tamanho do pedido".into(),percent:percent(reading.scope_score/2.0),band:None,reading:scope.into(),inverted:false},
        criterion("goal_is_clear","Objetivo claro",reading.goal_is_clear,demand,false,("o pedido diz o que quer","não dá para saber o que você quer ao final")),
        criterion("says_where","Diz onde mexer",reading.says_where,demand,false,("aponta arquivos, módulos ou telas","não aponta nenhum arquivo, módulo ou tela")),
        criterion("says_when_done","Diz como saber que ficou pronto",reading.says_when_done,demand,false,("traz um critério de pronto","não traz como conferir que ficou pronto")),
        criterion("bundles_requests","Vários pedidos juntos",reading.bundles_requests,demand,true,("é um pedido só","junta assuntos que renderiam pedidos separados")),
    ];
    let note=match verdict {
        EntryVerdict::Pass=>format!("Liberado: {scope} com o que precisa estar dito."),
        EntryVerdict::Ask=>"Liberado com ressalva: o modelo vai perguntar antes de começar.".into(),
        EntryVerdict::Block=>format!("Barrado: {scope} sem o mínimo de clareza exigido."),
    };
    EntryCheck{id:turn.id.clone(),at:Utc::now(),chat_id:turn.chat_id.clone(),turn:turn.code.clone(),prompt:preview(prompt),score:percent(clarity),demand:percent(demand),verdict,scope:scope.into(),criteria,source:source.into(),note}
}

fn preview(prompt:&str)->String {
    let trimmed=prompt.trim();
    if trimmed.chars().count()<=PROMPT_PREVIEW{return trimmed.into();}
    format!("{}…",trimmed.chars().take(PROMPT_PREVIEW).collect::<String>())
}

pub fn entry_state(prompt:&str,project:&str,languages:&[String])->serde_json::Value{json!({"user_request":prompt,"project":{"name":project,"languages":languages}})}

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
            json!({"question":"Does `user_request` state what the developer wants to be true once the work is finished?","guidance":"Look for the intended outcome, not for politeness or detail. A request can be short and still name its outcome exactly."}),
            json!({"when":"The outcome is stated: the request names the behaviour, artefact or answer it expects to exist afterwards.","examples":["asks for a named capability, file or fix","states the problem to be gone and what working looks like","asks a question whose answer would settle a decision"]}),
            json!({"when":"The outcome has to be guessed.","examples":["\"arruma isso\", \"melhora aqui\", \"deixa mais rápido\" with nothing to anchor them","names a topic without saying what should change about it","several possible goals with no sign of which one is meant"]}))),
        ("says_where".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` say where in the project the work belongs?","guidance":"A location can be a path, a file, a module, a function, a screen, a layer or a named subsystem. Judge whether someone who knows this project could open the right place without guessing."}),
            json!({"when":"The request points at a place: a path or filename, a named symbol, a module, a screen, a route, or a layer of the system."}),
            json!({"when":"No place is given and the request is not self-locating.","examples":["a change described only by its effect, in a project with many plausible homes for it","\"no sistema\", \"no código\", \"em algum lugar do backend\""],"not_a_defect":["a general question that does not touch this project at all"]}))),
        ("says_when_done".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` say how the developer will confirm the work is done?","guidance":"Look for something checkable: a test that should pass, a command whose output is stated, an input and its expected output, an acceptance condition, or a described end state precise enough to compare against."}),
            json!({"when":"A check is stated or clearly implied by a precise end state.","examples":["names a test, a command or an output that must appear","gives an input and the expected result","describes the finished behaviour precisely enough to verify"]}),
            json!({"when":"There is no way to tell the work apart from unfinished work.","examples":["only an adjective as the target: \"melhor\", \"mais limpo\", \"mais rápido\", with no measure","asks for a change with no stated effect to look for"]}))),
        ("bundles_requests".to_string(),Question::noul_with(
            json!({"question":"Does `user_request` bundle work that would be better asked for separately?","guidance":"Judge whether the parts share one outcome. Several files, several steps or a long description of one coherent job is a single request. Separate goals that merely arrived in the same message are a bundle."}),
            json!({"when":"Two or more independent goals are asked for at once, each of which could be delivered, reviewed and verified on its own.","examples":["a refactor plus an unrelated new feature plus a dependency upgrade","\"e já que você está aí, também...\"","a list of unrelated defects"]}),
            json!({"when":"Everything asked for serves one outcome, however many files or steps that takes.","also":"Necessary supporting work — the test for the feature, the migration the change needs — belongs to the same request."}))),
    ])
}

pub async fn evaluate_entry(prompt:&str,project:&str,languages:&[String])->Result<EntryReading> {
    let evaluation=jev::evaluate(entry_state(prompt,project,languages),entry_questions()).await?;
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

/// Uma leitura só com o texto do pedido, para quando `TYPESAFE_API_KEY` não
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
    pub fn parse(value:&str)->Result<Self>{Ok(match value{"cleared"=>Self::Cleared,"held"=>Self::Held,other=>return Err(anyhow!("veredito de saída desconhecido: `{other}`"))})}
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
    /// `comando` ou `arquivo`, como a coluna mostra.
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

/// A regra que segura um arquivo, se alguma segurar.
pub fn file_rule(config:&Config,firewall:&ContextFirewall,root:&Path,path:&str)->Option<String> {
    let info=firewall.check_file(path);
    if let Some(rule)=info.matched_rule {return Some(rule);}
    if escapes_root(root,path){return Some("workspace.root · fora do projeto".into());}
    match config.permissions.write.as_str() {
        "allow"=>None,
        other=>Some(format!("permissions.write · {other}")),
    }
}

fn escapes_root(root:&Path,path:&str)->bool {
    let candidate=Path::new(path);
    if candidate.components().any(|component|component.as_os_str()=="..") {return true;}
    candidate.is_absolute() && !candidate.starts_with(root)
}

/// Lê a resposta do modelo e devolve tudo que ele pediu para rodar ou mexer,
/// já confrontado com as regras da casa.
pub fn scan_answer(turn:&Turn,answer:&str,config:&Config,firewall:&ContextFirewall,root:&Path)->Vec<ExitCheck> {
    let mut checks=Vec::new();
    let mut seen=Vec::new();
    for line in shell_lines(answer) {
        if seen.contains(&line){continue;}
        seen.push(line.clone());
        checks.push(ExitCheck::new(turn,"comando",&line,command_rule(config,&line)));
    }
    for path in mentioned_paths(answer) {
        if seen.contains(&path){continue;}
        seen.push(path.clone());
        checks.push(ExitCheck::new(turn,"arquivo",&path,file_rule(config,firewall,root,&path)));
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

/// Os caminhos de arquivo que a resposta cita em `código`, em `FILE:` ou como
/// rótulo de um bloco de código.
pub fn mentioned_paths(answer:&str)->Vec<String> {
    static PATHS:OnceLock<(Regex,Regex)>=OnceLock::new();
    let (inline,marker)=PATHS.get_or_init(||(
        Regex::new(r"`([^`\n]{1,160})`").unwrap(),
        Regex::new(r"(?im)^\s*(?:FILE:|```[a-z]*\s+)([\w./-]+)\s*$").unwrap(),
    ));
    let mut paths=Vec::new();
    let mut push=|candidate:&str|{let candidate=candidate.trim().trim_start_matches("./");if looks_like_path(candidate)&&!paths.iter().any(|kept|kept==candidate){paths.push(candidate.to_string());}};
    for capture in marker.captures_iter(answer){push(&capture[1]);}
    for capture in inline.captures_iter(answer){push(&capture[1]);}
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
        assert!(entry_state("pedido","JayV",&["Rust".into()]).pointer("/project/name").is_some());
    }

    #[test]
    fn a_bigger_pedido_has_to_say_more_to_get_through() {
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
        assert_eq!((scope.band,scope.reading.as_str()),(None,"funcionalidade"));
        let bundles=check.criteria.last().expect("bundles");
        assert!(bundles.inverted && bundles.band==Some([0,45]));
        let says_where=&check.criteria[2];
        assert_eq!(says_where.band,Some([55,100]));
        assert!(check.failing().iter().any(|criterion|criterion.id=="says_where"));
        assert_eq!(check.score,percent(reading(1.0,0.9,0.2,0.8,0.4).clarity()));
    }

    #[test]
    fn a_blocked_pedido_explains_itself_and_an_asked_one_carries_an_instruction() {
        let blocked=judge(&turn_at("chat"),"arruma tudo ai",&reading(1.9,0.2,0.1,0.05,0.6),"heuristica");
        assert_eq!(blocked.verdict,EntryVerdict::Block);
        let reply=blocked.reply();
        assert!(reply.contains("sistema inteiro") && reply.contains("70") && reply.contains("onde mexer"),"{reply}");
        assert!(blocked.clarifying_note().is_none());
        let asked=judge(&turn_at("chat"),"Adicione paginação na listagem",&reading(1.0,0.7,0.4,0.05,0.05),"jev");
        assert_eq!(asked.verdict,EntryVerdict::Ask);
        let note=asked.clarifying_note().expect("ressalva");
        assert!(note.contains("uma única pergunta") && note.contains("pronto"),"{note}");
        assert!(judge(&turn_at("chat"),"x",&reading(0.2,0.95,0.95,0.95,0.0),"jev").clarifying_note().is_none());
    }

    #[test]
    fn a_released_pedido_reaches_the_model_rewritten_with_what_the_gate_read() {
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
        let answer="Rode isto:\n\n```bash\n$ cargo test --lib\nrm -rf target\n```\n\nDepois edite `src/router.rs` e nunca toque em `.env`.";
        let ask=config("ask","ask");
        let checks=scan_answer(&turn_at("chat"),answer,&ask,&firewall(),Path::new("/projeto"));
        let commands=checks.iter().filter(|check|check.kind=="comando").collect::<Vec<_>>();
        assert_eq!(commands.len(),2);
        assert_eq!(commands[0].target,"cargo test --lib");
        assert!(commands.iter().all(|check|check.verdict==ExitVerdict::Held && check.rule.as_deref()==Some("permissions.shell · ask")));
        let files=checks.iter().filter(|check|check.kind=="arquivo").collect::<Vec<_>>();
        assert_eq!(files.iter().map(|check|check.target.as_str()).collect::<Vec<_>>(),vec!["src/router.rs",".env"]);
        assert_eq!(files[1].rule.as_deref(),Some("privacy.deny · .env"));
        assert_eq!(files[0].rule.as_deref(),Some("permissions.write · ask"));
        let open=config("allow","allow");
        let relaxed=scan_answer(&turn_at("chat"),answer,&open,&firewall(),Path::new("/projeto"));
        assert!(relaxed.iter().filter(|check|check.target=="cargo test --lib").all(|check|check.verdict==ExitVerdict::Cleared));
        assert_eq!(relaxed.iter().find(|check|check.target==".env").expect("env").verdict,ExitVerdict::Held);
    }

    #[test]
    fn only_real_commands_and_real_paths_reach_the_feed() {
        assert_eq!(shell_lines("```bash\n# comenta\n\n$ npm run build\n```\ntexto `npm test` solto\n```rust\nlet x=1;\n```"),vec!["npm run build"]);
        let paths=mentioned_paths("veja `src/lib.rs`, a função `Config::load`, o valor `42`, `handler()`, `config.yaml` e `.env`");
        assert_eq!(paths,vec!["src/lib.rs","config.yaml",".env"]);
        assert!(mentioned_paths("nada aqui").is_empty());
        assert!(!looks_like_path("src/") && !looks_like_path("a b.rs") && looks_like_path("src-tauri/src/jev.rs"));
    }

    #[test]
    fn a_path_leaving_the_project_is_held_by_the_workspace_rule() {
        let open=config("allow","allow");
        let root=Path::new("/projeto");
        assert_eq!(file_rule(&open,&firewall(),root,"../outro/lib.rs").as_deref(),Some("workspace.root · fora do projeto"));
        assert_eq!(file_rule(&open,&firewall(),root,"/etc/hosts.md").as_deref(),Some("workspace.root · fora do projeto"));
        assert_eq!(file_rule(&open,&firewall(),root,"src/lib.rs"),None);
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
        assert_eq!((check.verdict,check.scope.as_str(),check.demand),(EntryVerdict::Ask,"sistema inteiro",70));
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
