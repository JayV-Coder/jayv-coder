//! O que cada ambiente fez com os mods: os instalados (ligados ou não, com a
//! versão instalada) e os que vêm instalados de fábrica e a pessoa
//! desinstalou, que não voltam sozinhos.
//!
//! Mora em `account_settings` (chave `installedMods`) e sincroniza com o
//! ambiente pela mesma fila das outras configurações: o site lê e grava a mesma
//! linha.
//!
//! Cada máquina guarda, fora da fila, o que já instalou nela (`mod_installs`,
//! com o manifesto da época): é por ele que um mod instalado ou desinstalado
//! em outro lugar — outra máquina, o site — roda aqui o que os pontos dele
//! gravam ou apagam, e que um mod que saiu do catálogo ainda se desinstala. A chave antiga `mods` (as escolhas do app 0.94, recurso → ligado)
//! continua sendo gravada, tirada do estado novo, para o app antigo de outra
//! máquina do mesmo ambiente seguir o que se escolheu aqui; ela só é lida na
//! primeira vez, quando ainda não há estado novo.

use crate::catalog::valid_id;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A chave do estado em `account_settings`.
pub const STATE_KEY:&str="installedMods";
/// A chave das escolhas do app 0.94.
pub const LEGACY_KEY:&str="mods";
/// Quantos mods um ambiente guarda: o catálogo inteiro cabe com folga.
pub const MAX_MODS:usize=500;

/// O que esta máquina já instalou, com o manifesto de quando instalou. Só
/// local: não está entre as tabelas sincronizadas.
pub const LEDGER:&str="
CREATE TABLE IF NOT EXISTS mod_installs (
  id TEXT PRIMARY KEY,
  manifest TEXT NOT NULL,
  installed_at TEXT NOT NULL
);";

/// Um mod instalado.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Installed {
    pub enabled:bool,
    /// A versão do mod quando foi instalado.
    #[serde(default,skip_serializing_if="Option::is_none")] pub version:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub installed_at:Option<String>,
}

impl Installed {
    pub fn on(version:Option<String>)->Self { Self{enabled:true,version,installed_at:None} }
}

/// O estado dos mods de um ambiente.
#[derive(Debug,Clone,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",default)]
pub struct InstalledMods {
    pub mods:BTreeMap<String,Installed>,
    /// Os mods de fábrica que a pessoa desinstalou.
    pub removed:BTreeSet<String>,
}

impl InstalledMods {
    pub fn is_on(&self,id:&str)->bool { self.mods.get(id).is_some_and(|installed|installed.enabled) }

    /// Só ids de mod, com teto; um id instalado não está entre os removidos.
    pub fn cleaned(mut self)->Self {
        self.mods.retain(|id,_|valid_id(id));
        while self.mods.len()>MAX_MODS { let Some(last)=self.mods.keys().next_back().cloned() else { break }; self.mods.remove(&last); }
        self.removed.retain(|id|valid_id(id)&&!self.mods.contains_key(id));
        while self.removed.len()>MAX_MODS { self.removed.pop_last(); }
        self
    }
}

fn setting(connection:&Connection,key:&str)->Result<Option<String>> {
    Ok(connection.query_row("SELECT value FROM account_settings WHERE key=?1",[key],|row|row.get(0)).optional()?)
}

