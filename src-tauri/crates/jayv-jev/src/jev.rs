use crate::providers::{retry_after, with_retry, RetryError, RetryPolicy};
use anyhow::{anyhow, Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap}, time::Duration};

pub const DEFAULT_MODEL:&str="jev-latest";
pub const DEFAULT_TIMEOUT:u64=30;
pub const DEFAULT_ATTEMPTS:u32=4;
pub const MAX_CHOICE_OPTIONS:usize=255;
pub const MIN_SCORE_LEVELS:usize=2;
pub const MAX_SCORE_LEVELS:usize=10;
pub const INTENTS:[&str;8]=["analysis","code","frontend","general","refactor","review","security","test"];
pub const COMPLEXITY_BUCKETS:[&str;4]=["trivial","simple","medium","complex"];
pub const ROUTING_QUESTION_IDS:[&str;5]=["complexity","intent","is_destructive","needs_repository_context","needs_tools"];
pub const VERIFICATION_QUESTION_IDS:[&str;3]=["addresses_request","unsupported_claims","verifiable_claims"];

/// O Jev está ao alcance quando há sessão: a chave da TypeSafe mora na função
/// `jev` do projeto, e é o token do usuário que abre a porta.
pub fn is_configured()->bool{crate::cloud::session::current().is_some()}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct NoulCriteria{#[serde(rename="true")] pub yes:Value,#[serde(rename="false")] pub no:Value}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(tag="type",rename_all="lowercase")]
pub enum Question {
    Noul{instructions:Value,#[serde(default,skip_serializing_if="Option::is_none")] criteria:Option<NoulCriteria>},
    Choice{instructions:Value,criteria:BTreeMap<String,Value>},
    Score{instructions:Value,criteria:Vec<Value>},
}

impl Question {
    pub fn noul(instructions:impl Into<Value>)->Self{Self::Noul{instructions:instructions.into(),criteria:None}}
    pub fn noul_with(instructions:impl Into<Value>,yes:impl Into<Value>,no:impl Into<Value>)->Self{Self::Noul{instructions:instructions.into(),criteria:Some(NoulCriteria{yes:yes.into(),no:no.into()})}}
    pub fn choice<K:Into<String>,V:Into<Value>>(instructions:impl Into<Value>,criteria:impl IntoIterator<Item=(K,V)>)->Self{Self::Choice{instructions:instructions.into(),criteria:criteria.into_iter().map(|(key,value)|(key.into(),value.into())).collect()}}
    pub fn score<L:Into<Value>>(instructions:impl Into<Value>,levels:impl IntoIterator<Item=L>)->Self{Self::Score{instructions:instructions.into(),criteria:levels.into_iter().map(Into::into).collect()}}
    pub fn kind(&self)->&'static str{match self{Self::Noul{..}=>"noul",Self::Choice{..}=>"choice",Self::Score{..}=>"score"}}
    pub fn options(&self)->Vec<&str>{match self{Self::Choice{criteria,..}=>criteria.keys().map(String::as_str).collect(),_=>vec![]}}
    pub fn levels(&self)->usize{match self{Self::Score{criteria,..}=>criteria.len(),_=>0}}
    pub fn validate(&self)->Result<()>{match self{
        Self::Noul{..}=>Ok(()),
        Self::Choice{criteria,..}=>if criteria.is_empty(){Err(anyhow!("a `choice` question needs at least one option in `criteria`"))}else if criteria.len()>MAX_CHOICE_OPTIONS{Err(anyhow!("a `choice` question accepts at most {MAX_CHOICE_OPTIONS} options, and this one has {}",criteria.len()))}else{Ok(())},
        Self::Score{criteria,..}=>if (MIN_SCORE_LEVELS..=MAX_SCORE_LEVELS).contains(&criteria.len()){Ok(())}else{Err(anyhow!("a `score` question needs {MIN_SCORE_LEVELS} to {MAX_SCORE_LEVELS} levels in `criteria`, and this one has {}",criteria.len()))},
    }}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(tag="type",rename_all="lowercase")]
pub enum Answer {
    Noul{noul:f64},
    Choice{choice:String,#[serde(default)] probabilities:HashMap<String,f64>,#[serde(default)] confidence:f64},
    Score{score:f64,#[serde(default)] legend:HashMap<String,Value>,#[serde(default)] probabilities:HashMap<String,f64>,#[serde(default)] confidence:f64},
}

impl Answer {
    pub fn kind(&self)->&'static str{match self{Self::Noul{..}=>"noul",Self::Choice{..}=>"choice",Self::Score{..}=>"score"}}
    pub fn as_noul(&self)->Option<f64>{match self{Self::Noul{noul}=>Some(*noul),_=>None}}
    pub fn as_choice(&self)->Option<&str>{match self{Self::Choice{choice,..}=>Some(choice.as_str()),_=>None}}
    pub fn as_score(&self)->Option<f64>{match self{Self::Score{score,..}=>Some(*score),_=>None}}
    pub fn confidence(&self)->Option<f64>{match self{Self::Noul{..}=>None,Self::Choice{confidence,..}|Self::Score{confidence,..}=>Some(*confidence)}}
    pub fn probabilities(&self)->Option<&HashMap<String,f64>>{match self{Self::Noul{..}=>None,Self::Choice{probabilities,..}|Self::Score{probabilities,..}=>Some(probabilities)}}
    pub fn probability(&self,key:&str)->f64{self.probabilities().and_then(|map|map.get(key)).copied().unwrap_or(0.0)}
    pub fn legend(&self)->Option<&HashMap<String,Value>>{match self{Self::Score{legend,..}=>Some(legend),_=>None}}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Request{pub state:Value,pub model:String,pub questions:BTreeMap<String,Question>}
impl Request {
    pub fn new(state:impl Into<Value>,model:impl Into<String>,questions:BTreeMap<String,Question>)->Self{Self{state:state.into(),model:model.into(),questions}}
    pub fn validate(&self)->Result<()>{
        if self.questions.is_empty(){return Err(anyhow!("a Jev request needs at least one question"));}
        for (id,question) in &self.questions {question.validate().with_context(||format!("question `{id}` is invalid"))?;}
        Ok(())
    }
}

#[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
pub struct Usage{#[serde(default)] pub input_tokens:u64,#[serde(default)] pub output_tokens:u64}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Evaluation{pub model:String,pub answers:HashMap<String,Answer>,#[serde(default)] pub usage:Usage}
impl Evaluation {
    pub fn answer(&self,id:&str)->Option<&Answer>{self.answers.get(id)}
    pub fn noul(&self,id:&str)->Option<f64>{self.answer(id).and_then(Answer::as_noul)}
    pub fn choice(&self,id:&str)->Option<&str>{self.answer(id).and_then(Answer::as_choice)}
    pub fn score(&self,id:&str)->Option<f64>{self.answer(id).and_then(Answer::as_score)}
    pub fn confidence(&self,id:&str)->Option<f64>{self.answer(id).and_then(Answer::confidence)}
    pub fn probabilities(&self,id:&str)->HashMap<String,f64>{self.answer(id).and_then(Answer::probabilities).cloned().unwrap_or_default()}
    fn require(&self,id:&str)->Result<&Answer>{self.answer(id).ok_or_else(||anyhow!("the Jev did not return the `{id}` answer"))}
}

pub fn retryable_status(status:u16)->bool{matches!(status,429|529|500|502|503|504)}
fn backoff(attempt:u32)->Duration{Duration::from_millis((400u64<<attempt.min(5)).min(8_000))}
fn retry_policy(attempts:u32)->RetryPolicy{RetryPolicy{attempts:attempts.max(1),base_delay:backoff(0),max_delay:backoff(9)}}

fn error_detail(body:&str)->String {
    let parsed=serde_json::from_str::<Value>(body).ok();
    parsed.as_ref().and_then(|value|["/error/message","/detail","/message","/error"].iter().find_map(|pointer|value.pointer(pointer)).map(|found|found.as_str().map(str::to_string).unwrap_or_else(||found.to_string())))
        .unwrap_or_else(||{let trimmed=body.trim();if trimmed.is_empty(){"no details".into()}else{trimmed.chars().take(400).collect()}})
}
fn status_error(status:u16,body:&str)->anyhow::Error {
    let detail=error_detail(body);
    match status {
        401=>anyhow!("session expired (401): sign in again to reach the Jev; until then the gate uses the local heuristics"),
        429 if is_daily_limit(body)=>anyhow!("daily Jev limit reached: the gate uses the local heuristics until tomorrow"),
        422=>anyhow!("TypeSafe rejected the request (422): {detail}"),
        429=>anyhow!("TypeSafe rate-limited the request (429) and the retries ran out: {detail}"),
        529=>anyhow!("TypeSafe is overloaded (529) and the retries ran out: {detail}"),
        _=>anyhow!("TypeSafe returned {status}: {detail}"),
    }
}
/// A função `jev` responde 429 com `code: daily_limit` quando o usuário gastou
/// as chamadas do dia — diferente do 429 passageiro da TypeSafe.
fn is_daily_limit(body:&str)->bool { serde_json::from_str::<Value>(body).ok().and_then(|value|value.get("code").and_then(Value::as_str).map(|code|code=="daily_limit")).unwrap_or(false) }
fn worth_retrying(status:u16,body:&str)->bool { retryable_status(status) && !is_daily_limit(body) }
fn transport_error(error:reqwest::Error)->anyhow::Error {
    if error.is_timeout(){anyhow!("the Jev timed out; check the network connection")}
    else if error.is_connect(){anyhow!("could not connect to the Jev; check the network connection")}
    else{anyhow!("failed to send the request to the Jev: {}",error.without_url())}
}

/// O corpo da chamada à função: o conjunto de perguntas, o estado e, quando
/// só parte do conjunto vale para este caso, quais perguntas.
pub fn call_body(set:&str,state:Value,include:Option<&[&str]>)->Value {
    let mut body=json!({"set":set,"state":state});
    if let Some(include)=include {body["include"]=json!(include);}
    body
}

pub struct Client{http:reqwest::Client,token:String,attempts:u32}
impl Client {
    pub fn from_session()->Result<Self> {
        let token=crate::cloud::session::current().ok_or_else(||anyhow!("no session: sign in for the Jev to evaluate requests"))?;
        Self::for_session(token)
    }
    pub fn for_session(token:impl Into<String>)->Result<Self> {
        let token=token.into().trim().to_string();
        if token.is_empty(){return Err(anyhow!("no session to reach the Jev"));}
        let http=crate::lockdown::http_client(Duration::from_secs(DEFAULT_TIMEOUT)).build().context("could not build the Jev HTTP client")?;
        Ok(Self{http,token,attempts:DEFAULT_ATTEMPTS})
    }
    pub fn with_attempts(mut self,attempts:u32)->Self{self.attempts=attempts.max(1);self}
    pub fn endpoint(&self)->String{format!("{}/functions/v1/jev",crate::cloud::PROJECT_URL)}

    /// Pede ao Jev a avaliação de `state` pelas perguntas do conjunto `set`,
    /// guardadas no Supabase.
    ///
    /// Cada chamada entra na conta do uso como `jev:<conjunto>`, com os tokens
    /// que a função devolveu — e a que falhou entra como falha, sem tokens.
    pub async fn evaluate(&self,set:&str,state:impl Into<Value>,include:Option<&[&str]>)->Result<Evaluation> {
        let started=std::time::Instant::now();
        let result=self.send(&call_body(set,state.into(),include)).await;
        crate::usage::spend(spend_of(set,result.as_ref().ok(),started.elapsed().as_millis() as u64));
        result
    }

    pub async fn route(&self,input:&RoutingInput)->Result<RoutingDecision> {
        let evaluation=self.evaluate("routing",routing_state(input),None).await?;
        RoutingDecision::from_evaluation(&evaluation)
    }

    pub async fn verify(&self,input:&VerificationInput)->Result<VerificationVerdict> {
        if !input.has_context(){return Ok(VerificationVerdict::unchecked(DEFAULT_MODEL));}
        let evaluation=self.evaluate("verification",verification_state(input),None).await?;
        VerificationVerdict::from_evaluation(&evaluation)
    }

    async fn send(&self,body:&Value)->Result<Evaluation> {
        let endpoint=self.endpoint();
        let endpoint=endpoint.as_str();
        with_retry(retry_policy(self.attempts),move |_attempt| async move {
            let response=self.http.post(endpoint).header("apikey",crate::cloud::PUBLISHABLE_KEY).bearer_auth(&self.token).json(body).send().await.map_err(|error|RetryError::retryable(transport_error(error),None))?;
            let status=response.status().as_u16();
            let pause=retry_after(response.headers());
            if let Some(quota)=daily_quota(response.headers(),chrono::Utc::now()) { crate::usage::quota(quota); }
            let body=response.text().await.unwrap_or_default();
            if (200..300).contains(&status){return serde_json::from_str::<Evaluation>(&body).map_err(|error|RetryError::fatal(anyhow::Error::new(error).context(format!("the Jev returned an unexpected response: {}",error_detail(&body)))));}
            let failure=status_error(status,&body);
            Err(if worth_retrying(status,&body){RetryError::retryable(failure,pause)}else{RetryError::fatal(failure)})
        }).await
    }
}

/// O gasto de uma chamada ao Jev. Sem avaliação, a chamada falhou: conta
/// como chamada, sem tokens.
pub fn spend_of(set:&str,evaluation:Option<&Evaluation>,duration_ms:u64)->crate::usage::Spend {
    use crate::usage::{Precision, Spend};
    match evaluation {
        Some(evaluation)=>Spend{input_tokens:evaluation.usage.input_tokens,output_tokens:evaluation.usage.output_tokens,duration_ms,..Spend::new(format!("jev:{set}"),evaluation.model.as_str(),Precision::Reported)},
        None=>Spend{duration_ms,success:false,..Spend::new(format!("jev:{set}"),DEFAULT_MODEL,Precision::Estimated)},
    }
}

/// As chamadas do dia, que a função `jev` conta e devolve nos cabeçalhos. O
/// dia é o do servidor (UTC) e recomeça à meia-noite UTC. O `plan` leva
/// `usadas/limite`, que é o que a tela mostra.
pub fn daily_quota(headers:&reqwest::header::HeaderMap,now:chrono::DateTime<chrono::Utc>)->Option<crate::usage::Quota> {
    let read=|name:&str|headers.get(name).and_then(|value|value.to_str().ok()).and_then(|value|value.trim().parse::<u64>().ok());
    let (used,limit)=(read("x-jev-calls-used")?,read("x-jev-daily-limit")?);
    if limit==0 { return None; }
    let midnight=(now.date_naive()+chrono::Days::new(1)).and_hms_opt(0,0,0)?.and_utc();
    Some(crate::usage::Quota{agent:"jev".into(),window:"day".into(),used_percent:Some((used as f64*100.0/limit as f64).min(100.0)),resets_at:Some(midnight.to_rfc3339_opts(chrono::SecondsFormat::Secs,true)),plan:Some(format!("{used}/{limit}"))})
}

pub async fn evaluate(set:&str,state:impl Into<Value>,include:Option<&[&str]>)->Result<Evaluation>{Client::from_session()?.evaluate(set,state,include).await}
pub async fn route(input:&RoutingInput)->Result<RoutingDecision>{Client::from_session()?.route(input).await}
pub async fn verify(input:&VerificationInput)->Result<VerificationVerdict>{Client::from_session()?.verify(input).await}

#[derive(Debug,Clone,Default,PartialEq,Serialize,Deserialize)]
pub struct RoutingInput {
    pub request:String,
    pub project_name:String,
    pub languages:Vec<String>,
    pub candidate_files:Vec<String>,
    pub recent_turns:Vec<String>,
}
impl RoutingInput {
    pub fn new(request:impl Into<String>)->Self{Self{request:request.into(),..Default::default()}}
    pub fn with_project(mut self,name:impl Into<String>,languages:Vec<String>)->Self{self.project_name=name.into();self.languages=languages;self}
    pub fn with_candidate_files(mut self,files:Vec<String>)->Self{self.candidate_files=files;self}
    pub fn with_recent_turns(mut self,turns:Vec<String>)->Self{self.recent_turns=turns;self}
}

pub fn routing_state(input:&RoutingInput)->Value {
    let mut state=serde_json::Map::new();
    state.insert("user_request".into(),Value::String(input.request.clone()));
    state.insert("project".into(),json!({"name":input.project_name,"languages":input.languages}));
    if !input.candidate_files.is_empty(){state.insert("candidate_files".into(),json!(input.candidate_files));}
    if !input.recent_turns.is_empty(){state.insert("recent_turns".into(),json!(input.recent_turns));}
    Value::Object(state)
}

pub fn routing_request(input:&RoutingInput,model:impl Into<String>)->Request{Request::new(routing_state(input),model,routing_questions())}

pub fn routing_questions()->BTreeMap<String,Question> {
    BTreeMap::from([
        ("intent".to_string(),intent_question()),
        ("complexity".to_string(),complexity_question()),
        ("needs_repository_context".to_string(),Question::noul_with(
            json!({"question":"Does answering `user_request` correctly require reading the actual source files of this repository?","context":"`project` names the repository and the languages it is written in. `candidate_files` lists files a keyword search already matched — they are hints that such files exist, not evidence that they are needed.","guidance":"Decide whether the correct answer depends on how this particular codebase is written, or whether it would be the same for any project."}),
            "The answer depends on this repository's own code, configuration or history: editing or deleting an existing file, explaining what a named symbol in this project does, diagnosing a failure in this project, or writing new code that must match interfaces already defined here.",
            "The answer is identical for any project: a language, framework or tooling question, a definition, a generic how-to, a piece of throwaway code, or small talk. Nothing in this repository changes the correct answer.")),
        ("needs_tools".to_string(),Question::noul_with(
            json!({"question":"Would fulfilling `user_request` require acting on the machine rather than only producing text?","guidance":"Acting means running commands or programs, executing a build or test suite, writing files to disk, querying a network service, or inspecting a running process. Judge what the request actually asks for, not what a thorough assistant might optionally do."}),
            "Satisfying the request means executing something or changing the filesystem: running tests, a build, a linter, a migration, a script or a shell command, applying an edit to a file on disk, or fetching live data from a service.",
            "A written answer fully satisfies the request: an explanation, a plan, a review, or a code snippet handed back to the developer to apply themselves."),
        ),
        ("is_destructive".to_string(),Question::noul_with(
            json!({"question":"Would carrying out `user_request` modify, overwrite or delete code, data, configuration or infrastructure that already exists?","guidance":"Judge the effect on things that exist now. Creating something entirely new alongside existing work is not destructive; replacing or removing existing work is."}),
            "Carrying it out overwrites or removes something that already exists: editing or deleting files, rewriting git history, dropping or migrating data, changing configuration or credentials, uninstalling dependencies, or running a command whose side effects are not trivially undone.",
            "Carrying it out only reads, explains, analyses or produces new content that the developer may choose to apply later. Nothing already on disk or in a running system is changed as a direct result."),
        ),
    ])
}

fn intent_question()->Question {
    Question::choice(
        json!({
            "question":"Classify what the developer in `user_request` is asking the assistant to do, so that the right kind of model and the right context can be selected for the work.",
            "focus":"Judge the goal of the request, not the vocabulary used to phrase it. A word such as \"test\", \"review\" or \"security\" appearing in passing does not by itself decide the category.",
            "tie_break":"If the request mixes several goals, pick the one that the bulk of the resulting work serves.",
            "background":"`project` describes the repository and its languages and `candidate_files` lists files a keyword search matched; both are background only. The developer may write in any language — judge the meaning the same way whatever the language."
        }),
        [
            ("analysis",json!({
                "what":"Understand or explain something that already exists: how a piece of code works, why a behaviour or a failure happens, what a dependency does, or how the project is laid out.",
                "not_for":"Judging whether the code is good, which is `review`; or changing the code, which is `code` or `refactor`.",
                "examples":["Explain how model routing works here","Why does this endpoint return 401 only in production?","Trace where this value is set before it reaches the database"]})),
            ("code",json!({
                "what":"Produce working code that does not exist yet, or repair code that is behaving incorrectly: a new feature, a new function, endpoint or script, a bug fix, or an integration with an external system.",
                "not_for":"Restructuring code that already behaves correctly, which is `refactor`; writing tests, which is `test`; user-interface work, which is `frontend`.",
                "examples":["Implement an HTTP client for the payments API","Fix the panic that happens when the config file is missing","Add a --json flag to the CLI"]})),
            ("frontend",json!({
                "what":"Work whose subject is the user interface: components, markup, styling, layout, client-side state, accessibility, or browser behaviour, in React, Vue, HTML, CSS or an equivalent.",
                "not_for":"Server-side or library code that merely happens to feed a UI, which is `code`.",
                "examples":["Create a paginated table component in React","The sidebar collapses incorrectly on mobile","Adjust the theme tokens so dark mode passes contrast checks"]})),
            ("general",json!({
                "what":"Anything none of the other options describe: greetings and small talk, questions about the assistant itself, process or planning questions, open-ended advice, or a request too vague to place. This is the deliberate no-match outcome.",
                "not_for":"Requests that clearly belong to another option even when phrased casually or briefly.",
                "examples":["Hi, how are you?","What should I work on next?","Can you help me with something?"]})),
            ("refactor",json!({
                "what":"Change the internal structure of code that already works, without changing what it does: renaming, extracting, de-duplicating, simplifying, reorganising modules, improving performance, or migrating to a different API or idiom.",
                "not_for":"Fixing behaviour that is wrong, which is `code`; or pointing out problems without changing anything, which is `review`.",
                "examples":["Extract this duplicated logic into a helper","Split this 800-line file into modules","Make this loop allocate less memory"]})),
            ("review",json!({
                "what":"Evaluate code, a diff or a design that already exists and report a judgement on it: correctness risks, quality, style, maintainability, or whether a change should be approved.",
                "not_for":"Explaining how something works without judging it, which is `analysis`; applying the improvements, which is `refactor`. If the judgement asked for is specifically about attacks, credentials or data exposure, prefer `security`.",
                "examples":["Review this pull request","Audit this module for bugs before I merge it","Is this the right approach for the cache layer?"]})),
            ("security",json!({
                "what":"Work whose subject is security: vulnerabilities, authentication and authorisation, secrets and credentials, injection, cryptography, dependency advisories, hardening, or meeting a security requirement.",
                "not_for":"General quality review with no security angle, which is `review`; ordinary bug fixing, which is `code`.",
                "examples":["Is this query vulnerable to SQL injection?","Stop leaking the API key into the logs and rotate it","Harden the file upload endpoint"]})),
            ("test",json!({
                "what":"Work whose subject is automated tests: writing or fixing unit, integration or end-to-end tests, fixtures, mocks or coverage, or diagnosing a failing or flaky test suite.",
                "not_for":"Fixing the production code that a failing test exposes as broken, which is `code`. Choose this only when the tests themselves are the deliverable.",
                "examples":["Write tests for the configuration parser","Why is this test flaky in CI?","Raise coverage on the router module"]})),
        ],
    )
}

fn complexity_question()->Question {
    Question::score(
        json!({
            "question":"How much work and how much of this codebase must an experienced software engineer take in to carry out `user_request` correctly and completely?",
            "focus":"Judge the difficulty of the underlying task, not the length, politeness or wording of the request. A single terse sentence can describe a repository-wide migration, and a long message can describe a one-line change.",
            "background":"`project` describes the repository and its languages and `candidate_files` lists files a keyword search matched. Use them to gauge how much of the system the work reaches into."
        }),
        [
            json!({"what":"Answerable straight away from general knowledge or a single obvious line: a factual question, a command or syntax lookup, a rename, a typo, a greeting. Nothing has to be read first to be confident the answer is right.","examples":["How do I list branches in git?","Fix this typo in the log message","Hi, how are you?"]}),
            json!({"what":"A self-contained change or explanation confined to one file or one function, where the right approach is obvious as soon as that code is in view. No design decision and no coordination between separate parts of the system.","examples":["Add a null check to this parser function","Explain what this single function does","Write a unit test for this pure helper"]}),
            json!({"what":"Several related files must be read and held together to get it right: a feature that touches a handful of call sites, a defect whose cause is somewhere other than where the symptom appears, or a change that must keep an existing interface or callers working.","examples":["Add a new provider adapter alongside the existing ones","Find why the cache returns stale results after a config reload","Thread a new option through the CLI, config and router"]}),
            json!({"what":"A design decision with consequences across the codebase is required first: a cross-cutting refactor or migration, work spanning several subsystems or layers, requirements ambiguous enough that they must be settled before coding, reasoning about concurrency, data integrity or security, or a change whose blast radius cannot be established without exploring the repository.","examples":["Migrate the whole persistence layer from JSON files to SQLite","Redesign how sessions and memory interact so multiple windows stay consistent","Make the orchestrator resilient to a provider failing mid-stream"]}),
        ],
    )
}

pub fn complexity_bucket(score:f64)->&'static str {
    if !score.is_finite()||score<0.5{COMPLEXITY_BUCKETS[0]}else if score<1.5{COMPLEXITY_BUCKETS[1]}else if score<2.5{COMPLEXITY_BUCKETS[2]}else{COMPLEXITY_BUCKETS[3]}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct RoutingDecision {
    pub intent:String,
    pub intent_confidence:f64,
    pub intent_probabilities:HashMap<String,f64>,
    pub complexity:String,
    pub complexity_score:f64,
    pub complexity_confidence:f64,
    pub complexity_probabilities:HashMap<String,f64>,
    pub needs_repository_context:f64,
    pub needs_tools:f64,
    pub is_destructive:f64,
    pub model:String,
    pub usage:Usage,
}

impl RoutingDecision {
    pub fn from_evaluation(evaluation:&Evaluation)->Result<Self> {
        let intent_answer=evaluation.require("intent")?;
        let intent=intent_answer.as_choice().ok_or_else(||anyhow!("the Jev returned `intent` as {} instead of `choice`",intent_answer.kind()))?;
        let intent=if INTENTS.contains(&intent){intent.to_string()}else{"general".to_string()};
        let complexity_answer=evaluation.require("complexity")?;
        let complexity_score=complexity_answer.as_score().ok_or_else(||anyhow!("the Jev returned `complexity` as {} instead of `score`",complexity_answer.kind()))?;
        let noul=|id:&str|->Result<f64>{let answer=evaluation.require(id)?;answer.as_noul().ok_or_else(||anyhow!("the Jev returned `{id}` as {} instead of `noul`",answer.kind()))};
        Ok(Self{
            intent,
            intent_confidence:intent_answer.confidence().unwrap_or(0.0),
            intent_probabilities:intent_answer.probabilities().cloned().unwrap_or_default(),
            complexity:complexity_bucket(complexity_score).into(),
            complexity_score,
            complexity_confidence:complexity_answer.confidence().unwrap_or(0.0),
            complexity_probabilities:complexity_answer.probabilities().cloned().unwrap_or_default(),
            needs_repository_context:noul("needs_repository_context")?,
            needs_tools:noul("needs_tools")?,
            is_destructive:noul("is_destructive")?,
            model:evaluation.model.clone(),
            usage:evaluation.usage,
        })
    }
    pub fn lowest_confidence(&self)->f64{self.intent_confidence.min(self.complexity_confidence)}
    pub fn is_confident(&self,threshold:f64)->bool{self.lowest_confidence()>=threshold}
    pub fn holds(probability:f64,threshold:f64)->bool{probability>=threshold}
}

#[derive(Debug,Clone,Default,PartialEq,Serialize,Deserialize)]
pub struct VerificationInput {
    pub request:String,
    pub repository_context:String,
    pub answer:String,
}
impl VerificationInput {
    pub fn new(request:impl Into<String>,answer:impl Into<String>)->Self{Self{request:request.into(),answer:answer.into(),..Default::default()}}
    pub fn with_repository_context(mut self,context:impl Into<String>)->Self{self.repository_context=context.into().trim().to_string();self}
    pub fn with_context_blocks<P:AsRef<str>,C:AsRef<str>>(self,blocks:impl IntoIterator<Item=(P,C)>)->Self{let joined=blocks.into_iter().map(|(path,content)|format!("FILE: {}\n{}",path.as_ref(),content.as_ref())).collect::<Vec<_>>().join("\n\n");self.with_repository_context(joined)}
    pub fn has_context(&self)->bool{!self.repository_context.trim().is_empty()}
}

pub fn verification_state(input:&VerificationInput)->Value{json!({"user_request":input.request,"repository_context":input.repository_context,"assistant_answer":input.answer})}

pub fn verification_request(input:&VerificationInput,model:impl Into<String>)->Request{Request::new(verification_state(input),model,verification_questions())}

pub fn verification_questions()->BTreeMap<String,Question> {
    BTreeMap::from([
        ("unsupported_claims".to_string(),Question::noul_with(
            json!({"question":"Does `assistant_answer` present something about this repository as a fact that `repository_context` does not support?","context":"`repository_context` holds the `FILE: <path>` blocks that were the assistant's only evidence about this repository while it answered `user_request`. Treat those blocks as the whole of the repository that was visible to it.","guidance":"Check each statement the answer makes about this repository — paths, modules, symbols, signatures, values, call sites, existing behaviour — against those blocks. Judge only whether the evidence is there; ignore style, length and whether the advice is good."}),
            json!({"when":"At least one statement about this repository is stated as fact and the blocks neither show it nor imply it.","examples":["names a file, module, symbol or setting that appears in no block","states a signature, default value or return type that differs from the one shown","describes existing behaviour that the code shown contradicts","attributes to this project a dependency, convention or layout that no block shows"]}),
            json!({"when":"Every statement about this repository can be traced to the blocks, allowing for paraphrase and summary.","also":"An answer that makes no factual claim about this repository belongs here too: general knowledge, restating the developer's own words, or code it openly offers as new rather than describing as already present.","not_a_defect":["saying that something is absent from the supplied context","declining to guess about code it was not shown"]}))),
        ("addresses_request".to_string(),Question::noul_with(
            json!({"question":"Does `assistant_answer` respond to what `user_request` actually asked for?","guidance":"Judge the fit between the request and the reply, not whether the reply is correct, grounded or thorough. The developer may write in any language — judge the meaning the same way whatever the language."}),
            json!({"when":"The reply delivers the kind of thing that was asked for — the explanation, the code, the review, the plan or the decision — even if it is partial or imperfect.","also":"Directly refusing the request, or asking for one detail genuinely needed to proceed, still counts as addressing it."}),
            json!({"when":"The reply is about something other than the request, or says nothing usable at all.","examples":["solves a different problem or answers a question that was not asked","generic filler, an apology or a restatement with no substance","empty, cut off before it says anything, or just a transport or provider error message"]}))),
        ("verifiable_claims".to_string(),Question::noul_with(
            json!({"question":"Does `assistant_answer` make any statement about this repository whose truth could be confirmed or refuted by `repository_context`?","guidance":"This asks only whether repository-specific claims are present, never whether they are right. A reply that needs no repository grounding is a normal, correct outcome, not a defect."}),
            json!({"when":"The reply describes this particular codebase: it names a file, symbol, signature, setting or existing behaviour of this project, or asserts that something here does or does not exist."}),
            json!({"when":"Nothing in the reply depends on this repository, so the blocks could neither confirm nor refute it.","examples":["a definition, or how a language, library or tool works in general","new code the reply proposes, presented as something to add rather than as something already here","a plan, an opinion, a question back to the developer, or small talk"]}))),
    ])
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum VerificationOutcome{Grounded,Unverifiable,OffTopic,Fabricated}
impl VerificationOutcome {
    pub fn as_str(&self)->&'static str{match self{Self::Grounded=>"grounded",Self::Unverifiable=>"unverifiable",Self::OffTopic=>"off_topic",Self::Fabricated=>"fabricated"}}
    pub fn is_acceptable(&self)->bool{matches!(self,Self::Grounded|Self::Unverifiable)}
}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct VerificationVerdict {
    pub unsupported_claims:f64,
    pub addresses_request:f64,
    pub verifiable_claims:f64,
    pub checked:bool,
    pub model:String,
    pub usage:Usage,
}

impl VerificationVerdict {
    pub fn from_evaluation(evaluation:&Evaluation)->Result<Self> {
        let noul=|id:&str|->Result<f64>{let answer=evaluation.require(id)?;answer.as_noul().ok_or_else(||anyhow!("the Jev returned `{id}` as {} instead of `noul`",answer.kind()))};
        Ok(Self{
            unsupported_claims:noul("unsupported_claims")?,
            addresses_request:noul("addresses_request")?,
            verifiable_claims:noul("verifiable_claims")?,
            checked:true,
            model:evaluation.model.clone(),
            usage:evaluation.usage,
        })
    }
    pub fn unchecked(model:impl Into<String>)->Self{Self{unsupported_claims:0.0,addresses_request:0.0,verifiable_claims:0.0,checked:false,model:model.into(),usage:Usage::default()}}
    pub fn outcome(&self,threshold:f64)->VerificationOutcome {
        let threshold=if threshold.is_finite(){threshold.clamp(0.0,1.0)}else{1.0};
        if !self.checked{VerificationOutcome::Unverifiable}
        else if RoutingDecision::holds(1.0-self.addresses_request,threshold){VerificationOutcome::OffTopic}
        else if RoutingDecision::holds(self.unsupported_claims,threshold){VerificationOutcome::Fabricated}
        else if RoutingDecision::holds(1.0-self.verifiable_claims,threshold){VerificationOutcome::Unverifiable}
        else{VerificationOutcome::Grounded}
    }
    pub fn is_acceptable(&self,threshold:f64)->bool{self.outcome(threshold).is_acceptable()}
}

#[cfg(test)] mod tests {
    use super::*;

    fn question<'a>(map:&'a BTreeMap<String,Question>,id:&str)->&'a Question{map.get(id).expect("pergunta de roteamento")}

    /// As linhas `('a', 'b', $json$…$json$::jsonb…)` do seed: os campos entre
    /// aspas antes do JSON, e o JSON.
    fn seed_rows(seed:&str)->Vec<(Vec<String>,Value)> {
        seed.lines().filter(|line|line.trim_start().starts_with("('")).map(|line|{
            let mut parts=line.split("$json$");
            let head=parts.next().expect("cabeça");
            let fields=head.trim().trim_start_matches('(').split(',').map(|field|field.trim().trim_matches('\'').to_string()).filter(|field|!field.is_empty()).collect();
            (fields,serde_json::from_str(parts.next().expect("json")).expect("json do seed"))
        }).collect()
    }

    /// O seed sai destas mesmas funções. Uma pergunta que mude aqui sem o seed
    /// ser gerado de novo deixaria o Supabase perguntando a versão antiga.
    #[test] fn a_jev_call_is_spent_under_its_set() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{},"usage":{"input_tokens":296,"output_tokens":20}}"#).unwrap();
        let spend=spend_of("entry",Some(&evaluation),40);
        assert_eq!((spend.source.as_str(),spend.model.as_str(),spend.input_tokens,spend.output_tokens,spend.duration_ms),("jev:entry","jev-1.13.0",296,20,40));
        let failed=spend_of("asking",None,5);
        assert!(!failed.success);
        assert_eq!(failed.input_tokens,0);
    }

    #[test] fn the_daily_calls_come_from_the_headers() {
        let mut headers=reqwest::header::HeaderMap::new();
        let now=chrono::DateTime::parse_from_rfc3339("2026-10-01T15:00:00Z").unwrap().to_utc();
        assert!(daily_quota(&headers,now).is_none());
        headers.insert("x-jev-calls-used","125".parse().unwrap());
        headers.insert("x-jev-daily-limit","500".parse().unwrap());
        let quota=daily_quota(&headers,now).unwrap();
        assert_eq!(quota.used_percent,Some(25.0));
        assert_eq!(quota.plan.as_deref(),Some("125/500"));
        assert_eq!(quota.resets_at.as_deref(),Some("2026-10-02T00:00:00Z"));
    }

    /// O seed mora no repositório `JayV-Coder/supabase`, clonado ao lado deste
    /// (ou onde `JAYV_SUPABASE_REPO` apontar). Sem ele o teste avisa e passa:
    /// não há contra o que comparar.
    fn jev_seed()->Option<String> {
        const SEED:&str="20261001120100_seed_jev_en.sql";
        let repository=std::env::var_os("JAYV_SUPABASE_REPO").map(std::path::PathBuf::from)
            .unwrap_or_else(||std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../supabase"));
        ["supabase/migrations","migrations"].iter().find_map(|folder|std::fs::read_to_string(repository.join(folder).join(SEED)).ok())
    }

    #[test]
    fn the_jev_seed_is_what_rust_asks_today() {
        let Some(seed)=jev_seed() else { eprintln!("seed do Jev não encontrado: clone JayV-Coder/supabase ao lado ou defina JAYV_SUPABASE_REPO"); return };
        let rows=seed_rows(&seed);
        let mut sets:BTreeMap<String,BTreeMap<String,Question>>=BTreeMap::new();
        let mut parameters:BTreeMap<String,Value>=BTreeMap::new();
        for (fields,value) in rows {
            match fields.as_slice() {
                [set,id]=>{sets.entry(set.clone()).or_default().insert(id.clone(),serde_json::from_value(value).expect("pergunta do seed"));}
                [key]=>{parameters.insert(key.clone(),value);}
                other=>panic!("linha fora do formato: {other:?}"),
            }
        }
        assert_eq!(sets["entry"],crate::gatekeeper::entry_questions());
        assert_eq!(sets["routing"],routing_questions());
        assert_eq!(sets["verification"],verification_questions());
        assert_eq!(sets["asking"],crate::asking::questions());
        for id in crate::gatekeeper::ENTRY_QUESTION_IDS {assert!(sets["entry"].contains_key(id),"{id}");}
        for id in ROUTING_QUESTION_IDS {assert!(sets["routing"].contains_key(id),"{id}");}
        for id in VERIFICATION_QUESTION_IDS {assert!(sets["verification"].contains_key(id),"{id}");}
        for id in [crate::asking::KIND_QUESTION,crate::asking::OPTIONS_QUESTION] {assert!(sets["asking"].contains_key(id),"{id}");}
        assert_eq!(parameters,crate::gatekeeper::parameters());
    }

    #[test]
    fn serializes_the_documented_noul_wire_shape() {
        assert_eq!(serde_json::to_value(Question::noul("Does this convey urgency?")).unwrap(),json!({"type":"noul","instructions":"Does this convey urgency?"}));
        assert_eq!(serde_json::to_value(Question::noul_with("Has the customer contacted support about this before?","Mentions a prior attempt, ticket, or that they have asked before","No sign of any previous contact")).unwrap(),
            json!({"type":"noul","instructions":"Has the customer contacted support about this before?","criteria":{"true":"Mentions a prior attempt, ticket, or that they have asked before","false":"No sign of any previous contact"}}));
    }

    #[test]
    fn serializes_the_documented_choice_wire_shape() {
        let asked=Question::choice("Which team should handle this?",[("returns",json!("Exchanges, wrong or damaged items")),("shipping",json!("Delivery status, delays, lost packages")),("billing",json!("Charges, invoices, payment problems"))]);
        assert_eq!(serde_json::to_value(asked).unwrap(),json!({"type":"choice","instructions":"Which team should handle this?","criteria":{"returns":"Exchanges, wrong or damaged items","shipping":"Delivery status, delays, lost packages","billing":"Charges, invoices, payment problems"}}));
    }

    #[test]
    fn serializes_the_documented_score_wire_shape() {
        let asked=Question::score("How severe is the reported issue?",["Cosmetic; no impact to functionality","Broken or degraded feature, but workaround exists","Blocking issue; no workaround exists"]);
        assert_eq!(serde_json::to_value(asked).unwrap(),json!({"type":"score","instructions":"How severe is the reported issue?","criteria":["Cosmetic; no impact to functionality","Broken or degraded feature, but workaround exists","Blocking issue; no workaround exists"]}));
    }

    #[test]
    fn serializes_the_documented_request_body() {
        let request=Request::new("Help! My payouts have been failing for 3 days.","jev-latest",BTreeMap::from([("is_urgent".to_string(),Question::noul("Does this convey urgency?"))]));
        assert_eq!(serde_json::to_value(&request).unwrap(),json!({"state":"Help! My payouts have been failing for 3 days.","model":"jev-latest","questions":{"is_urgent":{"type":"noul","instructions":"Does this convey urgency?"}}}));
        assert!(request.validate().is_ok());
    }

    #[test]
    fn deserializes_the_documented_noul_response() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"is_urgent":{"type":"noul","noul":0.95}},"usage":{"input_tokens":296,"output_tokens":20}}"#).unwrap();
        assert_eq!(evaluation.model,"jev-1.13.0");
        assert_eq!(evaluation.noul("is_urgent"),Some(0.95));
        assert_eq!(evaluation.confidence("is_urgent"),None);
        assert_eq!(evaluation.usage,Usage{input_tokens:296,output_tokens:20});
    }

    #[test]
    fn deserializes_the_documented_choice_response() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"department":{"type":"choice","choice":"returns","confidence":1.0,"probabilities":{"shipping":0.0,"returns":1.0,"billing":0.0}}},"usage":{"input_tokens":328,"output_tokens":34}}"#).unwrap();
        assert_eq!(evaluation.choice("department"),Some("returns"));
        assert_eq!(evaluation.confidence("department"),Some(1.0));
        assert_eq!(evaluation.answer("department").unwrap().probability("billing"),0.0);
        assert_eq!(evaluation.probabilities("department").len(),3);
    }

    #[test]
    fn deserializes_the_documented_score_response() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"bug_severity":{"type":"score","score":1.43,"confidence":0.35,"legend":{"0":"Cosmetic; no impact to functionality","1":"Broken or degraded feature, but workaround exists","2":"Blocking issue; no workaround exists"},"probabilities":{"0":0.0,"1":0.57,"2":0.43}}},"usage":{"input_tokens":332,"output_tokens":18}}"#).unwrap();
        assert_eq!(evaluation.score("bug_severity"),Some(1.43));
        assert_eq!(evaluation.confidence("bug_severity"),Some(0.35));
        assert_eq!(evaluation.answer("bug_severity").unwrap().legend().unwrap().len(),3);
        assert_eq!(evaluation.answer("bug_severity").unwrap().probability("1"),0.57);
        assert_eq!(evaluation.noul("bug_severity"),None);
    }

    #[test]
    fn routing_questions_cover_the_orchestrator_taxonomy() {
        let questions=routing_questions();
        let mut ids=questions.keys().map(String::as_str).collect::<Vec<_>>(); ids.sort();
        assert_eq!(ids,ROUTING_QUESTION_IDS.to_vec());
        for (id,asked) in &questions {asked.validate().unwrap_or_else(|error|panic!("{id}: {error}"));}
        let intent=question(&questions,"intent");
        assert_eq!(intent.kind(),"choice");
        let mut options=intent.options(); options.sort();
        let mut expected=INTENTS.to_vec(); expected.sort();
        assert_eq!(options,expected);
        assert!(intent.options().contains(&"general"));
        let complexity=question(&questions,"complexity");
        assert_eq!(complexity.kind(),"score");
        assert!((MIN_SCORE_LEVELS..=MAX_SCORE_LEVELS).contains(&complexity.levels()));
        assert_eq!(complexity.levels(),COMPLEXITY_BUCKETS.len());
        for id in ["needs_repository_context","needs_tools","is_destructive"] {assert_eq!(question(&questions,id).kind(),"noul");}
    }

    #[test]
    fn routing_state_names_every_field_the_instructions_reference() {
        let state=routing_state(&RoutingInput::new("Refatore o roteador").with_project("",vec!["Rust".into()]).with_candidate_files(vec!["src/router.rs".into()]));
        assert_eq!(state.pointer("/user_request").and_then(Value::as_str),Some("Refatore o roteador"));
        assert_eq!(state.pointer("/project/name").and_then(Value::as_str),Some(""));
        assert_eq!(state.pointer("/candidate_files/0").and_then(Value::as_str),Some("src/router.rs"));
        assert!(state.pointer("/recent_turns").is_none());
    }

    #[test]
    fn maps_the_score_onto_the_budget_buckets_at_the_boundaries() {
        for (score,bucket) in [(-1.0,"trivial"),(0.0,"trivial"),(0.49,"trivial"),(0.5,"simple"),(1.49,"simple"),(1.5,"medium"),(2.49,"medium"),(2.5,"complex"),(3.0,"complex"),(9.0,"complex"),(f64::NAN,"trivial")] {
            assert_eq!(complexity_bucket(score),bucket,"score {score}");
        }
    }

    #[test]
    fn retries_only_transient_statuses() {
        assert!(retryable_status(429)); assert!(retryable_status(529)); assert!(retryable_status(503));
        assert!(!retryable_status(401)); assert!(!retryable_status(422)); assert!(!retryable_status(400)); assert!(!retryable_status(200));
        assert!(backoff(0)<backoff(1)); assert!(backoff(9)<=Duration::from_secs(8));
    }

    #[test]
    fn surfaces_validation_detail_and_the_session_message() {
        let invalid=status_error(422,r#"{"detail":"questions.complexity.criteria: must contain at least 2 items"}"#).to_string();
        assert!(invalid.contains("422") && invalid.contains("questions.complexity.criteria"),"{invalid}");
        let unauthorized=status_error(401,r#"{"error":"sessão inválida ou expirada","code":"session"}"#).to_string();
        assert!(unauthorized.contains("session expired") && unauthorized.contains("local heuristics"),"{unauthorized}");
        assert_eq!(error_detail("boom"),"boom");
    }

    /// O limite do dia não passa esperando alguns segundos: tentar de novo
    /// só gastaria tempo. O 429 da TypeSafe, sim, é passageiro.
    #[test]
    fn the_daily_limit_is_not_retried() {
        let body=r#"{"error":"limite diário do Jev atingido","code":"daily_limit"}"#;
        assert!(status_error(429,body).to_string().contains("daily Jev limit"));
        assert!(!worth_retrying(429,body));
        assert!(worth_retrying(429,r#"{"error":{"message":"rate limited"}}"#));
        assert!(worth_retrying(503,""));
    }

    #[test]
    fn rejects_questions_the_api_would_refuse() {
        assert!(Question::choice("qual?",Vec::<(String,Value)>::new()).validate().is_err());
        assert!(Question::score("quanto?",["apenas um nível"]).validate().is_err());
        assert!(Question::score("quanto?",(0..11).map(|level|format!("nível {level}")).collect::<Vec<_>>()).validate().is_err());
        assert!(Question::score("quanto?",["baixo","alto"]).validate().is_ok());
        assert!(Request::new("estado","jev-latest",BTreeMap::new()).validate().is_err());
    }

    #[test]
    fn builds_a_routing_decision_from_a_batched_evaluation() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "intent":{"type":"choice","choice":"refactor","confidence":0.82,"probabilities":{"refactor":0.82,"code":0.12,"review":0.06}},
            "complexity":{"type":"score","score":2.6,"confidence":0.74,"legend":{"0":"a","1":"b","2":"c","3":"d"},"probabilities":{"0":0.0,"1":0.0,"2":0.4,"3":0.6}},
            "needs_repository_context":{"type":"noul","noul":0.97},
            "needs_tools":{"type":"noul","noul":0.61},
            "is_destructive":{"type":"noul","noul":0.88}},
            "usage":{"input_tokens":1200,"output_tokens":64}}"#).unwrap();
        let decision=RoutingDecision::from_evaluation(&evaluation).expect("decisão de roteamento");
        assert_eq!(decision.intent,"refactor");
        assert_eq!(decision.complexity,"complex");
        assert_eq!(decision.complexity_score,2.6);
        assert_eq!(decision.needs_repository_context,0.97);
        assert_eq!(decision.is_destructive,0.88);
        assert_eq!(decision.intent_probabilities.get("code"),Some(&0.12));
        assert_eq!(decision.model,"jev-1.13.0");
        assert_eq!(decision.usage.input_tokens,1200);
        assert_eq!(decision.lowest_confidence(),0.74);
        assert!(decision.is_confident(0.7) && !decision.is_confident(0.8));
        assert!(RoutingDecision::holds(decision.needs_tools,0.6));
    }

    #[test]
    fn falls_back_to_general_and_reports_missing_answers() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "intent":{"type":"choice","choice":"marketing","confidence":0.4,"probabilities":{"marketing":0.4}},
            "complexity":{"type":"score","score":0.2,"confidence":0.9,"probabilities":{"0":0.8,"1":0.2}},
            "needs_repository_context":{"type":"noul","noul":0.1},
            "needs_tools":{"type":"noul","noul":0.1},
            "is_destructive":{"type":"noul","noul":0.02}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        let decision=RoutingDecision::from_evaluation(&evaluation).expect("decisão de roteamento");
        assert_eq!(decision.intent,"general");
        assert_eq!(decision.complexity,"trivial");
        let partial:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"intent":{"type":"choice","choice":"code","confidence":1.0,"probabilities":{"code":1.0}}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        assert!(RoutingDecision::from_evaluation(&partial).unwrap_err().to_string().contains("complexity"));
    }

    fn verification_fixture()->VerificationInput {
        VerificationInput::new("Onde o roteador escolhe o modelo?","`select_model` em `src/router.rs` ordena os modelos por score.")
            .with_context_blocks([("src/router.rs","pub fn select_model() {}")])
    }
    fn verdict(unsupported:f64,addresses:f64,verifiable:f64)->VerificationVerdict{VerificationVerdict{unsupported_claims:unsupported,addresses_request:addresses,verifiable_claims:verifiable,checked:true,model:"jev-1.13.0".into(),usage:Usage::default()}}

    #[test]
    fn verification_questions_are_three_batched_nouls_with_explicit_boundaries() {
        let questions=verification_questions();
        let mut ids=questions.keys().map(String::as_str).collect::<Vec<_>>(); ids.sort();
        assert_eq!(ids,VERIFICATION_QUESTION_IDS.to_vec());
        for (id,asked) in &questions {
            asked.validate().unwrap_or_else(|error|panic!("{id}: {error}"));
            assert_eq!(asked.kind(),"noul","{id}");
            let Question::Noul{instructions,criteria}=asked else {panic!("{id} não é um noul")};
            let criteria=criteria.as_ref().unwrap_or_else(||panic!("{id} precisa de `criteria` explícito"));
            assert!(instructions.get("question").and_then(Value::as_str).is_some_and(|text|text.len()>40),"{id}");
            assert!(criteria.yes.is_object() && criteria.no.is_object(),"{id}");
            assert!(!serde_json::to_string(asked).expect("json").contains(id.as_str()),"{id} depende do próprio identificador");
        }
    }

    #[test]
    fn verification_instructions_only_reference_state_fields_that_exist() {
        let state=verification_state(&verification_fixture());
        for field in ["user_request","repository_context","assistant_answer"] {assert!(state.get(field).and_then(Value::as_str).is_some(),"{field}");}
        assert_eq!(state.pointer("/repository_context").and_then(Value::as_str),Some("FILE: src/router.rs\npub fn select_model() {}"));
        let wire=serde_json::to_string(&verification_questions()).expect("json");
        for quoted in ["`user_request`","`repository_context`","`assistant_answer`"] {
            let field=quoted.trim_matches('`');
            assert!(wire.contains(quoted),"nenhuma pergunta cita {quoted}");
            assert!(state.get(field).is_some(),"{field} citado mas ausente do estado");
        }
    }

    #[test]
    fn serializes_one_batched_verification_request() {
        let request=verification_request(&verification_fixture(),"jev-latest");
        request.validate().expect("requisição de verificação");
        let body=serde_json::to_value(&request).expect("json");
        assert_eq!(body.pointer("/model").and_then(Value::as_str),Some("jev-latest"));
        assert_eq!(body.pointer("/state/user_request").and_then(Value::as_str),Some("Onde o roteador escolhe o modelo?"));
        assert_eq!(body.pointer("/questions").and_then(Value::as_object).expect("perguntas").len(),VERIFICATION_QUESTION_IDS.len());
        for id in VERIFICATION_QUESTION_IDS {
            assert_eq!(body.pointer(&format!("/questions/{id}/type")).and_then(Value::as_str),Some("noul"),"{id}");
            assert!(body.pointer(&format!("/questions/{id}/criteria/true")).is_some() && body.pointer(&format!("/questions/{id}/criteria/false")).is_some(),"{id}");
        }
    }

    #[test]
    fn builds_a_verdict_from_a_batched_evaluation() {
        let evaluation:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "unsupported_claims":{"type":"noul","noul":0.91},
            "addresses_request":{"type":"noul","noul":0.88},
            "verifiable_claims":{"type":"noul","noul":0.96}},
            "usage":{"input_tokens":2480,"output_tokens":24}}"#).unwrap();
        let verdict=VerificationVerdict::from_evaluation(&evaluation).expect("veredito");
        assert!(verdict.checked);
        assert_eq!((verdict.unsupported_claims,verdict.addresses_request,verdict.verifiable_claims),(0.91,0.88,0.96));
        assert_eq!(verdict.model,"jev-1.13.0");
        assert_eq!(verdict.usage,Usage{input_tokens:2480,output_tokens:24});
        assert_eq!(verdict.outcome(0.7),VerificationOutcome::Fabricated);
        assert!(!verdict.is_acceptable(0.7));
    }

    #[test]
    fn reports_missing_and_mistyped_verification_answers() {
        let partial:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"unsupported_claims":{"type":"noul","noul":0.1}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        assert!(VerificationVerdict::from_evaluation(&partial).unwrap_err().to_string().contains("addresses_request"));
        let mistyped:Evaluation=serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{
            "unsupported_claims":{"type":"score","score":1.0,"confidence":0.5,"probabilities":{"0":0.5,"1":0.5}},
            "addresses_request":{"type":"noul","noul":0.9},
            "verifiable_claims":{"type":"noul","noul":0.9}},"usage":{"input_tokens":1,"output_tokens":1}}"#).unwrap();
        let error=VerificationVerdict::from_evaluation(&mistyped).unwrap_err().to_string();
        assert!(error.contains("unsupported_claims") && error.contains("score"),"{error}");
    }

    #[test]
    fn an_empty_repository_context_is_unverifiable_and_never_blocks_the_answer() {
        let input=VerificationInput::new("O que é uma mônada?","Uma mônada é um monoide na categoria dos endofuntores.");
        assert!(!input.has_context());
        assert!(!VerificationInput::new("pergunta","resposta").with_repository_context("   \n  ").has_context());
        let verdict=VerificationVerdict::unchecked(DEFAULT_MODEL);
        assert!(!verdict.checked);
        assert_eq!(verdict.usage,Usage::default());
        for threshold in [0.5,0.7,0.9,1.0] {
            assert_eq!(verdict.outcome(threshold),VerificationOutcome::Unverifiable,"{threshold}");
            assert!(verdict.is_acceptable(threshold),"{threshold}");
        }
    }

    #[test]
    fn the_policy_helper_uses_the_threshold_the_caller_supplies() {
        assert_eq!(verdict(0.02,0.97,0.99).outcome(0.7),VerificationOutcome::Grounded);
        assert_eq!(verdict(0.80,0.97,0.99).outcome(0.7),VerificationOutcome::Fabricated);
        assert_eq!(verdict(0.80,0.97,0.99).outcome(0.9),VerificationOutcome::Grounded);
        assert_eq!(verdict(0.99,0.10,0.99).outcome(0.7),VerificationOutcome::OffTopic);
        assert_eq!(verdict(0.02,0.99,0.04).outcome(0.7),VerificationOutcome::Unverifiable);
        assert!(verdict(0.02,0.99,0.04).is_acceptable(0.7));
        assert_eq!(verdict(0.99,0.99,0.04).outcome(0.7),VerificationOutcome::Fabricated);
        assert!(!verdict(0.99,0.99,0.04).is_acceptable(0.7));
        assert_eq!(verdict(0.55,0.55,0.55).outcome(0.7),VerificationOutcome::Grounded);
        assert_eq!(verdict(0.99,0.99,0.99).outcome(f64::NAN),VerificationOutcome::Grounded);
        assert_eq!(VerificationOutcome::OffTopic.as_str(),"off_topic");
        assert_eq!(serde_json::to_value(VerificationOutcome::Fabricated).unwrap(),json!("fabricated"));
        assert!(!VerificationOutcome::Fabricated.is_acceptable() && !VerificationOutcome::OffTopic.is_acceptable());
    }

    #[test]
    fn reuses_the_shared_backoff_without_changing_typesafe_retry_semantics() {
        for status in [429u16,529,500,502,503,504] {assert!(retryable_status(status) && crate::providers::is_retryable_status(status),"{status}");}
        for status in [401u16,422,400,403,404,200] {assert!(!retryable_status(status) && !crate::providers::is_retryable_status(status),"{status}");}
        assert_eq!(retry_policy(0).attempts,1);
        assert_eq!(retry_policy(DEFAULT_ATTEMPTS).attempts,DEFAULT_ATTEMPTS);
        let policy=retry_policy(DEFAULT_ATTEMPTS);
        assert!(policy.delay(1,None)<=backoff(0));
        assert!(policy.delay(20,None)<=backoff(9));
        assert_eq!(policy.delay(1,Some(Duration::from_secs(600))),backoff(9));
    }

    #[test]
    fn the_jev_is_the_project_function_called_with_the_session() {
        assert!(Client::for_session("   ").is_err());
        let client=Client::for_session("jwt").expect("cliente");
        assert_eq!(client.endpoint(),format!("{}/functions/v1/jev",crate::cloud::PROJECT_URL));
        assert_eq!(call_body("entry",json!({"user_request":"oi"}),None),json!({"set":"entry","state":{"user_request":"oi"}}));
        assert_eq!(call_body("asking",json!({}),Some(&["kind"])),json!({"set":"asking","state":{},"include":["kind"]}));
    }
}
