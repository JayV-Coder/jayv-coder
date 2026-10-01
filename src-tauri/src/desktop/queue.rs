//! O atendente da fila: tira do banco o próximo pedido, passa pela portaria,
//! chama o orquestrador e narra o caminho para a tela e para o disco.

use super::events::*;
use super::{DesktopState, QueueBell, SharedDesktopState, SharedWorkspace};
use crate::sync::{Connectivity, Link};
use crate::gatekeeper::{self, EntryCheck, EntryVerdict, ExitCheck};
use crate::progress::{Beat, Debounce, Pulse};
use crate::turns::{Turn, TurnStatus};
use crate::i18n::{self, Text};
use crate::{asking, jev, model, usage};
use std::{path::Path, time::Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

/// O atendente da fila: um pedido de cada vez, na ordem em que chegaram, até
/// não sobrar nenhum — e então volta a dormir no sino. Ele existe uma vez só no
/// aplicativo, e é por isso que dois envios seguidos nunca disputam o
/// orquestrador: o segundo não é uma chamada esperando na porta, é uma linha no
/// banco esperando a vez.
pub(crate) async fn serve_the_queue(app:AppHandle,desk:SharedDesktopState,workspace:SharedWorkspace,bell:QueueBell,connectivity:Connectivity) {
    loop {
        // O Jev mora no Supabase: sem conexão e sessão válida os pedidos
        // esperam na fila, e o sino da volta da rede os chama em ordem.
        while connectivity.get()==Link::Online {
            let claimed=workspace.lock().await.claim_next_turn();
            let next=match claimed {
                Ok(Some(next))=>next,
                Ok(None)=>break,
                Err(error)=>{eprintln!("fila: não consegui chamar o próximo pedido ({error})");break;}
            };
            let (turn,prompt)=next;
            let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
            serve(&app,&desk,&workspace,&turn,&prompt).await;
            let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
        }
        bell.notified().await;
    }
}

/// Abre o barramento do pedido, atende, e só então fecha o barramento. A ordem
/// importa: a narradora ainda pode ter um rascunho da resposta para gravar, e
/// apagar o rascunho antes de ela terminar deixaria a tela com duas versões do
/// mesmo texto. Por isso o rascunho é apagado no fim, depois de a resposta
/// definitiva estar em `messages` e de a narradora ter se despedido.
async fn serve(app:&AppHandle,desk:&SharedDesktopState,workspace:&SharedWorkspace,turn:&Turn,prompt:&str) {
    let (pulse,beats)=Pulse::channel();
    let narrator=tauri::async_runtime::spawn(narrate(app.clone(),workspace.clone(),turn.clone(),beats));
    // Tudo que o atendimento gastar — o Jev, o modelo, o batismo — é deste
    // turno, deste chat e deste projeto.
    let project_id=workspace.lock().await.chat_project(&turn.chat_id).unwrap_or(None);
    let scope=usage::Scope{project_id,chat_id:Some(turn.chat_id.clone()),turn_id:Some(turn.id.clone())};
    usage::within(scope,attend(app,desk,workspace,turn,prompt,&pulse)).await;
    drop(pulse);
    let _=narrator.await;
    let _=workspace.lock().await.clear_turn_partial(&turn.id);
}

/// A consumidora do barramento. Ela é a única que sabe que existe tela e banco:
/// o núcleo só empurra eventos. E trata os dois com ritmos diferentes de
/// propósito — a webview recebe cada pedaço na hora, porque é isso que faz o
/// texto crescer, e o disco recebe o texto acumulado com folga, porque gravar
/// token a token faria do SQLite um log de tokens.
///
/// Ela toca só o cadeado do banco, em trechos curtos, e nunca o do
/// orquestrador: a ordem de cadeados continua a mesma.
async fn narrate(app:AppHandle,workspace:SharedWorkspace,turn:Turn,mut beats:mpsc::UnboundedReceiver<Beat>) {
    let mut answer=String::new();
    let mut slack=Debounce::start(Instant::now());
    while let Some(beat)=beats.recv().await {
        if let Beat::Chunk{text}=&beat {
            answer.push_str(text);
            let _=app.emit(CHUNK_EVENT,ChunkEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone(),text:text.clone()});
            let now=Instant::now();
            if slack.accept(text.len(),now) {
                let _=workspace.lock().await.set_turn_partial(&turn.id,&answer);
                slack.wrote(now);
            }
            continue;
        }
        let (kind,mut detail,settles)=(beat.kind().to_string(),beat.detail(),beat.settles());
        // O fim de um turno medido leva a marca: a importação dos turnos
        // antigos, numa máquina que atualizar depois, não o conta de novo.
        if kind=="done" { detail["metered"]=serde_json::Value::Bool(true); }
        let seq={
            let mut workspace=workspace.lock().await;
            let seq=workspace.record_beat(&turn.id,&kind,&detail).unwrap_or_default();
            // O fim do turno paga a escrita extra: o que ficou na folga tem de
            // estar no disco antes de o pedido sair do ar.
            if settles && slack.waiting()>0 {
                let _=workspace.set_turn_partial(&turn.id,&answer);
                slack.wrote(Instant::now());
            }
            seq
        };
        let _=app.emit(BEAT_EVENT,BeatEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone(),seq,kind,detail});
    }
    // O canal fechou. Nem todo caminho passa por um desfecho anunciado — uma
    // pasta que sumiu, um portão que barrou —, então a descarga final é aqui.
    if slack.waiting()>0 {
        let _=workspace.lock().await.set_turn_partial(&turn.id,&answer);
    }
}

