//! `cache.sqlite3`: o que vale para a máquina e não para um usuário — o JWKS
//! para validar sem rede, o último usuário validado, os idiomas, as
//! traduções e os parâmetros do Jev.

use anyhow::{Context, Result};
use jsonwebtoken::jwk::JwkSet;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct LocaleRow { pub id:String, pub name:String, pub rtl:bool, pub position:i64 }

pub struct GlobalCache { connection:Connection }

impl GlobalCache {
    pub fn open(path:&Path)->Result<Self> {
        if let Some(parent)=path.parent(){fs::create_dir_all(parent).with_context(||format!("could not create {}",parent.display()))?;}
        Self::prepare(Connection::open(path).with_context(||format!("could not open {}",path.display()))?)
    }

    pub fn in_memory()->Result<Self> { Self::prepare(Connection::open_in_memory()?) }

    fn prepare(connection:Connection)->Result<Self> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS locales (id TEXT PRIMARY KEY, name TEXT NOT NULL, rtl INTEGER NOT NULL, position INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS translations (locale TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY (locale, key));
             CREATE TABLE IF NOT EXISTS jev_parameters (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Self{connection})
    }

    fn setting(&self,key:&str)->Result<Option<String>> {
        Ok(self.connection.query_row("SELECT value FROM settings WHERE key=?1",[key],|row|row.get(0)).optional()?)
    }

    fn set_setting(&self,key:&str,value:Option<&str>)->Result<()> {
        match value {
            Some(value)=>self.connection.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[key,value])?,
            None=>self.connection.execute("DELETE FROM settings WHERE key=?1",[key])?,
        };
        Ok(())
    }

    pub fn jwks(&self)->Result<Option<JwkSet>> {
        self.setting("jwks")?.map(|raw|serde_json::from_str(&raw).context("unreadable cached JWKS")).transpose()
    }

    pub fn save_jwks(&self,keys:&JwkSet)->Result<()> { self.set_setting("jwks",Some(&serde_json::to_string(keys)?)) }

    pub fn last_user(&self)->Result<Option<String>> { self.setting("last_user") }

    pub fn set_last_user(&self,user_id:Option<&str>)->Result<()> { self.set_setting("last_user",user_id) }

    pub fn locales(&self)->Result<Vec<LocaleRow>> {
        let mut statement=self.connection.prepare("SELECT id,name,rtl,position FROM locales ORDER BY position,id")?;
        Ok(statement.query_map([],|row|Ok(LocaleRow{id:row.get(0)?,name:row.get(1)?,rtl:row.get(2)?,position:row.get(3)?}))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Troca a lista inteira: um idioma que saiu do Supabase sai daqui também.
    pub fn save_locales(&mut self,rows:&[LocaleRow])->Result<()> {
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM locales",[])?;
        for row in rows {transaction.execute("INSERT INTO locales(id,name,rtl,position) VALUES(?1,?2,?3,?4)",params![row.id,row.name,row.rtl,row.position])?;}
        transaction.commit()?;
        Ok(())
    }

    /// As mensagens de um idioma: texto, ou as formas de plural.
    pub fn translations(&self,locale:&str)->Result<BTreeMap<String,Value>> {
        let mut statement=self.connection.prepare("SELECT key,value FROM translations WHERE locale=?1")?;
        let rows=statement.query_map([locale],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?;
        rows.map(|row|{let (key,value)=row?; Ok((key,serde_json::from_str(&value)?))}).collect()
    }

    pub fn save_translations(&mut self,locale:&str,rows:&BTreeMap<String,Value>)->Result<()> {
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM translations WHERE locale=?1",[locale])?;
        for (key,value) in rows {transaction.execute("INSERT INTO translations(locale,key,value) VALUES(?1,?2,?3)",params![locale,key,value.to_string()])?;}
        transaction.commit()?;
        Ok(())
    }

    /// Os valores crus que o painel mandou; quem os lê é
    /// `gatekeeper::JevParameters::from_values`.
    pub fn jev_parameter_values(&self)->Result<BTreeMap<String,Value>> {
        let mut statement=self.connection.prepare("SELECT key,value FROM jev_parameters")?;
        let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?;
        let mut values=BTreeMap::new();
        for row in rows {let (key,value)=row?; if let Ok(value)=serde_json::from_str(&value) {values.insert(key,value);}}
        Ok(values)
    }

    pub fn save_jev_parameters(&mut self,values:&BTreeMap<String,Value>)->Result<()> {
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM jev_parameters",[])?;
        for (key,value) in values {transaction.execute("INSERT INTO jev_parameters(key,value) VALUES(?1,?2)",params![key,value.to_string()])?;}
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn the_jwks_and_last_user_survive_closing() {
        let dir=tempfile::tempdir().expect("dir");
        let path=dir.path().join("cache.sqlite3");
        let keys:JwkSet=serde_json::from_str(include_str!("../../../jayv-cloud/src/testdata/jwks.json")).expect("jwks");
        {
            let cache=GlobalCache::open(&path).expect("cache");
            assert!(cache.jwks().expect("jwks").is_none());
            cache.save_jwks(&keys).expect("grava");
            cache.set_last_user(Some("u1")).expect("usuário");
        }
        let cache=GlobalCache::open(&path).expect("reabre");
        assert_eq!(cache.jwks().expect("jwks").expect("guardado").keys.len(),1);
        assert_eq!(cache.last_user().expect("usuário").as_deref(),Some("u1"));
        cache.set_last_user(None).expect("logout");
        assert_eq!(cache.last_user().expect("usuário"),None);
    }

    #[test] fn locales_and_translations_come_back_as_stored() {
        let mut cache=GlobalCache::in_memory().expect("cache");
        cache.save_locales(&[LocaleRow{id:"en".into(),name:"English".into(),rtl:false,position:1},LocaleRow{id:"ar".into(),name:"العربية".into(),rtl:true,position:0}]).expect("idiomas");
        assert_eq!(cache.locales().expect("idiomas").iter().map(|row|row.id.as_str()).collect::<Vec<_>>(),["ar","en"]);
        let messages=BTreeMap::from([("a".to_string(),json!("A")),("p".to_string(),json!({"one":"1","other":"n"}))]);
        cache.save_translations("en",&messages).expect("traduções");
        assert_eq!(cache.translations("en").expect("traduções"),messages);
        assert!(cache.translations("ar").expect("vazio").is_empty());
    }
}
