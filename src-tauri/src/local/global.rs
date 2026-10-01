//! `cache.sqlite3`: o que vale para a máquina e não para um usuário — o JWKS
//! para validar sem rede, o último usuário validado, os idiomas, as
//! traduções e os parâmetros do Jev.

use crate::gatekeeper;
use anyhow::{Context, Result};
use jsonwebtoken::jwk::JwkSet;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct LocaleRow { pub id:String, pub name:String, pub rtl:bool, pub position:i64 }

/// Os números da portaria. Cada um vem do cache quando lá está e é válido;
/// faltando ou torto, vale a constante do Rust.
#[derive(Debug,Clone,PartialEq)]
pub struct JevParameters {
    pub scope_demand:[f64;3],
    pub block_margin:f64,
    pub weights:BTreeMap<String,f64>,
    pub scope_levels:[String;3],
    pub noul_line:f64,
}

impl Default for JevParameters {
    fn default()->Self { Self::from_values(&gatekeeper::parameters()) }
}

impl JevParameters {
    pub fn from_values(values:&BTreeMap<String,Value>)->Self {
        let defaults=gatekeeper::parameters();
        let pick=|key:&str|->Value { values.get(key).cloned().unwrap_or_else(||defaults[key].clone()) };
        let fallback=|key:&str|defaults[key].clone();
        let unit=|value:&Value|value.as_f64().filter(|number|(0.0..=1.0).contains(number));
        let demand=|value:Value|->Option<[f64;3]> { let list:Vec<f64>=serde_json::from_value(value).ok()?; let array:[f64;3]=list.try_into().ok()?; array.iter().all(|number|(0.0..=1.0).contains(number)).then_some(array) };
        let levels=|value:Value|->Option<[String;3]> { let list:Vec<String>=serde_json::from_value(value).ok()?; list.try_into().ok() };
        let weights=|value:Value|->Option<BTreeMap<String,f64>> { let map:BTreeMap<String,f64>=serde_json::from_value(value).ok()?; (!map.is_empty() && map.values().all(|weight|*weight>=0.0)).then_some(map) };
        Self {
            scope_demand:demand(pick("scope_demand")).or_else(||demand(fallback("scope_demand"))).expect("padrão de scope_demand"),
            block_margin:unit(&pick("block_margin")).or_else(||unit(&fallback("block_margin"))).expect("padrão de block_margin"),
            weights:weights(pick("weights")).or_else(||weights(fallback("weights"))).expect("padrão de weights"),
            scope_levels:levels(pick("scope_levels")).or_else(||levels(fallback("scope_levels"))).expect("padrão de scope_levels"),
            noul_line:unit(&pick("noul_line")).or_else(||unit(&fallback("noul_line"))).expect("padrão de noul_line"),
        }
    }
}

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
        self.setting("jwks")?.map(|raw|serde_json::from_str(&raw).context("JWKS do cache ilegível")).transpose()
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

    pub fn jev_parameters(&self)->Result<JevParameters> {
        let mut statement=self.connection.prepare("SELECT key,value FROM jev_parameters")?;
        let rows=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?;
        let mut values=BTreeMap::new();
        for row in rows {let (key,value)=row?; if let Ok(value)=serde_json::from_str(&value) {values.insert(key,value);}}
        Ok(JevParameters::from_values(&values))
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

    #[test] fn o_jwks_e_o_ultimo_usuario_sobrevivem_ao_fechamento() {
        let dir=tempfile::tempdir().expect("dir");
        let path=dir.path().join("cache.sqlite3");
        let keys:JwkSet=serde_json::from_str(include_str!("../cloud/testdata/jwks.json")).expect("jwks");
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

    #[test] fn idiomas_e_traducoes_voltam_como_foram_gravados() {
        let mut cache=GlobalCache::in_memory().expect("cache");
        cache.save_locales(&[LocaleRow{id:"en".into(),name:"English".into(),rtl:false,position:1},LocaleRow{id:"ar".into(),name:"العربية".into(),rtl:true,position:0}]).expect("idiomas");
        assert_eq!(cache.locales().expect("idiomas").iter().map(|row|row.id.as_str()).collect::<Vec<_>>(),["ar","en"]);
        let messages=BTreeMap::from([("a".to_string(),json!("A")),("p".to_string(),json!({"one":"1","other":"n"}))]);
        cache.save_translations("en",&messages).expect("traduções");
        assert_eq!(cache.translations("en").expect("traduções"),messages);
        assert!(cache.translations("ar").expect("vazio").is_empty());
    }

    #[test] fn sem_parametros_no_cache_valem_as_constantes() {
        let cache=GlobalCache::in_memory().expect("cache");
        let parameters=cache.jev_parameters().expect("parâmetros");
        assert_eq!(parameters.scope_demand,gatekeeper::SCOPE_DEMAND);
        assert_eq!(parameters.block_margin,gatekeeper::BLOCK_MARGIN);
        assert_eq!(parameters.noul_line,crate::asking::NOUL_LINE);
        assert_eq!(parameters.weights.len(),gatekeeper::WEIGHTS.len());
    }

    /// Um valor torto vindo do painel não derruba a portaria: só ele volta ao
    /// padrão, os outros valem.
    #[test] fn o_valor_valido_substitui_e_o_torto_cai_no_padrao() {
        let mut cache=GlobalCache::in_memory().expect("cache");
        cache.save_jev_parameters(&BTreeMap::from([
            ("block_margin".to_string(),json!(0.3)),
            ("scope_demand".to_string(),json!([0.5,2.0])),
            ("noul_line".to_string(),json!("alto")),
        ])).expect("grava");
        let parameters=cache.jev_parameters().expect("parâmetros");
        assert_eq!(parameters.block_margin,0.3);
        assert_eq!(parameters.scope_demand,gatekeeper::SCOPE_DEMAND);
        assert_eq!(parameters.noul_line,crate::asking::NOUL_LINE);
    }
}
