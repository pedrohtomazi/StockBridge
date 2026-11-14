//! Dados lidos do Mercado Livre (admin): anúncios, importação como produtos e rendimento.

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{Duration as Dur, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::{Duration, Instant}};

use crate::{
    db::{self, de_centavos, para_centavos},
    security::{
        error::{ApiError, bad_request, internal},
        state::AppState,
        validation,
    },
    services::{
        meli,
        meli_leitura::{self, Anuncio},
    },
};

const VALIDADE_CACHE: Duration = Duration::from_secs(5 * 60);

/// Token válido ou erro 400 com a explicação (conta não conectada, precisa reconectar...).
async fn token(state: &AppState) -> Result<String, ApiError> {
    meli::token_valido(&state.pool, &state.cofre).await.map_err(|e| bad_request(&e))
}

async fn anuncios_em_cache(state: &AppState, atualizar: bool) -> Result<(Vec<Anuncio>, Instant), ApiError> {
    let mut cache = state.cache_anuncios.lock().await;
    if !atualizar
        && let Some((quando, lista)) = cache.as_ref()
            && quando.elapsed() < VALIDADE_CACHE {
                return Ok((lista.clone(), *quando));
            }
    let t = token(state).await?;
    let lista = meli_leitura::carregar_anuncios(&t).await.map_err(|e| bad_request(&e))?;
    let agora = Instant::now();
    *cache = Some((agora, lista.clone()));
    Ok((lista, agora))
}

#[derive(Deserialize)]
pub struct FiltroAnuncios {
    pub atualizar: Option<bool>,
}

#[derive(Serialize)]
pub struct Vinculo {
    #[serde(rename = "linkId")]
    pub link_id: String,
    #[serde(rename = "productId")]
    pub product_id: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    #[serde(rename = "productSku")]
    pub product_sku: String,
    /// Estoque do produto no sistema (soma das estantes).
    #[serde(rename = "estoqueSistema")]
    pub estoque_sistema: i64,
}

#[derive(Serialize)]
pub struct AnuncioComVinculo {
    #[serde(flatten)]
    pub anuncio: Anuncio,
    pub vinculo: Option<Vinculo>,
    /// Já existe produto no sistema com o mesmo SKU do anúncio (ainda sem vínculo)?
    #[serde(rename = "produtoMesmoSku")]
    pub produto_mesmo_sku: Option<String>,
}

#[derive(Serialize)]
pub struct AnunciosRes {
    pub anuncios: Vec<AnuncioComVinculo>,
    /// Há quantos segundos os dados foram lidos do Mercado Livre.
    #[serde(rename = "idadeSegundos")]
    pub idade_segundos: u64,
}

