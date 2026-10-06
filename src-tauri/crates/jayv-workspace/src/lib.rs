//! O workspace, do núcleo do JayV. O crate do app o reexporta com o mesmo
//! nome (`crate::workspace`).

pub mod workspace;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_agents::{llm, mcp, usage};
use jayv_base::{i18n, model};
use jayv_code::search;
use jayv_jev::{core_settings, expertise, gatekeeper, policy, turns};
use jayv_memory::{memory, project_memory};
use jayv_orgs::repo_keys;
use jayv_plans::features;
use jayv_store::local;
