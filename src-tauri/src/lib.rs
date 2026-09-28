pub mod agents;
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
pub mod providers;
pub mod rag;
pub mod router;
pub mod sandbox;
pub mod tools;
pub mod workspace;

use config::{ModelConfig, ProviderConfig};
use gatekeeper::{EntryCheck, EntryVerdict, ExitCheck, GateFeed, GateLog, Tally};
use model::{Context, Decision, IntentAnalysis, ModelSelection, ProcessResult, ProviderResponse, RoutingSignals};
use orchestrator::Orchestrator;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::{Path, PathBuf}, sync::Arc};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use workspace::{ChatRecord, ProjectRecord, WorkspaceData, WorkspaceStore};

/// O nome do portão quando ele mesmo responde, em vez de um modelo.
const GATE_AUTHOR:&str="portaria";
const ENTRY_EVENT:&str="gate-entry";
const EXIT_EVENT:&str="gate-exit";
const RENAME_EVENT:&str="chat-renamed";
const PROMPT_EVENT:&str="chat-prompt";

pub struct DesktopState {
    orchestrator: Orchestrator,
    workspace: WorkspaceStore,
    gate: GateLog,
    /// A raiz com que o aplicativo subiu: vale só para chat de projeto sem
    /// pasta escolhida.
    home_root: PathBuf,
}

pub type SharedDesktopState=Arc<Mutex<DesktopState>>;

#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProcessRequest { pub input:String,pub session_id:Option<String> }

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct EntryEvent{check:EntryCheck,tally:Tally}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct ExitEvent{checks:Vec<ExitCheck>,tally:Tally}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct RenameEvent{chat_id:String,title:String}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct PromptEvent{chat_id:String}

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

#[tauri::command]
async fn process_request(app:AppHandle,state:State<'_,SharedDesktopState>,request:ProcessRequest)->Result<model::ProcessResult,String>{
    let shared=state.inner().clone();
    let mut state=state.lock().await;
    let chat_id=request.session_id.as_deref().ok_or_else(||"selecione um chat antes de enviar".to_string())?;
    if !state.workspace.contains_chat(chat_id).map_err(|error|error.to_string())?{return Err("chat não encontrado".into());}

    let unnamed=state.workspace.chat_is_unnamed(chat_id).map_err(|error|error.to_string())?;
    focus_on_chat_project(&mut state,chat_id)?;

    let project=state.orchestrator.rag.project_info();
    let entry=entry_check(&project,chat_id,&request.input).await;
    let entry=state.gate.record_entry(entry);
    let _=app.emit(ENTRY_EVENT,EntryEvent{check:entry.clone(),tally:state.gate.tally()});
    if entry.verdict==EntryVerdict::Block {
        let reply=entry.reply();
        state.orchestrator.memory.add_message(chat_id,"user",request.input.trim().to_string());
        state.orchestrator.memory.add_message(chat_id,"assistant",reply.clone());
        state.workspace.append_exchange(chat_id,&request.input,&reply).map_err(|error|error.to_string())?;
        return Ok(blocked_result(&request.input,reply));
    }
    state.orchestrator.pending_gate_note=entry.clarifying_note();

    let stored=state.workspace.append_prompt(chat_id,&request.input).map_err(|error|error.to_string())?;
    let _=app.emit(PROMPT_EVENT,PromptEvent{chat_id:chat_id.to_string()});
    if !stored {forget_pending_prompt(&mut state,chat_id).map_err(|error|error.to_string())?;}

    let result=state.orchestrator.process(&request.input,Some(chat_id)).await;
    let assistant=result.result.as_ref().map(|response|response.response.clone()).or_else(||result.error.clone()).unwrap_or_else(||"A execução terminou sem resposta.".into());
    if result.result.is_none(){state.orchestrator.memory.add_message(chat_id,"assistant",assistant.clone());}
    state.workspace.append_answer(chat_id,&assistant).map_err(|error|error.to_string())?;

    let exits=exit_checks(&state,chat_id,&assistant);
    if !exits.is_empty() {
        let exits=state.gate.record_exits(exits);
        let _=app.emit(EXIT_EVENT,ExitEvent{checks:exits,tally:state.gate.tally()});
    }
    if unnamed {rename_in_background(app,shared,chat_id.to_string(),request.input.clone(),jev_reading(&result));}
    Ok(result)
}

