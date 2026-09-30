//! O aplicativo de mesa. Junta o orquestrador e o banco, sobe o atendente da
//! fila e expõe os comandos à tela. Cada funcionalidade mora no seu módulo:
//! `commands` recebe o que a tela pede, `queue` atende os pedidos e `events`
//! é o único caminho de volta para a tela.

pub mod commands;
pub mod events;
mod queue;

use crate::orchestrator::Orchestrator;
use crate::workspace::WorkspaceStore;
use commands::{gate, prompts, settings, system, workspace as projects};
use std::{path::PathBuf, sync::Arc};
use tauri::Manager;
use tokio::sync::{Mutex, Notify};

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
pub(crate) async fn both<'a>(desk:&'a SharedDesktopState,workspace:&'a SharedWorkspace)->(tokio::sync::MutexGuard<'a,DesktopState>,tokio::sync::MutexGuard<'a,WorkspaceStore>) {
    let desk=desk.lock().await;
    let workspace=workspace.lock().await;
    (desk,workspace)
}

pub fn run_desktop(config_path:PathBuf,root:PathBuf)->anyhow::Result<()> {
    let mut orchestrator=Orchestrator::new(config_path.clone(),root.clone())?;
    let database=crate::workspace::database_location(&config_path,&root);
    let legacy_workspace_path=database.with_file_name("workspace.json");
    let workspace=WorkspaceStore::open(database,Some(&legacy_workspace_path))?;
    orchestrator.use_llm(&workspace.llm_settings()?);
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
            tauri::async_runtime::spawn(queue::serve_the_queue(handle,desk,workspace,bell));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            prompts::enqueue_prompt,prompts::answer_question,prompts::dismiss_question,
            projects::get_workspace,projects::create_project,projects::create_chat,projects::clear_chat,projects::delete_chat,projects::delete_project,
            settings::get_settings,settings::save_settings,settings::check_agent,
            system::system_status,
            gate::gate_feed,
        ])
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
        let source=include_str!("commands/prompts.rs");
        let command=source.split("async fn enqueue_prompt").nth(1).expect("falta o comando de envio");
        let signature=command.split(')').next().expect("assinatura");
        assert!(!signature.contains("SharedDesktopState"),"o envio voltou a depender do cadeado do modelo: {signature}");
        assert!(signature.contains("SharedWorkspace"),"o envio precisa do banco, e só dele: {signature}");
    }

    #[test] fn the_folder_picker_talks_to_the_xdg_portal_on_linux() {
        let manifest=include_str!("../../Cargo.toml");
        let linux=manifest.split("[target.'cfg(any(target_os = \"linux\"").nth(1).expect("falta o bloco de dependências do Linux");
        let declaration=linux.lines().find(|line|line.starts_with("tauri-plugin-dialog")).expect("o Linux precisa declarar o plugin de diálogo");
        assert!(declaration.contains("default-features = false"),"o padrão gtk3 continua ligado: {declaration}");
        assert!(declaration.contains("xdg-portal"),"o Linux precisa do backend do portal XDG: {declaration}");
    }
}