fn put(connection:&Connection,key:&str,value:&str)->Result<()> {
    connection.execute(
        "INSERT INTO account_settings(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        params![key,value,chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// O estado gravado. Nulo: o ambiente ainda não decidiu nada (nunca gravou,
/// ou a linha não se lê) — vale o que vem de fábrica.
pub fn stored(connection:&Connection)->Result<Option<InstalledMods>> {
    Ok(setting(connection,STATE_KEY)?.and_then(|value|serde_json::from_str::<InstalledMods>(&value).ok()).map(InstalledMods::cleaned))
}

/// As escolhas do app 0.94 (`id` → ligado). Ilegível vale como nenhuma.
pub fn legacy(connection:&Connection)->Result<BTreeMap<String,bool>> {
    Ok(setting(connection,LEGACY_KEY)?.and_then(|value|serde_json::from_str(&value).ok()).unwrap_or_default())
}

/// Os mods instalados nesta máquina, com o manifesto de quando instalou. A
/// linha que não se lê sai da conta (como se não estivesse instalada).
pub fn ledger(connection:&Connection)->Result<BTreeMap<String,crate::Manifest>> {
    let mut statement=connection.prepare("SELECT id,manifest FROM mod_installs")?;
    let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?;
    let mut found=BTreeMap::new();
    for row in rows {
        let (id,manifest)=row?;
        if let Some(manifest)=serde_json::from_str::<serde_json::Value>(&manifest).ok().and_then(|value|crate::Manifest::read(&value)) { found.insert(id,manifest); }
    }
    Ok(found)
}

pub fn record(connection:&Connection,manifest:&crate::Manifest)->Result<()> {
    connection.execute(
        "INSERT INTO mod_installs(id,manifest,installed_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET manifest=excluded.manifest",
        params![manifest.id,serde_json::to_string(manifest)?,chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn forget(connection:&Connection,id:&str)->Result<()> {
    connection.execute("DELETE FROM mod_installs WHERE id=?1",[id])?;
    Ok(())
}

/// Grava o estado e, se vier, a chave antiga tirada dele.
pub fn write(connection:&Connection,state:&InstalledMods,legacy:Option<&BTreeMap<String,bool>>)->Result<()> {
    put(connection,STATE_KEY,&serde_json::to_string(state)?)?;
    if let Some(legacy)=legacy { put(connection,LEGACY_KEY,&serde_json::to_string(legacy)?)?; }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;

    pub fn database()->Connection {
        let connection=Connection::open_in_memory().expect("banco");
        connection.execute_batch("CREATE TABLE account_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);").expect("conta");
        connection.execute_batch(LEDGER).expect("registro");
        connection
    }

    #[test] fn the_state_round_trips_and_an_unreadable_one_counts_as_undecided() {
        let connection=database();
        assert!(stored(&connection).expect("lê").is_none());
        let state=InstalledMods{mods:BTreeMap::from([("stats".into(),Installed{enabled:false,version:Some("1.0.0".into()),installed_at:None})]),removed:BTreeSet::from(["ollama".into()])};
        write(&connection,&state,Some(&BTreeMap::from([("stats".into(),false)]))).expect("grava");
        assert_eq!(stored(&connection).expect("lê"),Some(state));
        assert_eq!(legacy(&connection).expect("lê"),BTreeMap::from([("stats".into(),false)]));
        connection.execute("UPDATE account_settings SET value='[1]' WHERE key=?1",[STATE_KEY]).expect("estraga");
        assert!(stored(&connection).expect("lê").is_none());
    }

    #[test] fn the_ledger_keeps_the_manifest_of_each_install() {
        let connection=database();
        let manifest=crate::Manifest::read(&serde_json::json!({"id":"stats","version":"1.0.0","contributes":{"features":["stats"]}})).expect("mod");
        record(&connection,&manifest).expect("grava");
        record(&connection,&manifest).expect("de novo");
        assert_eq!(ledger(&connection).expect("lê").into_iter().collect::<Vec<_>>(),[("stats".to_string(),manifest)]);
        connection.execute("INSERT INTO mod_installs VALUES('x','[]','now')",[]).expect("estraga");
        assert_eq!(ledger(&connection).expect("lê").len(),1,"a linha ilegível não conta");
        forget(&connection,"stats").expect("apaga");
        assert!(ledger(&connection).expect("lê").is_empty());
    }

    #[test] fn only_mod_ids_are_kept_and_installing_undoes_a_removal() {
        let state=InstalledMods{
            mods:BTreeMap::from([("stats".into(),Installed{enabled:true,version:None,installed_at:None}),("Bad id".into(),Installed{enabled:true,version:None,installed_at:None})]),
            removed:BTreeSet::from(["stats".into(),"ollama".into(),"x".into()]),
        }.cleaned();
        assert_eq!(state.mods.keys().collect::<Vec<_>>(),["stats"]);
        assert_eq!(state.removed,BTreeSet::from(["ollama".to_string()]));
        assert!(state.is_on("stats")&&!state.is_on("ollama"));
    }
}
