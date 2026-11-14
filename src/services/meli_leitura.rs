//! LEITURA da conta do Mercado Livre: anúncios, visitas e pedidos (rendimento).
//! Só consulta; nada aqui altera a conta.
//!
//! Rotas da API do ML usadas (com o token da conta conectada):
//!   GET /users/me                                   -> id do vendedor
//!   GET /users/{id}/items/search                     -> ids dos anúncios (paginado)
//!   GET /items?ids=A,B,...&include_attributes=all    -> detalhes, até 20 por vez
//!   GET /visits/items?ids=A,B,...                    -> visitas totais, até 50 por vez
//!   GET /orders/search?seller={id}&...               -> pedidos do período (paginado)

use chrono::{DateTime, FixedOffset, Utc};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
use url::form_urlencoded;

/// Endereço da API (trocável só para testes, com a variável STOCKBRIDGE_MELI_API).
pub fn base() -> String {
    std::env::var("STOCKBRIDGE_MELI_API").unwrap_or_else(|_| "https://api.mercadolibre.com".to_string())
}

const MAX_PEDIDOS: usize = 3000;

fn cliente() -> Result<Client, String> {
    Client::builder().timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())
}

async fn get_json(token: &str, caminho: &str) -> Result<Value, String> {
    let res = cliente()?
        .get(format!("{}{caminho}", base()))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Sem conexão com o Mercado Livre: {e}"))?;
    let status = res.status();
    let texto = res.text().await.unwrap_or_default();
    let rota = caminho.split('?').next().unwrap_or(caminho);
    if !status.is_success() {
        let detalhe: String = texto.chars().filter(|c| !c.is_control()).take(250).collect();
        return Err(format!("Mercado Livre recusou ({status}) em {rota}: {detalhe}"));
    }
    serde_json::from_str(&texto).map_err(|e| format!("Resposta inesperada do Mercado Livre em {rota}: {e}"))
}

