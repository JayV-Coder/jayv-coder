//! O que vale no fim: o estado que conta (o gravado mais o que vem de
//! fábrica), os mods ativos, os alvos desligados e a aplicação de um estado
//! novo — que roda, nesta máquina, o `install` dos pontos de cada mod que
//! entrou e o `uninstall` dos que saíram.

use crate::catalog::{self, Manifest};
use crate::point::Registry;
use crate::state::{self, Installed, InstalledMods};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

fn by_id(catalog:&[Manifest])->BTreeMap<&str,&Manifest> { catalog.iter().map(|manifest|(manifest.id.as_str(),manifest)).collect() }

/// O estado que conta: o gravado e, para cada mod de fábrica do catálogo que o
/// ambiente não tem nem desinstalou, instalado e ligado. Enquanto nada foi
/// gravado, as escolhas do app 0.94 (`legacy`) valem: o recurso que a pessoa
/// desligou lá chega instalado e desligado.
pub fn effective(stored:Option<&InstalledMods>,catalog:&[Manifest],legacy:&BTreeMap<String,bool>)->InstalledMods {
    let mut state=match stored {
        Some(stored)=>stored.clone(),
        None=>InstalledMods{mods:legacy.iter().map(|(id,on)|(id.clone(),Installed{enabled:*on,version:None,installed_at:None})).collect(),removed:BTreeSet::new()},
    };
    for manifest in catalog.iter().filter(|manifest|manifest.preinstalled) {
        if state.mods.contains_key(&manifest.id)||state.removed.contains(&manifest.id) { continue; }
        state.mods.insert(manifest.id.clone(),Installed::on(manifest.version.clone()));
    }
    with_requirements(with_required(state,catalog),catalog).cleaned()
}

/// O estado com os mods essenciais do catálogo instalados e ligados, seja o
/// que for que se gravou (o app de antes, outra máquina, um estado antigo):
/// ninguém os desliga nem os desinstala.
pub fn with_required(mut state:InstalledMods,catalog:&[Manifest])->InstalledMods {
    for manifest in catalog.iter().filter(|manifest|manifest.required) {
        state.removed.remove(&manifest.id);
        match state.mods.get_mut(&manifest.id) {
            Some(installed)=>installed.enabled=true,
            None=>{ state.mods.insert(manifest.id.clone(),Installed::on(manifest.version.clone())); }
        }
    }
    state
}

/// O estado com as dependências de cada mod instalado também instaladas (e
/// ligadas, quando chegam agora). Instalar um mod puxa o que ele pede.
pub fn with_requirements(mut state:InstalledMods,catalog:&[Manifest])->InstalledMods {
    let by_id=by_id(catalog);
    let mut queue:Vec<String>=state.mods.keys().cloned().collect();
    while let Some(id)=queue.pop() {
        let Some(manifest)=by_id.get(id.as_str()) else { continue };
        for other in &manifest.requires {
            let Some(target)=by_id.get(other.as_str()) else { continue };
            if state.mods.contains_key(other) { continue; }
            state.mods.insert(other.clone(),Installed::on(target.version.clone()));
            state.removed.remove(other);
            queue.push(other.clone());
        }
    }
    state
}

/// Os mods ativos: instalados, ligados, no catálogo, que rodam nesta versão e
/// com todas as dependências ativas. Uma dependência em ciclo não fica ativa.
pub fn active(state:&InstalledMods,catalog:&[Manifest],version:Option<&str>,registry:&Registry)->BTreeSet<String> {
    let by_id=by_id(catalog);
    let mut known:BTreeMap<String,bool>=BTreeMap::new();
    fn visit(id:&str,state:&InstalledMods,by_id:&BTreeMap<&str,&Manifest>,version:Option<&str>,registry:&Registry,known:&mut BTreeMap<String,bool>,open:&mut BTreeSet<String>)->bool {
        if let Some(&answer)=known.get(id) { return answer; }
        if !open.insert(id.to_string()) { return false; }
        let answer=state.is_on(id)&&by_id.get(id).is_some_and(|manifest|{
            registry.usable(manifest,version)&&manifest.requires.iter().all(|other|visit(other,state,by_id,version,registry,known,open))
        });
        open.remove(id);
        known.insert(id.to_string(),answer);
        answer
    }
    catalog.iter().filter(|manifest|visit(&manifest.id,state,&by_id,version,registry,&mut known,&mut BTreeSet::new())).map(|manifest|manifest.id.clone()).collect()
}

