//! Configuração inicial: cria o primeiro administrador pela tela (sem .env).

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    db,
    handlers::auth::{LoginRes, resposta_login},
    security::{
        auth::{IpCliente, hash_password_async},
        error::{ApiError, conflito, internal},
        state::AppState,
        validation,
    },
};

#[derive(Serialize)]
pub struct SetupStatus {
    #[serde(rename = "needsSetup")]
    pub needs_setup: bool,
    /// A página foi aberta no próprio computador servidor?
    pub local: bool,
}

pub async fn status(
    State(state): State<AppState>,
    IpCliente(ip): IpCliente,
) -> Result<Json<SetupStatus>, ApiError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(internal)?;
    Ok(Json(SetupStatus { needs_setup: total == 0, local: ip.is_loopback() }))
}

#[derive(Deserialize)]
pub struct SetupReq {
    pub email: String,
    pub password: String,
}

/// POST /api/setup: só funciona com o banco sem usuários E a partir do próprio servidor.
/// Sem a 2ª regra, qualquer pessoa da rede poderia "chegar antes" e virar admin.
pub async fn criar_admin(
    State(state): State<AppState>,
    IpCliente(ip): IpCliente,
    Json(p): Json<SetupReq>,
) -> Result<Json<LoginRes>, ApiError> {
    if !ip.is_loopback() {
        return Err((
            StatusCode::FORBIDDEN,
            "Faça a configuração inicial no próprio computador servidor.".to_string(),
        ));
    }
    let email = validation::email(&p.email)?;
    validation::senha(&p.password)?;
    let hash = hash_password_async(p.password).await?;

    let mut tx = db::transacao_escrita(&state.pool).await.map_err(internal)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    if total > 0 {
        return Err(conflito("O sistema já foi configurado. Entre com seu usuário."));
    }
    let id = db::novo_id();
    sqlx::query("INSERT INTO users (id, email, password_hash, role) VALUES (?, ?, ?, 'admin')")
        .bind(&id)
        .bind(&email)
        .bind(hash)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    tracing::info!("Administrador inicial criado: {email}");
    Ok(Json(resposta_login(&state, &id, &email, "admin", 0)?))
}