/// Atende um pedido do começo ao fim. O texto vem do banco, não da tela, e o
/// turno sai daqui sempre fechado — respondido, barrado ou falho. Um turno que
/// saísse em aberto travaria a fila inteira atrás dele.
async fn attend(app:&AppHandle,desk:&SharedDesktopState,workspace:&SharedWorkspace,turn:&Turn,prompt:&str,pulse:&Pulse) {
    let chat_id=turn.chat_id.as_str();
    // O que está gravado pode ser um aviso para a tela (a resposta a uma
    // pergunta); a portaria e o modelo o leem em inglês.
    let prompt=i18n::for_model(prompt);
    let prompt=prompt.as_str();
    let mut state=desk.lock().await;
    let unnamed=workspace.lock().await.chat_is_unnamed(chat_id).unwrap_or(false);

    // O pedido é lido dentro da pasta do projeto. Se ela sumiu do disco, o
    // atendimento morre aqui — mas com a mensagem já escrita, o turno dado por
    // falho e o motivo no chat, em vez de sumir da conversa.
    if let Err(error)=focus_on_chat_project(&mut state,workspace,chat_id).await {
        let error=i18n::notice(&[error]);
        pulse.beat(Beat::Failed{error:error.clone()});
        fail_turn(workspace,chat_id,turn,error).await;
        return;
    }
    let project=state.orchestrator.rag.project_info();

    // Um turno-resposta é julgado — e enviado — em par com a pergunta que o
    // originou. Um `SIM` sozinho seria barrado por faltas que o pedido de origem
    // já tinha suprido, e o modelo receberia uma palavra sem assunto. O que a
    // portaria pontua é exatamente o que chega ao modelo.
    let origin=workspace.lock().await.question_origin(&turn.id).unwrap_or(None);
    let paired=origin.map(|origin|asking::pair(&i18n::for_model(&origin),prompt));
    let request=paired.as_deref().unwrap_or(prompt);
    let entry=entry_check(&project,turn,request).await;
    {
        let mut workspace=workspace.lock().await;
        let _=workspace.record_entry_check(&entry);
    }
    usage::mark(usage::JevMark::count(format!("entry:{}",entry.verdict.as_str()),1));
    let _=app.emit(ENTRY_EVENT,EntryEvent{check:entry.clone()});
    pulse.beat(Beat::Gate{verdict:entry.verdict.as_str().into(),score:entry.score,demand:entry.demand});
    if entry.verdict==EntryVerdict::Block {
        // O que o pedido barrado teria custado: ele mesmo na ida e uma
        // resposta média na volta. Estimativa, e marcada assim.
        let typical=workspace.lock().await.average_output().unwrap_or(0);
        usage::mark(usage::JevMark::saved("blocked",usage::estimate(request)+typical));
        let reply=entry.reply();
        // O barrado também é resposta, e a tela mostra o motivo crescendo como
        // mostraria qualquer outra.
        pulse.beat(Beat::Chunk{text:reply.clone()});
        pulse.beat(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0});
        state.orchestrator.memory.add_message(chat_id,"user",prompt.trim().to_string());
        state.orchestrator.memory.add_message(chat_id,"assistant",i18n::for_model(&reply));
        let mut workspace=workspace.lock().await;
        let _=workspace.append_answer(chat_id,&turn.id,&reply);
        let _=workspace.set_turn_status(&turn.id,TurnStatus::Blocked);
        return;
    }
    state.orchestrator.pending_gate_note=entry.clarifying_note();
    state.orchestrator.pending_brief=entry.refined_prompt(request);

    // O `process` torna a anotar o pedido na memória da sessão, e ele já está
    // no banco desde o envio: sem esta poda o modelo receberia a mesma linha
    // duas vezes no histórico.
    if let Err(error)=forget_pending_prompt(&mut state,workspace,chat_id).await {eprintln!("fila: histórico da sessão desalinhado ({error})");}

    let result=state.orchestrator.process(request,Some(chat_id),pulse).await;
    let assistant=result.result.as_ref().map(|response|response.response.clone()).or_else(||result.error.clone()).unwrap_or_else(||i18n::notice(&[Text::new("turn.noAnswer")]));
    if result.result.is_none(){state.orchestrator.memory.add_message(chat_id,"assistant",i18n::for_model(&assistant));}
    // Só a resposta do modelo passa pelo portão de saída; um aviso de falha
    // não pede para rodar nem mexer em nada.
    let exits=if result.result.is_some(){exit_checks(&state,turn,&assistant)}else{vec![]};
    {
        let mut workspace=workspace.lock().await;
        let _=workspace.append_answer(chat_id,&turn.id,&assistant);
        let _=workspace.record_exit_checks(turn,&exits);
        let _=workspace.set_turn_status(&turn.id,if result.result.is_some(){TurnStatus::Answered}else{TurnStatus::Failed});
    }
    for exit in &exits { usage::mark(usage::JevMark::count(format!("exit:{}",exit.verdict.as_str()),1)); }
    if !exits.is_empty(){let _=app.emit(EXIT_EVENT,ExitEvent{checks:exits});}
    drop(state);
    if result.result.is_some() {enable_question(app,workspace,turn,&assistant).await;}
    if unnamed {name_in_background(app.clone(),desk.clone(),workspace.clone(),chat_id.to_string(),prompt.to_string(),jev_reading(&result));}
}

