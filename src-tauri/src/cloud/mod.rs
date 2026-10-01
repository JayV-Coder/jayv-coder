//! O único módulo que conhece o Supabase: o endereço do projeto, a chave
//! pública e o que se fala com ele.

pub mod remote;
pub mod session;

pub const PROJECT_URL:&str="https://exvsozyemolrjbjetqww.supabase.co";
/// A chave publicável: vai embutida no build e só identifica o projeto. A
/// secret key nunca entra aqui.
pub const PUBLISHABLE_KEY:&str="sb_publishable_7PeIEX0yh_svIEeNw3Gwew_lEbMVtMm";
pub const ISSUER:&str="https://exvsozyemolrjbjetqww.supabase.co/auth/v1";
pub const AUDIENCE:&str="authenticated";
