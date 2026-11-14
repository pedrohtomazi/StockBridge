use axum::{
    Extension, Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    db::{self, de_centavos},
    security::{
        auth::Usuario,
        error::{ApiError, bad_request, db_error, internal},
        state::AppState,
        validation,
    },
};

#[derive(Deserialize)]
pub struct MovimentoReq {
    #[serde(rename = "productId")]
    pub product_id: String,
    #[serde(rename = "locationId")]
    pub location_id: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub quantity: i64,
}

#[derive(Serialize)]
pub struct MovimentoRes {
    pub id: String,
    /// Saldo do produto NAQUELA estante depois da operação.
    pub new_balance: i64,
}

/// POST /api/movements (qualquer usuário logado)
pub async fn criar(
    State(state): State<AppState>,
    Extension(u): Extension<Usuario>,
    Json(p): Json<MovimentoReq>,
) -> Result<Json<MovimentoRes>, ApiError> {
    validation::id("productId", &p.product_id)?;
    validation::id("locationId", &p.location_id)?;
    if p.tipo != "IN" && p.tipo != "OUT" {
        return Err(bad_request("type deve ser exatamente \"IN\" ou \"OUT\"."));
    }
    validation::quantidade("quantity", p.quantity)?;
    let (id, saldo) = registrar(&state.pool, &u.id, &p.product_id, &p.location_id, &p.tipo, p.quantity).await?;
    // Mercado Livre: entra na fila de sincronização (em segundo plano, um por vez).
    state.sync.agendar(&p.product_id);
    Ok(Json(MovimentoRes { id, new_balance: saldo }))
}

