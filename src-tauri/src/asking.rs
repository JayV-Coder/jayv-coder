//! A pergunta que o modelo devolve, virada em interação.
//!
//! O caminho é o que o usuário pediu: a resposta do LLM passa pelo JEV, e é o
//! retorno do JEV que **habilita** a interação. O JEV, porém, nunca escreve
//! texto — `Answer` (`jev.rs:52`) só devolve número, chave entre as opções que
//! eu mandei, ou nível. Então o enunciado e as alternativas são extraídos aqui,
//! localmente, e oferecidos a ele como candidatos; ele diz se aquilo é pergunta,
//! de que tipo, e se a lista extraída é de verdade.

use crate::i18n::{self, Param, Text};
use crate::jev::{self, Question, MAX_CHOICE_OPTIONS};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const KIND_QUESTION:&str="kind";
pub const OPTIONS_QUESTION:&str="options_are_real";
pub const JEV_SOURCE:&str="jev";
pub const LOCAL_SOURCE:&str="local";
/// Meio a meio é indeciso, e indeciso não habilita nada.
pub const NOUL_LINE:f64=0.5;

/// O traje que o box vai vestir. `Score` ficou fora de propósito: nível de 0 a
/// 10 não é interação de tela neste app. `Form` é a resposta que termina com
/// várias perguntas: cada uma vira um cartão, com as alternativas dela ou um
/// campo de texto.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Shape { Noul, Single, Multiple, Form }

impl Shape {
    pub fn as_str(&self)->&'static str { match self { Self::Noul=>"noul", Self::Single=>"single", Self::Multiple=>"multiple", Self::Form=>"form" } }
    pub fn parse(value:&str)->Result<Self> { Ok(match value {
        "noul"=>Self::Noul, "single"=>Self::Single, "multiple"=>Self::Multiple, "form"=>Self::Form,
        other=>return Err(anyhow!("unknown question kind: `{other}`")),
    }) }
    pub fn wants_options(&self)->bool { !matches!(self,Self::Noul) }
}

/// Uma pergunta do formulário. Vai gravada como texto JSON dentro da lista de
/// opções da linha de `questions`, que continua sendo uma lista de textos: a
/// tabela e a sincronização não mudam.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct FormItem { pub prompt:String, pub options:Vec<String> }

/// Onde uma mensagem do agente termina e a próxima começa, dentro da resposta
/// gravada. A tela desenha cada mensagem no seu balão.
pub const MESSAGE_BREAK:char='\u{2063}';

/// A última mensagem da resposta: é nela que o agente pergunta.
fn last_message(answer:&str)->&str { answer.rsplit(MESSAGE_BREAK).next().unwrap_or(answer) }

/// O que a extração local achou: o enunciado e as alternativas logo abaixo dele.
#[derive(Debug,Clone,PartialEq)]
pub struct Candidate { pub prompt:String, pub options:Vec<String> }

/// A pergunta habilitada, do jeito que o box precisa dela.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Pending { pub kind:Shape, pub prompt:String, pub options:Vec<String>, pub source:String }

/// O enunciado e as alternativas, se a resposta terminar perguntando.
///
/// A pergunta tem de **fechar** a resposta: ou é a última linha com conteúdo, ou
/// é a linha logo acima da lista que fecha. Uma interrogação no meio de um texto
/// que segue explicando não é convite para responder, e travar o box por causa
/// dela seria pior que não ter interação nenhuma.
pub fn extract(answer:&str)->Option<Candidate> {
    extract_all(answer).pop()
}

