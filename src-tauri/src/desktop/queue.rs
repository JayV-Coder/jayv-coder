//! O atendente da fila: tira do banco o próximo pedido, passa pela portaria,
//! chama o orquestrador e narra o caminho para a tela e para o disco.

use super::events::*;
use super::{Cancels, DesktopState, Lanes, QueueBell, SharedDesktopState, SharedForget, SharedWorkspace};
use crate::sync::{Connectivity, Link};
use crate::gatekeeper::{self, EntryCheck, EntryVerdict, ExitCheck, ExitVerdict};
use crate::progress::{Beat, Debounce, Frame, Pulse, Stop, StopReason};
use crate::turns::{Turn, TurnStatus};
use crate::i18n::{self, Text};
use crate::{asking, features, jev, model, project_memory, search, usage};
use std::{collections::HashMap, path::Path, time::Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

/// O atendente da fila: chama os pedidos na ordem em que chegaram, até não
/// sobrar nenhum — e então volta a dormir no sino. Ele existe uma vez só no
/// aplicativo, e é por isso que dois envios seguidos nunca disputam um
/// orquestrador: o segundo não é uma chamada esperando na porta, é uma linha no
/// banco esperando a vez.
///
/// Com o plano permitindo, pedidos de projetos diferentes correm ao mesmo
/// tempo, cada um no seu atendente (`Lanes`); os de um mesmo projeto, e
/// portanto os de um mesmo chat, saem um de cada vez.
pub(crate) async fn serve_the_queue(app:AppHandle,lanes:Lanes,workspace:SharedWorkspace,bell:QueueBell,connectivity:Connectivity,forget:SharedForget,cancels:Cancels) {
    // Os atendimentos no ar, com o atendente e o turno de cada um.
    let mut running=tokio::task::JoinSet::new();
    let mut flying:HashMap<tokio::task::Id,(usize,Turn)>=HashMap::new();
    let mut busy:Vec<bool>=Vec::new();
    // O atendente que serviu cada projeto por último: o índice da pasta dele
    // já está lido ali.
    let mut served:HashMap<String,usize>=HashMap::new();
    loop {
        // Com a sessão válida a fila anda, com rede ou sem: sem o Supabase a
        // portaria e o roteamento decidem pela heurística, e os agentes falam
        // com os provedores deles. Sem sessão (ou com ela vencida) os pedidos
        // esperam, e o sino da volta os chama em ordem.
        if serves(connectivity.get()) {
            // Quantos ao mesmo tempo diz o plano (um, sem plano). Os de um
            // mesmo projeto saem sempre um de cada vez (`claim_next_turn_within`).
            let _calling=lanes.calling().await;
            let limit=workspace.lock().await.entitlements().map(|plan|plan.concurrent_turns()).unwrap_or(1);
            while running.len()<limit {
                let claimed=workspace.lock().await.claim_next_turn_within(limit);
                let (turn,prompt)=match claimed {
                    Ok(Some(next))=>next,
                    Ok(None)=>break,
                    Err(error)=>{eprintln!("fila: não consegui chamar o próximo pedido ({error})");break;}
                };
                let project=workspace.lock().await.chat_project(&turn.chat_id).ok().flatten().unwrap_or_else(||turn.chat_id.clone());
                let index=pick_lane(&busy,served.get(&project).copied());
                served.insert(project,index);
                let desk=match lanes.lane(index) {
                    Ok(desk)=>desk,
                    Err(error)=>{
                        eprintln!("fila: não consegui abrir outro atendente ({error:#})");
                        fail_turn(&workspace,&turn.chat_id,&turn,i18n::notice(&[i18n::failure(error)])).await;
                        let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
                        continue;
                    }
                };
                if index==busy.len() { busy.push(false); }
                busy[index]=true;
                let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
                let (app,workspace,forget,cancels,task)=(app.clone(),workspace.clone(),forget.clone(),cancels.clone(),turn.clone());
                let handle=running.spawn(async move {
                    serve(&app,&desk,&workspace,&forget,&cancels,&task,&prompt).await;
                    let _=app.emit(TURN_EVENT,TurnEvent{chat_id:task.chat_id.clone(),turn_id:task.id.clone()});
                });
                flying.insert(handle.id(),(index,turn));
            }
        }
        tokio::select! {
            _=bell.notified()=>{}
            Some(done)=running.join_next_with_id(), if !running.is_empty()=>{
                let id=match &done { Ok((id,()))=>*id, Err(error)=>error.id() };
                if let Some((index,turn))=flying.remove(&id) {
                    busy[index]=false;
                    // O atendimento que caiu no meio deixaria o turno no ar
                    // para sempre — e o projeto dele parado atrás.
                    if let Err(error)=done {
                        eprintln!("fila: o atendimento de `{}` caiu ({error})",turn.id);
                        fail_turn(&workspace,&turn.chat_id,&turn,i18n::notice(&[Text::new("turn.noAnswer")])).await;
                        let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
                    }
                }
            }
        }
    }
}

/// O atendente livre para o próximo pedido: o que serviu o projeto da última
/// vez, se estiver livre; senão o primeiro livre; sem nenhum livre, um novo.
fn pick_lane(busy:&[bool],preferred:Option<usize>)->usize {
    preferred.filter(|index|busy.get(*index)==Some(&false))
        .or_else(||busy.iter().position(|taken|!taken))
        .unwrap_or(busy.len())
}

/// Se a fila anda com esta conexão.
fn serves(link:Link)->bool { matches!(link,Link::Online|Link::Offline) }

/// Abre o barramento do pedido, atende, e só então fecha o barramento. A ordem
/// importa: a narradora ainda pode ter um rascunho da resposta para gravar, e
/// apagar o rascunho antes de ela terminar deixaria a tela com duas versões do
/// mesmo texto. Por isso o rascunho é apagado no fim, depois de a resposta
/// definitiva estar em `messages` e de a narradora ter se despedido.
///
/// O pedido parado ("Parar", teto de minutos) termina com o aviso da parada;
/// o que o agente já tinha dito entra antes dele, como a tela o mostrou.
async fn serve(app:&AppHandle,desk:&SharedDesktopState,workspace:&SharedWorkspace,forget:&SharedForget,cancels:&Cancels,turn:&Turn,prompt:&str) {
    let stop=cancels.open(&turn.id);
    let (pulse,beats)=Pulse::channel_with(stop.clone());
    let narrator=tauri::async_runtime::spawn(narrate(app.clone(),workspace.clone(),turn.clone(),beats));
    // Tudo que o atendimento gastar — o Jev, o modelo, o batismo — é deste
    // turno, deste chat e deste projeto.
    let project_id=workspace.lock().await.chat_project(&turn.chat_id).unwrap_or(None);
    let scope=usage::Scope{project_id,chat_id:Some(turn.chat_id.clone()),turn_id:Some(turn.id.clone())};
    usage::within(scope,crate::guard::within(attend(app,desk,workspace,forget,turn,prompt,&pulse))).await;
    drop(pulse);
    let said=narrator.await.unwrap_or_default();
    cancels.close(&turn.id);
    let mut workspace=workspace.lock().await;
    if stop.reason().is_some() && !said.trim().is_empty() && workspace.turn(&turn.id).ok().flatten().is_some_and(|now|now.status==TurnStatus::Failed) {
        if let Err(error)=workspace.keep_said_before(&turn.chat_id,&turn.id,&said) { eprintln!("fila: o que o agente disse antes de parar não foi guardado ({error:#})"); }
    }
    let _=workspace.clear_turn_partial(&turn.id);
}

/// O relógio do teto total de um pedido. Passado o prazo, o pedido para com o
/// motivo `turn.ceiling`; largado antes (o pedido acabou), o relógio para.
struct Ceiling(tokio::task::JoinHandle<()>);

impl Ceiling {
    fn start(stop:Stop,minutes:u64)->Self {
        Self(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(minutes.saturating_mul(60))).await;
            stop.stop(StopReason::Ceiling{minutes});
        }))
    }
}