/// Os alvos que os mods desligam. Com catálogo: todo alvo que o app já tem,
/// que algum mod do catálogo traz ou que um mod instalado nesta máquina trouxe
/// (`installed`, o registro local) fica desligado quando nenhum mod ativo o
/// traz. Sem catálogo nenhum (offline desde a instalação, servidor sem a
/// função), nada sai do ar por falta dele: só o alvo do app cujo mod — o de
/// mesmo id — a pessoa desligou ou desinstalou.
pub fn disabled(catalog:Option<&[Manifest]>,installed:&[Manifest],state:&InstalledMods,version:Option<&str>,registry:&Registry)->BTreeSet<String> {
    let built_in=registry.built_in();
    let Some(catalog)=catalog else {
        return built_in.into_iter().filter(|id|state.mods.get(id).is_some_and(|installed|!installed.enabled)||state.removed.contains(id)).collect();
    };
    let mut governed=built_in;
    for manifest in catalog.iter().chain(installed) { governed.extend(registry.known_targets(manifest)); }
    let active=active(state,catalog,version,registry);
    let on:BTreeSet<String>=catalog.iter().filter(|manifest|active.contains(&manifest.id)).filter_map(|manifest|registry.targets(manifest)).flatten().collect();
    governed.difference(&on).cloned().collect()
}

/// As escolhas no formato do app 0.94: cada alvo que já vem no app, ligado ou
/// não. O app antigo de outra máquina do ambiente lê só os recursos dele.
pub fn legacy_states(registry:&Registry,disabled:&BTreeSet<String>)->BTreeMap<String,bool> {
    registry.built_in().into_iter().map(|id|{ let on=!disabled.contains(&id); (id,on) }).collect()
}

/// O que esta máquina tem de fazer para chegar ao estado: instalar (ou
/// reinstalar, quando a versão do mod mudou) e desinstalar.
#[derive(Debug,Default,PartialEq)]
pub struct Plan { pub install:Vec<Manifest>, pub uninstall:Vec<Manifest> }

impl Plan {
    pub fn is_empty(&self)->bool { self.install.is_empty()&&self.uninstall.is_empty() }
}

/// O plano desta máquina: o mod instalado e ligado no estado que roda aqui e
/// não está no registro local (ou está numa versão antiga) entra; o que está no
/// registro e saiu do estado sai (com o manifesto de quando entrou, mesmo que
/// ele tenha saído do catálogo). O mod desligado que nunca entrou nesta máquina
/// espera ser ligado: é assim que o que falhou não é tentado a cada volta.
pub fn plan(state:&InstalledMods,catalog:&[Manifest],ledger:&BTreeMap<String,Manifest>,version:Option<&str>,registry:&Registry)->Plan {
    let mut plan=Plan::default();
    for manifest in catalog {
        if !state.is_on(&manifest.id)||!registry.usable(manifest,version) { continue; }
        match ledger.get(&manifest.id) {
            Some(done) if done.version==manifest.version=>{}
            _=>plan.install.push(manifest.clone()),
        }
    }
    for (id,manifest) in ledger { if !state.mods.contains_key(id) { plan.uninstall.push(manifest.clone()); } }
    plan
}

/// O resultado de aplicar um estado: o que ficou gravado e os mods cuja
/// instalação falhou (ficaram instalados e desligados).
#[derive(Debug)]
pub struct Applied { pub state:InstalledMods, pub failed:Vec<(String,String)> }

