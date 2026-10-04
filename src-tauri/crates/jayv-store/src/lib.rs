//! O armazenamento local do núcleo do JayV. O crate do app o reexporta com os
//! mesmos nomes (`crate::local`, `crate::cache`, `crate::checkpoint`).

pub mod cache;
pub mod checkpoint;
pub mod local;
