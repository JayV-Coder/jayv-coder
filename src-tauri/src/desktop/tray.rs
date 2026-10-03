//! O ícone na bandeja do sistema. Fechar a janela só a esconde: a fila, a
//! sincronização, o painel ao vivo e a busca de versão nova continuam rodando,
//! e o menu do ícone traz a janela de volta já na tela pedida. "Sair" é o único
//! caminho que encerra o app.
//!
//! O texto do menu vem da tela, no idioma dela (`set_tray_labels`); até ela
//! mandar, vale o inglês.

use super::events::{TrayAction, UpdateFound, TRAY_EVENT, UPDATE_EVENT};
use crate::i18n::Text;
use serde::Deserialize;
use std::{sync::atomic::{AtomicBool, Ordering}, time::Duration};
use tauri::{menu::{Menu, MenuItem, PredefinedMenuItem}, tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent}, AppHandle, Emitter, Manager, Runtime, Window, WindowEvent};

const TRAY_ID:&str="main";

/// O que o menu leva até a tela: cada um vira um `tray-action`, depois que a
/// janela aparece.
const ACTIONS:[&str;7]=["newChat","projects","organizations","stats","system","settings","update"];

/// De quanto em quanto tempo o núcleo pergunta se há versão nova. É ele quem
/// pergunta, e não a tela: com a janela escondida na bandeja, o navegador
/// embutido segura os relógios dela.
const UPDATE_EVERY:Duration=Duration::from_secs(15);

#[derive(Debug,Clone,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct TrayLabels{open:String,new_chat:String,projects:String,organizations:String,stats:String,system:String,settings:String,update:String,quit:String,tooltip:String}

impl Default for TrayLabels {
    fn default()->Self {
        let text=|value:&str|value.to_string();
        TrayLabels{open:text("Open JayV"),new_chat:text("New chat"),projects:text("Projects"),organizations:text("Organizations"),stats:text("Statistics"),system:text("System"),settings:text("Settings"),update:text("Check for updates"),quit:text("Quit JayV"),tooltip:text("JayV")}
    }
}

/// Se o ícone subiu. Sem ele (um Linux sem bandeja), fechar a janela fecha o
/// app: escondida, ninguém a traria de volta.
#[derive(Default)]
pub struct TrayReady(AtomicBool);

fn menu<R:Runtime>(app:&AppHandle<R>,labels:&TrayLabels)->tauri::Result<Menu<R>> {
    let item=|id:&str,text:&str|MenuItem::with_id(app,id,text,true,None::<&str>);
    Menu::with_items(app,&[
        &item("open",&labels.open)?,
        &PredefinedMenuItem::separator(app)?,
        &item("newChat",&labels.new_chat)?,
        &item("projects",&labels.projects)?,
        &item("organizations",&labels.organizations)?,
        &item("stats",&labels.stats)?,
        &item("system",&labels.system)?,
        &item("settings",&labels.settings)?,
        &PredefinedMenuItem::separator(app)?,
        &item("update",&labels.update)?,
        &PredefinedMenuItem::separator(app)?,
        &item("quit",&labels.quit)?,
    ])
}

/// Traz a janela de volta: da bandeja, minimizada ou atrás das outras.
pub fn show_main<R:Runtime>(app:&AppHandle<R>) {
    if let Some(window)=app.get_webview_window("main") {
        let _=window.show();
        let _=window.unminimize();
        let _=window.set_focus();
    }
}

fn on_menu<R:Runtime>(app:&AppHandle<R>,id:&str) {
    match id {
        "open"=>show_main(app),
        "quit"=>app.exit(0),
        action if ACTIONS.contains(&action)=>{
            show_main(app);
            let _=app.emit(TRAY_EVENT,TrayAction{action:action.to_string()});
        }
        _=>{}
    }
}

/// Põe o ícone na bandeja. Falhar não impede o app de abrir: só faz o botão
/// fechar voltar a fechar.
pub fn install<R:Runtime>(app:&AppHandle<R>) {
    let built=menu(app,&TrayLabels::default()).and_then(|menu|{
        let mut builder=TrayIconBuilder::with_id(TRAY_ID).menu(&menu).tooltip("JayV").show_menu_on_left_click(false)
            .on_menu_event(|app,event|on_menu(app,event.id().as_ref()))
            .on_tray_icon_event(|tray,event|{
                if let TrayIconEvent::Click{button:MouseButton::Left,button_state:MouseButtonState::Up,..}=event {show_main(tray.app_handle());}
            });
        if let Some(icon)=app.default_window_icon() {builder=builder.icon(icon.clone());}
        builder.build(app)
    });
    match built {
        Ok(_)=>app.state::<TrayReady>().0.store(true,Ordering::SeqCst),
        Err(error)=>eprintln!("bandeja: {error}"),
    }
}

/// O botão fechar da janela principal esconde em vez de fechar, quando há
/// bandeja para trazê-la de volta.
pub fn on_window_event<R:Runtime>(window:&Window<R>,event:&WindowEvent) {
    if let WindowEvent::CloseRequested{api,..}=event {
        if window.label()=="main" && window.app_handle().state::<TrayReady>().0.load(Ordering::SeqCst) {
            api.prevent_close();
            let _=window.hide();
        }
    }
}

/// O menu e a dica do ícone no idioma da tela.
#[tauri::command]
pub(crate) fn set_tray_labels(app:AppHandle,labels:TrayLabels)->Result<(),Text> {
    let Some(tray)=app.tray_by_id(TRAY_ID) else {return Ok(())};
    tray.set_menu(Some(menu(&app,&labels).map_err(Text::unexpected)?)).map_err(Text::unexpected)?;
    tray.set_tooltip(Some(&labels.tooltip)).map_err(Text::unexpected)
}

/// Pergunta ao repositório de releases, a cada `UPDATE_EVERY`, se há versão
/// nova, e avisa a tela uma vez por versão. Instalar continua com a tela, que
/// pede a mesma consulta e guarda o pacote até a pessoa mandar. Sem rede, ou
/// numa build de desenvolvimento, a falha fica calada e a próxima volta tenta
/// de novo.
pub async fn watch_updates<R:Runtime>(app:AppHandle<R>) {
    use tauri_plugin_updater::UpdaterExt;
    let mut announced:Option<String>=None;
    loop {
        if let Ok(Some(update))=match app.updater() {Ok(updater)=>updater.check().await, Err(error)=>Err(error)} {
            if announced.as_deref()!=Some(update.version.as_str()) {
                announced=Some(update.version.clone());
                let _=app.emit(UPDATE_EVENT,UpdateFound{version:update.version});
            }
        }
        tokio::time::sleep(UPDATE_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    /// Cada item do menu que leva a uma tela precisa de quem o cumpra do lado
    /// da tela; um item novo sem caso lá não faria nada ao ser clicado.
    #[test] fn every_tray_action_is_handled_by_the_screen() {
        let screen=include_str!("../../../src/modules/tray/index.ts");
        for action in super::ACTIONS {
            assert!(screen.contains(&format!("\"{action}\"")),"a tela não trata a ação {action} do menu da bandeja");
        }
    }
}
