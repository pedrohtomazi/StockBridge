//! Tela de configuração (admin): credenciais do app do Mercado Livre, backup e rede.

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    backup, db,
    security::{
        error::{ApiError, bad_request, internal},
        state::AppState,
        validation,
    },
    services::meli,
};

#[derive(Serialize)]
pub struct ConfigMeli {
    #[serde(rename = "appId")]
    pub app_id: String,
    /// O Secret nunca volta para a tela: só se está preenchido.
    #[serde(rename = "secretConfigured")]
    pub secret_configured: bool,
    #[serde(rename = "redirectUri")]
    pub redirect_uri: String,
}

#[derive(Serialize)]
pub struct ConfigBackup {
    pub pasta: String,
    #[serde(rename = "pastaPadrao")]
    pub pasta_padrao: String,
    pub personalizada: bool,
    pub ultimo: Option<String>,
    #[serde(rename = "ultimoErro")]
    pub ultimo_erro: Option<String>,
}

#[derive(Serialize)]
pub struct ConfigRede {
    pub nome: String,
    pub porta: u16,
    pub enderecos: Vec<String>,
    #[serde(rename = "caFingerprint")]
    pub ca_fingerprint: String,
}

#[derive(Serialize)]
pub struct ConfigRes {
    pub meli: ConfigMeli,
    pub backup: ConfigBackup,
    pub rede: ConfigRede,
}

pub async fn ler(State(state): State<AppState>) -> Result<Json<ConfigRes>, ApiError> {
    let pool = &state.pool;
    let app_id = db::ler_config(pool, meli::CHAVE_APP_ID).await.map_err(internal)?.unwrap_or_default();
    let secret = db::ler_config(pool, meli::CHAVE_SECRET).await.map_err(internal)?;
    let redirect = db::ler_config(pool, meli::CHAVE_REDIRECT)
        .await
        .map_err(internal)?
        .unwrap_or_else(|| meli::REDIRECT_PADRAO.to_string());
    let pasta = backup::pasta_configurada(pool, &state.dados).await;
    let padrao = backup::pasta_padrao(&state.dados);
    let rede = state.rede.read().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(Json(ConfigRes {
        meli: ConfigMeli { app_id, secret_configured: secret.is_some(), redirect_uri: redirect },
        backup: ConfigBackup {
            personalizada: pasta != padrao,
            pasta: pasta.display().to_string(),
            pasta_padrao: padrao.display().to_string(),
            ultimo: db::ler_config(pool, backup::CHAVE_ULTIMO).await.map_err(internal)?,
            ultimo_erro: db::ler_config(pool, backup::CHAVE_ERRO).await.map_err(internal)?,
        },
        rede: ConfigRede {
            nome: rede.nome,
            porta: rede.porta,
            enderecos: rede.enderecos,
            ca_fingerprint: rede.ca_digital,
        },
    }))
}

#[derive(Deserialize)]
pub struct MeliReq {
    #[serde(rename = "appId")]
    pub app_id: String,
    /// Vazio = mantém o Secret já gravado.
    pub secret: Option<String>,
    #[serde(rename = "redirectUri")]
    pub redirect_uri: String,
}

pub async fn salvar_meli(State(state): State<AppState>, Json(p): Json<MeliReq>) -> Result<StatusCode, ApiError> {
    let app_id = p.app_id.trim();
    if app_id.is_empty() || app_id.len() > 40 || !app_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(bad_request("App ID inválido: são só números (aparece no DevCenter do Mercado Livre)."));
    }
    let redirect = p.redirect_uri.trim();
    validation::texto("URI de redirecionamento", redirect, 300)?;
    if !redirect.starts_with("https://") {
        return Err(bad_request("A URI de redirecionamento precisa começar com https:// (igual à do DevCenter)."));
    }
    let secret = p.secret.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(s) = secret {
        validation::texto("Secret", s, 200)?;
    } else if db::ler_config(&state.pool, meli::CHAVE_SECRET).await.map_err(internal)?.is_none() {
        return Err(bad_request("Informe o Secret do app."));
    }
    db::gravar_config(&state.pool, meli::CHAVE_APP_ID, app_id).await.map_err(internal)?;
    db::gravar_config(&state.pool, meli::CHAVE_REDIRECT, redirect).await.map_err(internal)?;
    if let Some(s) = secret {
        let cifrado = state.cofre.cifrar(s).map_err(internal)?;
        db::gravar_config(&state.pool, meli::CHAVE_SECRET, &cifrado).await.map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct BackupReq {
    /// Pasta de destino (ex.: pendrive ou pasta de rede). Vazio = pasta padrão.
    pub pasta: Option<String>,
}

pub async fn salvar_backup(State(state): State<AppState>, Json(p): Json<BackupReq>) -> Result<StatusCode, ApiError> {
    match p.pasta.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => db::apagar_config(&state.pool, backup::CHAVE_PASTA).await.map_err(internal)?,
        Some(pasta) => {
            validation::texto("pasta", pasta, 400)?;
            backup::testar_pasta(std::path::Path::new(pasta)).map_err(|e| bad_request(&e))?;
            db::gravar_config(&state.pool, backup::CHAVE_PASTA, pasta).await.map_err(internal)?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct BackupRes {
    pub arquivo: String,
}

pub async fn backup_agora(State(state): State<AppState>) -> Result<Json<BackupRes>, ApiError> {
    let arquivo = backup::executar(&state.pool, &state.dados).await.map_err(|e| bad_request(&e))?;
    Ok(Json(BackupRes { arquivo: arquivo.display().to_string() }))
}

/// GET /api/rede/ca.crt (pública): certificado para instalar nos computadores da rede.
/// É um certificado PÚBLICO (a chave privada nunca sai do servidor).
pub async fn baixar_ca(State(state): State<AppState>) -> impl IntoResponse {
    let pem = state.rede.read().unwrap_or_else(|e| e.into_inner()).ca_pem.clone();
    (
        [
            (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"StockBridge-certificado.crt\""),
        ],
        pem,
    )
}
