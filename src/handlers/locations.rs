use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{self, Banco},
    security::{
        error::{ApiError, bad_request, conflito, db_error, internal, nao_encontrado},
        validation,
    },
};

#[derive(Deserialize)]
pub struct EstanteReq {
    #[serde(rename = "warehouseId")]
    pub warehouse_id: String,
    pub sector: String,
    pub rack: String,
    pub level: String,
    pub position: Option<String>,
    #[serde(rename = "fullCode")]
    pub full_code: String,
    #[serde(rename = "qrToken")]
    pub qr_token: String,
    #[serde(rename = "maxCapacity")]
    pub max_capacity: i64,
    /// Só usado na alteração (PUT). Estante inativa não recebe entradas.
    #[serde(rename = "isActive")]
    pub is_active: Option<bool>,
}

impl EstanteReq {
    fn validar(&self) -> Result<(), ApiError> {
        validation::id("warehouseId", &self.warehouse_id)?;
        validation::texto("sector", &self.sector, 20)?;
        validation::texto("rack", &self.rack, 20)?;
        validation::texto("level", &self.level, 20)?;
        if let Some(pos) = self.position.as_deref().filter(|p| !p.trim().is_empty()) {
            validation::texto("position", pos, 20)?;
        }
        validation::texto("fullCode", &self.full_code, 60)?;
        validation::texto("qrToken", &self.qr_token, 128)?;
        if !(1..=validation::MAX_QTD).contains(&self.max_capacity) {
            return Err(bad_request("maxCapacity deve estar entre 1 e 1.000.000."));
        }
        Ok(())
    }

    fn posicao(&self) -> Option<&str> {
        self.position.as_deref().map(str::trim).filter(|p| !p.is_empty())
    }
}

fn erro_estante(e: sqlx::Error) -> ApiError {
    match &e {
        sqlx::Error::Database(d) if d.is_foreign_key_violation() => {
            bad_request("Depósito não encontrado. Cadastre o depósito antes.")
        }
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            conflito("Já existe uma estante com esse código ou com esse QR Code.")
        }
        _ => db_error(e),
    }
}

#[derive(Serialize)]
pub struct IdRes {
    pub id: String,
}

/// POST /api/locations (admin)
pub async fn criar(State(pool): State<Banco>, Json(p): Json<EstanteReq>) -> Result<Json<IdRes>, ApiError> {
    p.validar()?;
    let id = db::novo_id();
    sqlx::query(
        r#"INSERT INTO locations (id, warehouse_id, sector, rack, level, position, full_code, qr_token, max_capacity, is_active)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"#,
    )
    .bind(&id)
    .bind(&p.warehouse_id)
    .bind(p.sector.trim())
    .bind(p.rack.trim())
    .bind(p.level.trim())
    .bind(p.posicao())
    .bind(p.full_code.trim())
    .bind(p.qr_token.trim())
    .bind(p.max_capacity)
    .execute(&pool)
    .await
    .map_err(erro_estante)?;
    Ok(Json(IdRes { id }))
}

#[derive(Deserialize)]
pub struct Filtro {
    #[serde(rename = "qrToken")]
    pub qr_token: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct EstanteItem {
    pub id: String,
    #[serde(rename = "warehouseId")]
    pub warehouse_id: String,
    #[serde(rename = "warehouseName")]
    pub warehouse_name: String,
    pub sector: String,
    pub rack: String,
    pub level: String,
    pub position: Option<String>,
    #[serde(rename = "fullCode")]
    pub full_code: String,
    #[serde(rename = "qrToken")]
    pub qr_token: String,
    #[serde(rename = "maxCapacity")]
    pub max_capacity: i64,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "currentQuantity")]
    pub current_quantity: i64,
}

