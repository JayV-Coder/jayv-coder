//! O aplicativo de mesa. Junta o orquestrador e o banco, sobe o atendente da
//! fila e expõe os comandos à tela. Cada funcionalidade mora no seu módulo:
//! `commands` recebe o que a tela pede, `queue` atende os pedidos e `events`
//! é o único caminho de volta para a tela.

mod books;
pub mod commands;
pub mod events;
mod queue;

use crate::cloud::remote::{Backend, Remote};
use crate::local::global::GlobalCache;
use crate::orchestrator::Orchestrator;
use crate::sync::Connectivity;
use crate::workspace::WorkspaceStore;
use commands::session::{SessionState, SharedSession};
use commands::{files, gate, prompts, repositories, session, settings, system, usage, workspace as projects};
use std::{path::PathBuf, sync::Arc, time::Duration};
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

/// Toca quando a sincronização precisa rodar já: login, sessão renovada,
/// logout. É um tipo próprio porque o Tauri guarda o estado pelo tipo, e o
/// sino da fila também é um `Arc<Notify>`.
pub struct SyncBell(pub Arc<Notify>);

/// Os comandos que escrevem só valem com alguém logado: sem sessão o banco é
/// o de memória, e o que se escrevesse nele sumiria no fechamento.
pub(crate) fn require_session()->Result<(),crate::i18n::Text> {
    crate::cloud::session::current().map(drop).ok_or_else(||crate::i18n::Text::new("session.required"))
}

/// Quem precisa dos dois cadeados pega sempre nesta ordem — orquestrador,
/// depois banco. O atendente segura o orquestrador do começo ao fim do pedido e
/// encosta no banco em trechos curtos; inverter a ordem em qualquer comando
/// travaria os dois.
pub(crate) async fn both<'a>(desk:&'a SharedDesktopState,workspace:&'a SharedWorkspace)->(tokio::sync::MutexGuard<'a,DesktopState>,tokio::sync::MutexGuard<'a,WorkspaceStore>) {
    let desk=desk.lock().await;
    let workspace=workspace.lock().await;
    (desk,workspace)
}

/// No Wayland com driver NVIDIA, o renderizador DMA-BUF do WebKitGTK derruba o
/// processo antes da janela aparecer ("Error 71 (Protocol error) dispatching to
/// Wayland display"): aberto pelo menu, o app abria e fechava. Quem já escolheu
/// um valor na sessão manda; sem ele, o WebKit desenha sem DMA-BUF.
#[cfg(target_os = "linux")]
fn keep_webkit_off_dmabuf() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: roda no começo da partida, antes do Tauri e do tokio subirem
        // qualquer thread que leia o ambiente.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER","1"); }
    }
}

