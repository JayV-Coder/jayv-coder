// As camadas que já são crates, com os nomes de sempre: `crate::i18n` continua
// valendo aqui dentro.
pub use jayv_base::{config, firewall, i18n, lockdown, model, progress};
pub use jayv_store::{cache, checkpoint, local};
pub use jayv_cloud as cloud;
pub use jayv_agents::{agents, llm, providers, router, tools, usage};

pub mod asking;
pub mod bench;
pub mod checkout;
pub mod desktop;
pub mod expertise;
pub mod features;
pub mod context_engine;
pub mod core_settings;
pub mod gatekeeper;
pub mod graph;
pub mod jev;
pub mod layers;
pub mod live_files;
pub mod mcp;
pub mod memory;
pub mod orchestrator;
mod outbox_tests;
mod usage_tests;
pub mod parallel;
pub mod policy;
pub mod project_map;
pub mod project_memory;
pub mod rag;
pub mod repo_keys;
pub mod review;
pub mod search;
pub mod split;
pub mod symbols;
pub mod sync;
pub mod turns;
pub mod workspace;

pub use desktop::run_desktop;
