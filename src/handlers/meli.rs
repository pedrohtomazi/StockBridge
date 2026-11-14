use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    db,
    security::{
        error::{ApiError, bad_request, conflito, db_error, internal, nao_encontrado},
        state::AppState,
        validation,
    },
    services::meli,
};

#[derive(Serialize)]
pub struct AuthorizeRes {
    pub url: String,
}

/// GET /api/meli/authorize (admin): URL de autorização com `state` aleatório de uso único.
pub async fn authorize(State(state): State<AppState>) -> Result<Json<AuthorizeRes>, ApiError> {
    let app = meli::exigir_app(&state.pool, &state.cofre).await.map_err(|e| bad_request(&e))?;
    Ok(Json(AuthorizeRes { url: meli::url_autorizacao(&app, &state.novo_state_oauth()) }))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// GET /api/meli/callback?code=...&state=... (pública: protegida pelo `state` que NÓS geramos;
/// sem isso, alguém mandaria um link com o `code` dele e o sistema usaria a conta errada).
pub async fn callback(State(state): State<AppState>, Query(q): Query<CallbackQuery>) -> Result<String, ApiError> {
    if !state.consumir_state_oauth(&q.state) {
        return Err(bad_request("Pedido de autorização inválido ou expirado. Gere um link novo pelo sistema."));
    }
    if q.code.is_empty() || q.code.len() > 512 {
        return Err(bad_request("Código de autorização inválido."));
    }
    let app = meli::exigir_app(&state.pool, &state.cofre).await.map_err(|e| bad_request(&e))?;
    let token = meli::trocar_codigo(&app, &q.code).await.map_err(|e| {
        tracing::error!("Falha na autorização do Mercado Livre: {e}");
        bad_request(&format!("O Mercado Livre não aceitou o código. {e}"))
    })?;
    meli::salvar_credenciais(&state.pool, &state.cofre, &token).await.map_err(internal)?;
    // Não sincroniza nada aqui: enviar o estoque do sistema logo ao conectar poderia zerar
    // anúncios de produtos que ainda não tiveram entrada lançada.
    *state.cache_anuncios.lock().await = None;
    Ok("Integração concluída! Feche esta janela e volte para o sistema.".to_string())
}

#[derive(Serialize)]
pub struct StatusRes {
    /// App ID e Secret preenchidos na configuração?
    pub configured: bool,
    pub connected: bool,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
    #[serde(rename = "needsReconnect")]
    pub needs_reconnect: bool,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
    /// O Mercado Livre mandou token de renovação? (sem ele, a conexão expira em ~6 h)
    #[serde(rename = "autoRenew")]
    pub auto_renew: bool,
}

/// GET /api/meli/status (admin). Nunca devolve tokens.
pub async fn status(State(state): State<AppState>) -> Result<Json<StatusRes>, ApiError> {
    let configured = meli::carregar_app(&state.pool, &state.cofre).await.map_err(internal)?.is_some();
    let linha = sqlx::query(
        "SELECT expires_at, needs_reconnect, last_error, refresh_token_enc FROM meli_credentials WHERE id = 1",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(match linha {
        Some(r) => StatusRes {
            configured,
            connected: true,
            expires_at: r.get("expires_at"),
            needs_reconnect: r.get::<i64, _>("needs_reconnect") == 1,
            last_error: r.get("last_error"),
            auto_renew: state
                .cofre
                .decifrar(&r.get::<String, _>("refresh_token_enc"))
                .map(|t| !t.is_empty())
                .unwrap_or(false),
        },
        None => StatusRes {
            configured,
            connected: false,
            expires_at: None,
            needs_reconnect: false,
            last_error: None,
            auto_renew: false,
        },
    }))
}

/// DELETE /api/meli/credentials (admin): desconecta a conta (apaga os tokens deste sistema).
pub async fn desconectar(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    sqlx::query("DELETE FROM meli_credentials").execute(&state.pool).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct VinculoReq {
    pub product_id: String,
    pub meli_item_id: String,
}

#[derive(Serialize)]
pub struct IdRes {
    pub id: String,
}

/// POST /api/meli/links (admin). NÃO envia estoque na hora (um produto com saldo 0 no sistema
/// zeraria o anúncio): envia na próxima movimentação ou no botão "Sincronizar".
pub async fn criar_vinculo(State(state): State<AppState>, Json(p): Json<VinculoReq>) -> Result<Json<IdRes>, ApiError> {
    validation::id("product_id", &p.product_id)?;
    let item = validation::anuncio_meli(&p.meli_item_id)?;
    let id = db::novo_id();
    sqlx::query("INSERT INTO meli_links (id, product_id, meli_item_id) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&p.product_id)
        .bind(&item)
        .execute(&state.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                conflito("Esse anúncio do Mercado Livre já está vinculado a um produto.")
            }
            sqlx::Error::Database(d) if d.is_foreign_key_violation() => bad_request("Produto não encontrado."),
            _ => db_error(e),
        })?;
    Ok(Json(IdRes { id }))
}

#[derive(Serialize)]
pub struct VinculoItem {
    pub id: String,
    #[serde(rename = "productId")]
    pub product_id: String,
    #[serde(rename = "meliItemId")]
    pub meli_item_id: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    pub sku: String,
    #[serde(rename = "totalQuantity")]
    pub total_quantity: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastSyncAt")]
    pub last_sync_at: Option<String>,
    #[serde(rename = "lastSyncOk")]
    pub last_sync_ok: Option<bool>,
    #[serde(rename = "lastSyncQty")]
    pub last_sync_qty: Option<i64>,
    #[serde(rename = "lastSyncError")]
    pub last_sync_error: Option<String>,
}

/// GET /api/meli/links (admin), com o resultado da última sincronização de cada anúncio.
pub async fn listar_vinculos(State(state): State<AppState>) -> Result<Json<Vec<VinculoItem>>, ApiError> {
    let linhas = sqlx::query(
        r#"SELECT l.id, l.product_id, l.meli_item_id, p.name, p.sku, l.created_at,
                  l.last_sync_at, l.last_sync_ok, l.last_sync_qty, l.last_sync_error,
                  COALESCE((SELECT SUM(s.quantity) FROM stocks s WHERE s.product_id = l.product_id), 0) AS total
           FROM meli_links l JOIN products p ON p.id = l.product_id
           ORDER BY l.created_at DESC"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(
        linhas
            .into_iter()
            .map(|r| VinculoItem {
                id: r.get("id"),
                product_id: r.get("product_id"),
                meli_item_id: r.get("meli_item_id"),
                product_name: r.get("name"),
                sku: r.get("sku"),
                total_quantity: r.get("total"),
                created_at: r.get("created_at"),
                last_sync_at: r.get("last_sync_at"),
                last_sync_ok: r.get::<Option<i64>, _>("last_sync_ok").map(|v| v == 1),
                last_sync_qty: r.get("last_sync_qty"),
                last_sync_error: r.get("last_sync_error"),
            })
            .collect(),
    ))
}

pub async fn remover_vinculo(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    let r = sqlx::query("DELETE FROM meli_links WHERE id = ?").bind(&id).execute(&state.pool).await.map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Vínculo não encontrado."));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/meli/links/{id}/sync (admin): "sincronizar agora".
pub async fn sincronizar_agora(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    let produto: Option<String> = sqlx::query_scalar("SELECT product_id FROM meli_links WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
    let produto = produto.ok_or_else(|| nao_encontrado("Vínculo não encontrado."))?;
    state.sync.agendar(&produto);
    Ok(StatusCode::ACCEPTED)
}