/// Todas as perguntas que fecham a resposta, na ordem em que aparecem: cada
/// enunciado com as alternativas logo abaixo dele. Para no primeiro trecho que
/// não é pergunta nem alternativa — o texto de antes é explicação.
pub fn extract_all(answer:&str)->Vec<Candidate> {
    let lines=last_message(answer).lines().collect::<Vec<_>>();
    let previous=|end:usize|lines[..end].iter().rposition(|line|!line.trim().is_empty());
    let mut found=Vec::new();
    let Some(mut index)=previous(lines.len()) else { return found };
    loop {
        let mut options=Vec::new();
        // Um item de lista que pergunta é o enunciado numerado ("1. Qual?"),
        // não uma alternativa da pergunta de baixo.
        while let Some(item)=bullet(lines[index]).filter(|item|asking(item).is_none()) {
            options.push(clean(item));
            match previous(index) { Some(above)=>index=above, None=>return found.into_iter().rev().collect() }
        }
        options.reverse();
        let line=bullet(lines[index]).unwrap_or(lines[index]);
        let Some(prompt)=asking(line) else { break };
        // A primeira pergunta sem alternativas fecha a resposta sozinha; acima
        // dela, uma lista longa demais não é escolha de ninguém.
        if options.len()>MAX_CHOICE_OPTIONS { break; }
        found.push(Candidate{prompt,options});
        match previous(index) { Some(above)=>index=above, None=>break }
    }
    found.reverse();
    found
}

/// A alternativa sem os enfeites do Markdown que a tela não desenharia no botão.
fn clean(item:&str)->String { item.replace("**","").replace("__","").replace('`',"").trim().to_string() }

/// O item de lista dentro de uma linha, nos quatro formatos que aparecem: `- `,
/// `* `, `1.` e `A)`. A etiqueta fica fora: o que vai para o botão é a opção.
fn bullet(line:&str)->Option<&str> {
    let line=line.trim();
    for mark in ["- ","* ","• ","+ "] {
        if let Some(rest)=line.strip_prefix(mark) { return Some(rest.trim()).filter(|rest|!rest.is_empty()); }
    }
    // Enumeração: até dois caracteres de etiqueta antes do ponto ou do
    // parêntese. Dois dígitos, ou uma letra só — "Ok." não é item de lista.
    let cut=line.find(['.',')'])?;
    let label=&line[..cut];
    let numbered=!label.is_empty()&&label.len()<=2&&label.bytes().all(|byte|byte.is_ascii_digit());
    let lettered=label.len()==1&&label.bytes().all(|byte|byte.is_ascii_alphabetic());
    if !numbered&&!lettered { return None; }
    Some(line[cut+1..].trim()).filter(|rest|!rest.is_empty())
}

/// A última frase da linha, se ela for uma pergunta. Negrito e cabeçalho saem:
/// `**Quer que eu continue?**` é a mesma pergunta sem os asteriscos.
fn asking(line:&str)->Option<String> {
    let line=line.replace("**","").replace("__","");
    let line=line.trim().trim_end_matches(['*','_',' ']).trim_end();
    if !line.ends_with('?') { return None; }
    // A frase termina na pontuação seguida de espaço: o ponto de `lib.rs` não
    // encerra frase nenhuma.
    let body=&line[..line.len()-1];
    let cut=body.char_indices().filter(|(at,mark)|matches!(mark,'.'|'!'|'?')&&body[at+1..].starts_with(char::is_whitespace)).last().map(|(at,_)|at+1).unwrap_or(0);
    let sentence=line[cut..].trim().trim_start_matches(['#','*','_','>','-',' ']).trim();
    (!sentence.is_empty()).then(||sentence.to_string())
}

/// O veredito sobre a resposta do modelo. O JEV decide; a reserva local só entra
/// quando ele não está configurado ou a chamada falha — um "não é pergunta" dele
/// é resposta, e não motivo para consultar a heurística por cima.
pub async fn classify(answer:&str)->Option<Pending> {
    let mut candidates=extract_all(answer);
    let candidate=candidates.pop()?;
    let verdict=match consult(answer,&candidate).await {
        Ok(verdict)=>verdict,
        Err(_)=>guess(&candidate),
    };
    if candidates.is_empty() { return verdict; }
    // Várias perguntas: o Jev confirma que a resposta termina perguntando, e
    // o formulário leva todas elas.
    let source=verdict?.source;
    candidates.push(candidate);
    Some(form(&candidates,&source))
}

