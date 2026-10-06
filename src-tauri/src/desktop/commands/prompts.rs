//! O envio: aceitar um pedido, responder e dispensar a pergunta em aberto.
//! Nenhum destes comandos chama modelo — eles escrevem no banco e tocam o sino
//! da fila.

use crate::i18n::{failure, Text};
use crate::desktop::events::*;
use crate::desktop::{Cancels, QueueBell, SharedWorkspace};
use crate::turns::{self, Turn, TurnStatus};
use crate::asking;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProcessRequest {
    pub input:String,
    pub session_id:Option<String>,
    /// Preenchido só no reenvio: é o turno que falhou voltando ao ar com o
    /// mesmo código, em vez de um pedido novo com um código novo.
    pub turn_id:Option<String>,
    /// O que o desenvolvedor liberou só para este pedido, no seletor de
    /// permissões da caixa de mensagem.
    #[serde(default)] pub grants:Option<crate::llm::Grants>,
}

/// Aceita o pedido e devolve o turno. Só isso — e é de propósito: esta chamada
/// escreve o que o desenvolvedor mandou, numera o pedido e volta na hora,
/// sem tocar em modelo nenhum. A partir do instante em que ela retorna, a
/// mensagem existe em disco e a tela a lê de lá, como lê qualquer mensagem
/// antiga; nada do que acontecer depois pode fazê-la sumir. Mandar outra coisa
/// por cima não atropela nada: o segundo pedido entra na fila atrás do
/// primeiro, com o seu próprio número, e espera a vez.
#[tauri::command]
pub(crate) async fn enqueue_prompt(app:AppHandle,workspace:State<'_,SharedWorkspace>,bell:State<'_,QueueBell>,request:ProcessRequest)->Result<Turn,Text>{crate::desktop::require_session()?;
    let chat_id=request.session_id.as_deref().ok_or_else(||Text::new("prompt.noChat"))?;
    let input=request.input.trim();
    if input.is_empty(){return Err(Text::new("prompt.empty"));}
    let turn={
        let mut workspace=workspace.lock().await;
        if !workspace.contains_chat(chat_id).map_err(failure)?{return Err(Text::new("chat.notFound"));}
        let turn=workspace.enqueue_prompt(chat_id,input,request.turn_id.as_deref()).map_err(failure)?;
        // O reenvio sem seletor mantém o que o pedido já tinha liberado.
        if let Some(grants)=&request.grants { workspace.set_turn_grants(&turn.id,grants).map_err(failure)?; }
        turn
    };
    let _=app.emit(PROMPT_EVENT,PromptEvent{chat_id:chat_id.to_string()});
    bell.notify_one();
    Ok(turn)
}

/// O que a tela manda quando o desenvolvedor responde. O tipo da pergunta não
/// vem daqui: ele está no banco, e é de lá que sai — a tela não redefine o que
/// foi perguntado.
#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct AnswerInput {
    pub question_turn_id:String,
    #[serde(default)] pub picked:Vec<String>,
    /// O caminho do `RESPONDER`: texto livre, que vale para qualquer tipo.
    #[serde(default)] pub text:Option<String>,
}

/// Responde à pergunta em aberto. O texto do pedido é composto aqui, e não na
/// tela: é ele que fica no chat, é ele que a Portaria pontua e é ele que chega
/// ao modelo — a tela não escolhe outras palavras para o que foi clicado.
///
/// O pedido entra na fila como qualquer outro. Nenhum caminho dispensa o portão.
#[tauri::command]
pub(crate) async fn answer_question(app:AppHandle,workspace:State<'_,SharedWorkspace>,bell:State<'_,QueueBell>,answer:AnswerInput)->Result<Option<Turn>,Text>{crate::desktop::require_session()?;
    let mut store=workspace.lock().await;
    let question=store.question_of(&answer.question_turn_id).map_err(failure)?.ok_or_else(||Text::new("question.gone"))?;
    if question.status!=turns::QUESTION_PENDING {return Err(Text::new("question.closed"));}
    if question.source==asking::PERMISSION_SOURCE { drop(store); return answer_permission(&app,workspace.inner(),bell.inner(),&question,&answer).await; }
    // A confirmação da portaria grava a escolha (ou o pedido completado), não
    // "Resposta à pergunta…": a fila a lê para mandar o pedido de origem.
    let composed=if question.source==crate::gatekeeper::GATE_SOURCE {
        crate::gatekeeper::gate_answer(&answer.picked,answer.text.as_deref()).map_err(failure)?
    } else {
        let kind=asking::Shape::parse(&question.kind).map_err(failure)?;
        asking::compose(&question.prompt,kind,&question.options,&answer.picked,answer.text.as_deref()).map_err(failure)?
    };
    let chat_id=store.chat_of_turn(&question.turn_id).map_err(failure)?.ok_or_else(||Text::new("question.originGone"))?;
    let turn=store.enqueue_prompt(&chat_id,&composed,None).map_err(failure)?;
    // O vínculo é o que faz a Portaria julgar a resposta em par com a pergunta.
    if !store.settle_question(&question.turn_id,turns::QUESTION_ANSWERED,Some(&turn.id)).map_err(failure)? {
        eprintln!("pergunta: `{}` foi encerrada por outro caminho enquanto era respondida",question.turn_id);
    }
    drop(store);
    let _=app.emit(PROMPT_EVENT,PromptEvent{chat_id});
    bell.notify_one();
    Ok(Some(turn))
}

