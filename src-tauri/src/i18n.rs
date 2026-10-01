//! O núcleo não escreve frase para pessoa nenhuma. Devolve a chave do i18n e
//! os valores que ela cita, e a tela monta a frase no idioma de quem lê
//! (`src/modules/i18n`). Um valor pode ser outra chave — o tamanho do pedido,
//! o nome de um campo —, e a tela traduz os dois.
//!
//! O que fica gravado no chat como aviso (pedido barrado, execução que falhou)
//! vai como `notice`: linhas de `Text` atrás de um prefixo. O modelo lê o
//! histórico em inglês, então `for_model` diz o mesmo aviso em inglês.

use crate::cloud::session::SessionError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(untagged)]
pub enum Param{Text(Text),Plain(String)}

impl From<Text> for Param { fn from(text:Text)->Self{Self::Text(text)} }
impl From<String> for Param { fn from(text:String)->Self{Self::Plain(text)} }
impl From<&str> for Param { fn from(text:&str)->Self{Self::Plain(text.into())} }
impl From<&String> for Param { fn from(text:&String)->Self{Self::Plain(text.clone())} }
macro_rules! plain_numbers { ($($kind:ty),*) => { $(impl From<$kind> for Param { fn from(number:$kind)->Self{Self::Plain(number.to_string())} })* } }
plain_numbers!(u8,u16,u32,u64,usize,i32,i64);

/// Um texto que a tela traduz: a chave do i18n e os valores que ela cita.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct Text{pub key:String,#[serde(default,skip_serializing_if="BTreeMap::is_empty")] pub params:BTreeMap<String,Param>}

impl Text {
    pub fn new(key:&str)->Self{Self{key:key.into(),params:BTreeMap::new()}}
    pub fn with(mut self,name:&str,value:impl Into<Param>)->Self{self.params.insert(name.into(),value.into());self}
    /// O erro de algo que não tem chave própria: a tela diz que deu errado e
    /// mostra o motivo técnico, que é em inglês.
    pub fn unexpected(reason:impl fmt::Display)->Self{Self::new("error.unexpected").with("reason",reason.to_string())}
    fn param(&self,name:&str)->String{self.params.get(name).map(|param|match param{Param::Text(text)=>english(text),Param::Plain(text)=>text.clone()}).unwrap_or_default()}
}

/// Para logs e para o modelo: o texto em inglês.
impl fmt::Display for Text { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(&english(self))} }
impl std::error::Error for Text {}

/// Qualquer erro, como a tela o recebe: a chave que alguém da corrente já
/// escolheu ou, na falta dela, o erro inesperado com o motivo técnico.
impl From<anyhow::Error> for Text {
    fn from(error:anyhow::Error)->Self {
        // O `downcast_ref` do próprio erro enxerga o que foi posto como
        // contexto; a corrente, o que veio por baixo.
        if let Some(text)=error.downcast_ref::<Text>() {return text.clone();}
        for cause in error.chain() {
            if let Some(text)=cause.downcast_ref::<Text>() {return text.clone();}
            if let Some(session)=cause.downcast_ref::<SessionError>() {return session.into();}
        }
        Self::unexpected(format!("{error:#}"))
    }
}

impl From<&SessionError> for Text {
    fn from(error:&SessionError)->Self {
        match error {
            SessionError::Expired=>Self::new("session.expired"),
            SessionError::UnknownKey=>Self::new("session.unknownKey"),
            SessionError::Invalid(reason)=>Self::new("session.invalid").with("reason",reason),
        }
    }
}
impl From<SessionError> for Text { fn from(error:SessionError)->Self{(&error).into()} }

/// O erro de um comando do Tauri: a tela recebe `{ key, params }`.
pub fn failure(error:impl Into<anyhow::Error>)->Text{Text::from(error.into())}

/// O idioma que o desenvolvedor escolheu no app: o modelo responde nele.
#[derive(Debug,Clone,PartialEq,Eq,Deserialize)]
pub struct ReplyLanguage{pub tag:String,pub name:String}

static REPLY_LANGUAGE:std::sync::RwLock<Option<ReplyLanguage>>=std::sync::RwLock::new(None);

/// A tela avisa o idioma ao abrir e a cada troca. Fica fora do orquestrador de
/// propósito: trocar o idioma não espera o pedido que está no ar.
pub fn set_reply_language(language:Option<ReplyLanguage>) {
    let language=language.filter(|language|!language.tag.trim().is_empty());
    *REPLY_LANGUAGE.write().unwrap_or_else(|poisoned|poisoned.into_inner())=language;
}

pub fn reply_language()->Option<ReplyLanguage>{REPLY_LANGUAGE.read().unwrap_or_else(|poisoned|poisoned.into_inner()).clone()}

const NOTICE:&str="jayv:notice:";

/// Um aviso gravado como mensagem do chat.
pub fn notice(lines:&[Text])->String{format!("{NOTICE}{}",serde_json::to_string(lines).unwrap_or_default())}