impl Drop for Ceiling { fn drop(&mut self) { self.0.abort(); } }

/// A consumidora do barramento. Ela é a única que sabe que existe tela e banco:
/// o núcleo só empurra eventos. E trata os dois com ritmos diferentes de
/// propósito — a webview recebe cada pedaço na hora, porque é isso que faz o
/// texto crescer, e o disco recebe o texto acumulado com folga, porque gravar
/// token a token faria do SQLite um log de tokens.
///
/// Ela toca só o cadeado do banco, em trechos curtos, e nunca o do
/// orquestrador: a ordem de cadeados continua a mesma.
///
/// Devolve o texto inteiro que narrou: é ele que fica no chat quando o pedido
/// para no meio.
async fn narrate(app:AppHandle,workspace:SharedWorkspace,turn:Turn,mut beats:mpsc::UnboundedReceiver<Beat>)->String {
    let mut answer=String::new();
    let mut slack=Debounce::start(Instant::now());
    // Os pedaços que ainda não foram para a tela. Com o Claude transmitindo
    // token a token, um aviso por pedaço era um redesenho da conversa por
    // token; juntos num quadro, o texto cresce igual e a tela não engasga.
    let mut frame=Frame::default();
    let emit=|text:String|{ let _=app.emit(CHUNK_EVENT,ChunkEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone(),text}); };
    loop {
        let beat=match frame.due() {
            None=>beats.recv().await,
            Some(due)=>tokio::select! {
                beat=beats.recv()=>beat,
                _=tokio::time::sleep_until(due)=>{ if let Some(text)=frame.take() { emit(text); } continue; }
            },
        };
        let Some(beat)=beat else { break };
        if let Beat::Chunk{text}=&beat {
            answer.push_str(text);
            if let Some(text)=frame.push(text,tokio::time::Instant::now()) { emit(text); }
            let now=Instant::now();
            if slack.accept(text.len(),now) {
                let _=workspace.lock().await.set_turn_partial(&turn.id,&answer);
                slack.wrote(now);
            }
            continue;
        }
        // O texto que esperava o quadro vai antes da etapa: a tela vê as duas
        // coisas na ordem em que aconteceram.
        if let Some(text)=frame.take() { emit(text); }
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
    if let Some(text)=frame.take() { emit(text); }
    if slack.waiting()>0 {
        let _=workspace.lock().await.set_turn_partial(&turn.id,&answer);
    }
    answer
}

