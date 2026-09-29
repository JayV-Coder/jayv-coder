//! A pergunta que o modelo devolve, virada em interação.
//!
//! O caminho é o que o usuário pediu: a resposta do LLM passa pelo JEV, e é o
//! retorno do JEV que **habilita** a interação. O JEV, porém, nunca escreve
//! texto — `Answer` (`jev.rs:52`) só devolve número, chave entre as opções que
//! eu mandei, ou nível. Então o enunciado e as alternativas são extraídos aqui,
//! localmente, e oferecidos a ele como candidatos; ele diz se aquilo é pergunta,
//! de que tipo, e se a lista extraída é de verdade.

use crate::jev::{self, Question, MAX_CHOICE_OPTIONS};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const KIND_QUESTION:&str="kind";
pub const OPTIONS_QUESTION:&str="options_are_real";
pub const JEV_SOURCE:&str="jev";
pub const LOCAL_SOURCE:&str="heurística local";
/// Meio a meio é indeciso, e indeciso não habilita nada.
pub const NOUL_LINE:f64=0.5;

/// O traje que o box vai vestir. `Score` ficou fora de propósito: nível de 0 a
/// 10 não é interação de tela neste app.
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Shape { Noul, Single, Multiple }

impl Shape {
    pub fn as_str(&self)->&'static str { match self { Self::Noul=>"noul", Self::Single=>"single", Self::Multiple=>"multiple" } }
    pub fn parse(value:&str)->Result<Self> { Ok(match value {
        "noul"=>Self::Noul, "single"=>Self::Single, "multiple"=>Self::Multiple,
        other=>return Err(anyhow!("tipo de pergunta desconhecido: `{other}`")),
    }) }
    pub fn wants_options(&self)->bool { !matches!(self,Self::Noul) }
}

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
    let lines=answer.lines().collect::<Vec<_>>();
    let mut index=lines.iter().rposition(|line|!line.trim().is_empty())?;
    let mut options=Vec::new();
    while let Some(item)=bullet(lines[index]) {
        options.push(item.to_string());
        index=lines[..index].iter().rposition(|line|!line.trim().is_empty())?;
    }
    options.reverse();
    if options.len()>MAX_CHOICE_OPTIONS { return None; }
    Some(Candidate{prompt:asking(lines[index])?,options})
}

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
    let line=line.trim().trim_end_matches(['*','_',' ']).trim_end();
    if !line.ends_with('?') { return None; }
    let cut=line[..line.len()-1].rfind(['.','!','?']).map(|cut|cut+1).unwrap_or(0);
    let sentence=line[cut..].trim().trim_start_matches(['#','*','_','>','-',' ']).trim();
    (!sentence.is_empty()).then(||sentence.to_string())
}

/// O veredito sobre a resposta do modelo. O JEV decide; a reserva local só entra
/// quando ele não está configurado ou a chamada falha — um "não é pergunta" dele
/// é resposta, e não motivo para consultar a heurística por cima.
pub async fn classify(answer:&str)->Option<Pending> {
    let candidate=extract(answer)?;
    match consult(answer,&candidate).await {
        Ok(verdict)=>verdict,
        Err(_)=>guess(&candidate),
    }
}

async fn consult(answer:&str,candidate:&Candidate)->Result<Option<Pending>> {
    let mut questions=BTreeMap::new();
    questions.insert(KIND_QUESTION.to_string(),Question::choice(
        "A resposta do assistente termina perguntando algo ao desenvolvedor, de modo que o trabalho depende do que ele responder?",
        [
            ("none","não há pergunta a responder: a resposta é informação, ou a pergunta é retórica"),
            ("noul","a pergunta se responde com sim ou não"),
            ("single","a pergunta oferece alternativas e espera uma só"),
            ("multiple","a pergunta oferece alternativas e aceita quantas o desenvolvedor quiser"),
        ]));
    if !candidate.options.is_empty() {
        questions.insert(OPTIONS_QUESTION.to_string(),Question::noul_with(
            "A lista em `options` é exatamente o conjunto de alternativas que a pergunta em `question` oferece?",
            "são as alternativas da pergunta","é outra coisa: passos, exemplos, itens de um relatório"));
    }
    let state=json!({"answer":answer,"question":candidate.prompt,"options":candidate.options});
    let evaluation=jev::evaluate(state,questions).await?;
    let kind=evaluation.choice(KIND_QUESTION).ok_or_else(||anyhow!("o Jev não devolveu a resposta `{KIND_QUESTION}`"))?;
    let real=match evaluation.noul(OPTIONS_QUESTION) { Some(noul)=>noul>=NOUL_LINE, None=>candidate.options.is_empty() };
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
        Shape::Noul=>match picked.first().map(String::as_str) {
            Some(YES)=>Ok(answer_line(question,"SIM")),
            Some(NO)=>Ok(answer_line(question,"NÃO")),
            _=>Err(anyhow!("responda `{YES}` ou `{NO}`, ou escreva a resposta")),
        },
        Shape::Single|Shape::Multiple=>{
            if picked.is_empty() { return Err(anyhow!("escolha ao menos uma alternativa, ou escreva a resposta")); }
            if kind==Shape::Single&&picked.len()>1 { return Err(anyhow!("esta pergunta aceita uma alternativa só")); }
            if let Some(stranger)=picked.iter().find(|choice|!options.contains(choice)) {
                return Err(anyhow!("`{stranger}` não é uma das alternativas oferecidas"));
            }
            Ok(answer_line(question,&picked.join(", ")))
        }
    }
}