/// A resposta do modelo volta ao Jev, e é o retorno dele que **habilita** a
/// interação no box. Nada aqui inventa pergunta: o enunciado e as alternativas
/// são extraídos do texto que o modelo escreveu, e o Jev diz se aquilo é
/// pergunta e de que tipo.
///
/// Roda fora do cadeado do orquestrador de propósito — é uma ida à rede, e a
/// fila não pode ficar parada atrás dela.
async fn enable_question(app:&AppHandle,workspace:&SharedWorkspace,turn:&Turn,answer:&str) {
    let Some(question)=asking::classify(answer).await else {return};
    let recorded={
        let mut workspace=workspace.lock().await;
        workspace.ask_question(&turn.id,question.kind.as_str(),&question.prompt,&question.options,&question.source)
    };
    match recorded {
        Ok(())=>{let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});}
        Err(error)=>eprintln!("pergunta: não consegui habilitar a interação do turno `{}` ({error})",turn.id),
    }
}

/// O título definitivo depende de outra ida ao modelo, e a fila não pode
/// esperar por ela: o chat já entrou na lista com o resumo local do pedido, e o
/// próximo da fila tem direito ao orquestrador antes de qualquer enfeite. O
/// batismo pega o cadeado quando ele estiver livre e avisa a interface.
fn name_in_background(app:AppHandle,desk:SharedDesktopState,workspace:SharedWorkspace,chat_id:String,prompt:String,reading:String) {
    // O batismo roda noutro task: o escopo do turno vai junto, à mão.
    let scope=usage::current_scope();
    tauri::async_runtime::spawn(usage::within(scope,async move {
        let state=desk.lock().await;
        let Some(title)=state.orchestrator.name_chat(&prompt,&reading).await else {return};
        drop(state);
        if workspace.lock().await.rename_chat(&chat_id,&title).is_ok() {let _=app.emit(RENAME_EVENT,RenameEvent{chat_id,title});}
    }));
}

