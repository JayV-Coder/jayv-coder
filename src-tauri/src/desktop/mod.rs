//! O aplicativo de mesa. Junta o orquestrador e o banco, sobe o atendente da
//! fila e expõe os comandos à tela. Cada funcionalidade mora no seu módulo:
//! `commands` recebe o que a tela pede, `queue` atende os pedidos e `events`
//! é o único caminho de volta para a tela.

mod books;
pub mod commands;
pub mod events;
mod live;
mod queue;
mod tray;

use crate::cloud::remote::{Backend, Remote};
use crate::local::global::GlobalCache;
use crate::orchestrator::Orchestrator;
use crate::sync::Connectivity;
use crate::workspace::WorkspaceStore;
use commands::session::{SessionState, SharedSession};
use commands::{files, gate, memory, prompts, repositories, session, settings, system, usage, workspace as projects};
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

/// Os atendentes de pedidos: um orquestrador por pedido no ar. O primeiro é o
/// de sempre (`SharedDesktopState`), que as telas usam; os outros nascem quando
/// o plano deixa mais de um pedido ao mesmo tempo, cada um com a sua pasta, os
/// seus agentes e o seu índice, e dividem com o primeiro o que os pedidos
/// dividem (`Orchestrator::share`).
///
/// Quem precisa de mais de um cadeado pega sempre nesta ordem: os
/// atendentes (o primeiro, depois os extras, em ordem), depois o banco. Cada
/// atendimento segura só o seu atendente e encosta no banco em trechos curtos;
/// inverter a ordem em qualquer comando travaria os dois.
#[derive(Clone)]
pub struct Lanes { first:SharedDesktopState, extra:Arc<std::sync::Mutex<Vec<SharedDesktopState>>>, config_path:PathBuf, home_root:PathBuf, shared:Arc<crate::orchestrator::Shared>, switching:Arc<tokio::sync::RwLock<()>> }

impl Lanes {
    pub(crate) fn new(first:SharedDesktopState,orchestrator:&Orchestrator,home_root:PathBuf)->Self {
        Self{first,extra:Arc::default(),config_path:orchestrator.config_path.clone(),home_root,shared:orchestrator.shared(),switching:Arc::default()}
    }

    /// A fila chama pedidos com isto na mão; a troca de usuário o pega por
    /// inteiro (`switching`). Assim nenhum pedido do banco antigo sai para um
    /// atendente novo enquanto o banco é trocado.
    pub(crate) async fn calling(&self)->tokio::sync::RwLockReadGuard<'_,()> { self.switching.read().await }
    pub(crate) async fn switching(&self)->tokio::sync::RwLockWriteGuard<'_,()> { self.switching.write().await }

    /// O primeiro atendente: o que as telas e os comandos usam.
    pub(crate) fn first(&self)->SharedDesktopState { self.first.clone() }

    fn extras(&self)->std::sync::MutexGuard<'_,Vec<SharedDesktopState>> { self.extra.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }

    /// O atendente `index`, criado na primeira vez que ele é preciso.
    pub(crate) fn lane(&self,index:usize)->anyhow::Result<SharedDesktopState> {
        if index==0 { return Ok(self.first.clone()); }
        let mut extra=self.extras();
        while extra.len()<index {
            let mut orchestrator=Orchestrator::unindexed(self.config_path.clone(),self.home_root.clone())?;
            orchestrator.share(self.shared.clone());
            extra.push(Arc::new(Mutex::new(DesktopState{orchestrator,home_root:self.home_root.clone()})));
        }
        Ok(extra[index-1].clone())
    }

    /// Espera os atendentes extras ficarem livres e os segura. Trocar de
    /// usuário não pode acontecer com um pedido no ar em nenhum deles.
    pub(crate) async fn hold_extras(&self)->Vec<tokio::sync::OwnedMutexGuard<DesktopState>> {
        let lanes:Vec<SharedDesktopState>=self.extras().clone();
        let mut held=Vec::with_capacity(lanes.len());
        for lane in lanes { held.push(lane.lock_owned().await); }
        held
    }

    /// Os atendentes extras vão embora: os próximos nascem do zero, sem a
    /// memória de conversa de outro usuário.
    pub(crate) fn retire_extras(&self) { self.extras().clear(); }
}

