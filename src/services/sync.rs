//! Fila de sincronização de estoque com o Mercado Livre.
//!
//! Antes: cada movimentação disparava uma tarefa solta. Duas movimentações quase juntas podiam
//! chegar ao Mercado Livre fora de ordem (o anúncio ficava com o total ANTIGO), duas renovações
//! de token podiam acontecer ao mesmo tempo, e uma falha só aparecia no terminal.
//!
//! Agora: UMA fila, processada UM produto por vez. O total é lido do banco na hora de enviar,
//! então o último envio sempre leva o saldo mais recente. O resultado de cada anúncio fica
//! gravado (tela Mercado Livre) e quem falhou é tentado de novo a cada 10 minutos.

use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::sync::mpsc;

use crate::{cripto::Cofre, db::{self, Banco}, services::meli};

const INTERVALO_NOVA_TENTATIVA: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub struct Sincronizador {
    fila: mpsc::UnboundedSender<String>,
}

impl Sincronizador {
    pub fn iniciar(pool: Banco, cofre: Arc<Cofre>) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        let (pool_w, cofre_w) = (pool.clone(), cofre.clone());
        tokio::spawn(async move {
            while let Some(primeiro) = rx.recv().await {
                // junta o que chegou enquanto isso: cada produto é enviado uma vez só
                let mut pendentes = BTreeSet::from([primeiro]);
                while let Ok(outro) = rx.try_recv() {
                    pendentes.insert(outro);
                }
                for produto in pendentes {
                    sincronizar_produto(&pool_w, &cofre_w, &produto).await;
                }
            }
        });

        // Nova tentativa para anúncios que falharam (ex.: internet caiu).
        let fila = tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(INTERVALO_NOVA_TENTATIVA).await;
                let conectado: Option<i64> =
                    sqlx::query_scalar("SELECT needs_reconnect FROM meli_credentials WHERE id = 1")
                        .fetch_optional(&pool)
                        .await
                        .ok()
                        .flatten();
                if conectado != Some(0) {
                    continue; // sem conta ou precisando reconectar: tentar não adianta
                }
                let falhos: Vec<String> =
                    sqlx::query_scalar("SELECT DISTINCT product_id FROM meli_links WHERE last_sync_ok = 0")
                        .fetch_all(&pool)
                        .await
                        .unwrap_or_default();
                for p in falhos {
                    let _ = fila.send(p);
                }
            }
        });

        Self { fila: tx }
    }

    /// Coloca o produto na fila (não espera: a movimentação responde na hora).
    pub fn agendar(&self, produto_id: &str) {
        let _ = self.fila.send(produto_id.to_string());
    }
}

async fn registrar(pool: &Banco, link_id: &str, ok: bool, qtd: Option<i64>, erro: Option<&str>) {
    let r = sqlx::query(
        "UPDATE meli_links SET last_sync_at = ?, last_sync_ok = ?, last_sync_qty = COALESCE(?, last_sync_qty), last_sync_error = ? WHERE id = ?",
    )
    .bind(db::agora_iso())
    .bind(ok as i64)
    .bind(qtd)
    .bind(erro)
    .bind(link_id)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::error!("Falha ao registrar sincronização: {e}");
    }
}

async fn sincronizar_produto(pool: &Banco, cofre: &Cofre, produto_id: &str) {
    let links: Vec<(String, String)> =
        match sqlx::query_as("SELECT id, meli_item_id FROM meli_links WHERE product_id = ?")
            .bind(produto_id)
            .fetch_all(pool)
            .await
        {
            Ok(l) => l,
            Err(e) => return tracing::error!("Erro ao buscar anúncios vinculados: {e}"),
        };
    if links.is_empty() {
        return;
    }
    let total: i64 = match sqlx::query_scalar("SELECT COALESCE(SUM(quantity), 0) FROM stocks WHERE product_id = ?")
        .bind(produto_id)
        .fetch_one(pool)
        .await
    {
        Ok(t) => t,
        Err(e) => return tracing::error!("Erro ao somar o estoque do produto {produto_id}: {e}"),
    };

    let token = match meli::token_valido(pool, cofre).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Sincronização adiada: {e}");
            for (link_id, _) in &links {
                registrar(pool, link_id, false, None, Some(&e)).await;
            }
            return;
        }
    };
    for (link_id, item) in links {
        match meli::atualizar_anuncio(&token, &item, total).await {
            Ok(()) => {
                tracing::info!("Anúncio {item} sincronizado ({total} un.)");
                registrar(pool, &link_id, true, Some(total), None).await;
            }
            Err(e) => {
                tracing::error!("Falha ao sincronizar {item}: {e}");
                registrar(pool, &link_id, false, None, Some(&e)).await;
            }
        }
    }
}
