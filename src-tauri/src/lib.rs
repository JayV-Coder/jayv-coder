// As camadas que já são crates, com os nomes de sempre: `crate::i18n` continua
// valendo aqui dentro.
pub use jayv_base::{config, firewall, i18n, lockdown, model, progress};
pub use jayv_store::{cache, checkpoint, local};
pub use jayv_cloud as cloud;
pub use jayv_agents::{agents, llm, providers, router, sandbox, tools, usage};
pub use jayv_jev::{asking, core_settings, expertise, gatekeeper, jev, policy, turns};
pub use jayv_plans::features;
pub use jayv_orgs::{checkout, repo_keys};

pub mod bench;
pub mod desktop;
pub mod context_engine;
pub mod graph;
pub mod layers;
pub mod live_files;
pub mod mcp;
pub mod memory;
pub mod orchestrator;
mod outbox_tests;
mod usage_tests;
pub mod parallel;
pub mod project_map;
pub mod project_memory;
pub mod rag;
pub mod review;
pub mod search;
pub mod split;
pub mod symbols;
pub mod sync;
pub mod workspace;

pub use desktop::run_desktop;
