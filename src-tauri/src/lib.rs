// As camadas que já são crates, com os nomes de sempre: `crate::i18n` continua
// valendo aqui dentro.
pub use jayv_base::{config, environment, firewall, i18n, lockdown, model, progress};
pub use jayv_store::{cache, checkpoint, local};
pub use jayv_cloud as cloud;
pub use jayv_agents::{agents, guard, llm, providers, router, tools, usage};
pub use jayv_jev::{asking, core_settings, expertise, gatekeeper, jev, policy, skill_choice, turns};
pub use jayv_plans::features;
pub use jayv_orgs::{checkout, repo_keys};
pub use jayv_code::{context_engine, graph, project_map, rag, search, symbols};
pub use jayv_memory::{memory, project_memory};
pub use jayv_workspace::{environments, workspace};
pub use jayv_live::live_files;
pub use jayv_orchestration::{orchestrator, parallel, review, split};

pub mod bench;
pub mod desktop;
pub mod layers;
pub mod mcp;
mod outbox_tests;
mod usage_tests;
pub mod sync;

pub use desktop::run_desktop;
