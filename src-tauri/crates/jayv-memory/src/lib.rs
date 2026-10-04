//! A memória, do núcleo do JayV. O crate do app a reexporta com os mesmos
//! nomes (`crate::memory`, `crate::project_memory`).

pub mod memory;
pub mod project_memory;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_base::{firewall, i18n, model};
use jayv_code::rag;
