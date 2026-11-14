use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{StatusCode, header, request::Parts},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::security::{
    error::{ApiError, internal},
    state::AppState,
};

/// Duração do login: 8 horas.
pub const TOKEN_TTL_SECS: i64 = 8 * 3600;

/// Dados dentro do token (assinado: não dá para alterar sem o segredo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // id do usuário
    pub ver: i64,    // versão do token do usuário (ver users.token_version)
    pub exp: usize,
}

/// Usuário logado, confirmado no banco a cada requisição.
#[derive(Debug, Clone)]
pub struct Usuario {
    pub id: String,
    pub email: String,
    pub role: String,
}

impl Usuario {
    pub fn eh_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// IP de quem fez a requisição (para o limite de tentativas e para a configuração inicial).
pub struct IpCliente(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for IpCliente {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ip = ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
            .await
            .map(|c| c.0.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        Ok(IpCliente(ip))
    }
}

/// Senha -> hash Argon2id com "sal" aleatório. É lento DE PROPÓSITO (dificulta força bruta).
pub fn hash_password(senha: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(senha.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("erro ao gerar hash: {e}"))
}

pub fn verify_password(senha: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default().verify_password(senha.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// Argon2 é pesado: roda fora do loop principal para não travar o servidor.
pub async fn hash_password_async(senha: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || hash_password(&senha))
        .await
        .map_err(internal)?
        .map_err(internal)
}

pub async fn verify_password_async(senha: String, hash: String) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || verify_password(&senha, &hash))
        .await
        .map_err(internal)
}

pub fn create_token(secret: &[u8], user_id: &str, versao: i64) -> Result<String, String> {
    let claims = Claims {
        sub: user_id.to_string(),
        ver: versao,
        exp: (Utc::now().timestamp() + TOKEN_TTL_SECS) as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))
        .map_err(|e| format!("erro ao criar token: {e}"))
}

/// Middleware: exige "Authorization: Bearer <token>" válido E usuário ativo no banco.
/// Conferir no banco é o que permite derrubar um login: desativar o usuário, trocar a senha
/// ou o papel aumenta `token_version` e todo token antigo deixa de valer na hora.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let nao_autenticado = || (StatusCode::UNAUTHORIZED, "Não autenticado.".to_string());
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(nao_autenticado)?;

    let dados = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_slice()),
        &Validation::default(),
    )
    .map_err(|_| (StatusCode::UNAUTHORIZED, "Sessão expirada. Entre novamente.".to_string()))?;

    let linha = sqlx::query("SELECT email, role, active, token_version FROM users WHERE id = ?")
        .bind(&dados.claims.sub)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
    let encerrada = || (StatusCode::UNAUTHORIZED, "Sessão encerrada. Entre novamente.".to_string());
    let linha = linha.ok_or_else(encerrada)?;
    if linha.get::<i64, _>("active") != 1 || linha.get::<i64, _>("token_version") != dados.claims.ver {
        return Err(encerrada());
    }

    req.extensions_mut().insert(Usuario {
        id: dados.claims.sub,
        email: linha.get("email"),
        role: linha.get("role"), // papel ATUAL do banco, não o da hora do login
    });
    Ok(next.run(req).await)
}

/// Middleware: só deixa passar quem é "admin". Precisa rodar DEPOIS do `require_auth`.
pub async fn require_admin(req: Request, next: Next) -> Result<Response, ApiError> {
    match req.extensions().get::<Usuario>().map(Usuario::eh_admin) {
        Some(true) => Ok(next.run(req).await),
        Some(false) => Err((StatusCode::FORBIDDEN, "Sem permissão para esta ação.".to_string())),
        None => Err((StatusCode::UNAUTHORIZED, "Não autenticado.".to_string())),
    }
}
