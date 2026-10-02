//! O que o build de produção fecha para quem tenta ler o código da tela ou
//! escutar as requisições do núcleo. O build de desenvolvimento fica como era.

use std::time::Duration;

/// Variáveis que abrem um inspetor remoto na webview sem passar pelo app:
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` no
/// Windows e o servidor de inspeção do WebKitGTK no Linux. As demais trocam o
/// navegador ou a pasta de dados que a WebView2 usa.
const INSPECTOR_SWITCHES:&[&str]=&[
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    "WEBKIT_INSPECTOR_SERVER",
    "WEBKIT_INSPECTOR_HTTP_SERVER",
];

/// No build de produção, apaga do ambiente as chaves que ligariam o inspetor.
/// Tem de rodar no começo da partida, antes de qualquer thread ler o ambiente.
pub fn strip_inspector_switches() {
    if cfg!(debug_assertions) {return}
    for name in INSPECTOR_SWITCHES {
        // SAFETY: chamado no começo da partida, antes do Tauri e do tokio
        // subirem qualquer thread que leia o ambiente.
        unsafe { std::env::remove_var(name); }
    }
}

/// O cliente HTTP de todo o núcleo. Só confia nas autoridades embutidas no
/// binário (Mozilla), nunca no repositório do sistema: um proxy que intercepta
/// HTTPS instalando a própria autoridade no sistema (Fiddler, Charles, mitmproxy)
/// é recusado no aperto de mão. Um proxy comum ainda funciona, porque o túnel
/// `CONNECT` passa a conexão cifrada de ponta a ponta. TLS abaixo de 1.2 não entra.
///
/// Por isso o `reqwest` entra no `Cargo.toml` só com `rustls-tls` (as raízes
/// webpki): ligar `rustls-tls-native-roots` ou `native-tls` devolveria a
/// confiança no repositório do sistema.
pub fn http_client(timeout:Duration)->reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(timeout)
        .tls_built_in_webpki_certs(true)
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_http_client_builds() {
        assert!(http_client(Duration::from_secs(5)).build().is_ok());
    }

    #[test]
    fn every_switch_is_a_webview_variable() {
        assert!(INSPECTOR_SWITCHES.iter().all(|name|name.starts_with("WEBVIEW2_")||name.starts_with("WEBKIT_")));
    }
}
