//! A escolha da skill de cada pedido: depois de ler o pedido, o Jev diz qual
//! das skills instaladas (no máximo uma) o agente deve usar, e o orquestrador
//! avisa o modelo.
//!
//! O Jev nunca escreve texto: ele devolve uma chave entre as que se mandam.
//! Como as skills mudam de pessoa para pessoa, a pergunta fica fixa no
//! servidor e as skills vão no estado, numeradas; a resposta é o número de uma
//! delas ou `none`. Sem o Jev (sem sessão, circuito aberto, limite do dia), uma
//! leitura local casa o pedido com o nome e a descrição das skills, e só
//! escolhe quando o casamento é claro.

use crate::jev::{self, Question};
use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

pub const SKILL_QUESTION:&str="skill";
pub const NONE:&str="none";
/// Quantas skills vão ao Jev de uma vez: mais que isso, só as que mais se
/// parecem com o pedido.
pub const MAX_CANDIDATES:usize=12;
/// A confiança abaixo da qual a escolha do Jev vale como "nenhuma".
pub const MIN_CONFIDENCE:f64=0.4;
const REQUEST_CHARS:usize=2000;
const DESCRIPTION_CHARS:usize=400;

/// Uma skill como o Jev a vê: o nome e quando usá-la.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Candidate { pub name:String, pub description:String }

/// A pergunta, como vai para o seed de `jev_questions`: escolher o número de
/// uma das skills de `skills` ou `none`.
pub fn questions()->BTreeMap<String,Question> {
    let mut options:Vec<(String,String)>=vec![(NONE.into(),"No skill clearly fits. Choose this whenever the request is a general question, is only loosely related to every description, or when two skills fit about equally.".into())];
    options.extend((1..=MAX_CANDIDATES).map(|number|(number.to_string(),format!("The skill whose `number` is {number} in `skills`: its description says when to use it, and this request is that case."))));
    BTreeMap::from([(SKILL_QUESTION.to_string(),Question::choice(
        "Which skill listed in `skills` should the coding agent use to carry out `user_request`? Each skill has a `number`, a `name` and a `description` that says when it applies. Choose the number of the single skill whose description matches what the developer is asking for, or `none`.",
        options))])
}

fn tokens(text:&str)->HashSet<String> {
    text.to_lowercase().split(|char:char|!char.is_alphanumeric()).filter(|word|word.chars().count()>=3).map(str::to_string).collect()
}

/// A nota de cada skill para este pedido, para a leitura local: o nome pesa o
/// dobro da descrição. Palavras muito comuns (que aparecem em quase todas as
/// skills) pesam menos.
fn scores(prompt:&str,candidates:&[Candidate])->Vec<f64> {
    let wanted=tokens(prompt);
    let named:Vec<(HashSet<String>,HashSet<String>)>=candidates.iter().map(|candidate|(tokens(&candidate.name.replace(['-','_'], " ")),tokens(&candidate.description))).collect();
    let total=candidates.len().max(1) as f64;
    named.iter().map(|(name,description)|{
        wanted.iter().map(|word|{
            let seen=named.iter().filter(|(other_name,other_description)|other_name.contains(word)||other_description.contains(word)).count() as f64;
            let weight=1.0+((total+1.0)/(seen+1.0)).ln();
            (if name.contains(word) {2.0} else {0.0}+if description.contains(word) {1.0} else {0.0})*weight
        }).sum()
    }).collect()
}

/// As que mais se parecem com o pedido, em ordem de nota, para o Jev não
/// receber mais que `MAX_CANDIDATES`. Com poucas skills, todas, na ordem em
/// que vieram.
pub fn shortlist(prompt:&str,candidates:&[Candidate])->Vec<usize> {
    let mut order:Vec<usize>=(0..candidates.len()).collect();
    if candidates.len()>MAX_CANDIDATES {
        let scored=scores(prompt,candidates);
        order.sort_by(|left,right|scored[*right].partial_cmp(&scored[*left]).unwrap_or(std::cmp::Ordering::Equal).then(left.cmp(right)));
        order.truncate(MAX_CANDIDATES);
        order.sort_unstable();
    }
    order
}

/// A leitura local: a skill cujo nome ou descrição casa de forma clara com o
/// pedido (nota mínima e distância da segunda colocada), ou nenhuma.
pub fn local_pick(prompt:&str,candidates:&[Candidate])->Option<usize> {
    const MIN_SCORE:f64=2.0;
    let scored=scores(prompt,candidates);
    let (best,top)=scored.iter().copied().enumerate().max_by(|left,right|left.1.partial_cmp(&right.1).unwrap_or(std::cmp::Ordering::Equal).then(right.0.cmp(&left.0)))?;
    let second=scored.iter().enumerate().filter(|(index,_)|*index!=best).map(|(_,score)|*score).fold(0.0,f64::max);
    (top>=MIN_SCORE&&top>=second*1.5).then_some(best)
}