/// O formulário de várias perguntas. O enunciado gravado é a lista delas, uma
/// por linha — é o que aparece onde só cabe texto.
pub fn form(candidates:&[Candidate],source:&str)->Pending {
    let items=candidates.iter().map(|candidate|FormItem{prompt:candidate.prompt.clone(),options:candidate.options.clone()}).collect::<Vec<_>>();
    Pending{
        kind:Shape::Form,
        prompt:items.iter().map(|item|item.prompt.as_str()).collect::<Vec<_>>().join("\n"),
        options:items.iter().filter_map(|item|serde_json::to_string(item).ok()).collect(),
        source:source.into(),
    }
}

/// As perguntas do formulário, lidas da lista gravada.
pub fn form_items(options:&[String])->Vec<FormItem> { options.iter().filter_map(|item|serde_json::from_str(item).ok()).collect() }

/// As duas perguntas deste módulo, como vão para o seed de `jev_questions`. A
/// segunda só vai quando o pedido trouxe opções.
pub fn questions()->BTreeMap<String,Question> {
    BTreeMap::from([
        (KIND_QUESTION.to_string(),Question::choice(
            "Does the assistant's answer end by asking the developer something, so that the work depends on what they reply?",
            [
                ("none","there is no question to answer: the answer is information, or the question is rhetorical"),
                ("noul","the question is answered with yes or no"),
                ("single","the question offers options and expects exactly one"),
                ("multiple","the question offers options and accepts as many as the developer wants"),
            ])),
        (OPTIONS_QUESTION.to_string(),Question::noul_with(
            "Is the list in `options` exactly the set of alternatives that the question in `question` offers?",
            "they are the question's alternatives","something else: steps, examples, items of a report")),
    ])
}

async fn consult(answer:&str,candidate:&Candidate)->Result<Option<Pending>> {
    let include:&[&str]=if candidate.options.is_empty() {&[KIND_QUESTION]} else {&[KIND_QUESTION,OPTIONS_QUESTION]};
    let state=json!({"answer":answer,"question":candidate.prompt,"options":candidate.options});
    let evaluation=jev::evaluate("asking",state,Some(include)).await?;
    let kind=evaluation.choice(KIND_QUESTION).ok_or_else(||anyhow!("the Jev did not return the `{KIND_QUESTION}` answer"))?;
    let real=match evaluation.noul(OPTIONS_QUESTION) { Some(noul)=>noul>=crate::local::global::current_parameters().noul_line, None=>candidate.options.is_empty() };
    Ok(shape(kind,candidate,real).map(|kind|enable(kind,candidate,JEV_SOURCE)))
}

/// O tipo pedido, conferido contra o que existe para desenhar. Uma escolha sem
/// alternativas de verdade não tem botão nenhum para oferecer, e oferecer SIM e
/// NÃO no lugar mudaria a pergunta — então o box fica livre e o desenvolvedor
/// escreve, como escreveria antes de tudo isto.
fn shape(kind:&str,candidate:&Candidate,real:bool)->Option<Shape> {
    match kind {
        "noul"=>Some(Shape::Noul),
        "single"|"multiple" if real&&!candidate.options.is_empty()=>Some(if kind=="single"{Shape::Single}else{Shape::Multiple}),
        _=>None,
    }
}

/// A reserva. Sem o JEV a leitura é a mais simples que se sustenta: lista
/// extraída é escolha única, enunciado sozinho é sim ou não. É o mesmo desenho
/// que o portão de entrada já usa quando o JEV não responde.
pub fn guess(candidate:&Candidate)->Option<Pending> {
    let kind=if candidate.options.is_empty() { Shape::Noul } else { Shape::Single };
    Some(enable(kind,candidate,LOCAL_SOURCE))
}

fn enable(kind:Shape,candidate:&Candidate,source:&str)->Pending {
    Pending{kind,prompt:candidate.prompt.clone(),options:if kind.wants_options(){candidate.options.clone()}else{vec![]},source:source.into()}
}

