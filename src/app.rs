//! Rotas da API em 3 grupos de permissão + as telas.

use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    middleware,
    routing::{delete, get, post, put},
};

use crate::{
    handlers::{auth, locations, meli, meli_dados, movements, products, settings, setup, users, warehouses},
    security::{
        auth::{require_admin, require_auth},
        headers::security_headers,
        state::AppState,
    },
    web,
};

/// GET /api/health: confere também o banco (antes respondia "ok" mesmo com o banco fora).
async fn saude(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, "API Rust Operante!"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "Banco de dados indisponível."),
    }
}

async fn api_inexistente() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Rota da API não encontrada.")
}

pub fn router(state: AppState) -> Router {
    // PÚBLICAS (sem login)
    let publicas = Router::new()
        .route("/api/health", get(saude))
        .route("/api/auth/login", post(auth::login))
        .route("/api/setup", get(setup::status).post(setup::criar_admin))
        .route("/api/meli/callback", get(meli::callback)) // protegida pelo `state`
        .route("/api/rede/ca.crt", get(settings::baixar_ca));

    // LOGADO (admin ou operador)
    let logado = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/password", post(auth::trocar_senha))
        .route("/api/products", get(products::listar))
        .route("/api/warehouses", get(warehouses::listar))
        .route("/api/locations", get(locations::listar))
        .route("/api/locations/{id}/stock", get(locations::estoque))
        .route("/api/movements", get(movements::listar).post(movements::criar))
        .route("/api/stock", get(movements::mapa))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // SÓ ADMIN. Os layers rodam do último para o primeiro: require_auth roda PRIMEIRO.
    let admin = Router::new()
        .route("/api/auth/register", post(users::criar)) // nome antigo, mantido
        .route("/api/users", get(users::listar).post(users::criar))
        .route("/api/users/{id}", put(users::alterar))
        .route("/api/users/{id}/password", post(users::redefinir_senha))
        .route("/api/products", post(products::criar))
        .route("/api/products/{id}", put(products::alterar).delete(products::excluir))
        .route("/api/warehouses", post(warehouses::criar))
        .route("/api/warehouses/{id}", put(warehouses::alterar).delete(warehouses::excluir))
        .route("/api/locations", post(locations::criar))
        .route("/api/locations/{id}", put(locations::alterar).delete(locations::excluir))
        .route("/api/meli/authorize", get(meli::authorize))
        .route("/api/meli/status", get(meli::status))
        .route("/api/meli/credentials", delete(meli::desconectar))
        .route("/api/meli/links", get(meli::listar_vinculos).post(meli::criar_vinculo))
        .route("/api/meli/links/{id}", delete(meli::remover_vinculo))
        .route("/api/meli/links/{id}/sync", post(meli::sincronizar_agora))
        .route("/api/meli/anuncios", get(meli_dados::anuncios))
        .route("/api/meli/importar", post(meli_dados::importar).layer(DefaultBodyLimit::max(256 * 1024)))
        .route("/api/meli/rendimento", get(meli_dados::rendimento))
        // Colar do Excel pode passar dos 16 KB padrão.
        .route("/api/movements/lote", post(movements::entrada_em_lote).layer(DefaultBodyLimit::max(512 * 1024)))
        .route("/api/config", get(settings::ler))
        .route("/api/config/meli", put(settings::salvar_meli))
        .route("/api/config/backup", put(settings::salvar_backup))
        .route("/api/config/backup/agora", post(settings::backup_agora))
        .route_layer(middleware::from_fn(require_admin))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Sem CORS: telas e API saem do MESMO endereço, então nenhum outro site é liberado.
    Router::new()
        .merge(publicas)
        .merge(logado)
        .merge(admin)
        .route("/api/{*resto}", get(api_inexistente).post(api_inexistente))
        .fallback(web::servir)
        .layer(DefaultBodyLimit::max(16 * 1024)) // JSON da API é pequeno: recusa corpo gigante
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
