// As camadas que já são crates, com os nomes de sempre: `crate::i18n` continua
// valendo aqui dentro.
pub use jayv_base::{config, firewall, i18n, lockdown, model, progress};
pub use jayv_store::{cache, checkpoint, local};
pub use jayv_cloud as cloud;
pub use jayv_agents::{agents, llm, providers, router, tools, usage};
pub use jayv_jev::{asking, core_settings, expertise, gatekeeper, jev, policy, turns};
pub use jayv_plans::features;
pub use jayv_orgs::{checkout, repo_keys};
pub use jayv_code::{context_engine, graph, project_map, rag, search, symbols};
pub use jayv_memory::{memory, project_memory};

pub mod bench;
pub mod desktop;
pub mod layers;
pub mod live_files;
pub mod mcp;
pub mod orchestrator;
mod outbox_tests;
mod usage_tests;
pub mod parallel;
pub mod review;
pub mod split;
pub mod sync;
pub mod workspace;

pub use desktop::run_desktop;