/// Grava UMA movimentação com todas as regras (capacidade, saldo, estante ativa, preço da época).
/// Devolve (id da movimentação, saldo do produto na estante). Usada também pela entrada em lote.
pub async fn registrar(
    pool: &db::Banco,
    usuario_id: &str,
    produto_id: &str,
    estante_id: &str,
    tipo: &str,
    quantidade: i64,
) -> Result<(String, i64), ApiError> {
    struct P<'a> {
        product_id: &'a str,
        location_id: &'a str,
        tipo: &'a str,
        quantity: i64,
    }
    let p = P { product_id: produto_id, location_id: estante_id, tipo, quantity: quantidade };

    // BEGIN IMMEDIATE: duas movimentações simultâneas entram em fila, então a checagem de
    // capacidade e de saldo abaixo não pode ser "furada" por uma corrida.
    let mut tx = db::transacao_escrita(pool).await.map_err(internal)?;

    let estante = sqlx::query("SELECT max_capacity, is_active FROM locations WHERE id = ?")
        .bind(p.location_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or_else(|| bad_request("Estante não encontrada."))?;
    let produto = sqlx::query("SELECT cost_cents, price_cents FROM products WHERE id = ?")
        .bind(p.product_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or_else(|| bad_request("Produto não encontrado."))?;

    let saldo: i64 = if p.tipo == "IN" {
        if estante.get::<i64, _>("is_active") != 1 {
            return Err(bad_request("Estante inativa: não aceita entradas."));
        }
        let capacidade: i64 = estante.get("max_capacity");
        let ocupado: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quantity), 0) FROM stocks WHERE location_id = ?")
            .bind(p.location_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(internal)?;
        if ocupado + p.quantity > capacidade {
            return Err(bad_request(&format!(
                "Capacidade excedida. A estante suporta {capacidade} itens, mas já tem {ocupado} guardados."
            )));
        }
        sqlx::query_scalar(
            r#"INSERT INTO stocks (product_id, location_id, quantity) VALUES (?, ?, ?)
               ON CONFLICT(product_id, location_id) DO UPDATE SET quantity = quantity + excluded.quantity
               RETURNING quantity"#,
        )
        .bind(p.product_id)
        .bind(p.location_id)
        .bind(p.quantity)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?
    } else {
        // Só subtrai se houver saldo suficiente (o saldo nunca fica negativo).
        sqlx::query_scalar(
            r#"UPDATE stocks SET quantity = quantity - ?1
               WHERE product_id = ?2 AND location_id = ?3 AND quantity >= ?1
               RETURNING quantity"#,
        )
        .bind(p.quantity)
        .bind(p.product_id)
        .bind(p.location_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or_else(|| bad_request("Estoque insuficiente para essa saída."))?
    };

    let id = db::novo_id();
    sqlx::query(
        r#"INSERT INTO movements (id, product_id, location_id, user_id, type, quantity, unit_cost_cents, unit_price_cents, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(p.product_id)
    .bind(p.location_id)
    .bind(usuario_id) // quem fez (auditoria)
    .bind(p.tipo)
    .bind(p.quantity)
    .bind(produto.get::<i64, _>("cost_cents")) // custo e preço DA ÉPOCA
    .bind(produto.get::<i64, _>("price_cents"))
    .bind(db::agora_iso())
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(internal)?;
    Ok((id, saldo))
}

#[derive(Deserialize)]
pub struct LinhaLote {
    pub sku: String,
    /// Código da estante (fullCode) ou conteúdo do QR (qrToken).
    pub estante: String,
    pub quantidade: i64,
}

#[derive(Deserialize)]
pub struct LoteReq {
    pub itens: Vec<LinhaLote>,
    /// Enviar o novo estoque ao Mercado Livre? Na carga inicial, deixe desligado até lançar tudo.
    #[serde(default)]
    pub sincronizar: bool,
}

#[derive(Serialize)]
pub struct ResultadoLinha {
    pub linha: usize,
    pub ok: bool,
    pub mensagem: String,
}

/// POST /api/movements/lote (admin): entradas em massa (colar do Excel: SKU, estante, quantidade).
/// Cada linha é uma entrada independente, com as mesmas regras de uma entrada normal.
pub async fn entrada_em_lote(
    State(state): State<AppState>,
    Extension(u): Extension<Usuario>,
    Json(p): Json<LoteReq>,
) -> Result<Json<Vec<ResultadoLinha>>, ApiError> {
    if p.itens.is_empty() || p.itens.len() > 5000 {
        return Err(bad_request("Envie de 1 a 5.000 linhas."));
    }
    let mut resultados = Vec::with_capacity(p.itens.len());
    let mut produtos_mexidos = std::collections::BTreeSet::new();
    for (i, item) in p.itens.iter().enumerate() {
        let linha = i + 1;
        let r: Result<String, ApiError> = async {
            let sku = item.sku.trim();
            let estante = item.estante.trim();
            validation::texto("SKU", sku, 64)?;
            validation::texto("estante", estante, 128)?;
            validation::quantidade("quantidade", item.quantidade)?;
            let produto: Option<String> = sqlx::query_scalar("SELECT id FROM products WHERE sku = ? COLLATE NOCASE")
                .bind(sku)
                .fetch_optional(&state.pool)
                .await
                .map_err(internal)?;
            let produto = produto.ok_or_else(|| bad_request(&format!("SKU {sku} não cadastrado.")))?;
            let local: Option<String> = sqlx::query_scalar(
                "SELECT id FROM locations WHERE full_code = ?1 COLLATE NOCASE OR qr_token = ?1 COLLATE NOCASE LIMIT 1",
            )
            .bind(estante)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?;
            let local = local.ok_or_else(|| bad_request(&format!("Estante {estante} não encontrada.")))?;
            let (_, saldo) = registrar(&state.pool, &u.id, &produto, &local, "IN", item.quantidade).await?;
            produtos_mexidos.insert(produto);
            Ok(format!("+{} {sku} em {estante} (saldo na estante: {saldo})", item.quantidade))
        }
        .await;
        resultados.push(match r {
            Ok(m) => ResultadoLinha { linha, ok: true, mensagem: m },
            Err((_, m)) => ResultadoLinha { linha, ok: false, mensagem: m },
        });
    }
    if p.sincronizar {
        for produto in &produtos_mexidos {
            state.sync.agendar(produto);
        }
    }
    Ok(Json(resultados))
}

#[derive(Deserialize)]
pub struct FiltroMapa {
    /// Busca por SKU, nome do produto ou código da estante.
    pub q: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct LinhaMapa {
    #[serde(rename = "productId")]
    pub product_id: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    pub sku: String,
    #[serde(rename = "locationId")]
    pub location_id: String,
    #[serde(rename = "locationCode")]
    pub location_code: String,
    #[serde(rename = "warehouseName")]
    pub warehouse_name: String,
    pub quantity: i64,
}

/// GET /api/stock?q=...: onde está cada SKU (todas as estantes com saldo).
pub async fn mapa(State(state): State<AppState>, Query(f): Query<FiltroMapa>) -> Result<Json<Vec<LinhaMapa>>, ApiError> {
    let termo = f.q.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if let Some(t) = termo {
        validation::texto("busca", t, 100)?;
    }
    let padrao = termo.map(|t| format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let linhas = sqlx::query_as::<_, LinhaMapa>(
        r#"SELECT p.id AS product_id, p.name AS product_name, p.sku, l.id AS location_id,
                  l.full_code AS location_code, w.name AS warehouse_name, s.quantity
           FROM stocks s
           JOIN products p ON p.id = s.product_id
           JOIN locations l ON l.id = s.location_id
           JOIN warehouses w ON w.id = l.warehouse_id
           WHERE s.quantity > 0
             AND (?1 IS NULL OR p.sku LIKE ?1 ESCAPE '\' OR p.name LIKE ?1 ESCAPE '\' OR l.full_code LIKE ?1 ESCAPE '\')
           ORDER BY p.sku COLLATE NOCASE, l.full_code COLLATE NOCASE"#,
    )
    .bind(padrao)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(linhas))
}

#[derive(Deserialize)]
pub struct FiltroHistorico {
    /// Quantos registros (padrão 1000, máximo 10000).
    pub limit: Option<i64>,
    /// Início e fim (UTC, ISO 8601 ou AAAA-MM-DD). Opcionais.
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Serialize)]
pub struct MovimentoItem {
    pub id: String,
    #[serde(rename = "productId")]
    pub product_id: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    pub sku: String,
    #[serde(rename = "locationId")]
    pub location_id: String,
    #[serde(rename = "locationCode")]
    pub location_code: String,
    #[serde(rename = "userEmail")]
    pub user_email: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub quantity: i64,
    /// Custo e preço unitários NA DATA da movimentação.
    pub cost: f64,
    pub price: f64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

fn data_valida(s: &str) -> bool {
    s.len() <= 25 && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | 'Z'))
}

/// GET /api/movements: mais recente primeiro.
pub async fn listar(
    State(state): State<AppState>,
    Query(f): Query<FiltroHistorico>,
) -> Result<Json<Vec<MovimentoItem>>, ApiError> {
    let limite = f.limit.unwrap_or(1000).clamp(1, 10_000);
    for d in [&f.from, &f.to].into_iter().flatten() {
        if !data_valida(d) {
            return Err(bad_request("Datas devem estar no formato AAAA-MM-DD ou AAAA-MM-DDTHH:MM:SSZ."));
        }
    }
    // "até 2026-11-13" inclui o dia inteiro
    let ate = f.to.as_ref().map(|t| if t.len() == 10 { format!("{t}T23:59:59Z") } else { t.clone() });
    let linhas = sqlx::query(
        r#"SELECT m.id, m.product_id, p.name, p.sku, m.location_id, l.full_code, u.email,
                  m.type, m.quantity, m.unit_cost_cents, m.unit_price_cents, m.created_at
           FROM movements m
           JOIN products p ON p.id = m.product_id
           JOIN locations l ON l.id = m.location_id
           JOIN users u ON u.id = m.user_id
           WHERE (?1 IS NULL OR m.created_at >= ?1) AND (?2 IS NULL OR m.created_at <= ?2)
           ORDER BY m.created_at DESC, m.rowid DESC
           LIMIT ?3"#,
    )
    .bind(f.from.as_deref())
    .bind(ate.as_deref())
    .bind(limite)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(
        linhas
            .into_iter()
            .map(|r| MovimentoItem {
                id: r.get("id"),
                product_id: r.get("product_id"),
                product_name: r.get("name"),
                sku: r.get("sku"),
                location_id: r.get("location_id"),
                location_code: r.get("full_code"),
                user_email: r.get("email"),
                tipo: r.get("type"),
                quantity: r.get("quantity"),
                cost: de_centavos(r.get("unit_cost_cents")),
                price: de_centavos(r.get("unit_price_cents")),
                created_at: r.get("created_at"),
            })
            .collect(),
    ))
}