/// Atende um pedido do começo ao fim. O texto vem do banco, não da tela, e o
/// turno sai daqui sempre fechado — respondido, barrado ou falho. Um turno que
/// saísse em aberto travaria a fila inteira atrás dele.
async fn attend(app:&AppHandle,desk:&SharedDesktopState,workspace:&SharedWorkspace,forget:&SharedForget,turn:&Turn,prompt:&str,pulse:&Pulse) {
    let chat_id=turn.chat_id.as_str();
    // O que está gravado pode ser um aviso para a tela (a resposta a uma
    // pergunta); a portaria e o modelo o leem em inglês.
    let stored=prompt;
    let prompt=i18n::for_model(prompt);
    let prompt=prompt.as_str();
    let mut state=desk.lock().await;
    // O teto conta daqui: a portaria, o plano, o agente e a revisão cabem
    // nele juntos.
    let _ceiling=Ceiling::start(pulse.stop().clone(),state.orchestrator.config.jev.turn_ceiling_minutes);
    forget_pending(&mut state,workspace,forget).await;
    let unnamed=workspace.lock().await.chat_is_unnamed(chat_id).unwrap_or(false);
    // A política de LLM do projeto e o plano vêm antes da pasta: eles podem
    // mudar a privacidade, e o índice da pasta é lido com o firewall já certo.
    // O que o desenvolvedor liberou para este pedido (ao aprovar o comando que
    // o agente pediu), o que ele deixou ligado no seletor do chat inteiro e os
    // comandos sempre permitidos no projeto. O próximo pedido lê de novo.
    let (grants,granted_here)={
        let workspace=workspace.lock().await;
        let mine=workspace.turn_grants(&turn.id).unwrap_or_default();
        let always=crate::llm::Grants{commands:workspace.allowed_commands(chat_id).unwrap_or_default(),..Default::default()};
        let chat=workspace.chat_grants(chat_id).unwrap_or_default();
        (mine.merged(&chat).merged(&always),!mine.is_empty())
    };
    let plan=apply_project_policy(&mut state,workspace,chat_id,&grants).await;
    // O nível é lido a cada pedido: a troca na tela, ou a que chegou de outro
    // computador pela sincronização, vale já para o próximo.
    state.orchestrator.expertise=workspace.lock().await.expertise().unwrap_or_default();
    state.orchestrator.lean_code=plan.lean_code(workspace.lock().await.lean_code().unwrap_or(true));

    // O pedido é lido dentro da pasta do projeto. Se ela sumiu do disco, o
    // atendimento morre aqui — mas com a mensagem já escrita, o turno dado por
    // falho e o motivo no chat, em vez de sumir da conversa.
    if let Err(error)=focus_on_chat_project(&mut state,workspace,chat_id).await {
        let error=i18n::notice(&[error]);
        pulse.beat(Beat::Failed{error:error.clone()});
        fail_turn(workspace,chat_id,turn,error).await;
        return;
    }
    // Daqui até o fim do pedido, a pasta do chat é olhada a cada segundo e a
    // tela vê cada arquivo que o agente mexer. O vigia para sozinho quando
    // este atendimento termina, por qualquer caminho.
    let chat_root=workspace.lock().await.chat_root(chat_id).unwrap_or(None);
    // Sem "arquivos ao vivo" no plano, ninguém olha a pasta: o `git status`
    // por segundo é custo que ninguém vê.
    let _live=match chat_root {
        Some(root) if plan.allows(features::LIVE_FILES)=>{
            let live=app.state::<super::live::SharedLive>().inner().clone();
            Some(super::live::watch(app,&live,chat_id,&turn.id,root,crate::firewall::ContextFirewall::new(state.orchestrator.config.privacy.clone())).await)
        }
        _=>None,
    };
    // A foto dos arquivos protegidos sai já, em paralelo com a portaria: no
    // fim, o que o agente mexeu neles fica segurado na saída.
    let guard=gatekeeper_guard(&state);
    let project=state.orchestrator.rag.project_info();
    let project_id=workspace.lock().await.chat_project(chat_id).unwrap_or(None);
    // As notas do projeto só entram com o recurso no plano.
    let notes_on=plan.allows(features::PROJECT_NOTES);
    let notes=match &project_id { Some(id) if notes_on=>workspace.lock().await.project_notes(id).unwrap_or_default(), _=>vec![] };

    // O pedido barrado e reenviado em seguida não poupou nada: o painel
    // desconta a economia que o bloqueio tinha contado.
    if let Ok(Some(saved))=workspace.lock().await.blocked_saving_to_revoke(&turn.id) {
        usage::mark(usage::JevMark{kind:crate::workspace::BLOCKED_SAVING.into(),amount:-saved,precision:usage::Precision::Estimated});
    }
    // "Não funcionou": o pedido anterior deste chat não resolveu, ainda que
    // a resposta tenha vindo inteira. O roteador aprende com isso.
    if crate::router::is_complaint(prompt) { state.orchestrator.mark_last_failed(chat_id); state.orchestrator.pending_retry=true; }

    // Um turno-resposta é julgado — e enviado — em par com a pergunta que o
    // originou. Um `SIM` sozinho seria barrado por faltas que o pedido de origem
    // já tinha suprido, e o modelo receberia uma palavra sem assunto. O que a
    // portaria pontua é exatamente o que chega ao modelo.
    // A corrente inteira vai junto: numa pergunta feita depois de outra
    // resposta, só a resposta anterior não diz qual era o pedido.
    let origin=workspace.lock().await.question_origin(&turn.id).unwrap_or_default();
    // A resposta à confirmação da portaria: a escolha manda o pedido de
    // origem como estava (ou reescrito); o texto livre o completa e é julgado
    // de novo, em par com ele.
    let gate=workspace.lock().await.answers_gate(&turn.id).unwrap_or(false);
    let choice=if gate { gatekeeper::gate_choice(stored) } else { None };
    let paired=(!origin.is_empty()).then(||{
        let origin=origin.iter().map(|said|i18n::for_model(said)).collect::<Vec<_>>().join("\n\n");
        if choice.is_some() { origin } else { asking::pair(&origin,prompt) }
    });
    let request=paired.as_deref().unwrap_or(prompt);
    // O `process` torna a anotar o pedido na memória da sessão, e ele já está
    // no banco desde o envio: sem esta poda o modelo receberia a mesma linha
    // duas vezes no histórico.
    if let Err(error)=forget_pending_prompt(&mut state,workspace,chat_id).await {eprintln!("fila: histórico da sessão desalinhado ({error})");}
    // A portaria e o roteamento do Jev saem juntos: são duas idas à rede que
    // não dependem uma da outra, e o pedido não espera uma depois da outra.
    // Se a portaria barrar, a leitura de roteamento é descartada.
    let routing=state.orchestrator.routes_with_jev().then(||state.orchestrator.routing_input_ahead(request,chat_id));
    let covered=project_memory::covered(&notes);
    let recent=state.orchestrator.recent_turns_ahead(chat_id);
    let (mut entry,routing)=tokio::join!(
        entry_check(&project,turn,request,state.orchestrator.expertise,&covered,&recent),
        async { match &routing { Some(input)=>Some(jev::route(input).await.map_err(|error|error.to_string())), None=>None } },
    );
    // Parado enquanto a portaria lia: nada de pergunta nem de agente.
    if let Some(reason)=pulse.stop().reason() {
        let error=i18n::notice(&[reason.text()]);
        pulse.beat(Beat::Failed{error:error.clone()});
        fail_turn(workspace,chat_id,turn,error).await;
        return;
    }
    if choice.is_some() {
        // O desenvolvedor confirmou o pedido que a portaria segurou: ele vai
        // ao agente liberado de vez.
        usage::mark(usage::JevMark::count("entry:confirmed",1));
        entry=entry.confirmed();
    } else if paired.is_some() {
        // Responder ao agente não é pedir de novo: a resposta herda a passagem
        // do pedido que levantou a pergunta. Sem leitura gravada (um turno de
        // antes da portaria), vale o que a pergunta prova: o modelo respondeu
        // aquele pedido.
        let origin=workspace.lock().await.question_verdict(&turn.id).ok().flatten().unwrap_or(EntryVerdict::Pass);
        entry=entry.inherit(origin);
    } else if gatekeeper::is_continuation(prompt) {
        // A continuação digitada ("pode implementar", "não funcionou") logo
        // depois de uma resposta herda o veredito do pedido que ela atendeu.
        // A leitura própria fica gravada na marca, para medir o efeito.
        let previous=workspace.lock().await.previous_answer(&turn.id).ok().flatten();
        let window=chrono::Duration::seconds((gatekeeper::current_parameters().continuation_minutes*60.0) as i64);
        if let Some((verdict,_))=previous.filter(|(verdict,at)|chrono::Utc::now()-*at<=window&&verdict.rank()>entry.verdict.rank()) {
            usage::mark(usage::JevMark::count(format!("entry_followed:{}",entry.verdict.as_str()),1));
            entry=entry.inherit(verdict);
        }
    }
    // Com um plano esperando no chat, a mensagem que manda executá-lo ("siga
    // com o desenvolvimento", "chega de plano") não é um pedido novo e vago:
    // a portaria não a barra, e o orquestrador a leva ao desenvolvimento com o
    // plano inteiro, mesmo que o app tenha sido reaberto depois do plano.
    let waiting_plan=workspace.lock().await.pending_plan(&turn.id).ok().flatten();
    state.orchestrator.pending_plan=None;
    if let Some(plan_text)=waiting_plan.filter(|_|choice.is_none()&&paired.is_none()&&crate::orchestrator::goes_ahead(prompt)) {
        if entry.verdict!=EntryVerdict::Pass { usage::mark(usage::JevMark::count(format!("entry_plan_followed:{}",entry.verdict.as_str()),1)); }
        entry=entry.inherit(EntryVerdict::Pass);
        state.orchestrator.pending_plan=Some(plan_text);
    }
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
    if entry.verdict==EntryVerdict::Ask {
        // A portaria já sabe o que falta: ela mesma pergunta, com o que
        // faltou e o pedido reescrito à mão, em vez de pagar uma sessão de
        // agente só para ele fazer a pergunta. Enviar como está ou reescrito
        // libera o pedido de vez; completar o texto o julga de novo.
        let question=entry.confirmation();
        pulse.beat(Beat::Chunk{text:question.clone()});
        pulse.beat(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0});
        state.orchestrator.memory.add_message(chat_id,"user",prompt.trim().to_string());
        state.orchestrator.memory.add_message(chat_id,"assistant",i18n::for_model(&question));
        usage::mark(usage::JevMark::count("entry:confirm_asked",1));
        {
            let mut workspace=workspace.lock().await;
            let _=workspace.append_answer(chat_id,&turn.id,&question);
            let _=workspace.ask_question(&turn.id,"single",&question,&gatekeeper::CONFIRM_OPTIONS.map(String::from),gatekeeper::GATE_SOURCE);
            let _=workspace.set_turn_status(&turn.id,TurnStatus::Answered);
        }
        let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
        return;
    }
    state.orchestrator.pending_gate_note=entry.clarifying_note();
    state.orchestrator.pending_gate_passed=Some(entry.verdict==EntryVerdict::Pass);
    // "Enviar como está" é o texto do desenvolvedor, sem a reescrita.
    state.orchestrator.pending_brief=if choice==Some(gatekeeper::GateChoice::AsIs) { None } else { entry.refined_prompt(request) };
    state.orchestrator.pending_routing=routing;

    // A portaria cobrou, no pedido anterior, onde fica ou como conferir, e
    // este pedido respondeu: a resposta vira nota do projeto, e a portaria
    // não cobra de novo.
    let mut notes=notes;
    if let Some(project_id)=project_id.as_ref().filter(|_|notes_on) {
        if learn_from_gate(workspace,project_id,turn,prompt,&entry).await { notes=workspace.lock().await.project_notes(project_id).unwrap_or(notes); }
    }
    state.orchestrator.project_notes=project_memory::notes_prompt(&notes);
    state.orchestrator.pending_context=project_memory::recipe_for(&notes,prompt).map(project_memory::recipe_prompt).into_iter().collect();
    if let Some(project_id)=project_id.as_ref().filter(|_|plan.allows(features::ANSWER_RECALL)) {
        match workspace.lock().await.recall(project_id,chat_id,prompt) {
            Ok(Some(recall))=>{ usage::mark(usage::JevMark::count("answer_recalled",1)); state.orchestrator.pending_context.push(search::recall_prompt(&recall)); }
            Ok(None)=>{}
            Err(error)=>eprintln!("memória: a busca nas conversas falhou ({error:#})"),
        }
    }

    // O modo do chat é lido agora, na vez do pedido: trocar de modo com
    // pedidos na fila vale para eles também.
    state.orchestrator.pending_work_mode=workspace.lock().await.work_mode(chat_id).ok();
    // Permissão liberada para este pedido é pedido de executar: no
    // automático ele vai ao desenvolvimento. O planejamento fixado continua.
    if granted_here&&state.orchestrator.pending_work_mode.as_deref().is_none_or(|mode|mode==crate::orchestrator::MODE_AUTO) {
        state.orchestrator.pending_work_mode=Some(crate::orchestrator::MODE_BUILD.into());
    }
    // A sessão do agente sobrevive ao reinício do app: a guardada volta para a
    // memória antes do pedido, e a de depois dele é guardada de novo. O banco
    // é quem manda: com pedidos em paralelo, o chat pode ter sido atendido da
    // última vez por outro atendente, ou limpo enquanto este atendia outro.
    match workspace.lock().await.agent_session(chat_id) {
        Ok(Some(kept))=>state.orchestrator.memory.keep_agent_session(chat_id,kept),
        Ok(None)=>state.orchestrator.memory.forget_agent_session(chat_id),
        Err(error)=>eprintln!("sessão do agente: não consegui ler a guardada ({error:#})"),
    }
    let before=guard.before.await.unwrap_or_default();
    let result=state.orchestrator.process(request,Some(chat_id),pulse).await;
    let touched={
        let (root,firewall)=(guard.root.clone(),crate::firewall::ContextFirewall::new(state.orchestrator.config.privacy.clone()));
        tokio::task::spawn_blocking(move ||gatekeeper::touched(&before,&gatekeeper::guarded(&root,&firewall))).await.unwrap_or_default()
    };
    {
        let mut workspace=workspace.lock().await;
        let saved=match state.orchestrator.memory.agent_session(chat_id) { Some(kept)=>workspace.keep_agent_session(chat_id,kept), None=>workspace.forget_agent_session(chat_id) };
        if let Err(error)=saved { eprintln!("sessão do agente: não consegui guardar ({error:#})"); }
    }
    // O Jev tirou o chat do planejamento: o chat fica em build até o
    // desenvolvedor desfazer ou escolher outro modo.
    if state.orchestrator.mode_switch.take().is_some() {
        if let Err(error)=workspace.lock().await.set_work_mode(chat_id,crate::orchestrator::MODE_BUILD) {eprintln!("modo: não consegui gravar a troca do Jev ({error:#})");}
    }
    let assistant=result.result.as_ref().map(|response|response.response.clone()).or_else(||result.error.clone()).unwrap_or_else(||i18n::notice(&[Text::new("turn.noAnswer")]));
    // Os comandos negados viram a pergunta do box; as linhas que os pediam
    // saem da resposta que fica no chat.
    let permissions=if result.result.is_some() { asking::permission_requests(&assistant) } else { vec![] };
    // O que a organização bloqueia não se pergunta: o painel não oferece
    // executar, e o turno registra quem bloqueou.
    let (permissions,blocked)=split_blocked_permissions(workspace,chat_id,permissions).await;
    if !blocked.is_empty() {
        let (commands,orgs):(Vec<String>,Vec<String>)=(blocked.iter().map(|(command,_)|command.clone()).collect(),{ let mut orgs:Vec<String>=Vec::new(); for (_,found) in &blocked { for org in found { if !orgs.contains(org) { orgs.push(org.clone()); } } } orgs });
        let _=workspace.lock().await.record_beat(&turn.id,"permission_blocked",&serde_json::json!({"commands":commands,"orgs":orgs}));
    }
    let assistant=asking::without_permission_marks(&assistant);
    if result.result.is_none(){state.orchestrator.memory.add_message(chat_id,"assistant",i18n::for_model(&assistant));}
    // Só a resposta do modelo passa pelo portão de saída; um aviso de falha
    // não pede para rodar nem mexer em nada.
    let mut exits=if result.result.is_some(){exit_checks(&state,turn,&assistant)}else{vec![]};
    // O protegido mexido entra mesmo no pedido que falhou: o agente pode ter
    // gravado antes de cair.
    exits.extend(gatekeeper::guarded_exits(turn,&touched,&state.orchestrator.firewall));
    {
        let mut workspace=workspace.lock().await;
        let _=workspace.append_answer(chat_id,&turn.id,&assistant);
        let _=workspace.record_exit_checks(turn,&exits);
        let _=workspace.set_turn_status(&turn.id,if result.result.is_some(){TurnStatus::Answered}else{TurnStatus::Failed});
    }
    for exit in &exits { usage::mark(usage::JevMark::count(format!("exit:{}",exit.verdict.as_str()),1)); }
    // A portaria de saída segurou o que o modelo devolveu: não resolveu.
    if exits.iter().any(|exit|exit.verdict==ExitVerdict::Held) { state.orchestrator.mark_last_failed(chat_id); }
    if !exits.is_empty(){let _=app.emit(EXIT_EVENT,ExitEvent{checks:exits});}
    let review=state.orchestrator.pending_review.take().filter(|_|result.result.is_some());
    drop(state);
    // A segunda opinião chega depois da resposta, no mesmo turno: o próximo
    // da fila não espera por ela.
    if let Some(review)=review { review_in_background(app.clone(),workspace.clone(),turn.clone(),review); }
    // A ida ao Jev para achar a pergunta não segura o próximo da fila.
    if result.result.is_some() {
        let (app,workspace,turn)=(app.clone(),workspace.clone(),turn.clone());
        tauri::async_runtime::spawn(async move {
            if permissions.is_empty() { enable_question(&app,&workspace,&turn,&assistant).await; } else { ask_permission(&app,&workspace,&turn,&permissions).await; }
        });
    }
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