pub const YES:&str="yes";
pub const NO:&str="no";

/// O texto do pedido que nasce de uma resposta. Determinístico de propósito: a
/// mesma escolha escreve sempre a mesma linha, e é essa linha que fica no chat,
/// vai ao portão e chega ao modelo. Quem clicou tem de reconhecer no histórico
/// o que clicou.
///
/// O texto livre vale para qualquer tipo: é o `RESPONDER`, que existe justamente
/// para quando nenhuma das alternativas serve.
pub fn compose(question:&str,kind:Shape,options:&[String],picked:&[String],text:Option<&str>)->Result<String> {
    if let Some(text)=text.map(str::trim).filter(|text|!text.is_empty()) { return Ok(answer_line(question,text)); }
    match kind {
        // No formulário, `picked` traz uma resposta por pergunta, na ordem
        // delas; a vazia é pergunta pulada.
        Shape::Form=>{
            let lines=form_items(options).iter().zip(picked).filter(|(_,answer)|!answer.trim().is_empty())
                .map(|(item,answer)|Text::new("ask.answer").with("question",&item.prompt).with("answer",answer.trim())).collect::<Vec<_>>();
            if lines.is_empty() { return Err(Text::new("answer.pickOne").into()); }
            Ok(i18n::notice(&lines))
        }
        Shape::Noul=>match picked.first().map(String::as_str) {
            Some(YES)=>Ok(answer_line(question,Text::new("ask.yes"))),
            Some(NO)=>Ok(answer_line(question,Text::new("ask.no"))),
            _=>Err(Text::new("answer.noul").into()),
        },
        Shape::Single|Shape::Multiple=>{
            if picked.is_empty() { return Err(Text::new("answer.pickOne").into()); }
            if kind==Shape::Single&&picked.len()>1 { return Err(Text::new("answer.single").into()); }
            if let Some(stranger)=picked.iter().find(|choice|!options.contains(choice)) {
                return Err(Text::new("answer.unknown").with("option",stranger).into());
            }
            Ok(answer_line(question,&picked.join(", ")))
        }
    }
}

/// A linha fica no chat como aviso, para cada um a ler no seu idioma; o
/// modelo e a Portaria a recebem em inglês (`i18n::for_model`).
fn answer_line(question:&str,answer:impl Into<Param>)->String { i18n::notice(&[Text::new("ask.answer").with("question",question).with("answer",answer)]) }