fn q(pares: &[(&str, &str)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in pares {
        s.append_pair(k, v);
    }
    s.finish()
}

// ---------------------------------------------------------------------------
// Anúncios
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Variacao {
    pub id: String,
    pub nome: String,
    pub sku: Option<String>,
    #[serde(rename = "estoqueMl")]
    pub estoque_ml: i64,
    pub vendidos: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Anuncio {
    pub id: String,
    pub titulo: String,
    pub preco: f64,
    #[serde(rename = "estoqueMl")]
    pub estoque_ml: i64,
    pub vendidos: i64,
    pub status: String,
    pub link: Option<String>,
    pub foto: Option<String>,
    pub sku: Option<String>,
    /// Anúncio no Full: o estoque é controlado pelo próprio Mercado Livre.
    pub full: bool,
    pub variacoes: Vec<Variacao>,
    pub visitas: Option<i64>,
}

/// SKU do vendedor: atributo SELLER_SKU ou o campo antigo seller_custom_field.
fn sku_de(v: &Value) -> Option<String> {
    let por_atributo = v["attributes"].as_array().and_then(|lista| {
        lista.iter().find(|a| a["id"] == "SELLER_SKU").and_then(|a| a["value_name"].as_str())
    });
    por_atributo
        .or_else(|| v["seller_custom_field"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn texto_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Converte o JSON de um item do ML no nosso formato.
pub fn anuncio_de_json(v: &Value) -> Option<Anuncio> {
    let id = v["id"].as_str()?.to_string();
    let variacoes = v["variations"]
        .as_array()
        .map(|lista| {
            lista
                .iter()
                .map(|var| Variacao {
                    id: texto_id(&var["id"]),
                    nome: var["attribute_combinations"]
                        .as_array()
                        .map(|c| c.iter().filter_map(|a| a["value_name"].as_str()).collect::<Vec<_>>().join(" / "))
                        .unwrap_or_default(),
                    sku: sku_de(var),
                    estoque_ml: var["available_quantity"].as_i64().unwrap_or(0),
                    vendidos: var["sold_quantity"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Anuncio {
        id,
        titulo: v["title"].as_str().unwrap_or("(sem título)").to_string(),
        preco: v["price"].as_f64().unwrap_or(0.0),
        estoque_ml: v["available_quantity"].as_i64().unwrap_or(0),
        vendidos: v["sold_quantity"].as_i64().unwrap_or(0),
        status: v["status"].as_str().unwrap_or("").to_string(),
        link: v["permalink"].as_str().map(String::from),
        foto: v["secure_thumbnail"]
            .as_str()
            .or_else(|| v["thumbnail"].as_str())
            .map(|f| f.replacen("http://", "https://", 1)),
        sku: sku_de(v),
        full: v["shipping"]["logistic_type"].as_str() == Some("fulfillment"),
        variacoes,
        visitas: None,
    })
}

pub async fn id_do_vendedor(token: &str) -> Result<i64, String> {
    get_json(token, "/users/me")
        .await?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| "O Mercado Livre não informou o id da conta.".to_string())
}

/// Ids de todos os anúncios da conta (ativos, pausados e encerrados).
async fn ids_dos_anuncios(token: &str, vendedor: i64) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    // Paginação comum (até 1.000 resultados)
    let mut offset = 0;
    let mut total;
    loop {
        let v = get_json(token, &format!("/users/{vendedor}/items/search?{}", q(&[("offset", &offset.to_string()), ("limit", "50")]))).await?;
        total = v["paging"]["total"].as_u64().unwrap_or(0) as usize;
        let lote: Vec<String> = v["results"].as_array().map(|r| r.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if lote.is_empty() {
            break;
        }
        ids.extend(lote);
        offset += 50;
        if offset >= total || offset >= 1000 {
            break;
        }
    }
    // Mais de 1.000 anúncios: o ML exige o modo "scan" (com scroll_id).
    if total > 1000 {
        ids.clear();
        let mut scroll: Option<String> = None;
        loop {
            let mut pares = vec![("search_type", "scan".to_string()), ("limit", "100".to_string())];
            if let Some(s) = &scroll {
                pares.push(("scroll_id", s.clone()));
            }
            let pares_ref: Vec<(&str, &str)> = pares.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let v = get_json(token, &format!("/users/{vendedor}/items/search?{}", q(&pares_ref))).await?;
            let lote: Vec<String> = v["results"].as_array().map(|r| r.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            if lote.is_empty() || ids.len() > 20_000 {
                break;
            }
            ids.extend(lote);
            scroll = v["scroll_id"].as_str().map(String::from);
            if scroll.is_none() {
                break;
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn detalhar(token: &str, ids: &[String]) -> Result<Vec<Anuncio>, String> {
    let mut lista = Vec::with_capacity(ids.len());
    for grupo in ids.chunks(20) {
        let v = get_json(token, &format!("/items?{}", q(&[("ids", &grupo.join(",")), ("include_attributes", "all")]))).await?;
        for item in v.as_array().into_iter().flatten() {
            if item["code"].as_i64().unwrap_or(200) == 200
                && let Some(a) = anuncio_de_json(&item["body"]) {
                    lista.push(a);
                }
        }
    }
    Ok(lista)
}

/// Visitas totais por anúncio. Se o ML recusar, segue sem visitas (não é essencial).
async fn visitas(token: &str, ids: &[String]) -> HashMap<String, i64> {
    let mut mapa = HashMap::new();
    for grupo in ids.chunks(50) {
        match get_json(token, &format!("/visits/items?{}", q(&[("ids", &grupo.join(","))]))).await {
            Ok(Value::Object(o)) => {
                for (k, v) in o {
                    if let Some(n) = v.as_i64() {
                        mapa.insert(k, n);
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Visitas indisponíveis: {e}");
                break;
            }
        }
    }
    mapa
}

pub async fn carregar_anuncios(token: &str) -> Result<Vec<Anuncio>, String> {
    let vendedor = id_do_vendedor(token).await?;
    let ids = ids_dos_anuncios(token, vendedor).await?;
    let mut anuncios = detalhar(token, &ids).await?;
    let v = visitas(token, &ids).await;
    for a in &mut anuncios {
        a.visitas = v.get(&a.id).copied();
    }
    anuncios.sort_by(|a, b| b.vendidos.cmp(&a.vendidos).then_with(|| a.titulo.cmp(&b.titulo)));
    Ok(anuncios)
}

// ---------------------------------------------------------------------------
// Pedidos e rendimento
// ---------------------------------------------------------------------------

/// Busca os pedidos do período (mais recentes primeiro). Devolve (pedidos, truncado?).
pub async fn pedidos(token: &str, de: DateTime<Utc>, ate: DateTime<Utc>) -> Result<(Vec<Value>, bool), String> {
    let vendedor = id_do_vendedor(token).await?;
    let fmt = |d: DateTime<Utc>| d.format("%Y-%m-%dT%H:%M:%S%.3f-00:00").to_string();
    let (de_s, ate_s) = (fmt(de), fmt(ate));
    let mut todos = Vec::new();
    let mut offset = 0usize;
    loop {
        let consulta = q(&[
            ("seller", &vendedor.to_string()),
            ("order.date_created.from", &de_s),
            ("order.date_created.to", &ate_s),
            ("sort", "date_desc"),
            ("offset", &offset.to_string()),
            ("limit", "50"),
        ]);
        let v = get_json(token, &format!("/orders/search?{consulta}")).await?;
        let lote = v["results"].as_array().cloned().unwrap_or_default();
        let total = v["paging"]["total"].as_u64().unwrap_or(0) as usize;
        if lote.is_empty() {
            break;
        }
        todos.extend(lote);
        offset += 50;
        if offset >= total {
            break;
        }
        if todos.len() >= MAX_PEDIDOS {
            return Ok((todos, true));
        }
    }
    Ok((todos, false))
}

#[derive(Debug, Serialize, Default)]
pub struct Dia {
    pub dia: String,
    pub pedidos: i64,
    pub unidades: i64,
    pub faturamento: f64,
}

#[derive(Debug, Serialize, Default)]
pub struct PorAnuncio {
    #[serde(rename = "meliItemId")]
    pub meli_item_id: String,
    pub titulo: String,
    pub sku: Option<String>,
    pub unidades: i64,
    pub faturamento: f64,
    pub tarifas: f64,
    /// Custo do produto vinculado (null = sem vínculo ou custo 0).
    #[serde(rename = "custoTotal")]
    pub custo_total: Option<f64>,
    #[serde(rename = "lucroEstimado")]
    pub lucro_estimado: Option<f64>,
}

#[derive(Debug, Serialize, Default)]
pub struct Rendimento {
    pub de: String,
    pub ate: String,
    pub pedidos: i64,
    pub cancelados: i64,
    pub unidades: i64,
    pub faturamento: f64,
    pub tarifas: f64,
    #[serde(rename = "ticketMedio")]
    pub ticket_medio: f64,
    /// Soma só dos anúncios com custo conhecido.
    #[serde(rename = "lucroEstimado")]
    pub lucro_estimado: f64,
    #[serde(rename = "faturamentoComCusto")]
    pub faturamento_com_custo: f64,
    #[serde(rename = "porDia")]
    pub por_dia: Vec<Dia>,
    #[serde(rename = "porAnuncio")]
    pub por_anuncio: Vec<PorAnuncio>,
    pub truncado: bool,
}

/// Junta os pedidos em totais, por dia (horário de Brasília) e por anúncio.
/// `custos` = custo unitário (em reais) do produto vinculado a cada anúncio.
pub fn resumir(pedidos: &[Value], custos: &HashMap<String, f64>, de: DateTime<Utc>, ate: DateTime<Utc>, truncado: bool) -> Rendimento {
    let brasilia = FixedOffset::west_opt(3 * 3600).expect("fuso válido");
    let mut r = Rendimento {
        de: de.with_timezone(&brasilia).format("%Y-%m-%d").to_string(),
        ate: ate.with_timezone(&brasilia).format("%Y-%m-%d").to_string(),
        truncado,
        ..Default::default()
    };
    let mut dias: HashMap<String, Dia> = HashMap::new();
    let mut por: HashMap<String, PorAnuncio> = HashMap::new();
    for p in pedidos {
        if p["status"].as_str() == Some("cancelled") {
            r.cancelados += 1;
            continue;
        }
        let dia = p["date_created"]
            .as_str()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
            .map(|d| d.with_timezone(&brasilia).format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let entrada = dias.entry(dia.clone()).or_insert_with(|| Dia { dia, ..Default::default() });
        entrada.pedidos += 1;
        r.pedidos += 1;
        for it in p["order_items"].as_array().into_iter().flatten() {
            let qtd = it["quantity"].as_i64().unwrap_or(0);
            let preco = it["unit_price"].as_f64().unwrap_or(0.0);
            // sale_fee: tarifa de venda POR UNIDADE informada pelo ML (não inclui frete).
            let tarifa = it["sale_fee"].as_f64().unwrap_or(0.0);
            let item_id = it["item"]["id"].as_str().unwrap_or("").to_string();
            let fat = preco * qtd as f64;
            r.unidades += qtd;
            r.faturamento += fat;
            r.tarifas += tarifa * qtd as f64;
            entrada.unidades += qtd;
            entrada.faturamento += fat;
            let a = por.entry(item_id.clone()).or_insert_with(|| PorAnuncio {
                meli_item_id: item_id.clone(),
                titulo: it["item"]["title"].as_str().unwrap_or("").to_string(),
                sku: it["item"]["seller_sku"].as_str().filter(|s| !s.is_empty()).map(String::from),
                ..Default::default()
            });
            a.unidades += qtd;
            a.faturamento += fat;
            a.tarifas += tarifa * qtd as f64;
        }
    }
    for a in por.values_mut() {
        if let Some(c) = custos.get(&a.meli_item_id).filter(|c| **c > 0.0) {
            let custo = c * a.unidades as f64;
            a.custo_total = Some(custo);
            let lucro = a.faturamento - a.tarifas - custo;
            a.lucro_estimado = Some(lucro);
            r.lucro_estimado += lucro;
            r.faturamento_com_custo += a.faturamento;
        }
    }
    r.ticket_medio = if r.pedidos > 0 { r.faturamento / r.pedidos as f64 } else { 0.0 };
    let mut por_dia: Vec<Dia> = dias.into_values().collect();
    por_dia.sort_by(|a, b| a.dia.cmp(&b.dia));
    r.por_dia = por_dia;
    let mut por_anuncio: Vec<PorAnuncio> = por.into_values().collect();
    por_anuncio.sort_by(|a, b| b.faturamento.total_cmp(&a.faturamento));
    r.por_anuncio = por_anuncio;
    r
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn le_anuncio_com_sku_e_variacoes() {
        let simples = json!({"id":"MLB1","title":"Camiseta","price":49.9,"available_quantity":7,"sold_quantity":30,
            "status":"active","permalink":"https://x","thumbnail":"http://foto","attributes":[{"id":"SELLER_SKU","value_name":"CAM-01"}],
            "shipping":{"logistic_type":"cross_docking"},"variations":[]});
        let a = anuncio_de_json(&simples).unwrap();
        assert_eq!(a.sku.as_deref(), Some("CAM-01"));
        assert_eq!(a.foto.as_deref(), Some("https://foto"));
        assert!(!a.full && a.variacoes.is_empty());

        let var = json!({"id":"MLB2","title":"Tênis","price":199.0,"available_quantity":5,"sold_quantity":2,"status":"paused",
            "seller_custom_field":"TEN-X","shipping":{"logistic_type":"fulfillment"},
            "variations":[{"id":123,"available_quantity":3,"sold_quantity":1,"attribute_combinations":[{"value_name":"Preto"},{"value_name":"40"}],
                "attributes":[{"id":"SELLER_SKU","value_name":"TEN-P-40"}]}]});
        let b = anuncio_de_json(&var).unwrap();
        assert_eq!(b.sku.as_deref(), Some("TEN-X"));
        assert!(b.full);
        assert_eq!(b.variacoes[0].nome, "Preto / 40");
        assert_eq!(b.variacoes[0].sku.as_deref(), Some("TEN-P-40"));
        assert_eq!(b.variacoes[0].id, "123");
    }

    #[test]
    fn resume_pedidos() {
        let de = "2026-11-01T03:00:00Z".parse().unwrap();
        let ate = "2026-11-14T03:00:00Z".parse().unwrap();
        let pedidos = vec![
            json!({"status":"paid","date_created":"2026-11-10T23:30:00.000-03:00","order_items":[
                {"item":{"id":"MLB1","title":"Camiseta","seller_sku":"CAM-01"},"quantity":2,"unit_price":50.0,"sale_fee":7.5}]}),
            json!({"status":"paid","date_created":"2026-11-11T10:00:00.000-03:00","order_items":[
                {"item":{"id":"MLB1","title":"Camiseta","seller_sku":"CAM-01"},"quantity":1,"unit_price":50.0,"sale_fee":7.5},
                {"item":{"id":"MLB9","title":"Boné","seller_sku":null},"quantity":1,"unit_price":30.0,"sale_fee":4.0}]}),
            json!({"status":"cancelled","date_created":"2026-11-11T11:00:00.000-03:00","order_items":[]}),
        ];
        let custos = HashMap::from([("MLB1".to_string(), 20.0)]);
        let r = resumir(&pedidos, &custos, de, ate, false);
        assert_eq!((r.pedidos, r.cancelados, r.unidades), (2, 1, 4));
        assert_eq!(r.faturamento, 180.0);
        assert_eq!(r.tarifas, 26.5);
        assert_eq!(r.ticket_medio, 90.0);
        // Camiseta: 150 - 22.5 de tarifa - 60 de custo = 67.5 ; Boné sem custo não entra no lucro
        assert_eq!(r.lucro_estimado, 67.5);
        assert_eq!(r.por_anuncio[0].meli_item_id, "MLB1");
        assert_eq!(r.por_dia.iter().map(|d| d.dia.as_str()).collect::<Vec<_>>(), vec!["2026-11-10", "2026-11-11"]);
    }
}
