//! Telas (HTML/CSS/JS) embutidas no executável.
//! Em desenvolvimento (`cargo run`), os arquivos são lidos direto da pasta `web/`:
//! basta recarregar o navegador para ver uma mudança, sem recompilar.

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/"]
#[exclude = "*.md"]
#[exclude = "*.py"]
#[exclude = "*.txt"]
struct Telas;

pub async fn servir(uri: Uri) -> Response {
    let mut caminho = uri.path().trim_start_matches('/').to_string();
    if caminho.is_empty() || caminho.ends_with('/') {
        caminho.push_str("index.html");
    }
    match Telas::get(&caminho) {
        Some(arquivo) => {
            let tipo = arquivo.metadata.mimetype();
            let tipo = if tipo.starts_with("text/") || tipo.contains("javascript") {
                format!("{tipo}; charset=utf-8")
            } else {
                tipo.to_string()
            };
            ([(header::CONTENT_TYPE, tipo)], Body::from(arquivo.data.into_owned())).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Página não encontrada.").into_response(),
    }
}
