pub mod agents;
pub mod asking;
pub mod cache;
pub mod checkpoint;
pub mod config;
pub mod context_engine;
pub mod firewall;
pub mod gatekeeper;
pub mod graph;
pub mod jev;
pub mod memory;
pub mod model;
pub mod orchestrator;
pub mod progress;
pub mod providers;
pub mod rag;
pub mod router;
pub mod sandbox;
pub mod tools;
pub mod turns;
pub mod workspace;

use config::{ModelConfig, ProviderConfig};
use gatekeeper::{EntryCheck, EntryVerdict, ExitCheck, GateFeed};

use orchestrator::Orchestrator;
use progress::{Beat, Debounce, Pulse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::{Path, PathBuf}, sync::Arc, time::Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, Mutex, Notify};
use turns::{Turn, TurnStatus};
use workspace::{ChatRecord, ProjectRecord, WorkspaceData, WorkspaceStore};

const ENTRY_EVENT:&str="gate-entry";
const EXIT_EVENT:&str="gate-exit";
const RENAME_EVENT:&str="chat-renamed";
const PROMPT_EVENT:&str="chat-prompt";
const TURN_EVENT:&str="turn-settled";
/// Uma etapa do pedido, gravada e desenhada.
const BEAT_EVENT:&str="turn-beat";
/// Um pedaço da resposta. Vai para a tela a cada chegada e para o disco com
/// folga: são dois ritmos diferentes de propósito.
const CHUNK_EVENT:&str="turn-chunk";

pub struct DesktopState {
    orchestrator: Orchestrator,
    /// A raiz com que o aplicativo subiu: vale só para chat de projeto sem
    /// pasta escolhida.
    home_root: PathBuf,
}

pub type SharedDesktopState=Arc<Mutex<DesktopState>>;

/// O banco vive atrás do seu próprio cadeado, separado do orquestrador. É esta
/// separação que faz a promessa da fila valer: aceitar um pedido é escrever uma
/// linha, e escrever essa linha não pode esperar o modelo que está respondendo
/// o pedido anterior. Enquanto os dois dividiam um cadeado só, o segundo envio
/// ficava parado na porta — sem chegar ao disco — e o primeiro, ao terminar,
/// redesenhava a conversa a partir do banco e apagava da tela o que o
/// desenvolvedor tinha acabado de escrever.
pub type SharedWorkspace=Arc<Mutex<WorkspaceStore>>;

/// Toca quando entra pedido novo. O atendente dorme nele em vez de ficar
/// perguntando ao banco se chegou alguma coisa.
pub type QueueBell=Arc<Notify>;

/// Quem precisa dos dois cadeados pega sempre nesta ordem — orquestrador,
/// depois banco. O atendente segura o orquestrador do começo ao fim do pedido e
/// encosta no banco em trechos curtos; inverter a ordem em qualquer comando
/// travaria os dois.
async fn both<'a>(desk:&'a SharedDesktopState,workspace:&'a SharedWorkspace)->(tokio::sync::MutexGuard<'a,DesktopState>,tokio::sync::MutexGuard<'a,WorkspaceStore>) {
    let desk=desk.lock().await;
    let workspace=workspace.lock().await;
    (desk,workspace)
}