pub fn run_desktop(config_path:PathBuf,root:PathBuf)->anyhow::Result<()> {
    crate::lockdown::strip_inspector_switches();
    #[cfg(target_os = "linux")]
    keep_webkit_off_dmabuf();
    let mut orchestrator=Orchestrator::unindexed(config_path.clone(),root.clone())?;
    // O app abre sem usuário: o banco é o de memória até o React entregar a
    // sessão, e aí vira o `workspace-<usuário>.sqlite3` desta pasta.
    let data_dir=crate::workspace::database_location(&config_path,&root).parent().map(PathBuf::from).unwrap_or_else(||root.join(".jev"));
    let cache=GlobalCache::open(&data_dir.join("cache.sqlite3"))?;
    crate::local::global::set_current_parameters(cache.jev_parameters()?);
    let workspace=WorkspaceStore::in_memory()?;
    orchestrator.use_llm(&workspace.llm_settings()?);

    let http=crate::lockdown::http_client(Duration::from_secs(30)).build()?;
    let desk:SharedDesktopState=Arc::new(Mutex::new(DesktopState{orchestrator,home_root:root}));
    let workspace:SharedWorkspace=Arc::new(Mutex::new(workspace));
    let bell:QueueBell=Arc::new(Notify::new());
    let sync_bell=SyncBell(Arc::new(Notify::new()));
    let connectivity=Connectivity::default();
    let session:SharedSession=Arc::new(Mutex::new(SessionState{cache,data_dir,identity:None,http:http.clone()}));
    tauri::Builder::default()
        // Primeiro de todos: o segundo processo — aberto pelo link do login —
        // entrega a URL a este e sai antes de subir qualquer outra coisa.
        .plugin(tauri_plugin_single_instance::init(|app,_args,_cwd|{
            if let Some(window)=app.get_webview_window("main") {let _=window.unminimize(); let _=window.set_focus();}
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(desk).manage(workspace).manage(bell).manage(sync_bell).manage(connectivity).manage(session)
        .setup(move |app|{
            // Em desenvolvimento e no AppImage o esquema `jayv://` não vem do
            // instalador: registra na partida. Falhar só desliga o login pelo
            // GitHub, não o app.
            #[cfg(any(target_os = "linux", windows))] {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(error)=app.deep_link().register_all() {eprintln!("deep link: {error}");}
            }
            let handle=app.handle().clone();
            let desk=app.state::<SharedDesktopState>().inner().clone();
            let workspace=app.state::<SharedWorkspace>().inner().clone();
            let bell=app.state::<QueueBell>().inner().clone();
            let sync_bell=app.state::<SyncBell>().0.clone();
            let connectivity=app.state::<Connectivity>().inner().clone();
            let backend=move ||crate::cloud::session::current().map(|token|Arc::new(Remote::new(http.clone(),Some(token))) as Arc<dyn Backend>);
            tauri::async_runtime::spawn(crate::sync::run(workspace.clone(),backend,connectivity.clone(),sync_bell,bell.clone()));
            tauri::async_runtime::spawn(session::announce_links(handle.clone(),connectivity.clone()));
            // A pia do uso: tudo que o núcleo gastar daqui em diante cai no
            // banco do usuário aberto no momento.
            let (sink,entries)=tokio::sync::mpsc::unbounded_channel();
            crate::usage::install(sink);
            tauri::async_runtime::spawn(books::keep_the_books(handle.clone(),workspace.clone(),entries));
            tauri::async_runtime::spawn(queue::serve_the_queue(handle,desk,workspace,bell,connectivity));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session::set_session,session::clear_session,session::connection_status,session::get_locales,session::get_translations,
            prompts::enqueue_prompt,prompts::answer_question,prompts::dismiss_question,
            projects::get_workspace,projects::create_project,projects::organization_project,projects::create_chat,projects::clear_chat,projects::delete_chat,projects::delete_project,
            settings::get_settings,settings::save_settings,settings::refresh_models,settings::check_agent,settings::set_reply_language,settings::get_core_settings,settings::save_core_settings,settings::save_expertise,
            system::system_status,
            gate::gate_feed,gate::scoped_gate_feed,
            files::open_file,
            repositories::scan_repositories,repositories::clone_repository,repositories::folder_repo_keys,
            usage::usage_report,usage::chat_usage,usage::refresh_quotas,
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
    #[test] fn accepting_a_request_does_not_wait_for_the_orchestrator() {
        let source=include_str!("commands/prompts.rs");
        let command=source.split("async fn enqueue_prompt").nth(1).expect("falta o comando de envio");
        let signature=command.split(')').next().expect("assinatura");
        assert!(!signature.contains("SharedDesktopState"),"o envio voltou a depender do cadeado do modelo: {signature}");
        assert!(signature.contains("SharedWorkspace"),"o envio precisa do banco, e só dele: {signature}");
    }

    /// O ambiente só pode mudar enquanto o processo tem uma thread só. Quem
    /// mover o contorno para depois do orquestrador ou do Builder corre o risco
    /// de o WebKit já ter lido o ambiente — e o app volta a abrir e fechar.
    #[test] fn webkit_leaves_dmabuf_before_anything_starts() {
        let source=include_str!("mod.rs");
        let body=source.split("pub fn run_desktop").nth(1).expect("falta run_desktop");
        let workaround=body.find("keep_webkit_off_dmabuf();").expect("run_desktop precisa desligar o DMA-BUF do WebKit");
        assert!(workaround<body.find("Orchestrator::unindexed").expect("orquestrador"),"o contorno tem de vir antes do orquestrador");
        assert!(workaround<body.find("tauri::Builder").expect("builder"),"o contorno tem de vir antes do Tauri");
    }

    #[test] fn the_folder_picker_talks_to_the_xdg_portal_on_linux() {
        let manifest=include_str!("../../Cargo.toml");
        let linux=manifest.split("[target.'cfg(any(target_os = \"linux\"").nth(1).expect("falta o bloco de dependências do Linux");
        let declaration=linux.lines().find(|line|line.starts_with("tauri-plugin-dialog")).expect("o Linux precisa declarar o plugin de diálogo");
        assert!(declaration.contains("default-features = false"),"o padrão gtk3 continua ligado: {declaration}");
        assert!(declaration.contains("xdg-portal"),"o Linux precisa do backend do portal XDG: {declaration}");
    }
}