/// O retrato do orquestrador que as telas de leitura mostram, guardado fora do
/// cadeado dele. O atendente segura o orquestrador do começo ao fim de um
/// pedido; sem este retrato, Configurações e Sistema ficavam em "Carregando…"
/// até o agente terminar.
#[derive(Debug,Clone,Default)]
pub struct OrchestratorFacts { pub config_path:PathBuf, pub providers:usize, pub models:usize, pub indexed_files:usize, pub cache_entries:usize, pub session_messages:usize, pub performance_records:usize }

impl OrchestratorFacts {
    pub fn of(orchestrator:&Orchestrator)->Self {
        Self{config_path:orchestrator.config_path.clone(),providers:orchestrator.executable_provider_count(),models:orchestrator.executable_model_count(),indexed_files:orchestrator.rag.len(),cache_entries:orchestrator.cache.len(),session_messages:orchestrator.memory.session_messages(),performance_records:orchestrator.performance_len()}
    }
}

pub type SharedFacts=Arc<std::sync::Mutex<OrchestratorFacts>>;

/// O retrato de agora, se o orquestrador estiver livre; com um pedido no ar,
/// o da última vez que ele esteve. Nunca espera.
pub(crate) fn facts(desk:&SharedDesktopState,facts:&SharedFacts)->OrchestratorFacts {
    let mut kept=facts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Ok(desk)=desk.try_lock() { *kept=OrchestratorFacts::of(&desk.orchestrator); }
    kept.clone()
}

/// Passa uma configuração já gravada ao orquestrador, se ele estiver livre.
/// Ocupado, não espera: o atendente relê as configurações do banco no começo
/// de cada pedido, então o próximo já sai com elas.
pub(crate) fn when_free(desk:&SharedDesktopState,apply:impl FnOnce(&mut Orchestrator)) {
    if let Ok(mut desk)=desk.try_lock() { apply(&mut desk.orchestrator); }
}

/// O banco vive atrás do seu próprio cadeado, separado do orquestrador. É esta
/// separação que faz a promessa da fila valer: aceitar um pedido é escrever uma
/// linha, e escrever essa linha não pode esperar o modelo que está respondendo
/// o pedido anterior. Enquanto os dois dividiam um cadeado só, o segundo envio
/// ficava parado na porta — sem chegar ao disco — e o primeiro, ao terminar,
/// redesenhava a conversa a partir do banco e apagava da tela o que o
/// desenvolvedor tinha acabado de escrever.
pub type SharedWorkspace=Arc<Mutex<WorkspaceStore>>;

/// Os chats que o orquestrador tem de esquecer — memória da conversa e sessão
/// do agente — porque foram apagados ou limpos enquanto ele atendia outro
/// pedido. Apagar e limpar não esperam o agente: gravam no banco na hora e
/// deixam o esquecimento aqui, para o começo do próximo atendimento.
pub type SharedForget=Arc<std::sync::Mutex<std::collections::HashSet<String>>>;

/// Esquece os chats agora, se o orquestrador está livre, ou no começo do
/// próximo atendimento. Nunca espera.
pub(crate) fn forget_chats(desk:&SharedDesktopState,forget:&SharedForget,chat_ids:Vec<String>) {
    match desk.try_lock() {
        Ok(mut desk)=>for chat_id in &chat_ids { desk.orchestrator.memory.clear_session(chat_id); },
        Err(_)=>forget.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend(chat_ids),
    }
}

/// As paradas dos pedidos no ar, por turno. O botão "Parar" dispara a do
/// turno; o atendente a abre quando chama o pedido e a fecha no fim. O "Parar"
/// que chega entre a fila chamar o pedido e o atendente abrir a parada fica
/// guardado aqui e vale do mesmo jeito.
#[derive(Clone,Default)]
pub struct Cancels(Arc<std::sync::Mutex<std::collections::HashMap<String,crate::progress::Stop>>>);

