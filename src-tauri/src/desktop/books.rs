//! O guarda-livros: grava no banco do usuário o que o núcleo gastou e avisa
//! a tela. É a única ponta que conhece banco e janela; quem gasta só diz
//! `usage::spend`.

use super::events::*;
use super::SharedWorkspace;
use crate::usage::Entry;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::UnboundedReceiver;

/// Os patamares que valem aviso quando o limite de um plano os cruza.
pub const THRESHOLDS:[f64;3]=[80.0,95.0,100.0];

/// O maior patamar que a leitura nova cruzou, subindo, desde a anterior.
pub fn crossed(previous:Option<f64>,current:Option<f64>)->Option<u8> {
    let current=current?;
    THRESHOLDS.iter().rev().find(|mark|current>=**mark&&previous.is_none_or(|before|before<**mark)).map(|mark|*mark as u8)
}

pub(crate) async fn keep_the_books(app:AppHandle,workspace:SharedWorkspace,mut entries:UnboundedReceiver<Entry>) {
    let mut last:HashMap<(String,String),Option<f64>>=HashMap::new();
    while let Some(entry)=entries.recv().await {
        let written=workspace.lock().await.record_usage(&entry);
        match (written,entry) {
            (Ok(true),Entry::Quota(quota))=>{
                let previous=last.insert((quota.agent.clone(),quota.window.clone()),quota.used_percent).flatten();
                let crossed=crossed(previous,quota.used_percent);
                let _=app.emit(QUOTA_EVENT,QuotaEvent{quota,crossed});
            }
            (Ok(true),Entry::Spend(scope,_)|Entry::Jev(scope,_))=>{
                let _=app.emit(USAGE_EVENT,UsageEvent{project_id:scope.project_id,chat_id:scope.chat_id});
            }
            (Ok(_),_)=>{}
            (Err(error),_)=>eprintln!("uso: não consegui gravar o registro ({error})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn only_rising_past_a_threshold_counts() {
        assert_eq!(crossed(None,Some(50.0)),None);
        assert_eq!(crossed(Some(70.0),Some(82.0)),Some(80));
        assert_eq!(crossed(Some(82.0),Some(90.0)),None,"já tinha passado dos 80");
        assert_eq!(crossed(Some(70.0),Some(100.0)),Some(100));
        assert_eq!(crossed(Some(96.0),Some(10.0)),None,"a janela renovou");
        assert_eq!(crossed(Some(96.0),None),None);
        assert_eq!(crossed(None,Some(96.0)),Some(95));
    }
}
