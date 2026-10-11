//! Os planos e recursos do núcleo do JayV. O crate do app o reexporta como
//! `crate::features`.

pub mod features;
pub mod mods;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_cloud as cloud;
use jayv_agents::llm;
use jayv_jev::core_settings;
#[cfg(test)]
use jayv_base::config;
