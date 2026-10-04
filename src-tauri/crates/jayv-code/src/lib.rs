//! O código dos projetos, do núcleo do JayV. O crate do app o reexporta com os
//! mesmos nomes (`crate::rag`, `crate::symbols` e os demais).

pub mod context_engine;
pub mod graph;
pub mod project_map;
pub mod rag;
pub mod search;
pub mod symbols;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_base::{firewall, model};
use jayv_orgs::checkout;
#[cfg(test)] use jayv_base::config;
