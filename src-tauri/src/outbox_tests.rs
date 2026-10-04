//! Os testes da fila (`crate::local::outbox`, no crate `jayv-store`) usam o
//! banco do workspace inteiro, que mora numa camada acima; por isso ficam no
//! crate do app.

#[cfg(test)]
mod tests {
    use crate::local::outbox::*;
    use rusqlite::OptionalExtension;
    use std::collections::BTreeMap;
    use serde_json::Value;
    use crate::workspace::WorkspaceStore;
    use crate::turns::TurnStatus;
    use serde_json::json;

    fn settle_all(store:&WorkspaceStore) {
        for entry in pending(store.connection(),10_000).expect("fila") {settle(store.connection(),entry.seq,entry.version).expect("settle");}
    }

    fn entries(store:&WorkspaceStore,table:&str)->Vec<Pending> {
        pending(store.connection(),10_000).expect("fila").into_iter().filter(|entry|entry.table==table).collect()
    }

    fn project_name(store:&WorkspaceStore,id:&str)->Option<String> {
        store.connection().query_row("SELECT name FROM projects WHERE id=?1",[id],|row|row.get(0)).optional().expect("projeto")
    }

    /// Os padrões dos agentes nascem em toda máquina nova. Se subissem, a
    /// primeira abertura num computador novo apagaria a configuração que o
    /// desenvolvedor já tem no Supabase.
    #[test] fn a_new_database_starts_with_an_empty_queue() {
        let store=WorkspaceStore::in_memory().expect("store");
        assert!(pending(store.connection(),100).expect("fila").is_empty());
        assert!(!store.llm_settings().expect("llm").agents.is_empty());
    }

    /// O nível é da conta: sobe como as outras linhas, e o que chega de outro
    /// computador vale aqui.
    #[test] fn the_level_travels_with_the_account() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        store.save_expertise("senior").expect("nível");
        let queue=entries(&store,"account_settings");
        assert_eq!(queue.len(),1);
        assert_eq!(queue[0].key,json!(["expertise_level"]));
        settle(store.connection(),queue[0].seq,queue[0].version).expect("enviado");
        apply_remote(store.connection_mut(),table("account_settings").unwrap(),&[json!({"key":"expertise_level","value":"architect","updated_at":"2026-10-02T12:00:00+00:00"})]).expect("aplica");
        assert_eq!(store.expertise().expect("nível"),crate::expertise::Expertise::Architect);
    }

    fn git_folder(url:&str)->tempfile::TempDir {
        let dir=tempfile::tempdir().expect("pasta");
        std::fs::create_dir(dir.path().join(".git")).expect(".git");
        std::fs::write(dir.path().join(".git/config"),format!("[remote \"origin\"]\n\turl = {url}\n")).expect("config");
        dir
    }

    fn repo_keys(store:&WorkspaceStore,id:&str)->String {
        store.connection().query_row("SELECT repo_keys FROM projects WHERE id=?1",[id],|row|row.get(0)).expect("repo_keys")
    }

    /// As chaves dos remotes sobem com o projeto: é por elas que o servidor
    /// sabe de que organização ele é.
    #[test] fn a_project_carries_the_keys_of_its_remotes() {
        let folder=git_folder("git@github.com:Acme/API.git");
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Api",Some(folder.path().to_string_lossy().into())).expect("projeto");
        assert_eq!(repo_keys(&store,&project.id),r#"["github.com/acme/api"]"#);
        assert!(table("projects").unwrap().columns.contains(&"repo_keys"));
    }

    /// Recalcular na abertura só escreve o que mudou: sem mudança, a fila não
    /// ganha nada.
    #[test] fn refreshing_the_keys_only_writes_what_changed() {
        let folder=git_folder("git@github.com:acme/api.git");
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Api",Some(folder.path().to_string_lossy().into())).expect("projeto");
        let queued=entries(&store,"projects");
        settle(store.connection(),queued[0].seq,queued[0].version).expect("enviado");
        store.refresh_repo_keys().expect("recalcula");
        assert!(entries(&store,"projects").is_empty(),"nada mudou, nada sobe");
        std::fs::write(folder.path().join(".git/config"),"[remote \"origin\"]\n\turl = https://gitlab.com/acme/api\n").expect("config");
        store.refresh_repo_keys().expect("recalcula");
        assert_eq!(repo_keys(&store,&project.id),r#"["gitlab.com/acme/api"]"#);
        assert_eq!(entries(&store,"projects").len(),1,"a mudança sobe");
    }

    /// Um banco antigo tem o gatilho de update sem a coluna nova: a instalação
    /// o troca, senão a mudança de `repo_keys` nunca subiria.
    #[test] fn old_triggers_are_replaced_when_the_columns_change() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        store.connection().execute_batch(
            "DROP TRIGGER outbox_projects_update;
             CREATE TRIGGER outbox_projects_update AFTER UPDATE OF id,name,created_at ON projects BEGIN SELECT 1; END;").expect("gatilho velho");
        install(store.connection()).expect("instala");
        let project=store.create_project("Loja",None).expect("projeto");
        let queued=entries(&store,"projects");
        settle(store.connection(),queued[0].seq,queued[0].version).expect("enviado");
        store.connection().execute("UPDATE projects SET repo_keys='[\"github.com/a/b\"]' WHERE id=?1",[&project.id]).expect("update");
        assert_eq!(entries(&store,"projects").len(),1);
    }

    #[test] fn creating_a_project_queues_an_entry() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let queue=entries(&store,"projects");
        assert_eq!(queue.len(),1);
        assert_eq!(queue[0].key,json!([project.id]));
        assert_eq!(queue[0].op,Op::Upsert);
        assert_eq!(queue[0].version,1);
        assert_eq!(row(store.connection(),table("projects").unwrap(),&queue[0].key).expect("linha"),Some(json!({"id":project.id,"name":"Loja","created_at":project.created_at.to_rfc3339(),"repo_keys":"[]","org_id":null})));
    }

    #[test] fn writes_in_the_same_turn_become_one_entry() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        store.set_turn_status(&turn.id,TurnStatus::Flying).expect("no ar");
        store.set_turn_status(&turn.id,TurnStatus::Answered).expect("respondido");
        let queue=entries(&store,"turns");
        assert_eq!(queue.len(),1);
        assert_eq!(queue[0].version,3);
    }

    /// O rascunho do streaming muda a cada pedaço que chega. Se ele subisse,
    /// cada resposta viraria centenas de envios.
    #[test] fn the_streaming_draft_is_not_queued() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        settle_all(&store);
        store.set_turn_partial(&turn.id,"meia resp").expect("rascunho");
        store.clear_turn_partial(&turn.id).expect("limpa");
        assert!(pending(store.connection(),100).expect("fila").is_empty());
    }

    /// A recusa de um turno é do chat dele e do projeto do chat; a de uma
    /// configuração de agente não é de projeto nenhum.
    #[test] fn refusals_are_counted_by_their_owner() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        let other=store.create_project("Outro",None).expect("outro");
        store.connection().execute("INSERT INTO outbox(tbl,row_key,op,at,status) VALUES('llm_agents','[\"claude\"]','upsert','x','failed')",[]).expect("agente");
        store.connection().execute("INSERT INTO outbox(tbl,row_key,op,at,status) VALUES('chats','[\"sumiu\"]','delete','x','failed')",[]).expect("excluído");
        for (tbl,key) in [("turns",&turn.id),("projects",&other.id)] {
            store.connection().execute("UPDATE outbox SET status='failed' WHERE tbl=?1 AND row_key=json_array(?2)",[tbl,key.as_str()]).expect("recusa");
        }
        let found=refusals(store.connection()).expect("contagem");
        assert_eq!(found.by_chat,BTreeMap::from([(chat.id.clone(),1)]));
        assert_eq!(found.by_project,BTreeMap::from([(project.id.clone(),1),(other.id.clone(),1)]));
        assert_eq!(found.unplaced,2);
    }

    #[test] fn deleting_a_chat_also_deletes_its_children_remotely() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"oi",None).expect("turno");
        settle_all(&store);
        store.delete_chat(&chat.id).expect("apaga");
        let queue=pending(store.connection(),100).expect("fila");
        let deleted:Vec<(&str,&Value)>=queue.iter().filter(|entry|entry.op==Op::Delete).map(|entry|(entry.table.as_str(),&entry.key)).collect();
        assert!(deleted.contains(&("chats",&json!([chat.id]))),"{deleted:?}");
        assert!(deleted.contains(&("turns",&json!([turn.id]))),"{deleted:?}");
        assert!(deleted.iter().any(|(table,_)|*table=="messages"),"{deleted:?}");
        assert!(!queue.iter().any(|entry|entry.table=="projects"),"o projeto não foi tocado");
    }

    #[test] fn what_comes_from_remote_does_not_go_back_to_the_queue() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let applied=apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":"p1","name":"Remoto","created_at":"2026-09-30T12:00:00+00:00","row_updated_at":"2026-09-30T12:00:00Z","row_deleted_at":null,"synced_at":"2026-09-30T12:00:00Z"})]).expect("aplica");
        assert_eq!(applied,1);
        assert_eq!(project_name(&store,"p1").as_deref(),Some("Remoto"));
        assert!(pending(store.connection(),100).expect("fila").is_empty());
    }

    /// A escrita local ainda não subiu: quem decide entre as duas é o
    /// servidor, na subida. Aplicar a remota aqui apagaria a local antes disso.
    #[test] fn a_row_with_a_pending_write_is_not_overwritten() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Local",None).expect("projeto");
        let applied=apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":project.id,"name":"Remoto","created_at":"2026-09-30T12:00:00+00:00","row_deleted_at":null})]).expect("aplica");
        assert_eq!(applied,0);
        assert_eq!(project_name(&store,&project.id).as_deref(),Some("Local"));
    }

    #[test] fn a_row_deleted_remotely_leaves_the_cache() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        settle_all(&store);
        apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":project.id,"name":"Loja","created_at":"2026-09-30T12:00:00+00:00","row_deleted_at":"2026-09-30T13:00:00Z"})]).expect("aplica");
        assert_eq!(project_name(&store,&project.id),None);
        assert!(!store.contains_chat(&chat.id).expect("chat"));
        assert!(pending(store.connection(),100).expect("fila").is_empty(),"apagar o que veio do remoto não sobe de volta");
    }

    /// A escrita chegou enquanto a anterior subia. Dar a entrada por entregue
    /// perderia a segunda.
    #[test] fn delivering_an_old_version_keeps_the_entry() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let sent=entries(&store,"chats").remove(0);
        store.rename_chat(&chat.id,"Outro nome").expect("renomeia");
        settle(store.connection(),sent.seq,sent.version).expect("settle");
        let left=entries(&store,"chats");
        assert_eq!(left.len(),1);
        assert_eq!(left[0].version,sent.version+1);
        settle(store.connection(),left[0].seq,left[0].version).expect("settle");
        assert!(entries(&store,"chats").is_empty());
    }

    #[test] fn a_failed_entry_leaves_the_queue_until_the_next_write() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let project=store.create_project("Loja",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let sent=entries(&store,"chats").remove(0);
        fail(store.connection(),sent.seq,"recusado").expect("falha");
        assert!(entries(&store,"chats").is_empty());
        store.rename_chat(&chat.id,"Outro nome").expect("renomeia");
        assert_eq!(entries(&store,"chats").len(),1);
    }

    fn remote_chat(store:&mut WorkspaceStore)->(String,String) {
        apply_remote(store.connection_mut(),table("projects").unwrap(),&[json!({"id":"p1","name":"Remoto","created_at":"2026-09-30T12:00:00+00:00"})]).expect("projeto");
        apply_remote(store.connection_mut(),table("chats").unwrap(),&[json!({"id":"c1","code":"ABC123","project_id":"p1","title":"Remoto","named":1,"created_at":"2026-09-30T12:00:00+00:00","updated_at":"2026-09-30T12:00:00+00:00"})]).expect("chat");
        apply_remote(store.connection_mut(),table("turns").unwrap(),&[json!({"id":"t1","chat_id":"c1","ordinal":1,"status":"queued","created_at":"2026-09-30T12:00:00+00:00"})]).expect("turno");
        ("c1".into(),"t1".into())
    }

    /// Dois computadores com o mesmo usuário: o pedido que outro computador
    /// pôs na fila é dele, e atender aqui também faria o trabalho duas vezes.
    #[test] fn the_queue_only_serves_this_machines_requests() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let (chat,turn)=remote_chat(&mut store);
        apply_remote(store.connection_mut(),table("messages").unwrap(),&[json!({"uid":"m1","chat_id":chat,"turn_id":turn,"role":"user","content":"oi","created_at":"2026-09-30T12:00:00+00:00"})]).expect("mensagem");
        assert!(store.claim_next_turn().expect("fila").is_none());
        apply_remote(store.connection_mut(),table("turns").unwrap(),&[json!({"id":"t2","chat_id":chat,"ordinal":2,"status":"flying","created_at":"2026-09-30T12:01:00+00:00"})]).expect("turno no ar");
        let mine=store.enqueue_prompt(&chat,"daqui",None).expect("turno");
        let (claimed,prompt)=store.claim_next_turn().expect("fila").expect("o pedido daqui é atendido mesmo com outro no ar lá");
        assert_eq!((claimed.id,prompt.as_str()),(mine.id,"daqui"));
    }

    #[test] fn messages_downloaded_out_of_order_show_by_time() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let (chat,turn)=remote_chat(&mut store);
        apply_remote(store.connection_mut(),table("messages").unwrap(),&[
            json!({"uid":"m2","chat_id":chat,"turn_id":turn,"role":"assistant","content":"resposta","created_at":"2026-09-30T12:00:05+00:00"}),
            json!({"uid":"m1","chat_id":chat,"turn_id":turn,"role":"user","content":"pedido","created_at":"2026-09-30T12:00:00+00:00"}),
        ]).expect("mensagens");
        let contents:Vec<String>=store.conversation(&chat).expect("conversa").into_iter().map(|message|message.content).collect();
        assert_eq!(contents,["pedido","resposta"]);
    }

    /// O pai foi apagado aqui e a exclusão ainda não subiu: o filho que chega
    /// do remoto não tem onde se pendurar. Ele fica de fora, e o resto da
    /// página entra — uma linha órfã não pode travar a sincronização inteira.
    #[test] fn a_row_without_a_local_parent_is_skipped_and_the_rest_applies() {
        let mut store=WorkspaceStore::in_memory().expect("store");
        let (chat,_)=remote_chat(&mut store);
        let applied=apply_remote(store.connection_mut(),table("turns").unwrap(),&[
            json!({"id":"orfao","chat_id":"chat-que-nao-existe","ordinal":1,"status":"answered","created_at":"2026-09-30T12:00:00+00:00"}),
            json!({"id":"t9","chat_id":chat,"ordinal":9,"status":"answered","created_at":"2026-09-30T12:00:00+00:00"}),
        ]).expect("a página entra mesmo com um órfão");
        assert_eq!(applied,1);
        assert!(store.turn("t9").expect("turno").is_some());
        assert!(store.turn("orfao").expect("turno").is_none());
    }

    #[test] fn each_tables_cursor_is_kept() {
        let store=WorkspaceStore::in_memory().expect("store");
        let chats=table("chats").unwrap();
        assert_eq!(cursor(store.connection(),chats).expect("cursor"),None);
        set_cursor(store.connection(),chats,"2026-09-30T12:00:00Z").expect("grava");
        assert_eq!(cursor(store.connection(),chats).expect("cursor").as_deref(),Some("2026-09-30T12:00:00Z"));
        assert_eq!(cursor(store.connection(),table("projects").unwrap()).expect("cursor"),None);
    }
}
