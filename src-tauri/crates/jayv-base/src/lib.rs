//! A camada de baixo do núcleo do JayV. O crate do app a reexporta com os
//! mesmos nomes (`crate::i18n`, `crate::config`...), então quem a usa não
//! precisa saber que ela mora aqui.

pub mod config;
pub mod firewall;
pub mod i18n;
pub mod lockdown;
pub mod model;
pub mod progress;
