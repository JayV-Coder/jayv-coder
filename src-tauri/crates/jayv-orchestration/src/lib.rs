//! A orquestração, a camada de cima do núcleo do JayV. O crate do app a
//! reexporta com os mesmos nomes (`crate::orchestrator`, `crate::parallel`,
//! `crate::split`, `crate::review`, `crate::snapshot`, `crate::validator`).

pub mod orchestrator;
pub mod parallel;
pub mod review;
pub mod snapshot;
pub mod split;
pub mod validator;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_agents::{agents, llm, mcp, providers, router, usage};
use jayv_base::{config, firewall, i18n, model, progress};
use jayv_code::{context_engine, graph, project_map, rag, symbols};
use jayv_jev::{core_settings, expertise, jev, skill_choice};
#[cfg(test)] use jayv_jev::policy;
use jayv_live::live_files;
use jayv_memory::memory;
use jayv_store::cache;
use jayv_workspace::workspace;
