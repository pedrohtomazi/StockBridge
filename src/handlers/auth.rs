use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::security::{
    auth::{IpCliente, TOKEN_TTL_SECS, Usuario, create_token, hash_password_async, verify_password_async},
    error::{ApiError, bad_request, internal},
    state::AppState,
    validation,
};

#[derive(Deserialize)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginRes {
    pub token: String,
    pub role: String,
    pub email: String,
    pub expires_in: i64,
}

pub fn resposta_login(state: &AppState, id: &str, email: &str, role: &str, versao: i64) -> Result<LoginRes, ApiError> {
    Ok(LoginRes {
        token: create_token(&state.jwt_secret, id, versao).map_err(internal)?,
        role: role.to_string(),
        email: email.to_string(),
        expires_in: TOKEN_TTL_SECS,
    })
}

/// POST /api/auth/login  (pública)
pub async fn login(
    State(state): State<AppState>,
    IpCliente(ip): IpCliente,
    Json(payload): Json<LoginReq>,
) -> Result<Json<LoginRes>, ApiError> {
    let email = payload.email.trim().to_lowercase();
    if email.is_empty() || email.len() > 254 || payload.password.is_empty() || payload.password.len() > 128 {
        return Err(bad_request("Informe e-mail e senha."));
    }
    state.login_permitido(ip, &email)?;

    let linha = sqlx::query("SELECT id, password_hash, role, active, token_version FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
    let hash = match &linha {
        Some(r) => r.get::<String, _>("password_hash"),
        None => (*state.hash_falso).clone(),
    };
    let senha_ok = verify_password_async(payload.password, hash).await?;

    let usuario = match (senha_ok, linha) {
        (true, Some(r)) => r,
        _ => {
            state.registrar_falha_login(ip, &email);
            return Err((StatusCode::UNAUTHORIZED, "E-mail ou senha incorretos.".to_string()));
        }
    };
    state.limpar_falhas_login(ip, &email);
    if usuario.get::<i64, _>("active") != 1 {
        return Err((StatusCode::FORBIDDEN, "Usuário desativado. Fale com o administrador.".to_string()));
    }
    let id: String = usuario.get("id");
    let role: String = usuario.get("role");
    Ok(Json(resposta_login(&state, &id, &email, &role, usuario.get("token_version"))?))
}

#[derive(Serialize)]
pub struct MeRes {
    pub id: String,
    pub email: String,
    pub role: String,
}

/// GET /api/auth/me
pub async fn me(Extension(u): Extension<Usuario>) -> Json<MeRes> {
    Json(MeRes { id: u.id, email: u.email, role: u.role })
}

#[derive(Deserialize)]
pub struct TrocarSenhaReq {
    pub current: String,
    pub new: String,
}

/// POST /api/auth/password: a própria pessoa troca a senha. Derruba os outros logins dela
/// e devolve um token novo para esta aba continuar logada.
pub async fn trocar_senha(
    State(state): State<AppState>,
    Extension(u): Extension<Usuario>,
    Json(p): Json<TrocarSenhaReq>,
) -> Result<Json<LoginRes>, ApiError> {
    validation::senha(&p.new)?;
    let hash_atual: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(&u.id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal)?;
    if !verify_password_async(p.current, hash_atual).await? {
        return Err(bad_request("A senha atual não confere."));
    }
    let novo_hash = hash_password_async(p.new).await?;
    let versao: i64 = sqlx::query_scalar(
        "UPDATE users SET password_hash = ?, token_version = token_version + 1 WHERE id = ? RETURNING token_version",
    )
    .bind(novo_hash)
    .bind(&u.id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(resposta_login(&state, &u.id, &u.email, &u.role, versao)?))
}
