//! O painel "Ao vivo", do núcleo do JayV. O crate do app o reexporta com o
//! mesmo nome (`crate::live_files`).

pub mod live_files;

// As camadas de baixo com os nomes de sempre dentro deste crate.
use jayv_base::firewall;
use jayv_orgs::checkout;
#[cfg(test)] use jayv_base::config;
