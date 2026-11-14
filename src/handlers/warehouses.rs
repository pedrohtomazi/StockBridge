use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{self, Banco},
    security::{
        error::{ApiError, conflito, internal, nao_encontrado},
        validation,
    },
};

#[derive(Deserialize)]
pub struct DepositoReq {
    pub name: String,
}

#[derive(Serialize)]
pub struct IdRes {
    pub id: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DepositoItem {
    pub id: String,
    pub name: String,
}

pub async fn criar(State(pool): State<Banco>, Json(p): Json<DepositoReq>) -> Result<Json<IdRes>, ApiError> {
    validation::texto("name", &p.name, 120)?;
    let id = db::novo_id();
    sqlx::query("INSERT INTO warehouses (id, name) VALUES (?, ?)")
        .bind(&id)
        .bind(p.name.trim())
        .execute(&pool)
        .await
        .map_err(internal)?;
    Ok(Json(IdRes { id }))
}

pub async fn listar(State(pool): State<Banco>) -> Result<Json<Vec<DepositoItem>>, ApiError> {
    let lista = sqlx::query_as::<_, DepositoItem>("SELECT id, name FROM warehouses ORDER BY name COLLATE NOCASE")
        .fetch_all(&pool)
        .await
        .map_err(internal)?;
    Ok(Json(lista))
}

pub async fn alterar(
    State(pool): State<Banco>,
    Path(id): Path<String>,
    Json(p): Json<DepositoReq>,
) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    validation::texto("name", &p.name, 120)?;
    let r = sqlx::query("UPDATE warehouses SET name = ? WHERE id = ?")
        .bind(p.name.trim())
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Depósito não encontrado."));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Só exclui depósito sem estantes.
pub async fn excluir(State(pool): State<Banco>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    validation::id("id", &id)?;
    let estantes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locations WHERE warehouse_id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .map_err(internal)?;
    if estantes > 0 {
        return Err(conflito("Este depósito tem estantes. Exclua ou mude as estantes de depósito antes."));
    }
    let r = sqlx::query("DELETE FROM warehouses WHERE id = ?").bind(&id).execute(&pool).await.map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(nao_encontrado("Depósito não encontrado."));
    }
    Ok(StatusCode::NO_CONTENT)
}