/// Separa os comandos que o agente pediu entre os que a pessoa pode aprovar e
/// os que a política da organização bloqueia (com quem os bloqueia).
async fn split_blocked_permissions(workspace:&SharedWorkspace,chat_id:&str,commands:Vec<String>)->(Vec<String>,Vec<(String,Vec<String>)>) {
    if commands.is_empty() { return (commands,vec![]); }
    let policy=workspace.lock().await.chat_policy(chat_id).unwrap_or_else(|error|{eprintln!("política de LLM: {error:#}"); None});
    let Some(project)=policy else { return (commands,vec![]) };
    let mut allowed=Vec::new();
    let mut blocked=Vec::new();
    for command in commands {
        if project.policy.blocks_command(&command) { let orgs=project.policy.blocking_orgs(&command,&project.org_slug); blocked.push((command,orgs)); } else { allowed.push(command); }
    }
    (allowed,blocked)
}

/// O agente esbarrou numa permissão: a pergunta não passa pelo Jev — a linha
/// de permissão já diz o que é —, e o box oferece executar, negar ou sempre
/// permitir o que foi negado.
async fn ask_permission(app:&AppHandle,workspace:&SharedWorkspace,turn:&Turn,commands:&[String]) {
    let question=asking::permission_question(commands);
    let recorded=workspace.lock().await.ask_question(&turn.id,question.kind.as_str(),&question.prompt,&question.options,&question.source);
    match recorded {
        Ok(())=>{let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});}
        Err(error)=>eprintln!("permissão: não consegui perguntar no turno `{}` ({error})",turn.id),
    }
}

