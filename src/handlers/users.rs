//! Usuários (somente admin): listar, criar, trocar papel, desativar e redefinir senha.
//! Toda mudança aumenta `token_version`: a pessoa afetada é deslogada na hora.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    db,
    security::{
        auth::{Usuario, hash_password_async},
        error::{ApiError, bad_request, conflito, internal, nao_encontrado},
        state::AppState,
        validation,
    },
};

#[derive(Serialize, sqlx::FromRow)]
pub struct UsuarioItem {
    pub id: String,
    pub email: String,
    pub role: String,
    pub active: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

pub async fn listar(State(state): State<AppState>) -> Result<Json<Vec<UsuarioItem>>, ApiError> {
    let lista = sqlx::query_as::<_, UsuarioItem>(
        "SELECT id, email, role, active, created_at FROM users ORDER BY email",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(lista))
}

#[derive(Deserialize)]
pub struct CriarReq {
    pub email: String,
    pub password: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct IdRes {
    pub id: String,
}

/// POST /api/users (e o nome antigo, POST /api/auth/register)
pub async fn criar(State(state): State<AppState>, Json(p): Json<CriarReq>) -> Result<Json<IdRes>, ApiError> {
    let email = validation::email(&p.email)?;
    validation::senha(&p.password)?;
    validation::papel(&p.role)?;
    let hash = hash_password_async(p.password).await?;
    let id = db::novo_id();
    sqlx::query("INSERT INTO users (id, email, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&email)
        .bind(hash)
        .bind(&p.role)
        .execute(&state.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => conflito("E-mail já cadastrado."),
            _ => internal(e),
        })?;
    Ok(Json(IdRes { id }))
}

#[derive(Deserialize)]
pub struct AlterarReq {
    pub role: Option<String>,
    pub active: Option<bool>,
}

/// PUT /api/users/{id}: muda papel e/ou ativo. Nunca deixa o sistema sem admin ativo.
pub async fn alterar(
    State(state): State<AppState>,
    Extension(eu): Extension<Usuario>,
    Path(id): Path<String>,
    Json(p): Json<AlterarReq>,
) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    if let Some(r) = &p.role {
        validation::papel(r)?;
    }
    let mut tx = db::transacao_escrita(&state.pool).await.map_err(internal)?;
    let atual = sqlx::query("SELECT role, active FROM users WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or_else(|| nao_encontrado("Usuário não encontrado."))?;
    let papel_atual: String = atual.get("role");
    let ativo_atual = atual.get::<i64, _>("active") == 1;
    let novo_papel = p.role.clone().unwrap_or_else(|| papel_atual.clone());
    let novo_ativo = p.active.unwrap_or(ativo_atual);

    let perde_admin = papel_atual == "admin" && ativo_atual && (novo_papel != "admin" || !novo_ativo);
    if perde_admin {
        if id == eu.id {
            return Err(bad_request("Você não pode tirar o seu próprio acesso de administrador."));
        }
        let outros: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE role = 'admin' AND active = 1 AND id <> ?",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
        if outros == 0 {
            return Err(bad_request("Precisa sobrar pelo menos um administrador ativo."));
        }
    }
    sqlx::query("UPDATE users SET role = ?, active = ?, token_version = token_version + 1 WHERE id = ?")
        .bind(&novo_papel)
        .bind(novo_ativo as i64)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SenhaReq {
    pub password: String,
}

/// POST /api/users/{id}/password: admin redefine a senha de alguém (que é deslogado).
pub async fn redefinir_senha(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SenhaReq>,
) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    validation::senha(&p.password)?;
    let hash = hash_password_async(p.password).await?;
    let r = sqlx::query("UPDATE users SET password_hash = ?, token_version = token_version + 1 WHERE id = ?")
        .bind(hash)
        .bind(&id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Usuário não encontrado."));
    }
    Ok(StatusCode::NO_CONTENT)
}