/// GET /api/meli/anuncios (?atualizar=true para ignorar o cache de 5 min)
pub async fn anuncios(State(state): State<AppState>, Query(f): Query<FiltroAnuncios>) -> Result<Json<AnunciosRes>, ApiError> {
    let (lista, quando) = anuncios_em_cache(&state, f.atualizar.unwrap_or(false)).await?;
    let vinculos: Vec<(String, String, String, String, String, i64)> = sqlx::query_as(
        r#"SELECT l.meli_item_id, l.id, p.id, p.name, p.sku,
                  COALESCE((SELECT SUM(s.quantity) FROM stocks s WHERE s.product_id = p.id), 0)
           FROM meli_links l JOIN products p ON p.id = l.product_id"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    let por_item: HashMap<String, Vinculo> = vinculos
        .into_iter()
        .map(|(item, link_id, product_id, product_name, product_sku, estoque_sistema)| {
            (item, Vinculo { link_id, product_id, product_name, product_sku, estoque_sistema })
        })
        .collect();
    let skus: Vec<(String, String)> = sqlx::query_as("SELECT lower(sku), name FROM products")
        .fetch_all(&state.pool)
        .await
        .map_err(internal)?;
    let por_sku: HashMap<String, String> = skus.into_iter().collect();
    let anuncios = lista
        .into_iter()
        .map(|a| {
            let vinculo = por_item.get(&a.id).map(|v| Vinculo {
                link_id: v.link_id.clone(),
                product_id: v.product_id.clone(),
                product_name: v.product_name.clone(),
                product_sku: v.product_sku.clone(),
                estoque_sistema: v.estoque_sistema,
            });
            let produto_mesmo_sku = if vinculo.is_none() {
                a.sku.as_ref().and_then(|s| por_sku.get(&s.to_lowercase()).cloned())
            } else {
                None
            };
            AnuncioComVinculo { anuncio: a, vinculo, produto_mesmo_sku }
        })
        .collect();
    Ok(Json(AnunciosRes { anuncios, idade_segundos: quando.elapsed().as_secs() }))
}

#[derive(Deserialize)]
pub struct ImportarReq {
    pub ids: Vec<String>,
}

#[derive(Serialize)]
pub struct ResultadoImportacao {
    #[serde(rename = "meliItemId")]
    pub meli_item_id: String,
    /// "criado", "vinculado", "ja_vinculado", "ignorado" ou "erro"
    pub resultado: String,
    pub mensagem: String,
}

/// POST /api/meli/importar: cria produtos a partir dos anúncios (casando pelo SKU) e vincula.
/// NÃO envia estoque ao Mercado Livre: o produto novo tem saldo 0 no sistema, e enviar agora
/// zeraria o anúncio. O envio acontece na próxima movimentação ou no botão "Sincronizar".
pub async fn importar(State(state): State<AppState>, Json(p): Json<ImportarReq>) -> Result<Json<Vec<ResultadoImportacao>>, ApiError> {
    if p.ids.is_empty() || p.ids.len() > 2000 {
        return Err(bad_request("Selecione de 1 a 2.000 anúncios."));
    }
    let (lista, _) = anuncios_em_cache(&state, false).await?;
    let por_id: HashMap<&str, &Anuncio> = lista.iter().map(|a| (a.id.as_str(), a)).collect();
    let mut resultados = Vec::with_capacity(p.ids.len());
    for id in &p.ids {
        let res = |resultado: &str, mensagem: String| ResultadoImportacao {
            meli_item_id: id.clone(),
            resultado: resultado.into(),
            mensagem,
        };
        let Some(a) = por_id.get(id.as_str()) else {
            resultados.push(res("erro", "Anúncio não encontrado na sua conta (clique em Atualizar).".into()));
            continue;
        };
        if !a.variacoes.is_empty() {
            resultados.push(res(
                "ignorado",
                format!("Anúncio com {} variações: o sistema ainda não controla estoque por variação.", a.variacoes.len()),
            ));
            continue;
        }
        // SKU do anúncio (se for válido); senão, o próprio código do anúncio vira o SKU.
        let (sku, aviso) = match a.sku.as_deref() {
            Some(s) if validation::sku(s).is_ok() => (s.to_string(), String::new()),
            Some(s) => (a.id.clone(), format!(" (SKU \"{s}\" tem caracteres não aceitos; usado {})", a.id)),
            None => (a.id.clone(), format!(" (anúncio sem SKU; usado {})", a.id)),
        };
        let r: Result<ResultadoImportacao, ApiError> = async {
            let mut tx = db::transacao_escrita(&state.pool).await.map_err(internal)?;
            let ja: Option<String> = sqlx::query_scalar(
                "SELECT p.name FROM meli_links l JOIN products p ON p.id = l.product_id WHERE l.meli_item_id = ?",
            )
            .bind(&a.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;
            if let Some(nome) = ja {
                return Ok(res("ja_vinculado", format!("Já vinculado a \"{nome}\".")));
            }
            let existente: Option<(String, String)> =
                sqlx::query_as("SELECT id, name FROM products WHERE sku = ? COLLATE NOCASE")
                    .bind(&sku)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(internal)?;
            let (produto_id, saida) = match existente {
                Some((pid, nome)) => (pid, res("vinculado", format!("Vinculado ao produto existente \"{nome}\" (SKU {sku}).{aviso}"))),
                None => {
                    let pid = db::novo_id();
                    let nome: String = a.titulo.chars().take(200).collect();
                    let preco = if a.preco.is_finite() && a.preco >= 0.0 { a.preco.min(1_000_000_000.0) } else { 0.0 };
                    sqlx::query("INSERT INTO products (id, name, sku, cost_cents, price_cents) VALUES (?, ?, ?, 0, ?)")
                        .bind(&pid)
                        .bind(&nome)
                        .bind(&sku)
                        .bind(para_centavos(preco))
                        .execute(&mut *tx)
                        .await
                        .map_err(internal)?;
                    (pid, res("criado", format!("Produto criado (SKU {sku}). Preencha o custo em Produtos.{aviso}")))
                }
            };
            sqlx::query("INSERT INTO meli_links (id, product_id, meli_item_id) VALUES (?, ?, ?)")
                .bind(db::novo_id())
                .bind(&produto_id)
                .bind(&a.id)
                .execute(&mut *tx)
                .await
                .map_err(internal)?;
            tx.commit().await.map_err(internal)?;
            Ok(saida)
        }
        .await;
        resultados.push(r.unwrap_or_else(|(_, m)| res("erro", m)));
    }
    Ok(Json(resultados))
}

#[derive(Deserialize)]
pub struct FiltroRendimento {
    /// Últimos N dias (1 a 365). Padrão 30.
    pub dias: Option<i64>,
}

/// GET /api/meli/rendimento?dias=30: vendas do período, lidas dos pedidos do Mercado Livre.
pub async fn rendimento(
    State(state): State<AppState>,
    Query(f): Query<FiltroRendimento>,
) -> Result<Json<meli_leitura::Rendimento>, ApiError> {
    let dias = f.dias.unwrap_or(30);
    if !(1..=365).contains(&dias) {
        return Err(bad_request("O período deve ser de 1 a 365 dias."));
    }
    let t = token(&state).await?;
    let ate = Utc::now();
    let de = ate - Dur::days(dias);
    let (pedidos, truncado) = meli_leitura::pedidos(&t, de, ate).await.map_err(|e| bad_request(&e))?;
    let custos: Vec<(String, i64)> = sqlx::query_as(
        "SELECT l.meli_item_id, p.cost_cents FROM meli_links l JOIN products p ON p.id = l.product_id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    let custos: HashMap<String, f64> = custos.into_iter().map(|(i, c)| (i, de_centavos(c))).collect();
    Ok(Json(meli_leitura::resumir(&pedidos, &custos, de, ate, truncado)))
}