/// A resposta ao comando que o agente pediu. Executar e sempre permitir
/// repetem o pedido de origem com a permissão liberada só para ele (a pergunta
/// e a resposta vão em par, como em qualquer pergunta); sempre permitir grava
/// o começo do comando no projeto. Negar fecha a pergunta e não chama agente
/// nenhum: o agente já disse o que fez sem o comando.
async fn answer_permission(app:&AppHandle,workspace:&SharedWorkspace,bell:&QueueBell,question:&turns::QuestionView,answer:&AnswerInput)->Result<Option<Turn>,Text> {
    let mut store=workspace.lock().await;
    let commands=question.prompt.lines().map(str::trim).filter(|line|!line.is_empty()).map(String::from).collect::<Vec<_>>();
    let chat_id=store.chat_of_turn(&question.turn_id).map_err(failure)?.ok_or_else(||Text::new("question.originGone"))?;
    let choice=answer.picked.first().map(String::as_str).unwrap_or_default();
    match choice {
        asking::PERMISSION_DENY=>{
            if !store.settle_question(&question.turn_id,turns::QUESTION_DISMISSED,None).map_err(failure)? { return Err(Text::new("question.closed")); }
            let _=store.record_beat(&question.turn_id,"permission_denied",&serde_json::json!({"commands":commands}));
            drop(store);
            let _=app.emit(TURN_EVENT,TurnEvent{chat_id,turn_id:question.turn_id.clone()});
            Ok(None)
        }
        asking::PERMISSION_RUN|asking::PERMISSION_ALWAYS=>{
            let always=choice==asking::PERMISSION_ALWAYS;
            if always { for command in &commands { store.allow_command(&chat_id,&crate::llm::command_prefix(command)).map_err(failure)?; } }
            let turn=store.enqueue_prompt(&chat_id,&asking::permission_answer(&commands,always),None).map_err(failure)?;
            store.set_turn_grants(&turn.id,&crate::llm::Grants{commands:commands.clone(),..Default::default()}).map_err(failure)?;
            if !store.settle_question(&question.turn_id,turns::QUESTION_ANSWERED,Some(&turn.id)).map_err(failure)? {
                eprintln!("permissão: `{}` foi encerrada por outro caminho enquanto era respondida",question.turn_id);
            }
            drop(store);
            let _=app.emit(PROMPT_EVENT,PromptEvent{chat_id});
            bell.notify_one();
            Ok(Some(turn))
        }
        other=>Err(Text::new("answer.unknown").with("option",other)),
    }
}

/// Os comandos sempre permitidos no projeto do chat.
#[tauri::command]
pub(crate) async fn allowed_commands(workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<Vec<String>,Text>{crate::desktop::require_session()?;
    workspace.lock().await.allowed_commands(&chat_id).map_err(failure)
}

/// Tira um comando dos sempre permitidos: volta a ser perguntado.
#[tauri::command]
pub(crate) async fn forget_allowed_command(workspace:State<'_,SharedWorkspace>,chat_id:String,command:String)->Result<Vec<String>,Text>{crate::desktop::require_session()?;
    let mut store=workspace.lock().await;
    store.forget_allowed_command(&chat_id,&command).map_err(failure)?;
    store.allowed_commands(&chat_id).map_err(failure)
}

/// Descarta a pergunta e devolve o box ao desenvolvedor. A decisão fica
/// registrada: a linha em `questions` guarda que foi ignorada e quando, e a
/// narração do turno ganha o evento. Ignorar é uma escolha, e escolha não some.
#[tauri::command]
pub(crate) async fn dismiss_question(app:AppHandle,workspace:State<'_,SharedWorkspace>,question_turn_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let mut store=workspace.lock().await;
    let question=store.question_of(&question_turn_id).map_err(failure)?.ok_or_else(||Text::new("question.gone"))?;
    if !store.settle_question(&question_turn_id,turns::QUESTION_DISMISSED,None).map_err(failure)? {
        return Err(Text::new("question.closed"));
    }
    let _=store.record_beat(&question_turn_id,"dismissed",&serde_json::json!({"prompt":question.prompt}));
    let chat_id=store.chat_of_turn(&question_turn_id).map_err(failure)?.unwrap_or_default();
    drop(store);
    let _=app.emit(TURN_EVENT,TurnEvent{chat_id,turn_id:question_turn_id});
    Ok(())
}

/// Para um pedido. O que espera na fila sai dela como falho, com o motivo no
/// chat e o reenvio no balão; o que está no ar recebe o sinal, e o agente cai
/// junto com tudo o que abriu. O pedido já fechado não muda.
///
/// A leitura e a mudança de estado acontecem sob o cadeado do banco — o mesmo
/// com que a fila chama o próximo —, então o pedido nunca é dado por falho e
/// atendido ao mesmo tempo.
#[tauri::command]
pub(crate) async fn cancel_turn(app:AppHandle,workspace:State<'_,SharedWorkspace>,cancels:State<'_,Cancels>,turn_id:String)->Result<(),Text>{crate::desktop::require_session()?;
    let mut store=workspace.lock().await;
    let Some(turn)=store.turn(&turn_id).map_err(failure)? else { return Ok(()) };
    match turn.status {
        TurnStatus::Queued=>{
            let notice=crate::i18n::notice(&[Text::new("turn.cancelled")]);
            store.append_answer(&turn.chat_id,&turn.id,&notice).map_err(failure)?;
            store.set_turn_status(&turn.id,TurnStatus::Failed).map_err(failure)?;
        }
        TurnStatus::Flying=>cancels.stop(&turn.id,crate::progress::StopReason::Asked),
        _=>return Ok(()),
    }
    drop(store);
    let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id,turn_id});
    Ok(())
}
