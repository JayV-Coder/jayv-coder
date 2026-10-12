//! O sistema de mods do JayV, como o dos jogos: um catálogo de pacotes (o
//! repositório `JayV-Coder/jayv-coder-mods`, pela função `mods` do Supabase),
//! cada um com nome, versão e autor, que quem usa instala, liga, desliga e
//! desinstala por ambiente.
//!
//! Nada aqui sabe o que um mod faz. Um mod diz, em `contributes`, o que traz
//! para cada **ponto de extensão** do app (`ExtensionPoint`); a camada dona de
//! cada assunto implementa o ponto dela e o registra (`Registry`). Um ponto
//! novo é um tipo novo de funcionalidade que os mods podem trazer, sem mexer
//! aqui.
//!
//! - `catalog`: o manifesto e o cache do catálogo (formato genérico e o antigo).
//! - `point`: o contrato de um ponto de extensão e o registro dos pontos.
//! - `state`: o que o ambiente instalou, ligou e desinstalou.
//! - `resolve`: o que vale no fim — os mods ativos, o que fica desligado, o
//!   que se instala sozinho e a aplicação de um estado novo.
//!
//! O núcleo é Rust compilado no instalador e não executa nada baixado: o ponto
//! é o app sabendo aplicar um tipo de contribuição.

pub mod catalog;
pub mod point;
pub mod resolve;
pub mod state;

pub use catalog::Manifest;
pub use point::{ExtensionPoint, Registry};
pub use state::{Installed, InstalledMods};