/// Leva o ambiente de `before` a `after` e grava, numa transação. O mod de
/// fábrica que sai fica entre os removidos, para não voltar sozinho; quem
/// entra traz as dependências. Esta máquina roda o `uninstall` do que saiu e o
/// `install` do que entrou, pelo registro local.
///
/// `strict` é o pedido da tela: a falha de um mod volta como erro e nada se
/// grava. Sem ele (a instalação sozinha no fim do sync), o mod que falha fica
/// instalado e desligado e o resto segue.
pub fn apply(connection:&mut Connection,registry:&Registry,catalog:&[Manifest],before:&InstalledMods,after:InstalledMods,version:Option<&str>,strict:bool)->Result<Applied> {
    let by_id=by_id(catalog);
    let mut state=with_requirements(with_required(after.cleaned(),catalog),catalog);
    let now=chrono::Utc::now().to_rfc3339();
    for id in before.mods.keys().filter(|id|!state.mods.contains_key(*id)) {
        if by_id.get(id.as_str()).is_some_and(|manifest|manifest.preinstalled) { state.removed.insert(id.clone()); }
    }
    for (id,installed) in state.mods.iter_mut() {
        if before.mods.contains_key(id) { continue; }
        installed.installed_at.get_or_insert_with(||now.clone());
        if let Some(manifest)=by_id.get(id.as_str()) { installed.version=manifest.version.clone(); }
    }
    let mut transaction=connection.transaction()?;
    let ledger=state::ledger(&transaction)?;
    let work=plan(&state,catalog,&ledger,version,registry);
    for manifest in &work.uninstall {
        let mut savepoint=transaction.savepoint()?;
        let outcome=registry.parts(manifest).try_for_each(|(point,value)|point.uninstall(&savepoint,manifest,value)).and_then(|()|state::forget(&savepoint,&manifest.id));
        match outcome {
            Ok(())=>savepoint.commit()?,
            Err(error) if strict=>return Err(error).with_context(||format!("uninstall mod {}",manifest.id)),
            // O que não saiu fica no registro: a próxima volta tenta de novo.
            Err(error)=>{ savepoint.rollback()?; eprintln!("mods: desinstalar {}: {error:#}",manifest.id); }
        }
    }
    let mut failed=Vec::new();
    for manifest in &work.install {
        let mut savepoint=transaction.savepoint()?;
        let outcome=registry.parts(manifest).try_for_each(|(point,value)|point.install(&savepoint,manifest,value)).and_then(|()|state::record(&savepoint,manifest));
        match outcome {
            Ok(())=>savepoint.commit()?,
            Err(error) if strict=>return Err(error).with_context(||format!("install mod {}",manifest.id)),
            Err(error)=>{
                savepoint.rollback()?;
                eprintln!("mods: instalar {}: {error:#}",manifest.id);
                failed.push((manifest.id.clone(),format!("{error:#}")));
                // O essencial continua ligado: a próxima volta tenta de novo.
                if !manifest.required { if let Some(installed)=state.mods.get_mut(&manifest.id) { installed.enabled=false; } }
            }
        }
    }
    let state=state.cleaned();
    let left:Vec<Manifest>=state::ledger(&transaction)?.into_values().collect();
    let off=disabled(Some(catalog),&left,&state,version,registry);
    // Gravar de novo o mesmo estado só faria subir a linha outra vez.
    let legacy=legacy_states(registry,&off);
    if state::stored(&transaction)?.as_ref()!=Some(&state)||state::legacy(&transaction)?!=legacy { state::write(&transaction,&state,Some(&legacy))?; }
    transaction.commit()?;
    Ok(Applied{state,failed})
}

