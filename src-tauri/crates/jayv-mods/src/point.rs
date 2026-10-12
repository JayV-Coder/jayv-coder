//! Os pontos de extensão: os lugares do app onde um mod pode acrescentar
//! alguma coisa. Cada camada implementa os pontos do assunto dela (os recursos
//! do plano, os motores de LLM, os agentes…) e o núcleo os junta num
//! `Registry`. Um mod que usa um ponto que esta versão não conhece fica
//! esperando um app mais novo, como o que pede um `minApp` maior.
//!
//! O que um ponto entrega é um conjunto de **alvos**: ids que ficam ligados
//! enquanto um mod que os traz está instalado e ligado (um recurso, um motor, um
//! agente). O alvo que algum mod do catálogo traz, ou que já vem no app
//! (`built_in`), fica desligado quando nenhum mod ativo o traz — é assim que
//! desinstalar, desligar ou tirar um mod do catálogo desliga o que ele trazia.

use crate::catalog::{fits, Manifest};
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;

pub trait ExtensionPoint:Send+Sync {
    /// A chave do ponto em `contributes`.
    fn key(&self)->&'static str;

    /// Os alvos que o mod traz por este ponto. Nulo quando esta versão não lê
    /// a contribuição: o mod espera um app mais novo.
    fn targets(&self,manifest:&Manifest,value:&Value)->Option<Vec<String>>;

    /// Os alvos que o app já tem sem mod nenhum. Com um catálogo conhecido,
    /// cada um só fica ligado se um mod ativo o traz; sem catálogo, vale a
    /// escolha de quem usa.
    fn built_in(&self)->Vec<String> { Vec::new() }

    /// O que instalar o mod grava. A maioria dos pontos não grava nada — o mod
    /// estar ligado basta —; o de agentes cria os agentes de quem instalou.
    /// Roda em cada máquina, uma vez, quando o mod entra nela (e de novo
    /// quando a versão do mod muda): precisa aceitar rodar de novo sem estragar
    /// nada — o que já existe fica. Roda dentro de uma transação.
    fn install(&self,_connection:&Connection,_manifest:&Manifest,_value:&Value)->Result<()> { Ok(()) }

    /// O que desinstalar o mod apaga. `manifest` é o de quando ele foi
    /// instalado nesta máquina.
    fn uninstall(&self,_connection:&Connection,_manifest:&Manifest,_value:&Value)->Result<()> { Ok(()) }
}

/// Os pontos de extensão que esta versão do app sabe aplicar.
pub struct Registry { points:&'static [&'static dyn ExtensionPoint] }

impl Registry {
    pub const fn new(points:&'static [&'static dyn ExtensionPoint])->Self { Self{points} }

    pub fn get(&self,key:&str)->Option<&'static dyn ExtensionPoint> { self.points.iter().copied().find(|point|point.key()==key) }

    /// As chaves dos pontos, na ordem do registro.
    pub fn keys(&self)->Vec<&'static str> { self.points.iter().map(|point|point.key()).collect() }

    /// Os alvos que já vêm no app.
    pub fn built_in(&self)->BTreeSet<String> { self.points.iter().flat_map(|point|point.built_in()).collect() }

    /// Todos os alvos que o mod traz, ou nulo quando ele usa um ponto que esta
    /// versão não conhece ou não lê.
    pub fn targets(&self,manifest:&Manifest)->Option<BTreeSet<String>> {
        let mut found=BTreeSet::new();
        for (key,value) in &manifest.contributes { found.extend(self.get(key)?.targets(manifest,value)?); }
        Some(found)
    }

    /// Os alvos que o mod traz pelos pontos que esta versão lê — mesmo que ele
    /// não rode aqui por causa de outro ponto: são alvos que o catálogo
    /// governa.
    pub fn known_targets(&self,manifest:&Manifest)->BTreeSet<String> {
        manifest.contributes.iter().filter_map(|(key,value)|self.get(key)?.targets(manifest,value)).flatten().collect()
    }

    /// O mod roda nesta versão: o `minApp` cabe e todo ponto dele se lê.
    pub fn usable(&self,manifest:&Manifest,version:Option<&str>)->bool { fits(manifest,version)&&self.targets(manifest).is_some() }

    /// Os pontos do mod que esta versão conhece, com a contribuição de cada um.
    pub fn parts<'a>(&self,manifest:&'a Manifest)->impl Iterator<Item=(&'static dyn ExtensionPoint,&'a Value)> {
        manifest.contributes.iter().filter_map(|(key,value)|Some((self.get(key)?,value)))
    }
}

#[cfg(test)] pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// Um ponto de mentira: cada id da lista é um alvo; `built` já vem no app.
    pub struct Listed { pub key:&'static str, pub built:&'static [&'static str] }

    impl ExtensionPoint for Listed {
        fn key(&self)->&'static str { self.key }
        fn targets(&self,_manifest:&Manifest,value:&Value)->Option<Vec<String>> {
            value.as_array()?.iter().map(|item|item.as_str().map(str::to_string)).collect()
        }
        fn built_in(&self)->Vec<String> { self.built.iter().map(|id|id.to_string()).collect() }
    }

    pub static FEATURES:Listed=Listed{key:"features",built:&["stats","mcp"]};
    pub static THINGS:Listed=Listed{key:"things",built:&[]};
    pub static REGISTRY:Registry=Registry::new(&[&FEATURES,&THINGS]);

    pub fn manifest(id:&str,contributes:Value)->Manifest {
        Manifest::read(&json!({"id":id,"version":"1.0.0","minApp":"0.95.0","preinstalled":true,"contributes":contributes})).expect("mod")
    }

    #[test] fn a_mod_runs_only_when_this_version_knows_all_its_points() {
        let both=manifest("pack",json!({"features":["stats"],"things":["a"]}));
        assert_eq!(REGISTRY.targets(&both),Some(BTreeSet::from(["stats".to_string(),"a".to_string()])));
        assert!(REGISTRY.usable(&both,Some("0.95.0")));
        assert!(!REGISTRY.usable(&both,Some("0.94.0")),"pede um app mais novo");
        let future=manifest("future",json!({"features":["mcp"],"themes":["dark"]}));
        assert!(REGISTRY.targets(&future).is_none()&&!REGISTRY.usable(&future,None));
        assert_eq!(REGISTRY.known_targets(&future),BTreeSet::from(["mcp".to_string()]),"o que se lê continua governado");
        let broken=manifest("broken",json!({"things":"a"}));
        assert!(!REGISTRY.usable(&broken,None),"a contribuição que não se lê também espera");
        assert_eq!(REGISTRY.keys(),["features","things"]);
        assert_eq!(REGISTRY.built_in(),BTreeSet::from(["stats".to_string(),"mcp".to_string()]));
    }
}