impl Cancels {
    fn entries(&self)->std::sync::MutexGuard<'_,std::collections::HashMap<String,crate::progress::Stop>> { self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }
    /// A parada do pedido que vai ser atendido.
    pub(crate) fn open(&self,turn_id:&str)->crate::progress::Stop { self.entries().entry(turn_id.to_string()).or_default().clone() }
    pub(crate) fn close(&self,turn_id:&str) { self.entries().remove(turn_id); }
    /// Para o pedido no ar (ou o que está para entrar no ar).
    pub(crate) fn stop(&self,turn_id:&str,reason:crate::progress::StopReason) { self.entries().entry(turn_id.to_string()).or_default().stop(reason); }
}

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
    crate::gatekeeper::set_current_parameters(crate::gatekeeper::JevParameters::from_values(&cache.jev_parameter_values()?));
    let workspace=WorkspaceStore::in_memory()?;
    orchestrator.use_llm(&workspace.llm_settings()?);

    let http=crate::lockdown::http_client(Duration::from_secs(30)).build()?;
    let facts:SharedFacts=Arc::new(std::sync::Mutex::new(OrchestratorFacts::of(&orchestrator)));
    let first:SharedDesktopState=Arc::new(Mutex::new(DesktopState{orchestrator,home_root:root.clone()}));
    let lanes=Lanes::new(first.clone(),&first.try_lock().expect("recém-criado").orchestrator,root);
    let desk=first;
    let workspace:SharedWorkspace=Arc::new(Mutex::new(workspace));
    let bell:QueueBell=Arc::new(Notify::new());
    let sync_bell=SyncBell(Arc::new(Notify::new()));
    let connectivity=Connectivity::default();
    let session:SharedSession=Arc::new(Mutex::new(SessionState{cache,data_dir,identity:None,http:http.clone()}));
    tauri::Builder::default()
        // Primeiro de todos: o segundo processo — aberto pelo link do login —
        // entrega a URL a este e sai antes de subir qualquer outra coisa.
        .plugin(tauri_plugin_single_instance::init(|app,_args,_cwd|{
            // Escondida na bandeja ou minimizada, a janela volta para a frente.
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(desk).manage(facts).manage(workspace).manage(bell).manage(sync_bell).manage(connectivity).manage(session)
        .manage(live::SharedLive::default())
        .manage(SharedForget::default())
        .manage(Cancels::default())
        .manage(lanes)
        .manage(tray::TrayReady::default())
        .on_window_event(tray::on_window_event)
        .setup(move |app|{
            // Em desenvolvimento e no AppImage o esquema `jayv://` não vem do
            // instalador: registra na partida. Falhar só desliga o login pelo
            // GitHub, não o app.
            #[cfg(any(target_os = "linux", windows))] {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(error)=app.deep_link().register_all() {eprintln!("deep link: {error}");}
            }
            let handle=app.handle().clone();
            tray::install(&handle);
            tauri::async_runtime::spawn(tray::watch_updates(handle.clone()));
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
            let forget=app.state::<SharedForget>().inner().clone();
            let cancels=app.state::<Cancels>().inner().clone();
            let lanes=app.state::<Lanes>().inner().clone();
            tauri::async_runtime::spawn(queue::serve_the_queue(handle,lanes,workspace,bell,connectivity,forget,cancels));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session::set_session,session::clear_session,session::connection_status,session::get_locales,session::get_translations,
            prompts::enqueue_prompt,prompts::answer_question,prompts::dismiss_question,prompts::cancel_turn,prompts::allowed_commands,prompts::forget_allowed_command,
            projects::get_workspace,projects::get_chat,projects::create_project,projects::organization_project,projects::create_chat,projects::clear_chat,projects::set_work_mode,projects::delete_chat,projects::delete_project,
            settings::get_settings,settings::save_settings,settings::get_mcp_servers,settings::save_mcp_servers,settings::get_skills,settings::get_org_extensions,settings::install_skill_folder,settings::install_skill_text,settings::set_skill_enabled,settings::remove_skill,settings::draft_mcp,settings::refresh_models,settings::check_agent,settings::check_gateway,settings::set_reply_language,settings::get_core_settings,settings::save_core_settings,settings::save_expertise,settings::save_lean_code,
            system::system_status,
            gate::gate_feed,gate::scoped_gate_feed,gate::turn_evidence,
            files::open_file,
            live::live_files,live::live_file,live::editors,live::open_in_editor,
            repositories::scan_repositories,repositories::clone_repository,repositories::folder_repo_keys,repositories::repository_states,
            usage::usage_report,usage::chat_usage,usage::refresh_quotas,
            tray::set_tray_labels,
            memory::project_memory,memory::save_project_note,memory::delete_project_note,memory::search_chats,
        ])
        .build(tauri::generate_context!())?
        .run(|_app,_event|{
            // No macOS, clicar no ícone do Dock com a janela escondida a traz
            // de volta.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen{..}=_event {tray::show_main(_app);}
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Apagar e limpar um chat gravam no banco e não esperam o agente que
    /// atende outro pedido. Quem voltar a pegar o cadeado do orquestrador
    /// nesses comandos traz de volta o clique que fica parado minutos.
    #[test] fn deleting_and_clearing_do_not_wait_for_the_orchestrator() {
        let source=include_str!("commands/workspace.rs");
        for command in ["async fn clear_chat","async fn delete_chat","async fn delete_project"] {
            let body=source.split(command).nth(1).expect(command).split("#[tauri::command]").next().expect("corpo");
            assert!(!body.contains("both(")&&!body.contains("state.lock()"),"{command} voltou a esperar o orquestrador");
            assert!(body.contains("forget_chats("),"{command} precisa fazer o orquestrador esquecer o chat");
        }
    }

    /// Com o orquestrador ocupado, o chat fica na lista e é esquecido no
    /// começo do próximo atendimento; livre, é esquecido na hora.
    #[tokio::test] async fn a_busy_orchestrator_forgets_the_chat_later() {
        let dir=tempfile::tempdir().expect("pasta");
        let mut orchestrator=Orchestrator::unindexed(dir.path().join("missing.yaml"),dir.path().to_path_buf()).expect("orquestrador");
        orchestrator.memory.add_message("busy","user","oi");
        orchestrator.memory.add_message("free","user","oi");
        let desk:SharedDesktopState=Arc::new(Mutex::new(DesktopState{orchestrator,home_root:dir.path().to_path_buf()}));
        let forget=SharedForget::default();
        {
            let _serving=desk.lock().await;
            forget_chats(&desk,&forget,vec!["busy".into()]);
        }
        assert!(forget.lock().unwrap().contains("busy"),"ocupado: fica para depois");
        assert_eq!(desk.lock().await.orchestrator.memory.conversation("busy").len(),1);
        forget_chats(&desk,&forget,vec!["free".into()]);
        assert!(desk.lock().await.orchestrator.memory.conversation("free").is_empty(),"livre: esquece na hora");
        assert!(!forget.lock().unwrap().contains("free"));
    }

    /// O "Parar" que chega antes de o atendente abrir a parada do turno vale
    /// do mesmo jeito; fechada, a parada some.
    #[test] fn a_stop_sent_before_the_turn_opens_still_counts() {
        let cancels=Cancels::default();
        cancels.stop("t1",crate::progress::StopReason::Asked);
        assert_eq!(cancels.open("t1").reason(),Some(crate::progress::StopReason::Asked));
        cancels.close("t1");
        assert_eq!(cancels.open("t1").reason(),None,"o mesmo turno reenviado começa sem parada");
    }

    /// O segundo atendente nasce com a sua pasta e os seus agentes, mas
    /// divide com o primeiro o que os pedidos aprendem; trocar de usuário o
    /// aposenta.
    #[tokio::test] async fn a_second_lane_shares_what_turns_learn() {
        let dir=tempfile::tempdir().expect("pasta");
        let orchestrator=Orchestrator::unindexed(dir.path().join("missing.yaml"),dir.path().to_path_buf()).expect("orquestrador");
        let first:SharedDesktopState=Arc::new(Mutex::new(DesktopState{orchestrator,home_root:dir.path().to_path_buf()}));
        let lanes=Lanes::new(first.clone(),&first.lock().await.orchestrator,dir.path().to_path_buf());
        assert!(Arc::ptr_eq(&lanes.lane(0).expect("primeiro"),&first));
        let second=lanes.lane(1).expect("segundo");
        assert!(!Arc::ptr_eq(&second,&first),"outro orquestrador");
        assert!(Arc::ptr_eq(&second.lock().await.orchestrator.shared(),&first.lock().await.orchestrator.shared()),"o mesmo aprendizado");
        assert!(Arc::ptr_eq(&lanes.lane(1).expect("de novo"),&second),"o mesmo atendente volta");
        assert_eq!(lanes.hold_extras().await.len(),1);
        lanes.retire_extras();
        assert!(!Arc::ptr_eq(&lanes.lane(1).expect("novo"),&second),"depois da troca de usuário, nasce outro");
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