/// O par que a Portaria pontua e o modelo recebe. Um `SIM` sozinho não diz
/// objetivo, não aponta arquivo e não traz critério de pronto: os critérios de
/// `judge` (`gatekeeper.rs:124`) o barrariam por faltas que o pedido de origem
/// já tinha suprido. O par herda tudo isso do pedido que fez a pergunta nascer.
///
/// A pergunta não entra de novo: ela já está citada dentro da linha da resposta,
/// e repeti-la seria pagar duas vezes pelos mesmos tokens.
pub fn pair(origin:&str,answer:&str)->String {
    let origin=origin.trim();
    if origin.is_empty() { return answer.trim().to_string(); }
    format!("{origin}\n\n{}",answer.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(prompt:&str,options:&[&str])->Candidate { Candidate{prompt:prompt.into(),options:options.iter().map(|option|option.to_string()).collect()} }

    #[test] fn the_question_at_the_end_of_the_answer_is_the_prompt() {
        let extracted=extract("Li os três arquivos e o índice está desatualizado.\n\nQuer que eu regenere o índice?").expect("pergunta");
        assert_eq!(extracted,candidate("Quer que eu regenere o índice?",&[]));
    }

    /// Uma interrogação no meio de um texto que segue explicando não é convite
    /// para responder. Travar o box por causa dela seria pior que não ter
    /// interação nenhuma.
    #[test] fn a_question_in_the_middle_of_the_text_does_not_count() {
        assert_eq!(extract("Por que estava lento? Porque o índice era relido a cada busca. Já corrigi."),None);
    }

    #[test] fn the_list_right_below_the_prompt_is_the_options() {
        for list in [
            "- Vite\n- Webpack\n- esbuild",
            "* Vite\n* Webpack\n* esbuild",
            "1. Vite\n2. Webpack\n3. esbuild",
            "A) Vite\nB) Webpack\nC) esbuild",
        ] {
            let extracted=extract(&format!("Pronto para escolher o empacotador.\n\nQual deles?\n\n{list}")).expect("lista");
            assert_eq!(extracted,candidate("Qual deles?",&["Vite","Webpack","esbuild"]),"{list}");
        }
    }

    #[test] fn bold_and_headings_are_stripped_from_the_prompt() {
        assert_eq!(extract("**Devo aplicar a migração agora?**").expect("pergunta").prompt,"Devo aplicar a migração agora?");
        assert_eq!(extract("## E o teste, escrevo antes?").expect("pergunta").prompt,"E o teste, escrevo antes?");
    }

    /// Uma lista sem pergunta acima dela é um relatório, não uma escolha.
    #[test] fn a_list_without_a_prompt_enables_nothing() {
        assert_eq!(extract("Mudei três arquivos:\n\n- lib.rs\n- turns.rs\n- main.js"),None);
    }

    #[test] fn a_plain_sentence_is_not_a_list_item() {
        assert_eq!(bullet("Ok. Continuo amanhã"),None);
        assert_eq!(bullet("- "),None);
        assert_eq!(bullet("1."),None);
        assert_eq!(bullet("12) doze"),Some("doze"));
    }

    /// Sem o JEV a leitura é a mais simples que se sustenta. Ela habilita a
    /// interação do mesmo jeito: o box não fica esperando uma chave de API.
    #[test] fn the_fallback_reads_a_list_as_a_choice_and_a_lone_prompt_as_yes_or_no() {
        let choice=guess(&candidate("Qual deles?",&["Vite","Webpack"])).expect("escolha");
        assert_eq!((choice.kind,choice.options.len(),choice.source.as_str()),(Shape::Single,2,LOCAL_SOURCE));
        let confirmation=guess(&candidate("Regenero o índice?",&[])).expect("sim ou não");
        assert_eq!((confirmation.kind,confirmation.options.len()),(Shape::Noul,0));
    }

    /// O "não é pergunta" do JEV é veredito, não silêncio: quem voltar a
    /// consultar a heurística por cima dele faz o box travar contra a decisão
    /// de quem devia decidir.
    #[test] fn a_no_question_verdict_enables_nothing() {
        let extracted=candidate("Qual deles?",&["Vite","Webpack"]);
        assert_eq!(shape("none",&extracted,true),None);
        assert_eq!(shape("single",&extracted,true),Some(Shape::Single));
        // Alternativas que não são alternativas não têm botão para oferecer.
        assert_eq!(shape("single",&extracted,false),None);
        assert_eq!(shape("multiple",&candidate("Quais?",&[]),true),None);
        // Sim ou não não depende de lista nenhuma.
        assert_eq!(shape("noul",&candidate("Sigo?",&[]),false),Some(Shape::Noul));
    }

    #[test] fn a_clicked_choice_always_becomes_the_same_line() {
        let options=vec!["Vite".to_string(),"esbuild".to_string()];
        assert_eq!(i18n::for_model(&compose("Sigo?",Shape::Noul,&[],&[YES.into()],None).expect("sim")),"Answer to the question «Sigo?»: YES");
        assert_eq!(i18n::for_model(&compose("Sigo?",Shape::Noul,&[],&[NO.into()],None).expect("não")),"Answer to the question «Sigo?»: NO");
        assert_eq!(i18n::for_model(&compose("Qual?",Shape::Single,&options,&["Vite".into()],None).expect("única")),"Answer to the question «Qual?»: Vite");
        assert_eq!(i18n::for_model(&compose("Quais?",Shape::Multiple,&options,&["Vite".into(),"esbuild".into()],None).expect("múltipla")),"Answer to the question «Quais?»: Vite, esbuild");
        // RESPONDER vale para qualquer tipo: ele existe para quando nenhuma
        // alternativa serve.
        assert_eq!(i18n::for_model(&compose("Qual?",Shape::Single,&options,&[],Some(" só no build ")).expect("texto")),"Answer to the question «Qual?»: só no build");
    }

    #[test] fn a_choice_the_question_did_not_offer_is_refused() {
        let options=vec!["Vite".to_string()];
        assert!(compose("Qual?",Shape::Single,&options,&["Rollup".into()],None).is_err(),"a tela não inventa alternativa");
        assert!(compose("Qual?",Shape::Single,&options,&[],None).is_err(),"sem escolha e sem texto não há resposta");
        assert!(compose("Quais?",Shape::Single,&options,&["Vite".into(),"Vite".into()],None).is_err(),"escolha única é uma só");
        assert!(compose("Sigo?",Shape::Noul,&[],&["talvez".into()],None).is_err(),"sim ou não é sim ou não");
    }

    /// Um `SIM` sozinho seria barrado por faltas que o pedido de origem já tinha
    /// suprido. Quem voltar a pontuar a resposta sozinha faz a portaria barrar o
    /// próprio fluxo que ela mandou o modelo abrir.
    #[test] fn the_pair_carries_the_request_that_raised_the_question() {
        let paired=pair("Regenere o índice do RAG em src-tauri/src/rag.rs e diga quantos arquivos entraram.","Answer to the question «Sigo?»: YES");
        assert!(paired.starts_with("Regenere o índice"),"o pedido original abre o par: {paired}");
        assert!(paired.ends_with("YES"),"a resposta fecha o par: {paired}");
        assert!(paired.contains("«Sigo?»"),"a pergunta viaja dentro da linha da resposta: {paired}");
        assert_eq!(pair("   ","Answer: YES"),"Answer: YES","sem pedido de origem, o par é a resposta");
    }

    /// A prova do par contra a portaria de verdade: a mesma resposta, pontuada
    /// sozinha e em par. O portão não sabe que é resposta — ele lê texto —, e é
    /// por isso que o par tem de chegar montado até ele.
    #[test] fn the_gate_scores_the_paired_answer_higher_than_a_bare_yes() {
        use crate::{gatekeeper::{self,EntryVerdict},turns::{Turn,TurnStatus}};
        let turn=Turn{id:"t1".into(),chat_id:"c1".into(),code:"XY4T9B·02".into(),ordinal:2,status:TurnStatus::Flying,created_at:chrono::Utc::now()};
        let judge=|text:&str|gatekeeper::judge(&turn,text,&gatekeeper::heuristic_entry(text),"local");

        let bare_answer=compose("Regenero o índice agora?",Shape::Noul,&[],&[YES.into()],None).expect("resposta");
        let paired=pair("Regenere o índice do RAG em src-tauri/src/rag.rs; pronto quando cargo test passar.",&bare_answer);

        let bare=judge(&bare_answer);
        let paired_check=judge(&paired);
        assert!(paired_check.score>bare.score,"o par diz onde mexer e quando está pronto: {} contra {}",paired_check.score,bare.score);
        assert_ne!(paired_check.verdict,EntryVerdict::Block,"a portaria não barra o fluxo que ela própria mandou abrir");
    }

    /// O caso que chegou do Isaac: no meio de um plano o agente perguntou se
    /// ele já tinha saído do modo plano, e a resposta foi barrada como pedido
    /// de funcionalidade. A resposta herda a passagem do pedido de origem; a
    /// nota e os critérios continuam os que a portaria leu.
    #[test] fn an_answer_inherits_the_pass_of_the_request_that_raised_the_question() {
        use crate::{gatekeeper::{self,EntryVerdict},turns::{Turn,TurnStatus}};
        let turn=Turn{id:"t3".into(),chat_id:"c1".into(),code:"XY4T9B·03".into(),ordinal:3,status:TurnStatus::Flying,created_at:chrono::Utc::now()};
        let answer=i18n::for_model(&compose("Já saiu do modo plano?",Shape::Noul,&[],&[],Some("Ainda não, aviso quando sair")).expect("resposta"));
        // A leitura que o Jev deu no caso real: funcionalidade, sem objetivo,
        // sem onde e sem quando — 32 de 100.
        let reading=gatekeeper::EntryReading{scope_score:1.0,goal_is_clear:0.1,says_where:0.0,says_when_done:0.0,bundles_requests:0.0};
        let judged=gatekeeper::judge(&turn,&answer,&reading,"jev");
        assert_eq!(judged.verdict,EntryVerdict::Block,"sozinha, a resposta não diz objetivo, onde nem quando: {}",judged.score);

        let inherited=judged.clone().inherit(EntryVerdict::Pass);
        assert_eq!(inherited.verdict,EntryVerdict::Pass,"o agente perguntou porque a portaria liberou o pedido");
        assert_eq!(inherited.note,"entry.note.pass");
        assert_eq!((inherited.score,&inherited.criteria),(judged.score,&judged.criteria),"o que a portaria leu não muda");
        assert_eq!(judged.clone().inherit(EntryVerdict::Ask).verdict,EntryVerdict::Ask,"liberado com ressalva continua com ressalva");
        assert_eq!(judged.clone().inherit(EntryVerdict::Block).verdict,EntryVerdict::Block,"nada herda um bloqueio para passar");
    }

    /// O caso que motivou o formulário: o agente não conseguiu abrir o dele e
    /// mandou as perguntas numeradas, cada uma com as suas alternativas.
    #[test] fn several_closing_questions_become_one_form() {
        let answer="Li o projeto.\u{2063}\nNão consegui abrir o formulário de perguntas, então vão aqui.\n\n**1. Escopo** — Mexo só no `src/lib.rs`?\n\n2. Qual banco usar?\n- **Postgres** — o atual\n- SQLite\n\n3. Quer testes?\n";
        let found=extract_all(answer);
        assert_eq!(found.iter().map(|candidate|candidate.prompt.as_str()).collect::<Vec<_>>(),["Escopo — Mexo só no `src/lib.rs`?","Qual banco usar?","Quer testes?"]);
        assert_eq!(found[1].options,["Postgres — o atual","SQLite"]);
        assert!(found[0].options.is_empty()&&found[2].options.is_empty());
        let pending=form(&found,LOCAL_SOURCE);
        assert_eq!(pending.kind,Shape::Form);
        assert_eq!(form_items(&pending.options).len(),3);
        assert_eq!(extract(answer).map(|candidate|candidate.prompt),Some("Quer testes?".into()),"a pergunta única continua sendo a última");
    }

    #[test] fn only_the_last_message_can_ask() {
        assert!(extract_all("Quer que eu leia o arquivo?\n\u{2063}\nPronto, terminei.").is_empty());
    }

    #[test] fn a_form_answer_names_each_question_and_skips_the_blank_ones() {
        let pending=form(&[candidate("Qual banco?",&["Postgres","SQLite"]),candidate("Quer testes?",&[])],LOCAL_SOURCE);
        let line=compose(&pending.prompt,Shape::Form,&pending.options,&["SQLite".into(),"".into()],None).expect("resposta");
        let read=i18n::read_notice(&line).expect("aviso");
        assert_eq!(read.len(),1);
        assert_eq!(i18n::for_model(&line),"Answer to the question «Qual banco?»: SQLite");
        assert!(compose(&pending.prompt,Shape::Form,&pending.options,&["".into(),"  ".into()],None).is_err(),"nada respondido não vira pedido");
    }

    #[test] fn the_kind_round_trips_through_the_database_as_text() {
        for kind in [Shape::Noul,Shape::Single,Shape::Multiple] {
            assert_eq!(Shape::parse(kind.as_str()).expect("tipo"),kind);
        }
        assert!(Shape::parse("score").is_err(),"`score` ficou fora de propósito");
    }
}