/// O chat mora num projeto, e o pedido tem de ser lido dentro da pasta desse
/// projeto: é dela que saem os arquivos do contexto, o nome no prompt da
/// portaria e a varredura da saída. Projeto sem pasta cai na raiz de partida.
fn focus_on_chat_project(state:&mut DesktopState,chat_id:&str)->Result<(),String> {
    let root=state.workspace.chat_root(chat_id).map_err(|error|error.to_string())?.unwrap_or_else(||state.home_root.clone());
    state.orchestrator.focus_on(&root).map_err(|error|error.to_string())
}

/// Na retentativa o pedido já está gravado desde a tentativa anterior, e o
/// `process` torna a anotá-lo na memória da sessão: sem isto o modelo receberia
/// a mesma linha duas vezes no histórico.
fn forget_pending_prompt(state:&mut DesktopState,chat_id:&str)->anyhow::Result<()> {
    let mut history=state.workspace.conversation(chat_id)?;
    if history.last().is_some_and(|message|message.role=="user") {history.pop();}
    state.orchestrator.memory.set_conversation(chat_id.to_string(),history);
    Ok(())
}

/// Como o Jev entendeu o pedido, em uma linha: é isso que vai junto do prompt
/// quando o modelo escolhe o título.
fn jev_reading(result:&model::ProcessResult)->String{format!("{} task, {} complexity, routed to {}",result.intent_analysis.intent,result.complexity,result.model_selection.model_name)}

/// O título definitivo depende de outra ida ao modelo, e ninguém deve esperar
/// por ela para ler a resposta: o chat já entrou na lista com o resumo local do
/// pedido e é rebatizado depois, avisando a interface pelo evento.
fn rename_in_background(app:AppHandle,shared:SharedDesktopState,chat_id:String,prompt:String,reading:String) {
    tauri::async_runtime::spawn(async move {
        let mut state=shared.lock().await;
        let Some(title)=state.orchestrator.name_chat(&prompt,&reading).await else {return};
        if state.workspace.rename_chat(&chat_id,&title).is_ok() {let _=app.emit(RENAME_EVENT,RenameEvent{chat_id,title});}
    });
}

/// Sem projeto, a portaria mostra a sessão inteira; com projeto, só os chats
/// dele.
#[tauri::command]
async fn gate_feed(state:State<'_,SharedDesktopState>,project_id:Option<String>)->Result<GateFeed,String>{
    let state=state.lock().await;
    let Some(project_id)=project_id else {return Ok(state.gate.feed());};
    let chats=state.workspace.chat_ids_for_project(&project_id).map_err(|error|error.to_string())?.into_iter().collect();
    Ok(state.gate.feed_for(&chats))
}

/// Pontua o pedido no Jev quando há credencial e nas heurísticas locais quando
/// não há — ou quando a chamada falha, para que o portão nunca trave o envio.
async fn entry_check(project:&model::ProjectInfo,chat_id:&str,input:&str)->EntryCheck {
    if jev::is_configured() {
        match gatekeeper::evaluate_entry(input,&project.name,&project.languages).await {
            Ok(reading)=>return gatekeeper::judge(chat_id,input,&reading,"jev"),
            Err(error)=>eprintln!("portaria: o Jev não respondeu, usando heurísticas locais ({error})"),
        }
    }
    gatekeeper::judge(chat_id,input,&gatekeeper::heuristic_entry(input),"heurística local")
}

fn exit_checks(state:&DesktopState,chat_id:&str,answer:&str)->Vec<ExitCheck> {
    let root=state.orchestrator.rag.project_info().root;
    gatekeeper::scan_answer(chat_id,answer,&state.orchestrator.config,&state.orchestrator.firewall,Path::new(&root))
}

