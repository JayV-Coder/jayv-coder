// As camadas que já são crates, com os nomes de sempre: `crate::i18n` continua
// valendo aqui dentro.
pub use jayv_base::{config, firewall, i18n, lockdown, model, progress};
pub use jayv_store::{cache, checkpoint, local};

pub mod agents;
pub mod asking;
pub mod bench;
pub mod checkout;
pub mod cloud;
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
pub mod llm;
pub mod mcp;
pub mod memory;
pub mod orchestrator;
mod outbox_tests;
pub mod parallel;
pub mod policy;
pub mod project_map;
pub mod project_memory;
pub mod providers;
pub mod rag;
pub mod repo_keys;
pub mod review;
pub mod router;
pub mod search;
pub mod split;
pub mod symbols;
pub mod sync;
pub mod tools;
pub mod turns;
pub mod usage;
pub mod workspace;

pub use desktop::run_desktop;