fn answer_line(question:&str,answer:&str)->String { format!("Resposta à pergunta «{question}»: {answer}") }

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

    #[test] fn a_pergunta_no_fim_da_resposta_e_o_enunciado() {
        let extracted=extract("Li os três arquivos e o índice está desatualizado.\n\nQuer que eu regenere o índice?").expect("pergunta");
        assert_eq!(extracted,candidate("Quer que eu regenere o índice?",&[]));
    }

    /// Uma interrogação no meio de um texto que segue explicando não é convite
    /// para responder. Travar o box por causa dela seria pior que não ter
    /// interação nenhuma.
    #[test] fn uma_pergunta_no_meio_do_texto_nao_conta() {
        assert_eq!(extract("Por que estava lento? Porque o índice era relido a cada busca. Já corrigi."),None);
    }

    #[test] fn a_lista_logo_abaixo_do_enunciado_e_a_alternativa() {
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

    #[test] fn negrito_e_cabecalho_saem_do_enunciado() {
        assert_eq!(extract("**Devo aplicar a migração agora?**").expect("pergunta").prompt,"Devo aplicar a migração agora?");
        assert_eq!(extract("## E o teste, escrevo antes?").expect("pergunta").prompt,"E o teste, escrevo antes?");
    }

    /// Uma lista sem pergunta acima dela é um relatório, não uma escolha.
    #[test] fn uma_lista_sem_enunciado_nao_habilita_nada() {
        assert_eq!(extract("Mudei três arquivos:\n\n- lib.rs\n- turns.rs\n- main.js"),None);
    }

    #[test] fn uma_frase_comum_nao_e_item_de_lista() {
        assert_eq!(bullet("Ok. Continuo amanhã"),None);
        assert_eq!(bullet("- "),None);
        assert_eq!(bullet("1."),None);
        assert_eq!(bullet("12) doze"),Some("doze"));
    }

    /// Sem o JEV a leitura é a mais simples que se sustenta. Ela habilita a
    /// interação do mesmo jeito: o box não fica esperando uma chave de API.
    #[test] fn a_reserva_le_lista_como_escolha_e_enunciado_sozinho_como_sim_ou_nao() {
        let escolha=guess(&candidate("Qual deles?",&["Vite","Webpack"])).expect("escolha");
        assert_eq!((escolha.kind,escolha.options.len(),escolha.source.as_str()),(Shape::Single,2,LOCAL_SOURCE));
        let confirmacao=guess(&candidate("Regenero o índice?",&[])).expect("sim ou não");
        assert_eq!((confirmacao.kind,confirmacao.options.len()),(Shape::Noul,0));
    }

    /// O "não é pergunta" do JEV é veredito, não silêncio: quem voltar a
    /// consultar a heurística por cima dele faz o box travar contra a decisão
    /// de quem devia decidir.
    #[test] fn o_veredito_de_que_nao_ha_pergunta_nao_habilita_nada() {
        let extracted=candidate("Qual deles?",&["Vite","Webpack"]);
        assert_eq!(shape("none",&extracted,true),None);
        assert_eq!(shape("single",&extracted,true),Some(Shape::Single));
        // Alternativas que não são alternativas não têm botão para oferecer.
        assert_eq!(shape("single",&extracted,false),None);
        assert_eq!(shape("multiple",&candidate("Quais?",&[]),true),None);
        // Sim ou não não depende de lista nenhuma.
        assert_eq!(shape("noul",&candidate("Sigo?",&[]),false),Some(Shape::Noul));
    }

    #[test] fn a_escolha_clicada_vira_sempre_a_mesma_linha() {
        let options=vec!["Vite".to_string(),"esbuild".to_string()];
        assert_eq!(compose("Sigo?",Shape::Noul,&[],&[YES.into()],None).expect("sim"),"Resposta à pergunta «Sigo?»: SIM");
        assert_eq!(compose("Sigo?",Shape::Noul,&[],&[NO.into()],None).expect("não"),"Resposta à pergunta «Sigo?»: NÃO");
        assert_eq!(compose("Qual?",Shape::Single,&options,&["Vite".into()],None).expect("única"),"Resposta à pergunta «Qual?»: Vite");
        assert_eq!(compose("Quais?",Shape::Multiple,&options,&["Vite".into(),"esbuild".into()],None).expect("múltipla"),"Resposta à pergunta «Quais?»: Vite, esbuild");
        // RESPONDER vale para qualquer tipo: ele existe para quando nenhuma
        // alternativa serve.
        assert_eq!(compose("Qual?",Shape::Single,&options,&[],Some(" só no build ")).expect("texto"),"Resposta à pergunta «Qual?»: só no build");
    }

    #[test] fn a_escolha_que_a_pergunta_nao_ofereceu_e_recusada() {
        let options=vec!["Vite".to_string()];
        assert!(compose("Qual?",Shape::Single,&options,&["Rollup".into()],None).is_err(),"a tela não inventa alternativa");
        assert!(compose("Qual?",Shape::Single,&options,&[],None).is_err(),"sem escolha e sem texto não há resposta");
        assert!(compose("Quais?",Shape::Single,&options,&["Vite".into(),"Vite".into()],None).is_err(),"escolha única é uma só");
        assert!(compose("Sigo?",Shape::Noul,&[],&["talvez".into()],None).is_err(),"sim ou não é sim ou não");
    }

    /// Um `SIM` sozinho seria barrado por faltas que o pedido de origem já tinha
    /// suprido. Quem voltar a pontuar a resposta sozinha faz a portaria barrar o
    /// próprio fluxo que ela mandou o modelo abrir.
    #[test] fn o_par_carrega_o_pedido_que_fez_a_pergunta_nascer() {
        let par=pair("Regenere o índice do RAG em src-tauri/src/rag.rs e diga quantos arquivos entraram.","Resposta à pergunta «Sigo?»: SIM");
        assert!(par.starts_with("Regenere o índice"),"o pedido original abre o par: {par}");
        assert!(par.ends_with("SIM"),"a resposta fecha o par: {par}");
        assert!(par.contains("«Sigo?»"),"a pergunta viaja dentro da linha da resposta: {par}");
        assert_eq!(pair("   ","Resposta: SIM"),"Resposta: SIM","sem pedido de origem, o par é a resposta");
    }

    /// A prova do par contra a portaria de verdade: a mesma resposta, pontuada
    /// sozinha e em par. O portão não sabe que é resposta — ele lê texto —, e é
    /// por isso que o par tem de chegar montado até ele.
    #[test] fn a_portaria_pontua_melhor_a_resposta_em_par_do_que_o_sim_sozinho() {
        use crate::{gatekeeper::{self,EntryVerdict},turns::{Turn,TurnStatus}};
        let turn=Turn{id:"t1".into(),chat_id:"c1".into(),code:"XY4T9B·02".into(),ordinal:2,status:TurnStatus::Flying,created_at:chrono::Utc::now()};
        let judge=|text:&str|gatekeeper::judge(&turn,text,&gatekeeper::heuristic_entry(text),"heurística local");

        let sozinho=compose("Regenero o índice agora?",Shape::Noul,&[],&[YES.into()],None).expect("resposta");
        let par=pair("Regenere o índice do RAG em src-tauri/src/rag.rs; pronto quando cargo test passar.",&sozinho);

        let solto=judge(&sozinho);
        let junto=judge(&par);
        assert!(junto.score>solto.score,"o par diz onde mexer e quando está pronto: {} contra {}",junto.score,solto.score);
        assert_ne!(junto.verdict,EntryVerdict::Block,"a portaria não barra o fluxo que ela própria mandou abrir");
    }

    #[test] fn o_tipo_vai_e_volta_do_banco_como_texto() {
        for kind in [Shape::Noul,Shape::Single,Shape::Multiple] {
            assert_eq!(Shape::parse(kind.as_str()).expect("tipo"),kind);
        }
        assert!(Shape::parse("score").is_err(),"`score` ficou fora de propósito");
    }
}