/// O título definitivo depende de outra ida ao modelo, e a fila não pode
/// esperar por ela: o chat já entrou na lista com o resumo local do pedido, e o
/// próximo da fila tem direito ao orquestrador antes de qualquer enfeite. O
/// batismo só pega o cadeado para montar o pedido; a ida ao modelo roda solta,
/// e a interface é avisada no fim.
fn name_in_background(app:AppHandle,desk:SharedDesktopState,workspace:SharedWorkspace,chat_id:String,prompt:String,reading:String) {
    // O batismo roda noutro task: o escopo do turno vai junto, à mão.
    let scope=usage::current_scope();
    tauri::async_runtime::spawn(usage::within(scope,async move {
        let request=desk.lock().await.orchestrator.title_request(&prompt,&reading);
        let Some(title)=(match request { Some(request)=>request.run().await, None=>None }) else {return};
        if workspace.lock().await.rename_chat(&chat_id,&title).is_ok() {let _=app.emit(RENAME_EVENT,RenameEvent{chat_id,title});}
    }));
}

/// Roda a revisão fora do cadeado e a põe no chat como mensagem do turno que
/// ela revisou. O pedido seguinte do chat a lê no histórico.
fn review_in_background(app:AppHandle,workspace:SharedWorkspace,turn:Turn,review:crate::orchestrator::ReviewRequest) {
    let scope=usage::current_scope();
    tauri::async_runtime::spawn(usage::within(scope,async move {
        let Some(section)=review.run().await else { return };
        let mut workspace=workspace.lock().await;
        if workspace.append_answer(&turn.chat_id,&turn.id,section.trim()).is_ok() {
            // A narração só dizia que a revisão foi pedida; agora ela chegou.
            if let Err(error)=workspace.record_beat(&turn.id,crate::turns::REVIEWED_EVENT,&serde_json::json!({})) { eprintln!("revisão: não consegui marcar a chegada no turno `{}` ({error:#})",turn.id); }
            let _=app.emit(TURN_EVENT,TurnEvent{chat_id:turn.chat_id.clone(),turn_id:turn.id.clone()});
        }
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
    state.orchestrator.focus(&root).await.map_err(i18n::failure)
}

/// As configurações de quem usa, passadas pelo plano e pela política de LLM
/// do projeto do chat. Lidas a cada pedido: a troca na tela, a política ou o
/// plano que a sincronização acabou de trazer e o chat de outro projeto valem
/// já para este. A ordem é a que só aperta: o plano tira o que não tem, a
/// política aperta por cima, e o que o plano trava liga por último — nada
/// abaixo dele afrouxa o núcleo. Devolve o plano, que o atendimento consulta.
async fn apply_project_policy(state:&mut DesktopState,workspace:&SharedWorkspace,chat_id:&str,grants:&crate::llm::Grants)->features::Entitlements {
    let defaults=state.orchestrator.core_defaults();
    let (llm,core,policy,plan,servers,skills)={
        let workspace=workspace.lock().await;
        let plan=workspace.entitlements().unwrap_or_default();
        let servers=workspace.mcp_servers().unwrap_or_else(|error|{eprintln!("mcp: servidores ilegíveis ({error:#})"); vec![]});
        let skills=workspace.skills().unwrap_or_else(|error|{eprintln!("skills: ilegíveis ({error:#})"); vec![]});
        // O que a organização do projeto dá vale por cima do que a pessoa tem.
        let (org_servers,org_skills)=workspace.chat_org_extensions(chat_id).unwrap_or_else(|error|{eprintln!("MCP e skills da organização: ilegíveis ({error:#})"); (vec![],vec![])});
        let servers=jayv_agents::org_extensions::merge_servers(servers,org_servers);
        let skills=jayv_agents::org_extensions::merge_skills(skills,org_skills);
        (workspace.llm_settings(),workspace.core_settings(&plan.seed_core(&defaults)).map(|core|plan.restrict_core(&core)),workspace.chat_policy(chat_id),plan,servers,skills)
    };
    let (Ok(llm),Ok(core))=(llm,core) else { eprintln!("política de LLM: configurações ilegíveis, mantidas as anteriores"); return plan };
    // O login dos agentes, renovado em segundo plano: o pedido não espera, e
    // o agente sem login sai do roteamento dos próximos.
    crate::llm::refresh_logins(&llm);
    crate::llm::warm_up(&llm);
    let policy=policy.unwrap_or_else(|error|{eprintln!("política de LLM: {error:#}"); None});
    // O liberado para o pedido entra antes da política: ela ainda aperta por
    // cima, e o plano por último.
    // Os comandos que a organização bloqueia nunca chegam ao agente: saem do
    // liberado para o pedido e o agente fica sem poder rodá-los.
    let (grants,blocked)=match &policy {
        Some(project)=>(project.policy.restrict_grants(grants),project.policy.blocked_commands.clone()),
        None=>(grants.clone(),vec![]),
    };
    let llm=llm.with_grants(&grants).with_mcp(&servers);
    let (llm,core)=match &policy {
        Some(project)=>(project.policy.restrict_llm(&llm),project.policy.restrict_core(&core)),
        None=>(llm,core),
    };
    // A privacidade vem antes dos agentes: o Claude leva os arquivos
    // protegidos na própria linha de comando.
    state.orchestrator.use_core(&plan.enforce_core(&core));
    state.orchestrator.use_llm(&plan.apply_llm(&llm));
    state.orchestrator.use_skills(skills);
    state.orchestrator.blocked_commands=blocked;
    state.orchestrator.policy_scope=policy.map(|project|project.org_slug);
    plan
}

/// Os chats apagados ou limpos enquanto o orquestrador atendia outro pedido.
/// A memória da conversa sai, e a sessão do agente que aquele pedido possa ter
/// guardado no banco depois da limpeza também: o chat limpo é conversa nova.
async fn forget_pending(state:&mut DesktopState,workspace:&SharedWorkspace,forget:&SharedForget) {
    let chat_ids:Vec<String>=forget.lock().unwrap_or_else(std::sync::PoisonError::into_inner).drain().collect();
    if chat_ids.is_empty() { return; }
    let mut workspace=workspace.lock().await;
    for chat_id in chat_ids {
        state.orchestrator.memory.clear_session(&chat_id);
        let _=workspace.forget_agent_session(&chat_id);
    }
}

/// O pedido está gravado desde o envio, e o `process` torna a anotá-lo na
/// memória da sessão: sem isto o modelo receberia a mesma linha duas vezes no
/// histórico.
async fn forget_pending_prompt(state:&mut DesktopState,workspace:&SharedWorkspace,chat_id:&str)->anyhow::Result<()> {
    let history=workspace.lock().await.history_before_open_turns(chat_id)?;
    state.orchestrator.memory.set_conversation(chat_id.to_string(),history);
    Ok(())
}

/// Como o Jev entendeu o pedido, em uma linha: é isso que vai junto do prompt
/// quando o modelo escolhe o título.
fn jev_reading(result:&model::ProcessResult)->String{format!("{} task, {} complexity, routed to {}",result.intent_analysis.intent,result.complexity,result.model_selection.model_name)}

/// Pontua o pedido no Jev quando há credencial e nas heurísticas locais quando
/// não há — ou quando a chamada falha, para que o portão nunca trave o envio.
async fn entry_check(project:&model::ProjectInfo,turn:&Turn,input:&str,level:crate::expertise::Expertise,covered:&[String],recent:&[String])->EntryCheck {
    if jev::reachable() {
        match gatekeeper::evaluate_entry(input,&project.name,&project.languages,recent).await {
            Ok(reading)=>return gatekeeper::judge_for(turn,input,&gatekeeper::with_notes(reading,covered),"jev",level),
            Err(error)=>eprintln!("portaria: o Jev não respondeu, usando heurísticas locais ({error})"),
        }
    }
    gatekeeper::judge_for(turn,input,&gatekeeper::with_notes(gatekeeper::heuristic_entry(input),covered),asking::LOCAL_SOURCE,level)
}

/// Aprende com a portaria: o critério que segurou o pedido anterior do chat
/// e que este pedido atendeu — com a frase que o atende — vira nota do
/// projeto. Devolve se aprendeu alguma coisa.
async fn learn_from_gate(workspace:&SharedWorkspace,project_id:&str,turn:&Turn,prompt:&str,entry:&EntryCheck)->bool {
    let mut workspace=workspace.lock().await;
    let failing=workspace.previous_failing_criteria(&turn.id).unwrap_or_default();
    let met=|id:&str|entry.criteria.iter().any(|criterion|criterion.id==id&&!entry.failing().iter().any(|failing|failing.id==id));
    let mut learned=false;
    for criterion in gatekeeper::LEARNABLE_CRITERIA.iter().filter(|id|failing.iter().any(|failing|failing==*id)&&met(id)) {
        let Some(sentence)=gatekeeper::evidence(prompt,criterion) else { continue };
        match workspace.learn_project_note(project_id,criterion,&sentence) {
            Ok(Some(_))=>{ usage::mark(usage::JevMark::count("note_learned",1)); learned=true; }
            Ok(None)=>{}
            Err(error)=>eprintln!("memória: não consegui guardar a nota aprendida ({error:#})"),
        }
    }
    learned
}

/// A foto dos protegidos tirada antes do pedido, correndo fora do atendente.
struct Guard { root:std::path::PathBuf, before:tokio::task::JoinHandle<gatekeeper::Guarded> }

fn gatekeeper_guard(state:&DesktopState)->Guard {
    let root=std::path::PathBuf::from(state.orchestrator.rag.project_info().root);
    let firewall=crate::firewall::ContextFirewall::new(state.orchestrator.config.privacy.clone());
    let before=tokio::task::spawn_blocking({ let root=root.clone(); move ||gatekeeper::guarded(&root,&firewall) });
    Guard{root,before}
}

fn exit_checks(state:&DesktopState,turn:&Turn,answer:&str)->Vec<ExitCheck> {
    let root=state.orchestrator.rag.project_info().root;
    gatekeeper::scan_answer(turn,answer,&state.orchestrator.config,&state.orchestrator.firewall,Path::new(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O projeto volta ao atendente que já leu a pasta dele, se estiver livre.
    #[test] fn a_project_goes_back_to_the_lane_that_knows_it() {
        assert_eq!(pick_lane(&[],None),0,"o primeiro pedido abre o primeiro atendente");
        assert_eq!(pick_lane(&[true,false,false],Some(2)),2,"o que já conhece o projeto");
        assert_eq!(pick_lane(&[true,false,false],Some(0)),1,"ocupado: o primeiro livre");
        assert_eq!(pick_lane(&[true,true],Some(1)),2,"todos ocupados: um novo");
    }

    /// Com sessão, a fila anda com rede ou sem; sem sessão, espera.
    #[test] fn the_queue_runs_with_a_session_even_offline() {
        assert!(serves(Link::Online)&&serves(Link::Offline));
        assert!(!serves(Link::SignedOut)&&!serves(Link::Expired));
    }

    /// O teto para o pedido com o motivo dele; o relógio largado não para nada.
    #[tokio::test(start_paused=true)] async fn the_ceiling_stops_the_request_and_a_dropped_clock_does_not() {
        let stop=Stop::default();
        let clock=Ceiling::start(stop.clone(),1);
        tokio::time::sleep(std::time::Duration::from_secs(61)).await;
        assert_eq!(stop.reason(),Some(StopReason::Ceiling{minutes:1}));
        drop(clock);
        let quiet=Stop::default();
        drop(Ceiling::start(quiet.clone(),1));
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        assert_eq!(quiet.reason(),None);
    }
}
