//! Os testes da conta de uso (`crate::usage::store`, no crate `jayv-agents`)
//! usam o banco do workspace inteiro, que mora numa camada acima; por isso
//! ficam no crate do app.

#[cfg(test)]
mod tests {
    use crate::usage::store::*;
    use crate::usage::{Entry, JevMark, Precision, Quota, Scope, Spend};
    use crate::workspace::WorkspaceStore;

    fn spend(source:&str,input:u64,output:u64,precision:Precision)->Spend {
        Spend{input_tokens:input,output_tokens:output,..Spend::new(source,"model-a",precision)}
    }
    fn scoped(project:&str,chat:&str,turn:&str)->Scope { Scope{project_id:Some(project.into()),chat_id:Some(chat.into()),turn_id:Some(turn.into())} }
    fn query(scope:ReportScope)->Query { Query{scope,from:None,to:None,utc_offset_minutes:0} }

    fn store_with_chat()->(WorkspaceStore,String,String) {
        let mut store=WorkspaceStore::in_memory().unwrap();
        let project=store.create_project("demo",None).unwrap();
        let chat=store.create_chat(&project.id,Some("primeiro".into())).unwrap();
        (store,project.id,chat.id)
    }

    /// A conta de uma organização soma só os projetos dela; sem projeto
    /// nenhum, não soma nada (e não cai na conta inteira).
    #[test] fn a_set_of_projects_counts_only_those_projects() {
        let store=WorkspaceStore::in_memory().unwrap();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped("p1","c1","t1"),spend("claude",100,10,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped("p2","c2","t2"),spend("codex",50,5,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped("p3","c3","t3"),spend("codex",7,1,Precision::Reported))).unwrap();
        write(connection,&Entry::Jev(scoped("p1","c1","t1"),JevMark::count("entry:block",1))).unwrap();
        write(connection,&Entry::Jev(scoped("p3","c3","t3"),JevMark::count("entry:block",1))).unwrap();
        let both=report(connection,&query(ReportScope::Projects(vec!["p1".into(),"p2".into()]))).unwrap();
        assert_eq!(both.totals.input_tokens,150);
        assert_eq!(both.by_project.len(),2);
        assert_eq!(both.jev.work.get("entry:block"),Some(&1.0));
        let none=report(connection,&query(ReportScope::Projects(Vec::new()))).unwrap();
        assert_eq!(none.totals.calls,0);
    }

    #[test] fn the_scope_reads_a_set_of_projects_from_the_screen() {
        let scope:ReportScope=serde_json::from_str(r#"{"kind":"projects","id":["p1","p2"]}"#).unwrap();
        assert_eq!(scope,ReportScope::Projects(vec!["p1".into(),"p2".into()]));
    }

    #[test] fn the_scope_separates_chat_project_and_global() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",100,10,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,"outro-chat","t2"),spend("codex",50,5,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(Scope::default(),spend("jev:asking",7,1,Precision::Reported))).unwrap();

        assert_eq!(report(connection,&query(ReportScope::Chat(chat.clone()))).unwrap().totals.input_tokens,100);
        assert_eq!(report(connection,&query(ReportScope::Project(project.clone()))).unwrap().totals.input_tokens,150);
        let global=report(connection,&query(ReportScope::Global)).unwrap();
        assert_eq!(global.totals.input_tokens,157);
        assert_eq!(global.totals.calls,3);
        assert_eq!(global.by_source.len(),3);
        assert_eq!(global.by_project[0].label.as_deref(),Some("demo"));
    }

    /// O custo só soma o que as ferramentas informaram; sem nenhum, a tela
    /// mostra "—" e não zero.
    #[test] fn cost_is_empty_when_no_tool_reported_it() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("codex",10,1,Precision::Reported))).unwrap();
        assert_eq!(report(connection,&query(ReportScope::Global)).unwrap().totals.cost_usd,None);
        write(connection,&Entry::Spend(scoped(&project,&chat,"t2"),Spend{cost_usd:Some(0.25),..spend("claude",10,1,Precision::Reported)})).unwrap();
        assert_eq!(report(connection,&query(ReportScope::Global)).unwrap().totals.cost_usd,Some(0.25));
    }

    #[test] fn mixed_precision_reports_the_estimated_share() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",90,10,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t2"),spend("copilot",20,5,Precision::Estimated))).unwrap();
        let totals=report(connection,&query(ReportScope::Global)).unwrap().totals;
        assert_eq!(totals.estimated_tokens,25);
        assert_eq!(totals.input_tokens+totals.output_tokens,125);
    }

    /// O uso do chat apagado continua no total e no projeto, só sem título.
    #[test] fn a_deleted_chat_keeps_its_usage() {
        let (mut store,project,chat)=store_with_chat();
        write(store.connection(),&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",40,4,Precision::Reported))).unwrap();
        store.delete_chat(&chat).unwrap();
        let global=report(store.connection(),&query(ReportScope::Global)).unwrap();
        assert_eq!(global.totals.input_tokens,40);
        assert_eq!(global.by_chat[0].key,chat);
        assert_eq!(global.by_chat[0].label,None);
    }

    #[test] fn the_period_cuts_by_creation_time() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",40,4,Precision::Reported))).unwrap();
        let future=Query{from:Some("2999-01-01T00:00:00Z".into()),..query(ReportScope::Global)};
        let past=Query{to:Some("2000-01-01T00:00:00Z".into()),..query(ReportScope::Global)};
        assert_eq!(report(connection,&future).unwrap().totals.calls,0);
        assert_eq!(report(connection,&past).unwrap().totals.calls,0);
        let open=Query{from:Some("2000-01-01T00:00:00Z".into()),..query(ReportScope::Global)};
        let day=report(connection,&open).unwrap();
        assert_eq!(day.totals.calls,1);
        assert_eq!(day.daily.len(),1);
    }

    #[test] fn a_repeated_quota_reading_is_dropped() {
        let (store,..)=store_with_chat();
        let connection=store.connection();
        let reading=Quota{agent:"claude".into(),window:"session".into(),used_percent:Some(9.0),resets_at:Some("2026-10-01T21:00:00Z".into()),plan:None};
        assert!(write(connection,&Entry::Quota(reading.clone())).unwrap());
        assert!(!write(connection,&Entry::Quota(reading.clone())).unwrap());
        assert!(write(connection,&Entry::Quota(Quota{used_percent:Some(12.0),..reading})).unwrap());
        let quotas=report(connection,&query(ReportScope::Global)).unwrap().quotas;
        assert_eq!(quotas.len(),1);
        assert_eq!(quotas[0].used_percent,Some(12.0));
    }

    #[test] fn jev_work_and_savings_stay_apart() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Jev(scoped(&project,&chat,"t1"),JevMark::count("entry:block",1))).unwrap();
        write(connection,&Entry::Jev(scoped(&project,&chat,"t1"),JevMark::saved("blocked",300))).unwrap();
        let jev=report(connection,&query(ReportScope::Chat(chat))).unwrap().jev;
        assert_eq!(jev.work.get("entry:block"),Some(&1.0));
        assert_eq!(jev.saved.get("blocked"),Some(&300.0));
        assert!(!jev.work.contains_key("saved_tokens:blocked"));
    }

    /// Os turnos de antes da contagem entram como `legacy`, uma vez só, com o
    /// agente e o modelo da etapa de rota.
    #[test] fn old_turns_are_imported_as_legacy_once() {
        let (mut store,_project,chat)=store_with_chat();
        let turn=store.enqueue_prompt(&chat,"oi",None).unwrap();
        store.record_beat(&turn.id,"route",&serde_json::json!({"kind":"route","provider":"claude","model":"sonnet","reason":"x"})).unwrap();
        store.record_beat(&turn.id,"done",&serde_json::json!({"kind":"done","inputTokens":120,"outputTokens":0,"latencyMs":900})).unwrap();
        let connection=store.connection();
        connection.execute("DELETE FROM app_metadata WHERE key=?1",[LEGACY_MARK]).unwrap();
        ensure(connection).unwrap();
        ensure(connection).unwrap();
        connection.execute("DELETE FROM app_metadata WHERE key=?1",[LEGACY_MARK]).unwrap();
        ensure(connection).unwrap();
        let global=report(connection,&query(ReportScope::Global)).unwrap();
        assert_eq!(global.totals.calls,1);
        assert_eq!(global.totals.estimated_tokens,120);
        assert_eq!(global.by_model[0].key,"claude/sonnet");
    }

    /// Um turno já medido por esta versão, vindo de outra máquina pelo sync,
    /// não entra de novo como `legacy`: ele já tem os registros dele.
    #[test] fn a_metered_turn_is_not_imported_again() {
        let (mut store,_project,chat)=store_with_chat();
        let turn=store.enqueue_prompt(&chat,"oi",None).unwrap();
        store.record_beat(&turn.id,"done",&serde_json::json!({"kind":"done","inputTokens":120,"outputTokens":30,"latencyMs":900,"metered":true})).unwrap();
        let connection=store.connection();
        connection.execute("DELETE FROM app_metadata WHERE key=?1",[LEGACY_MARK]).unwrap();
        ensure(connection).unwrap();
        assert_eq!(report(connection,&query(ReportScope::Global)).unwrap().totals.calls,0);
    }

    #[test] fn the_new_tables_enter_the_outbox() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        connection.execute("DELETE FROM outbox",[]).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",1,1,Precision::Reported))).unwrap();
        write(connection,&Entry::Jev(Scope::default(),JevMark::count("cache_hit",1))).unwrap();
        write(connection,&Entry::Quota(Quota{agent:"codex".into(),window:"week".into(),used_percent:Some(1.0),resets_at:None,plan:None})).unwrap();
        let tables:Vec<String>=connection.prepare("SELECT tbl FROM outbox ORDER BY seq").unwrap().query_map([],|row|row.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(tables,["usage_records","jev_records","quota_snapshots"]);
    }

    #[test] fn the_average_output_counts_turns_not_calls() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        assert_eq!(average_output(connection).unwrap(),0);
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",1,100,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",1,100,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t2"),spend("codex",1,50,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t3"),spend("copilot",1,9000,Precision::Estimated))).unwrap();
        assert_eq!(average_output(connection).unwrap(),125);
    }

    #[test] fn turn_usage_leaves_the_jev_out() {
        let (store,project,chat)=store_with_chat();
        let connection=store.connection();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("claude",30,3,Precision::Reported))).unwrap();
        write(connection,&Entry::Spend(scoped(&project,&chat,"t1"),spend("jev:entry",300,3,Precision::Reported))).unwrap();
        let turns=chat_turns(connection,&chat).unwrap();
        assert_eq!(turns.len(),1);
        assert_eq!(turns[0].input_tokens,30);
        assert!(!turns[0].estimated);
    }
}