fn blocked_result(input:&str,reply:String)->ProcessResult {
    let response=ProviderResponse{response:reply,input_tokens:0,output_tokens:0,model:GATE_AUTHOR.into(),provider:"jev".into(),latency_ms:0};
    ProcessResult {
        user_input:input.into(), normalized_input:input.trim().into(),
        intent_analysis:IntentAnalysis{intent:"general".into(),scores:HashMap::new(),confidence:0.0},
        complexity:"trivial".into(), context_plan:vec![], context:Context::default(),
        strategy:"gate_blocked".into(),
        model_selection:ModelSelection{model_name:GATE_AUTHOR.into(),provider:"jev".into(),estimated_tokens:0,score:0.0,reason:"o portão de entrada barrou o pedido".into()},
        result:Some(response), validation:true,
        decision:Decision{model_provider:"jev".into(),model_name:GATE_AUTHOR.into(),estimated_tokens:0,context_files_count:0,rag_files_count:0},
        routing:RoutingSignals::default(), error:None,
    }
}

#[tauri::command]
async fn system_status(state:State<'_,SharedDesktopState>)->Result<SystemStatus,String>{
    let state=state.lock().await;
    let orchestrator=&state.orchestrator;
    Ok(SystemStatus{version:env!("CARGO_PKG_VERSION"),config_path:orchestrator.config_path.display().to_string(),database_path:state.workspace.database_path().display().to_string(),database_name:state.workspace.database_name(),tables:state.workspace.table_counts().unwrap_or_default(),providers:orchestrator.executable_provider_count(),models:orchestrator.executable_model_count(),indexed_files:orchestrator.rag.len(),cache_entries:orchestrator.cache.len(),session_messages:orchestrator.memory.session_messages(),performance_records:orchestrator.performance.len()})
}

#[tauri::command]
async fn get_workspace(state:State<'_,SharedDesktopState>)->Result<WorkspaceData,String>{state.lock().await.workspace.snapshot().map_err(|error|error.to_string())}

#[tauri::command]
async fn create_project(state:State<'_,SharedDesktopState>,name:String,root_path:Option<String>)->Result<ProjectRecord,String>{state.lock().await.workspace.create_project(&name,root_path).map_err(|error|error.to_string())}

#[tauri::command]
async fn create_chat(state:State<'_,SharedDesktopState>,project_id:String,title:Option<String>)->Result<ChatRecord,String>{state.lock().await.workspace.create_chat(&project_id,title).map_err(|error|error.to_string())}

#[tauri::command]
async fn clear_chat(state:State<'_,SharedDesktopState>,chat_id:String)->Result<(),String>{
    let mut state=state.lock().await;
    state.workspace.clear_chat(&chat_id).map_err(|error|error.to_string())?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
async fn delete_chat(state:State<'_,SharedDesktopState>,chat_id:String)->Result<(),String>{
    let mut state=state.lock().await;
    state.workspace.delete_chat(&chat_id).map_err(|error|error.to_string())?;
    state.orchestrator.memory.clear_session(&chat_id);
    Ok(())
}

#[tauri::command]
async fn delete_project(state:State<'_,SharedDesktopState>,project_id:String)->Result<(),String>{
    let mut state=state.lock().await;
    let chat_ids=state.workspace.delete_project(&project_id).map_err(|error|error.to_string())?;
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
    let state=Arc::new(Mutex::new(DesktopState{orchestrator,workspace,gate:GateLog::default(),home_root:root}));
    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).manage(state).invoke_handler(tauri::generate_handler![process_request,system_status,get_workspace,create_project,create_chat,clear_chat,delete_chat,delete_project,get_settings,save_settings,discover_provider_models,gate_feed]).run(tauri::generate_context!()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    /// O seletor de pastas já ficou mudo uma vez: no Linux o backend gtk3 do rfd
    /// abre a janela pela thread GTK dele, que disputa o loop de eventos do
    /// Tauri, e a chamada volta vazia sem nada aparecer. Quem voltar a declarar
    /// o plugin numa linha só reativa o padrão gtk3 e traz o bug de volta.
    #[test] fn the_folder_picker_talks_to_the_xdg_portal_on_linux() {
        let manifest=include_str!("../Cargo.toml");
        let linux=manifest.split("[target.'cfg(any(target_os = \"linux\"").nth(1).expect("falta o bloco de dependências do Linux");
        let declaration=linux.lines().find(|line|line.starts_with("tauri-plugin-dialog")).expect("o Linux precisa declarar o plugin de diálogo");
        assert!(declaration.contains("default-features = false"),"o padrão gtk3 continua ligado: {declaration}");
        assert!(declaration.contains("xdg-portal"),"o Linux precisa do backend do portal XDG: {declaration}");
    }
}
