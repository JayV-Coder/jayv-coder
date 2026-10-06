//! Os agentes e modelos do núcleo do JayV. O crate do app os reexporta com os
//! mesmos nomes (`crate::llm`, `crate::providers`, `crate::usage`...).

pub mod agents;
pub mod llm;
pub mod mcp;
pub mod org_extensions;
pub mod providers;
pub mod router;
pub mod skills;
pub mod tools;
pub mod usage;

// A base com os nomes de sempre: `crate::config`, `crate::i18n` etc.
// continuam valendo dentro deste crate.
use jayv_base::{config, i18n, lockdown, model, progress};