fn state(prompt:&str,shown:&[&Candidate])->serde_json::Value {
    json!({
        "user_request":prompt.chars().take(REQUEST_CHARS).collect::<String>(),
        "skills":shown.iter().enumerate().map(|(index,candidate)|json!({"number":index+1,"name":candidate.name,"description":candidate.description.chars().take(DESCRIPTION_CHARS).collect::<String>()})).collect::<Vec<_>>(),
    })
}

/// O número escolhido, de volta para o índice da skill. `None` para `none`,
/// para um número que não foi oferecido e para a escolha pouco confiante.
pub fn resolve(choice:&str,confidence:Option<f64>,offered:&[usize])->Option<usize> {
    if choice==NONE||confidence.is_some_and(|confidence|confidence<MIN_CONFIDENCE) { return None; }
    let number:usize=choice.trim().parse().ok()?;
    offered.get(number.checked_sub(1)?).copied()
}

async fn ask(prompt:&str,candidates:&[Candidate])->Result<Option<usize>> {
    let offered=shortlist(prompt,candidates);
    let shown:Vec<&Candidate>=offered.iter().map(|index|&candidates[*index]).collect();
    let evaluation=jev::evaluate("skills",state(prompt,&shown),None).await?;
    let choice=evaluation.choice(SKILL_QUESTION).ok_or_else(||anyhow!("the Jev did not return the `{SKILL_QUESTION}` answer"))?;
    Ok(resolve(choice,evaluation.confidence(SKILL_QUESTION),&offered))
}

/// O índice da skill que o Jev escolheu para o pedido, ou `None` quando
/// nenhuma serve. Sem o Jev ao alcance (ou com o roteamento dele desligado,
/// `with_jev` falso), ou se ele falhar, vale a leitura local. Sem skills, nem
/// chama.
pub async fn choose(prompt:&str,candidates:&[Candidate],with_jev:bool)->Option<usize> {
    if candidates.is_empty()||prompt.trim().is_empty() { return None; }
    if with_jev&&jev::reachable() {
        match ask(prompt,candidates).await {
            Ok(picked)=>return picked,
            Err(error)=>eprintln!("skills: o Jev não escolheu ({error:#}); vale a leitura local"),
        }
    }
    local_pick(prompt,candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name:&str,description:&str)->Candidate { Candidate{name:name.into(),description:description.into()} }

    #[test] fn the_question_offers_none_and_one_number_per_candidate_slot() {
        let question=&questions()[SKILL_QUESTION];
        let options=question.options();
        assert!(options.contains(&NONE)&&options.contains(&"1")&&options.contains(&"12")&&!options.contains(&"13"),"{options:?}");
        assert!(question.validate().is_ok());
    }

    #[test] fn a_number_resolves_to_the_offered_skill_and_the_rest_to_nothing() {
        let offered=[4,7,9];
        assert_eq!(resolve("2",Some(0.9),&offered),Some(7));
        assert_eq!(resolve("none",Some(0.99),&offered),None);
        assert_eq!(resolve("4",Some(0.9),&offered),None,"only three were offered");
        assert_eq!(resolve("0",None,&offered),None);
        assert_eq!(resolve("1",Some(0.1),&offered),None,"an unsure choice is no choice");
        assert_eq!(resolve("1",None,&offered),Some(4));
    }

    #[test] fn only_the_closest_skills_go_to_the_jev_when_there_are_many() {
        let many:Vec<Candidate>=(0..20).map(|index|candidate(&format!("skill-{index}"),&format!("handles topic{index}"))).collect();
        let offered=shortlist("please use topic17 for this",&many);
        assert_eq!(offered.len(),MAX_CANDIDATES);
        assert!(offered.contains(&17),"{offered:?}");
        assert_eq!(shortlist("anything",&many[..3]),vec![0,1,2],"few skills all go, in order");
    }

    #[test] fn the_local_reading_picks_a_clear_match_and_leaves_vague_requests_alone() {
        let skills=[candidate("pdf-forms","Fill and read PDF forms"),candidate("release-notes","Write release notes from merged pull requests"),candidate("db-migration","Create and review database migrations")];
        assert_eq!(local_pick("fill this pdf form with the data",&skills),Some(0));
        assert_eq!(local_pick("write the release notes for 1.2",&skills),Some(1));
        assert_eq!(local_pick("what does this function do?",&skills),None);
        assert_eq!(local_pick("anything",&[]),None);
    }

    #[tokio::test] async fn without_skills_or_a_request_nothing_is_asked() {
        assert_eq!(choose("write the release notes",&[],true).await,None);
        assert_eq!(choose("   ",&[candidate("a","b")],true).await,None);
        let skills=[candidate("release-notes","Write release notes from merged pull requests")];
        assert_eq!(choose("write the release notes for 1.2",&skills,false).await,Some(0),"without the Jev the local reading decides");
    }
}