/// GET /api/locations  (filtro opcional ?qrToken=..., usado ao bipar a estante)
pub async fn listar(State(pool): State<Banco>, Query(f): Query<Filtro>) -> Result<Json<Vec<EstanteItem>>, ApiError> {
    if let Some(t) = &f.qr_token
        && t.chars().count() > 128 {
            return Err(bad_request("qrToken excede 128 caracteres."));
        }
    let lista = sqlx::query_as::<_, EstanteItem>(
        r#"SELECT l.id, l.warehouse_id, w.name AS warehouse_name, l.sector, l.rack, l.level, l.position,
                  l.full_code, l.qr_token, l.max_capacity, l.is_active,
                  COALESCE((SELECT SUM(s.quantity) FROM stocks s WHERE s.location_id = l.id), 0) AS current_quantity
           FROM locations l JOIN warehouses w ON w.id = l.warehouse_id
           WHERE (?1 IS NULL OR l.qr_token = ?1)
           ORDER BY l.full_code COLLATE NOCASE"#,
    )
    .bind(f.qr_token.as_deref().map(str::trim))
    .fetch_all(&pool)
    .await
    .map_err(internal)?;
    Ok(Json(lista))
}

/// PUT /api/locations/{id} (admin). A capacidade não pode ficar menor do que já está guardado.
pub async fn alterar(
    State(pool): State<Banco>,
    Path(id): Path<String>,
    Json(p): Json<EstanteReq>,
) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    p.validar()?;
    let mut tx = db::transacao_escrita(&pool).await.map_err(internal)?;
    let ocupado: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE((SELECT SUM(quantity) FROM stocks WHERE location_id = l.id), 0) FROM locations l WHERE l.id = ?",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?;
    let ocupado = ocupado.ok_or_else(|| nao_encontrado("Estante não encontrada."))?;
    if p.max_capacity < ocupado {
        return Err(bad_request(&format!(
            "A estante já tem {ocupado} itens guardados: a capacidade não pode ser menor que isso."
        )));
    }
    sqlx::query(
        r#"UPDATE locations SET warehouse_id = ?, sector = ?, rack = ?, level = ?, position = ?,
                  full_code = ?, qr_token = ?, max_capacity = ?, is_active = COALESCE(?, is_active)
           WHERE id = ?"#,
    )
    .bind(&p.warehouse_id)
    .bind(p.sector.trim())
    .bind(p.rack.trim())
    .bind(p.level.trim())
    .bind(p.posicao())
    .bind(p.full_code.trim())
    .bind(p.qr_token.trim())
    .bind(p.max_capacity)
    .bind(p.is_active.map(|a| a as i64))
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(erro_estante)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/locations/{id} (admin). Estante com histórico não é apagada: desative-a.
pub async fn excluir(State(pool): State<Banco>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    let mut tx = db::transacao_escrita(&pool).await.map_err(internal)?;
    let movs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movements WHERE location_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    if movs > 0 {
        return Err(conflito(
            "Esta estante tem movimentações e não pode ser excluída. Desative-a para parar de receber entradas.",
        ));
    }
    sqlx::query("DELETE FROM stocks WHERE location_id = ?").bind(&id).execute(&mut *tx).await.map_err(internal)?;
    let r = sqlx::query("DELETE FROM locations WHERE id = ?").bind(&id).execute(&mut *tx).await.map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Estante não encontrada."));
    }
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ItemEstoque {
    pub product_id: String,
    pub product_name: String,
    pub sku: String,
    pub quantity: i64,
}

/// GET /api/locations/{id}/stock: só produtos com saldo (antes vinham também os zerados).
pub async fn estoque(State(pool): State<Banco>, Path(id): Path<String>) -> Result<Json<Vec<ItemEstoque>>, ApiError> {
    validation::id("id", &id)?;
    let itens = sqlx::query_as::<_, ItemEstoque>(
        r#"SELECT p.id AS product_id, p.name AS product_name, p.sku, s.quantity
           FROM stocks s JOIN products p ON p.id = s.product_id
           WHERE s.location_id = ? AND s.quantity > 0
           ORDER BY s.quantity DESC"#,
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .map_err(internal)?;
    Ok(Json(itens))
}