/// O pedido que não sai do lugar: o turno é dado por falho e o motivo entra no
/// chat como resposta. Sem isto ele ficaria voando até o aplicativo reabrir, e
/// o balão não ofereceria o reenvio a quem acabou de ver o erro.
async fn fail_turn(workspace:&SharedWorkspace,chat_id:&str,turn:&Turn,error:String) {
    let mut workspace=workspace.lock().await;
    let _=workspace.append_answer(chat_id,&turn.id,&error);
    let _=workspace.set_turn_status(&turn.id,TurnStatus::Failed);
}

/// O chat mora num projeto, e o pedido tem de ser lido dentro da pasta desse
/// projeto: é dela que saem os arquivos do contexto, o nome no prompt da
/// portaria e a varredura da saída. Projeto sem pasta cai na raiz de partida.
async fn focus_on_chat_project(state:&mut DesktopState,workspace:&SharedWorkspace,chat_id:&str)->Result<(),Text> {
    let root=workspace.lock().await.chat_root(chat_id).map_err(i18n::failure)?.unwrap_or_else(||state.home_root.clone());
    state.orchestrator.focus_on(&root).map_err(i18n::failure)
}

/// O pedido está gravado desde o envio, e o `process` torna a anotá-lo na
/// memória da sessão: sem isto o modelo receberia a mesma linha duas vezes no
/// histórico.
async fn forget_pending_prompt(state:&mut DesktopState,workspace:&SharedWorkspace,chat_id:&str)->anyhow::Result<()> {
    let mut history=workspace.lock().await.conversation(chat_id)?;
    if history.last().is_some_and(|message|message.role=="user") {history.pop();}
    state.orchestrator.memory.set_conversation(chat_id.to_string(),history);
    Ok(())
}

/// Como o Jev entendeu o pedido, em uma linha: é isso que vai junto do prompt
/// quando o modelo escolhe o título.
fn jev_reading(result:&model::ProcessResult)->String{format!("{} task, {} complexity, routed to {}",result.intent_analysis.intent,result.complexity,result.model_selection.model_name)}

/// Pontua o pedido no Jev quando há credencial e nas heurísticas locais quando
/// não há — ou quando a chamada falha, para que o portão nunca trave o envio.
async fn entry_check(project:&model::ProjectInfo,turn:&Turn,input:&str)->EntryCheck {
    if jev::is_configured() {
        match gatekeeper::evaluate_entry(input,&project.name,&project.languages).await {
            Ok(reading)=>return gatekeeper::judge(turn,input,&reading,"jev"),
            Err(error)=>eprintln!("portaria: o Jev não respondeu, usando heurísticas locais ({error})"),
        }
    }
    gatekeeper::judge(turn,input,&gatekeeper::heuristic_entry(input),asking::LOCAL_SOURCE)
}

fn exit_checks(state:&DesktopState,turn:&Turn,answer:&str)->Vec<ExitCheck> {
    let root=state.orchestrator.rag.project_info().root;
    gatekeeper::scan_answer(turn,answer,&state.orchestrator.config,&state.orchestrator.firewall,Path::new(&root))
}
