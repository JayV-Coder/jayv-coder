//! O Jev do núcleo do JayV. O crate do app o reexporta com os mesmos nomes
//! (`crate::gatekeeper`, `crate::jev`, `crate::turns`...).

pub mod asking;
pub mod core_settings;
pub mod expertise;
pub mod gatekeeper;
pub mod jev;
pub mod policy;
pub mod skill_choice;
pub mod turns;

// As camadas de baixo com os nomes de sempre: `crate::config`, `crate::llm`
// etc. continuam valendo dentro deste crate.
use jayv_agents::{llm, providers, usage};
use jayv_base::{config, firewall, i18n, lockdown, progress};
use jayv_cloud as cloud;
#[cfg(test)]
use jayv_store::local;
