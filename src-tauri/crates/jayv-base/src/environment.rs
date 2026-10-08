//! O ambiente em que o app está: o pessoal ou o de uma organização. Cada um
//! tem o seu banco local, as suas configurações e os seus dados; o texto
//! (`personal` ou o id da organização) é o mesmo `environment_id` do Supabase.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// O texto do ambiente pessoal.
pub const PERSONAL:&str="personal";

#[derive(Debug,Clone,PartialEq,Eq,Hash,Default)]
pub enum Environment {
    #[default]
    Personal,
    Organization(Uuid),
}

impl Environment {
    /// Lê o texto do servidor ou da tela. Só `personal` e um UUID valem: o
    /// texto vira nome de arquivo e filtro de consulta.
    pub fn parse(text:&str)->Option<Self> {
        let text=text.trim();
        if text==PERSONAL {return Some(Self::Personal);}
        Uuid::parse_str(text).ok().map(Self::Organization)
    }

    pub fn is_personal(&self)->bool { matches!(self,Self::Personal) }

    /// O texto que o Supabase guarda em `environment_id`.
    pub fn id(&self)->String {
        match self {Self::Personal=>PERSONAL.to_string(),Self::Organization(org)=>org.hyphenated().to_string()}
    }

    /// O banco do usuário neste ambiente. O pessoal mantém o nome de sempre, então
    /// quem já usa o app não perde nada; cada organização ganha o seu arquivo.
    pub fn database_name(&self,user:&Uuid)->String {
        match self {
            Self::Personal=>format!("workspace-{}.sqlite3",user.hyphenated()),
            Self::Organization(org)=>format!("workspace-{}-{}.sqlite3",user.hyphenated(),org.hyphenated()),
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self,formatter:&mut fmt::Formatter<'_>)->fmt::Result { formatter.write_str(&self.id()) }
}

impl Serialize for Environment {
    fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> { serializer.serialize_str(&self.id()) }
}

impl<'de> Deserialize<'de> for Environment {
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
        let text=String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(||serde::de::Error::custom(format!("invalid environment: `{text}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG:&str="11111111-1111-1111-1111-111111111111";
    const USER:&str="22222222-2222-2222-2222-222222222222";

    #[test] fn only_personal_and_an_id_are_environments() {
        assert_eq!(Environment::parse("personal"),Some(Environment::Personal));
        assert_eq!(Environment::parse(&format!(" {ORG} ")),Some(Environment::Organization(Uuid::parse_str(ORG).unwrap())));
        assert_eq!(Environment::parse("../etc"),None);
        assert_eq!(Environment::parse(""),None);
        assert_eq!(Environment::parse("acme"),None);
    }

    #[test] fn the_personal_database_keeps_its_old_name() {
        let user=Uuid::parse_str(USER).unwrap();
        assert_eq!(Environment::Personal.database_name(&user),format!("workspace-{USER}.sqlite3"));
        assert_eq!(Environment::parse(ORG).unwrap().database_name(&user),format!("workspace-{USER}-{ORG}.sqlite3"));
    }

    #[test] fn the_text_round_trips_through_serde() {
        let org=Environment::parse(ORG).unwrap();
        assert_eq!(serde_json::to_string(&org).unwrap(),format!("\"{ORG}\""));
        assert_eq!(serde_json::from_str::<Environment>("\"personal\"").unwrap(),Environment::Personal);
        assert!(serde_json::from_str::<Environment>("\"x\"").is_err());
    }
}