/// Tudo que os mods desligam neste banco agora.
pub fn disabled_in(connection:&Connection,registry:&Registry,version:Option<&str>)->Result<BTreeSet<String>> {
    let cached=catalog::cached(connection)?;
    let mods=cached.as_ref().map(|(mods,_)|mods.as_slice()).unwrap_or_default();
    let state=effective(state::stored(connection)?.as_ref(),mods,&state::legacy(connection)?);
    let installed:Vec<Manifest>=state::ledger(connection)?.into_values().collect();
    Ok(disabled(cached.as_ref().map(|(mods,_)|mods.as_slice()),&installed,&state,version,registry))
}

/// A instalação sozinha, no fim de cada volta do sync (a primeira é a do
/// login): grava o estado que conta quando ele muda — os mods de fábrica que o
/// ambiente ainda não tem, as dependências que faltam — e roda nesta máquina o
/// que outra máquina ou o site instalou e desinstalou. Sem catálogo, não há o
/// que instalar. Nulo: nada mudou.
pub fn reconcile(connection:&mut Connection,registry:&Registry,version:Option<&str>)->Result<Option<Applied>> {
    let Some((catalog,_))=catalog::cached(connection)? else { return Ok(None) };
    let stored=state::stored(connection)?;
    let after=effective(stored.as_ref(),&catalog,&state::legacy(connection)?);
    let work=plan(&after,&catalog,&state::ledger(connection)?,version,registry);
    if stored.as_ref()==Some(&after)&&work.is_empty() { return Ok(None); }
    let before=stored.unwrap_or_default();
    apply(connection,registry,&catalog,&before,after,version,false).map(Some)
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::point::tests::{manifest, REGISTRY};
    use crate::point::ExtensionPoint;
    use serde_json::{json, Value};
    use std::sync::Mutex;

    fn on(enabled:bool)->Installed { Installed{enabled,version:None,installed_at:None} }
    fn state(mods:&[(&str,bool)])->InstalledMods { InstalledMods{mods:mods.iter().map(|(id,enabled)|(id.to_string(),on(*enabled))).collect(),removed:BTreeSet::new()} }
    fn ids(set:&BTreeSet<String>)->Vec<&str> { set.iter().map(String::as_str).collect() }
    fn catalog()->Vec<Manifest> { vec![manifest("stats",json!({"features":["stats"]})),manifest("mcp",json!({"features":["mcp"]}))] }

    #[test] fn the_preinstalled_mods_count_until_someone_decides() {
        let mut extra=manifest("pack",json!({"things":["a"]}));
        extra.preinstalled=false;
        let mut all=catalog(); all.push(extra);
        let fresh=effective(None,&all,&BTreeMap::new());
        assert_eq!(fresh.mods.keys().collect::<Vec<_>>(),["mcp","stats"],"os de fábrica entram, o resto não");
        assert!(fresh.is_on("stats")&&fresh.is_on("mcp"));
        // O recurso que a pessoa desligou no app 0.94 chega desligado.
        let migrated=effective(None,&all,&BTreeMap::from([("stats".to_string(),false)]));
        assert!(!migrated.is_on("stats")&&migrated.mods.contains_key("stats"));
        // Quem desinstalou não recebe de volta; o gravado manda.
        let decided=InstalledMods{mods:BTreeMap::new(),removed:BTreeSet::from(["stats".to_string()])};
        let later=effective(Some(&decided),&all,&BTreeMap::from([("mcp".to_string(),false)]));
        assert_eq!(later.mods.keys().collect::<Vec<_>>(),["mcp"]);
        assert!(later.is_on("mcp"),"a chave antiga só vale antes de gravar");
    }

    /// O essencial fica instalado e ligado, mesmo que o estado gravado diga
    /// outra coisa (o app de antes deixava desligar).
    #[test] fn required_mods_stay_installed_and_on() {
        let mut all=catalog();
        for manifest in all.iter_mut() { manifest.required=true; }
        let mut optional=manifest("pack",json!({"things":["a"]}));
        optional.preinstalled=false;
        all.push(optional);
        let stored=InstalledMods{mods:BTreeMap::from([("stats".to_string(),on(false)),("pack".to_string(),on(false))]),removed:BTreeSet::from(["mcp".to_string()])};
        let state=effective(Some(&stored),&all,&BTreeMap::new());
        assert!(state.is_on("stats")&&state.is_on("mcp")&&state.removed.is_empty(),"desligado e desinstalado voltam");
        assert!(!state.is_on("pack"),"o que não é essencial fica como a pessoa deixou");
        assert!(effective(None,&all,&BTreeMap::from([("stats".to_string(),false)])).is_on("stats"),"a escolha do app 0.94 não desliga o essencial");
        assert!(disabled(Some(&all),&[],&state,None,&REGISTRY).iter().all(|id|id=="a"),"nada do essencial sai");
    }

    #[test] fn installing_brings_the_requirements() {
        let mut pack=manifest("pack",json!({"things":["a"]}));
        pack.requires=vec!["mcp".into(),"ghost".into()];
        pack.preinstalled=false;
        let mut all=catalog(); all.push(pack);
        let removed=InstalledMods{mods:BTreeMap::from([("pack".to_string(),on(true))]),removed:BTreeSet::from(["mcp".to_string()])};
        let state=with_requirements(removed,&all);
        assert!(state.is_on("mcp")&&!state.removed.contains("mcp"),"a dependência removida volta, pedida");
        assert!(!state.mods.contains_key("ghost"),"o que não está no catálogo não entra");
    }

    #[test] fn a_mod_is_active_with_its_requirements_and_without_cycles() {
        let mut a=manifest("aa",json!({"things":["a"]})); a.requires=vec!["bb".into()];
        let mut b=manifest("bb",json!({"things":["b"]})); b.requires=vec!["aa".into()];
        let mut c=manifest("cc",json!({"things":["c"]})); c.requires=vec!["mcp".into()];
        let mut all=catalog(); all.extend([a,b,c]);
        let everything=state(&[("aa",true),("bb",true),("cc",true),("mcp",true),("stats",true)]);
        assert_eq!(ids(&active(&everything,&all,None,&REGISTRY)),["cc","mcp","stats"],"o ciclo não fica ativo");
        let without=state(&[("cc",true),("mcp",false),("stats",true)]);
        assert_eq!(ids(&active(&without,&all,None,&REGISTRY)),["stats"],"sem a dependência ligada, o mod para");
    }

    #[test] fn what_no_active_mod_brings_is_turned_off() {
        let mut pack=manifest("pack",json!({"features":["stats"],"things":["a"]}));
        pack.preinstalled=false;
        let mut all=catalog(); all.push(pack);
        let state_of=|mods:&[(&str,bool)]|disabled(Some(&all),&[],&state(mods),None,&REGISTRY);
        assert_eq!(ids(&state_of(&[("stats",true),("mcp",true)])),["a"],"o alvo do mod não instalado sai");
        assert_eq!(ids(&state_of(&[("stats",false),("mcp",true),("pack",true)])),Vec::<&str>::new(),"outro mod ativo traz o mesmo recurso");
        assert_eq!(ids(&state_of(&[("stats",false),("mcp",true)])),["a","stats"]);
        // Fora do catálogo: o alvo do app sai para todo mundo, e o do mod que
        // esta máquina instalou também.
        let gone=manifest("gone",json!({"things":["g"]}));
        assert_eq!(ids(&disabled(Some(&all[..1]),&[gone],&state(&[("stats",true),("mcp",true),("gone",true)]),None,&REGISTRY)),["g","mcp"]);
        // Sem catálogo, vale só a escolha de quem usa, pelo id do alvo.
        let mut removed=state(&[("stats",false)]); removed.removed.insert("mcp".into());
        assert_eq!(ids(&disabled(None,&[],&removed,None,&REGISTRY)),["mcp","stats"]);
        assert!(disabled(None,&[],&InstalledMods::default(),None,&REGISTRY).is_empty(),"sem catálogo nada sai do ar");
        assert_eq!(legacy_states(&REGISTRY,&BTreeSet::from(["stats".to_string()])),BTreeMap::from([("mcp".to_string(),true),("stats".to_string(),false)]));
    }

    #[test] fn each_machine_installs_what_it_can_run_and_has_not_yet() {
        let mut later=manifest("later",json!({"things":["l"]})); later.min_app=Some("9.0.0".into());
        let mut all=catalog(); all.push(later);
        let wanted=state(&[("stats",true),("mcp",false),("later",true)]);
        let done=BTreeMap::from([("stats".to_string(),all[0].clone()),("old".to_string(),manifest("old",json!({"things":["o"]})))]);
        let work=plan(&wanted,&all,&done,Some("0.95.0"),&REGISTRY);
        assert!(work.install.is_empty(),"o desligado espera ser ligado; o do app novo, o app");
        let on=state(&[("stats",true),("mcp",true)]);
        assert_eq!(plan(&on,&all,&done,Some("0.95.0"),&REGISTRY).install.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["mcp"]);
        assert_eq!(work.uninstall.iter().map(|manifest|manifest.id.as_str()).collect::<Vec<_>>(),["old"],"o que saiu do estado sai, mesmo fora do catálogo");
        let mut newer=all[0].clone(); newer.version=Some("1.1.0".into());
        assert_eq!(plan(&wanted,&[newer],&done,Some("0.95.0"),&REGISTRY).install.len(),1,"versão nova reinstala");
    }

    /// Um ponto que anota o que instalou e desinstalou e falha no mod `bad`.
    struct Recorder { log:Mutex<Vec<String>> }
    impl ExtensionPoint for Recorder {
        fn key(&self)->&'static str { "things" }
        fn targets(&self,_manifest:&Manifest,value:&Value)->Option<Vec<String>> { value.as_array()?.iter().map(|item|item.as_str().map(str::to_string)).collect() }
        fn install(&self,connection:&Connection,manifest:&Manifest,_value:&Value)->Result<()> {
            connection.execute("INSERT INTO written(id) VALUES(?1)",[&manifest.id])?;
            if manifest.id=="bad" { anyhow::bail!("no room"); }
            self.log.lock().unwrap().push(format!("+{}",manifest.id)); Ok(())
        }
        fn uninstall(&self,_connection:&Connection,manifest:&Manifest,_value:&Value)->Result<()> { self.log.lock().unwrap().push(format!("-{}",manifest.id)); Ok(()) }
    }

    fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch(catalog::CACHE).expect("cache");
        connection.execute_batch(state::LEDGER).expect("registro");
        connection.execute_batch("CREATE TABLE account_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE written (id TEXT);").expect("conta");
        connection
    }

    fn cache(connection:&Connection,mods:&[Manifest]) {
        let values:Vec<Value>=mods.iter().map(|manifest|serde_json::to_value(manifest).expect("json")).collect();
        assert!(catalog::replace(connection,&json!({"schema":2,"mods":values})).expect("grava"));
    }

    fn written(connection:&Connection)->Vec<String> {
        let mut statement=connection.prepare("SELECT id FROM written ORDER BY id").expect("lê");
        statement.query_map([],|row|row.get(0)).expect("lê").collect::<rusqlite::Result<Vec<String>>>().expect("lê")
    }

    #[test] fn signing_in_installs_the_preinstalled_mods_once() {
        static RECORDER:Recorder=Recorder{log:Mutex::new(Vec::new())};
        static POINTS:Registry=Registry::new(&[&RECORDER]);
        let log=||RECORDER.log.lock().unwrap().clone();
        let mut connection=database();
        assert!(reconcile(&mut connection,&POINTS,None).expect("sem catálogo").is_none());
        let mut later=manifest("later",json!({"things":["l"]})); later.min_app=Some("9.0.0".into());
        let mods=vec![manifest("good",json!({"things":["g"]})),manifest("bad",json!({"things":["b"]})),later];
        cache(&connection,&mods);
        let applied=reconcile(&mut connection,&POINTS,Some("0.95.0")).expect("aplica").expect("mudou");
        assert_eq!(log(),["+good"]);
        assert_eq!(written(&connection),["good"],"o que o mod que falhou gravou volta atrás");
        assert_eq!(applied.failed.iter().map(|(id,_)|id.as_str()).collect::<Vec<_>>(),["bad"]);
        assert!(applied.state.is_on("good")&&!applied.state.is_on("bad")&&applied.state.is_on("later"),"o que falhou fica instalado e desligado");
        assert!(applied.state.mods["good"].installed_at.is_some());
        assert_eq!(state::stored(&connection).expect("lê"),Some(applied.state.clone()));
        assert!(reconcile(&mut connection,&POINTS,Some("0.95.0")).expect("de novo").is_none(),"a segunda volta não muda nada");
        // O app novo roda o que esperava.
        reconcile(&mut connection,&POINTS,Some("9.0.0")).expect("app novo").expect("mudou");
        assert_eq!(log(),["+good","+later"]);
        // Desinstalar em outro lugar (o site): esta máquina roda o uninstall
        // e o mod de fábrica não volta.
        let mut elsewhere=state::stored(&connection).expect("lê").expect("estado");
        elsewhere.mods.remove("good"); elsewhere.removed.insert("good".into());
        state::write(&connection,&elsewhere,None).expect("chega pelo sync");
        reconcile(&mut connection,&POINTS,Some("9.0.0")).expect("desinstala").expect("mudou");
        assert_eq!(log().last().map(String::as_str),Some("-good"));
        assert!(reconcile(&mut connection,&POINTS,Some("9.0.0")).expect("de novo").is_none());
        assert!(!state::ledger(&connection).expect("lê").contains_key("good"));
        // Pela tela, desligar ou desinstalar um essencial não pega.
        let mut essential=mods.clone(); essential[0].required=true;
        let before=state::stored(&connection).expect("lê").expect("estado");
        let mut off=before.clone(); off.mods.insert("good".into(),on(false));
        assert!(apply(&mut connection,&POINTS,&essential,&before,off,Some("9.0.0"),true).expect("grava").state.is_on("good"),"o essencial que tinha saído volta, instalado");
        let mut gone=state::stored(&connection).expect("lê").expect("estado"); gone.mods.remove("good");
        let kept=apply(&mut connection,&POINTS,&essential,&before,gone,Some("9.0.0"),true).expect("grava").state;
        assert!(kept.is_on("good")&&!kept.removed.contains("good"));
        let before=state::stored(&connection).expect("lê").expect("estado");
        let mut forget=before.clone(); forget.mods.remove("good"); forget.removed.insert("good".into());
        apply(&mut connection,&POINTS,&mods,&before,forget,Some("9.0.0"),true).expect("volta ao opcional");
        // Pela tela, a falha volta como erro e nada se grava.
        let before=state::stored(&connection).expect("lê").expect("estado");
        let mut retry=before.clone(); retry.mods.remove("bad");
        apply(&mut connection,&POINTS,&mods,&before,retry,Some("9.0.0"),true).expect("tira o que falhou");
        let before=state::stored(&connection).expect("lê").expect("estado");
        assert!(before.removed.contains("bad"),"o de fábrica que sai não volta sozinho");
        let mut again=before.clone(); again.mods.insert("bad".into(),on(true));
        assert!(apply(&mut connection,&POINTS,&mods,&before,again,Some("9.0.0"),true).is_err());
        assert_eq!(state::stored(&connection).expect("lê").expect("estado"),before);
        assert_eq!(written(&connection),["good","good","later"],"o essencial desinstalado antes voltou a se instalar");
    }
}