#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProcessRequest {
    pub input:String,
    pub session_id:Option<String>,
    /// Preenchido só no reenvio: é o turno que falhou voltando ao ar com o
    /// mesmo código, em vez de um pedido novo com um código novo.
    pub turn_id:Option<String>,
}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct EntryEvent{check:EntryCheck}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct ExitEvent{checks:Vec<ExitCheck>}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct RenameEvent{chat_id:String,title:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct PromptEvent{chat_id:String}

/// A fila andou: um pedido entrou no ar ou acabou de se fechar.
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct TurnEvent{chat_id:String,turn_id:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct BeatEvent{chat_id:String,turn_id:String,seq:u32,kind:String,detail:Value}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct ChunkEvent{chat_id:String,turn_id:String,text:String}

#[derive(Debug,Serialize)]
pub struct SystemStatus { pub version:&'static str,pub config_path:String,pub database_path:String,pub database_name:String,pub tables:Vec<workspace::TableCount>,pub providers:usize,pub models:usize,pub indexed_files:usize,pub cache_entries:usize,pub session_messages:usize,pub performance_records:usize }

#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ProviderSettings {
    pub name:String,
    pub enabled:bool,
    pub kind:String,
    pub has_api_key:bool,
    pub base_url:String,
    pub command:String,
    pub timeout:u64,
    pub args:Vec<String>,
}

#[derive(Debug,Clone,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProviderInput {
    pub name:String,
    #[serde(default)] pub original_name:Option<String>,
    pub enabled:bool,
    pub kind:String,
    #[serde(default)] pub api_key:String,
    #[serde(default)] pub clear_api_key:bool,
    #[serde(default)] pub base_url:String,
    #[serde(default)] pub command:String,
    #[serde(default="default_provider_timeout")] pub timeout:u64,
    #[serde(default)] pub args:Vec<String>,
}

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ModelSettings {
    pub name:String,
    pub enabled:bool,
    pub provider:String,
    pub model:String,
    pub capabilities:Vec<String>,
    pub cost_class:String,
    pub speed:String,
    pub context_window:usize,
}

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SettingsSnapshot {
    pub config_path:String,
    pub providers:Vec<ProviderSettings>,
    pub models:Vec<ModelSettings>,
}

#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SaveSettingsInput { pub providers:Vec<ProviderInput>,pub models:Vec<ModelSettings> }

/// Aceita o pedido e devolve o turno. Só isso — e é de propósito: esta chamada
/// escreve o que o desenvolvedor mandou, numera o pedido e volta na hora,
/// sem tocar em modelo nenhum. A partir do instante em que ela retorna, a
/// mensagem existe em disco e a tela a lê de lá, como lê qualquer mensagem
/// antiga; nada do que acontecer depois pode fazê-la sumir. Mandar outra coisa
/// por cima não atropela nada: o segundo pedido entra na fila atrás do
/// primeiro, com o seu próprio número, e espera a vez.
#[tauri::command]
async fn enqueue_prompt(app:AppHandle,workspace:State<'_,SharedWorkspace>,bell:State<'_,QueueBell>,request:ProcessRequest)->Result<Turn,String>{
    let chat_id=request.session_id.as_deref().ok_or_else(||"selecione um chat antes de enviar".to_string())?;
    let input=request.input.trim();
    if input.is_empty(){return Err("não há o que enviar".into());}
    let turn={
        let mut workspace=workspace.lock().await;
        if !workspace.contains_chat(chat_id).map_err(|error|error.to_string())?{return Err("chat não encontrado".into());}
        workspace.enqueue_prompt(chat_id,input,request.turn_id.as_deref()).map_err(|error|error.to_string())?
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
async fn answer_question(app:AppHandle,workspace:State<'_,SharedWorkspace>,bell:State<'_,QueueBell>,answer:AnswerInput)->Result<Turn,String>{
    let mut store=workspace.lock().await;
    let question=store.question_of(&answer.question_turn_id).map_err(|error|error.to_string())?.ok_or_else(||"essa pergunta não existe mais".to_string())?;
    if question.status!=turns::QUESTION_PENDING {return Err("essa pergunta já foi encerrada".into());}
    let kind=asking::Shape::parse(&question.kind).map_err(|error|error.to_string())?;
    let composed=asking::compose(&question.prompt,kind,&question.options,&answer.picked,answer.text.as_deref()).map_err(|error|error.to_string())?;
    let chat_id=store.chat_of_turn(&question.turn_id).map_err(|error|error.to_string())?.ok_or_else(||"o pedido dessa pergunta não existe mais".to_string())?;
    let turn=store.enqueue_prompt(&chat_id,&composed,None).map_err(|error|error.to_string())?;
    // O vínculo é o que faz a Portaria julgar a resposta em par com a pergunta.
    if !store.settle_question(&question.turn_id,turns::QUESTION_ANSWERED,Some(&turn.id)).map_err(|error|error.to_string())? {
        eprintln!("pergunta: `{}` foi encerrada por outro caminho enquanto era respondida",question.turn_id);
    }
    drop(store);
    let _=app.emit(PROMPT_EVENT,PromptEvent{chat_id});
    bell.notify_one();
    Ok(turn)
}

/// Descarta a pergunta e devolve o box ao desenvolvedor. A decisão fica
/// registrada: a linha em `questions` guarda que foi ignorada e quando, e a
/// narração do turno ganha o evento. Ignorar é uma escolha, e escolha não some.
#[tauri::command]
async fn dismiss_question(app:AppHandle,workspace:State<'_,SharedWorkspace>,question_turn_id:String)->Result<(),String>{
    let mut store=workspace.lock().await;
    let question=store.question_of(&question_turn_id).map_err(|error|error.to_string())?.ok_or_else(||"essa pergunta não existe mais".to_string())?;
    if !store.settle_question(&question_turn_id,turns::QUESTION_DISMISSED,None).map_err(|error|error.to_string())? {
        return Err("essa pergunta já foi encerrada".into());
    }
    let _=store.record_beat(&question_turn_id,"dismissed",&serde_json::json!({"prompt":question.prompt}));
    let chat_id=store.chat_of_turn(&question_turn_id).map_err(|error|error.to_string())?.unwrap_or_default();
    drop(store);
    let _=app.emit(TURN_EVENT,TurnEvent{chat_id,turn_id:question_turn_id});
    Ok(())
}

/// O atendente da fila: um pedido de cada vez, na ordem em que chegaram, até
/// não sobrar nenhum — e então volta a dormir no sino. Ele existe uma vez só no
/// aplicativo, e é por isso que dois envios seguidos nunca disputam o
/// orquestrador: o segundo não é uma chamada esperando na porta, é uma linha no
/// banco esperando a vez.
async fn serve_the_queue(app:AppHandle,desk:SharedDesktopState,workspace:SharedWorkspace,bell:QueueBell) {
    loop {
        loop {
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
    attend(app,desk,workspace,turn,prompt,&pulse).await;
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
        let (kind,detail,settles)=(beat.kind().to_string(),beat.detail(),beat.settles());
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
    let mut state=desk.lock().await;
    let unnamed=workspace.lock().await.chat_is_unnamed(chat_id).unwrap_or(false);

    // O pedido é lido dentro da pasta do projeto. Se ela sumiu do disco, o
    // atendimento morre aqui — mas com a mensagem já escrita, o turno dado por
    // falho e o motivo no chat, em vez de sumir da conversa.
    if let Err(error)=focus_on_chat_project(&mut state,workspace,chat_id).await {
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
    let paired=origin.map(|origin|asking::pair(&origin,prompt));
    let request=paired.as_deref().unwrap_or(prompt);
    let entry=entry_check(&project,turn,request).await;
    {
        let mut workspace=workspace.lock().await;
        let _=workspace.record_entry_check(&entry);
    }
    let _=app.emit(ENTRY_EVENT,EntryEvent{check:entry.clone()});
    pulse.beat(Beat::Gate{verdict:entry.verdict.as_str().into(),score:entry.score,demand:entry.demand});
    if entry.verdict==EntryVerdict::Block {
        let reply=entry.reply();
        // O barrado também é resposta, e a tela mostra o motivo crescendo como
        // mostraria qualquer outra.
        pulse.beat(Beat::Chunk{text:reply.clone()});
        pulse.beat(Beat::Done{input_tokens:0,output_tokens:0,latency_ms:0});
        state.orchestrator.memory.add_message(chat_id,"user",prompt.trim().to_string());
        state.orchestrator.memory.add_message(chat_id,"assistant",reply.clone());
        let mut workspace=workspace.lock().await;
        let _=workspace.append_answer(chat_id,&turn.id,&reply);
        let _=workspace.set_turn_status(&turn.id,TurnStatus::Blocked);
        return;
    }
    state.orchestrator.pending_gate_note=entry.clarifying_note();

    // O `process` torna a anotar o pedido na memória da sessão, e ele já está
    // no banco desde o envio: sem esta poda o modelo receberia a mesma linha
    // duas vezes no histórico.
    if let Err(error)=forget_pending_prompt(&mut state,workspace,chat_id).await {eprintln!("fila: histórico da sessão desalinhado ({error})");}

    let result=state.orchestrator.process(request,Some(chat_id),pulse).await;
    let assistant=result.result.as_ref().map(|response|response.response.clone()).or_else(||result.error.clone()).unwrap_or_else(||"A execução terminou sem resposta.".into());
    if result.result.is_none(){state.orchestrator.memory.add_message(chat_id,"assistant",assistant.clone());}
    let exits=exit_checks(&state,turn,&assistant);
    {
        let mut workspace=workspace.lock().await;
        let _=workspace.append_answer(chat_id,&turn.id,&assistant);
        let _=workspace.record_exit_checks(turn,&exits);
        let _=workspace.set_turn_status(&turn.id,if result.result.is_some(){TurnStatus::Answered}else{TurnStatus::Failed});
    }
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
    tauri::async_runtime::spawn(async move {
        let state=desk.lock().await;
        let Some(title)=state.orchestrator.name_chat(&prompt,&reading).await else {return};
        drop(state);
        if workspace.lock().await.rename_chat(&chat_id,&title).is_ok() {let _=app.emit(RENAME_EVENT,RenameEvent{chat_id,title});}
    });
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
async fn focus_on_chat_project(state:&mut DesktopState,workspace:&SharedWorkspace,chat_id:&str)->Result<(),String> {
    let root=workspace.lock().await.chat_root(chat_id).map_err(|error|error.to_string())?.unwrap_or_else(||state.home_root.clone());
    state.orchestrator.focus_on(&root).map_err(|error|error.to_string())
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

/// Sem projeto, a portaria mostra a sessão inteira; com projeto, só os chats
/// dele.
#[tauri::command]
async fn gate_feed(workspace:State<'_,SharedWorkspace>,project_id:Option<String>)->Result<GateFeed,String>{
    let workspace=workspace.lock().await;
    let Some(project_id)=project_id else {return workspace.gate_feed(None).map_err(|error|error.to_string());};
    let chats=workspace.chat_ids_for_project(&project_id).map_err(|error|error.to_string())?.into_iter().collect();
    workspace.gate_feed(Some(&chats)).map_err(|error|error.to_string())
}

/// Pontua o pedido no Jev quando há credencial e nas heurísticas locais quando
/// não há — ou quando a chamada falha, para que o portão nunca trave o envio.
async fn entry_check(project:&model::ProjectInfo,turn:&Turn,input:&str)->EntryCheck {
    if jev::is_configured() {
        match gatekeeper::evaluate_entry(input,&project.name,&project.languages).await {
            Ok(reading)=>return gatekeeper::judge(turn,input,&reading,"jev"),
            Err(error)=>eprintln!("portaria: o Jev não respondeu, usando heurísticas locais ({error})"),
        }
    }
    gatekeeper::judge(turn,input,&gatekeeper::heuristic_entry(input),"heurística local")
}

fn exit_checks(state:&DesktopState,turn:&Turn,answer:&str)->Vec<ExitCheck> {
    let root=state.orchestrator.rag.project_info().root;
    gatekeeper::scan_answer(turn,answer,&state.orchestrator.config,&state.orchestrator.firewall,Path::new(&root))
}

#[tauri::command]
async fn system_status(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>)->Result<SystemStatus,String>{
    let (state,workspace)=both(&state,&workspace).await;
    let orchestrator=&state.orchestrator;
    Ok(SystemStatus{version:env!("CARGO_PKG_VERSION"),config_path:orchestrator.config_path.display().to_string(),database_path:workspace.database_path().display().to_string(),database_name:workspace.database_name(),tables:workspace.table_counts().unwrap_or_default(),providers:orchestrator.executable_provider_count(),models:orchestrator.executable_model_count(),indexed_files:orchestrator.rag.len(),cache_entries:orchestrator.cache.len(),session_messages:orchestrator.memory.session_messages(),performance_records:orchestrator.performance.len()})
}

#[tauri::command]
async fn get_workspace(workspace:State<'_,SharedWorkspace>)->Result<WorkspaceData,String>{workspace.lock().await.snapshot().map_err(|error|error.to_string())}

#[tauri::command]
async fn create_project(workspace:State<'_,SharedWorkspace>,name:String,root_path:Option<String>)->Result<ProjectRecord,String>{workspace.lock().await.create_project(&name,root_path).map_err(|error|error.to_string())}

#[tauri::command]
async fn create_chat(workspace:State<'_,SharedWorkspace>,project_id:String,title:Option<String>)->Result<ChatRecord,String>{workspace.lock().await.create_chat(&project_id,title).map_err(|error|error.to_string())}

#[tauri::command]
async fn clear_chat(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),String>{
    let (mut state,mut workspace)=both(&state,&workspace).await;
    workspace.clear_chat(&chat_id).map_err(|error|error.to_string())?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
async fn delete_chat(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,chat_id:String)->Result<(),String>{
    let (mut state,mut workspace)=both(&state,&workspace).await;
    workspace.delete_chat(&chat_id).map_err(|error|error.to_string())?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
async fn delete_project(state:State<'_,SharedDesktopState>,workspace:State<'_,SharedWorkspace>,project_id:String)->Result<(),String>{
    let (mut state,mut workspace)=both(&state,&workspace).await;
    let chat_ids=workspace.delete_project(&project_id).map_err(|error|error.to_string())?;
    for chat_id in chat_ids {state.orchestrator.memory.clear_session(&chat_id);}
    Ok(())
}

#[tauri::command]
async fn get_settings(state:State<'_,SharedDesktopState>)->Result<SettingsSnapshot,String>{
    let state=state.lock().await;
    Ok(settings_snapshot(&state.orchestrator))
}

#[tauri::command]
async fn save_settings(state:State<'_,SharedDesktopState>,settings:SaveSettingsInput)->Result<SettingsSnapshot,String>{
    let mut state=state.lock().await;
    let current=state.orchestrator.config.clone();
    let mut providers=HashMap::new();
    for input in settings.providers {
        let name=input.name.trim().to_string();
        if name.is_empty(){return Err("todo provedor precisa de um nome".into());}
        let existing=input.original_name.as_deref().and_then(|original|current.providers.get(original)).or_else(||current.providers.get(&name));
        let retained=existing.and_then(|provider|provider.api_key.clone());
        let api_key=if input.clear_api_key{None}else if input.api_key.trim().is_empty(){retained}else{Some(input.api_key.trim().to_string())};
        if providers.insert(name.clone(),ProviderConfig{enabled:input.enabled,kind:input.kind,api_key,base_url:optional(input.base_url),command:optional(input.command),timeout:input.timeout,args:input.args,local:existing.and_then(|provider|provider.local)}).is_some(){return Err(format!("há mais de um provedor chamado `{name}`"));}
    }
    let mut models=HashMap::new();
    for input in settings.models {
        let name=input.name.trim().to_string();
        if name.is_empty(){return Err("todo modelo precisa de um nome".into());}
        if !providers.contains_key(input.provider.trim()){return Err(format!("o modelo `{name}` referencia um provedor inexistente"));}
        if models.insert(name.clone(),ModelConfig{enabled:input.enabled,provider:input.provider.trim().into(),model:input.model.trim().into(),capabilities:input.capabilities,cost_class:input.cost_class,speed:input.speed,context_window:input.context_window}).is_some(){return Err(format!("há mais de um modelo chamado `{name}`"));}
    }
    state.orchestrator.config.providers=providers;
    state.orchestrator.config.models=models;
    state.orchestrator.config.save(&state.orchestrator.config_path).map_err(|error|error.to_string())?;
    state.orchestrator.reload().map_err(|error|error.to_string())?;
    Ok(settings_snapshot(&state.orchestrator))
}

#[tauri::command]
async fn discover_provider_models(state:State<'_,SharedDesktopState>,provider:ProviderInput)->Result<Vec<String>,String>{
    let config={
        let state=state.lock().await;
        let existing=provider.original_name.as_deref().and_then(|original|state.orchestrator.config.providers.get(original)).or_else(||state.orchestrator.config.providers.get(provider.name.trim()));
        let api_key=if provider.clear_api_key{None}else if provider.api_key.trim().is_empty(){existing.and_then(|item|item.api_key.clone())}else{Some(provider.api_key.trim().into())};
        ProviderConfig{enabled:true,kind:provider.kind.clone(),api_key,base_url:optional(provider.base_url.clone()),command:optional(provider.command.clone()),timeout:provider.timeout,args:provider.args.clone(),local:existing.and_then(|item|item.local)}
    };
    providers::discover_models(provider.name.trim(),&config).await.map_err(|error|error.to_string())
}

fn settings_snapshot(orchestrator:&Orchestrator)->SettingsSnapshot {
    let mut providers=orchestrator.config.providers.iter().map(|(name,provider)|ProviderSettings{name:name.clone(),enabled:provider.enabled,kind:provider.kind.clone(),has_api_key:provider.api_key.as_deref().is_some_and(|key|!key.trim().is_empty()),base_url:provider.base_url.clone().unwrap_or_default(),command:provider.command.clone().unwrap_or_default(),timeout:provider.timeout,args:provider.args.clone()}).collect::<Vec<_>>();
    providers.sort_by(|a,b|a.name.cmp(&b.name));
    let mut models=orchestrator.config.models.iter().map(|(name,model)|ModelSettings{name:name.clone(),enabled:model.enabled,provider:model.provider.clone(),model:model.model.clone(),capabilities:model.capabilities.clone(),cost_class:model.cost_class.clone(),speed:model.speed.clone(),context_window:model.context_window}).collect::<Vec<_>>();
    models.sort_by(|a,b|a.name.cmp(&b.name));
    SettingsSnapshot{config_path:orchestrator.config_path.display().to_string(),providers,models}
}

fn optional(value:String)->Option<String>{let value=value.trim().to_string();if value.is_empty(){None}else{Some(value)}}
fn default_provider_timeout()->u64{30}

pub fn run_desktop(config_path:PathBuf,root:PathBuf)->anyhow::Result<()> {
    let mut orchestrator=Orchestrator::new(config_path.clone(),root.clone())?;
    let workspace_directory=config_path.parent().unwrap_or(&root).join(".jev");
    let legacy_workspace_path=workspace_directory.join("workspace.json");
    let workspace=WorkspaceStore::open(workspace_directory.join("workspace.sqlite3"),Some(&legacy_workspace_path))?;
    let workspace_data=workspace.snapshot()?;
    for chat in &workspace_data.chats {
        orchestrator.memory.set_conversation(chat.id.clone(),workspace.conversation(&chat.id)?);
    }

    let desk:SharedDesktopState=Arc::new(Mutex::new(DesktopState{orchestrator,home_root:root}));
    let workspace:SharedWorkspace=Arc::new(Mutex::new(workspace));
    let bell:QueueBell=Arc::new(Notify::new());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(desk).manage(workspace).manage(bell)
        .setup(|app|{
            // O sino toca uma vez na partida: a abertura do banco devolveu à
            // fila o que o fechamento anterior pegou pela metade, e esses
            // pedidos têm de ser retomados sem esperar por um envio novo.
            let (handle,desk,workspace,bell)=(app.handle().clone(),app.state::<SharedDesktopState>().inner().clone(),app.state::<SharedWorkspace>().inner().clone(),app.state::<QueueBell>().inner().clone());
            bell.notify_one();
            tauri::async_runtime::spawn(serve_the_queue(handle,desk,workspace,bell));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![enqueue_prompt,answer_question,dismiss_question,system_status,get_workspace,create_project,create_chat,clear_chat,delete_chat,delete_project,get_settings,save_settings,discover_provider_models,gate_feed])
        .run(tauri::generate_context!()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    /// O seletor de pastas já ficou mudo uma vez: no Linux o backend gtk3 do rfd
    /// abre a janela pela thread GTK dele, que disputa o loop de eventos do
    /// Tauri, e a chamada volta vazia sem nada aparecer. Quem voltar a declarar
    /// o plugin numa linha só reativa o padrão gtk3 e traz o bug de volta.
    /// Aceitar um pedido não pode depender do orquestrador. Enquanto os dois
    /// dividiam um cadeado, o segundo envio ficava parado na porta até o modelo
    /// devolver o primeiro — e sumia da tela no redesenho. Quem voltar a pedir
    /// o estado do orquestrador aqui traz o bug de volta inteiro.
    #[test] fn aceitar_um_pedido_nao_espera_pelo_orquestrador() {
        let source=include_str!("lib.rs");
        let command=source.split("async fn enqueue_prompt").nth(1).expect("falta o comando de envio");
        let signature=command.split(')').next().expect("assinatura");
        assert!(!signature.contains("SharedDesktopState"),"o envio voltou a depender do cadeado do modelo: {signature}");
        assert!(signature.contains("SharedWorkspace"),"o envio precisa do banco, e só dele: {signature}");
    }

    #[test] fn the_folder_picker_talks_to_the_xdg_portal_on_linux() {
        let manifest=include_str!("../Cargo.toml");
        let linux=manifest.split("[target.'cfg(any(target_os = \"linux\"").nth(1).expect("falta o bloco de dependências do Linux");
        let declaration=linux.lines().find(|line|line.starts_with("tauri-plugin-dialog")).expect("o Linux precisa declarar o plugin de diálogo");
        assert!(declaration.contains("default-features = false"),"o padrão gtk3 continua ligado: {declaration}");
        assert!(declaration.contains("xdg-portal"),"o Linux precisa do backend do portal XDG: {declaration}");
    }
}