pub fn read_notice(content:&str)->Option<Vec<Text>>{serde_json::from_str(content.strip_prefix(NOTICE)?).ok()}

/// A mensagem como o modelo a lê: o aviso em inglês, o resto como está.
pub fn for_model(content:&str)->String {
    match read_notice(content) {
        Some(lines)=>lines.iter().map(english).collect::<Vec<_>>().join("\n"),
        None=>content.to_string(),
    }
}

/// O inglês dos avisos que chegam ao modelo. O resto sai como a chave e os
/// valores, que bastam para um log.
fn english(text:&Text)->String {
    let param=|name:&str|text.param(name);
    match text.key.as_str() {
        "ask.answer"=>format!("Answer to the question «{}»: {}",param("question"),param("answer")),
        "ask.yes"=>"YES".into(),
        "ask.no"=>"NO".into(),
        "gate.blocked"=>format!("The JayV entry gate blocked this request with {} out of 100 (the minimum for a {} is {}).",param("score"),param("scope"),param("demand")),
        "gate.missing"=>"What is missing:".into(),
        "gate.missing.item"=>format!("- {}: {}",param("criterion"),param("reading")),
        "turn.noAnswer"=>"The run ended without an answer.".into(),
        "guidance.failed"=>format!("The request could not run because {}.",param("problem")),
        "guidance.fix"=>"Open Settings, turn an agent on and keep at least one of its models active.".into(),
        "guidance.nothingConfigured"=>"no LLM provider or model is configured".into(),
        "guidance.noModels"=>"providers are declared but no model is configured".into(),
        "guidance.noAgent"=>"no agent is on".into(),
        "guidance.noFittingModel"=>"no configured model fits this request".into(),
        "guidance.unknownProvider"=>format!("the selected model points to the provider `{}`, which does not exist",param("provider")),
        "explain.last"=>format!("The last request used {} through {}. The context had {} files and an estimated budget of {} tokens.",param("model"),param("provider"),param("files"),param("tokens")),
        "explain.none"=>"There is no previous routing decision in this session.".into(),
        key if key.starts_with("scope.")=>key[6..].parse::<usize>().ok().and_then(|level|crate::gatekeeper::SCOPE_LEVELS.get(level)).map_or_else(||key.to_string(),|scope|scope.to_string()),
        key if key.starts_with("criterion.")=>key[10..].replace('_'," ").replace('.'," "),
        key if text.params.is_empty()=>key.to_string(),
        key=>format!("{key} ({})",text.params.keys().map(|name|format!("{name}: {}",param(name))).collect::<Vec<_>>().join(", ")),
    }
}

#[cfg(test)] mod tests {
    use super::*;

    #[test] fn a_text_reaches_the_screen_as_key_and_params() {
        let text=Text::new("gate.blocked").with("score",40u8).with("scope",Text::new("scope.1"));
        assert_eq!(serde_json::to_value(&text).unwrap(),serde_json::json!({"key":"gate.blocked","params":{"score":"40","scope":{"key":"scope.1"}}}));
        assert_eq!(serde_json::from_value::<Text>(serde_json::to_value(&text).unwrap()).unwrap(),text);
    }

    #[test] fn any_error_becomes_a_key() {
        let chosen=anyhow::Error::new(Text::new("chat.notFound")).context("lendo o chat");
        assert_eq!(Text::from(chosen).key,"chat.notFound");
        assert_eq!(Text::from(anyhow::Error::new(SessionError::Expired)).key,"session.expired");
        let other=Text::from(anyhow::anyhow!("disk full"));
        assert_eq!((other.key.as_str(),other.params.get("reason")),("error.unexpected",Some(&Param::Plain("disk full".into()))));
    }

    #[test] fn a_notice_is_kept_for_the_screen_and_told_to_the_model_in_english() {
        let content=notice(&[Text::new("ask.answer").with("question","Proceed?").with("answer",Text::new("ask.yes"))]);
        assert_eq!(read_notice(&content).unwrap()[0].key,"ask.answer");
        assert_eq!(for_model(&content),"Answer to the question «Proceed?»: YES");
        assert_eq!(for_model("plain text"),"plain text");
        let blocked=notice(&[Text::new("gate.blocked").with("score",30u8).with("demand",55u8).with("scope",Text::new("scope.1"))]);
        assert_eq!(for_model(&blocked),"The JayV entry gate blocked this request with 30 out of 100 (the minimum for a feature is 55).");
    }

    #[test] fn the_reply_language_follows_the_app_and_falls_back_to_the_request() {
        set_reply_language(Some(ReplyLanguage{tag:"ja".into(),name:"日本語".into()}));
        let note=crate::orchestrator::language_note();
        assert!(note.contains("日本語") && note.contains("`ja`"),"{note}");
        set_reply_language(Some(ReplyLanguage{tag:"  ".into(),name:"".into()}));
        assert!(reply_language().is_none(),"an empty tag is no choice");
        assert!(crate::orchestrator::language_note().contains("language their request is written in"));
    }
}
