//! As organizações no disco, do núcleo do JayV. O crate do app as reexporta
//! com os mesmos nomes (`crate::checkout`, `crate::repo_keys`).

pub mod checkout;
pub mod repo_keys;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_agents::providers;
use jayv_base::i18n;
