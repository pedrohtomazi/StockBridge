use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    db::{self, Banco, de_centavos, para_centavos},
    security::{
        error::{ApiError, conflito, db_error, internal, nao_encontrado},
        validation,
    },
};

#[derive(Deserialize)]
pub struct ProdutoReq {
    pub name: String,
    pub sku: String,
    pub cost: f64,
    pub price: f64,
}

impl ProdutoReq {
    fn validar(&self) -> Result<(), ApiError> {
        validation::texto("name", &self.name, 200)?;
        validation::sku(self.sku.trim())?;
        validation::dinheiro("cost", self.cost)?;
        validation::dinheiro("price", self.price)
    }
}

#[derive(Serialize)]
pub struct IdRes {
    pub id: String,
}

#[derive(Serialize)]
pub struct ProdutoItem {
    pub id: String,
    pub name: String,
    pub sku: String,
    pub cost: f64,
    pub price: f64,
    #[serde(rename = "totalQuantity")]
    pub total_quantity: i64,
}

fn sku_repetido(e: sqlx::Error) -> ApiError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => conflito("Já existe um produto com esse SKU."),
        _ => db_error(e),
    }
}

/// POST /api/products (admin)
pub async fn criar(State(pool): State<Banco>, Json(p): Json<ProdutoReq>) -> Result<Json<IdRes>, ApiError> {
    p.validar()?;
    let id = db::novo_id();
    sqlx::query("INSERT INTO products (id, name, sku, cost_cents, price_cents) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(p.name.trim())
        .bind(p.sku.trim())
        .bind(para_centavos(p.cost))
        .bind(para_centavos(p.price))
        .execute(&pool)
        .await
        .map_err(sku_repetido)?;
    Ok(Json(IdRes { id }))
}

/// GET /api/products: catálogo com o estoque total (soma de todas as estantes)
pub async fn listar(State(pool): State<Banco>) -> Result<Json<Vec<ProdutoItem>>, ApiError> {
    let linhas = sqlx::query(
        r#"SELECT p.id, p.name, p.sku, p.cost_cents, p.price_cents,
                  COALESCE(SUM(s.quantity), 0) AS total
           FROM products p LEFT JOIN stocks s ON s.product_id = p.id
           GROUP BY p.id ORDER BY p.name COLLATE NOCASE"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(internal)?;
    Ok(Json(
        linhas
            .into_iter()
            .map(|r| ProdutoItem {
                id: r.get("id"),
                name: r.get("name"),
                sku: r.get("sku"),
                cost: de_centavos(r.get("cost_cents")),
                price: de_centavos(r.get("price_cents")),
                total_quantity: r.get("total"),
            })
            .collect(),
    ))
}

/// PUT /api/products/{id} (admin). Mudar o preço NÃO altera o histórico: cada movimentação
/// guarda o custo e o preço da época.
pub async fn alterar(
    State(pool): State<Banco>,
    Path(id): Path<String>,
    Json(p): Json<ProdutoReq>,
) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    p.validar()?;
    let r = sqlx::query("UPDATE products SET name = ?, sku = ?, cost_cents = ?, price_cents = ? WHERE id = ?")
        .bind(p.name.trim())
        .bind(p.sku.trim())
        .bind(para_centavos(p.cost))
        .bind(para_centavos(p.price))
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(sku_repetido)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Produto não encontrado."));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/products/{id} (admin). Só se nunca foi movimentado (senão o histórico se perderia).
pub async fn excluir(State(pool): State<Banco>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    let mut tx = db::transacao_escrita(&pool).await.map_err(internal)?;
    let movs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movements WHERE product_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    if movs > 0 {
        return Err(conflito(
            "Este produto já tem movimentações e não pode ser excluído (o histórico seria perdido).",
        ));
    }
    let vinculos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meli_links WHERE product_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    if vinculos > 0 {
        return Err(conflito("Remova antes os anúncios do Mercado Livre vinculados a este produto."));
    }
    sqlx::query("DELETE FROM stocks WHERE product_id = ?").bind(&id).execute(&mut *tx).await.map_err(internal)?;
    let r = sqlx::query("DELETE FROM products WHERE id = ?").bind(&id).execute(&mut *tx).await.map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Produto não encontrado."));
    }
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
