//! Banco SQLite: abertura, migrações e pequenos auxiliares.

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Sqlite, Transaction};
use std::path::Path;
use std::time::Duration;

pub type Banco = SqlitePool;

/// Abre (ou cria) o arquivo do banco e aplica as migrações pendentes.
pub async fn abrir(arquivo: &Path) -> Result<Banco, String> {
    if let Some(pasta) = arquivo.parent() {
        std::fs::create_dir_all(pasta)
            .map_err(|e| format!("Não foi possível criar a pasta {}: {e}", pasta.display()))?;
    }
    let opcoes = SqliteConnectOptions::new()
        .filename(arquivo)
        .create_if_missing(true)
        .foreign_keys(true) // chaves estrangeiras valem de verdade
        .journal_mode(SqliteJournalMode::Wal) // leitores não travam quem grava
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opcoes)
        .await
        .map_err(|e| format!("Não foi possível abrir o banco {}: {e}", arquivo.display()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| format!("Falha ao atualizar a estrutura do banco: {e}"))?;
    Ok(pool)
}

/// Transação de ESCRITA. `BEGIN IMMEDIATE` reserva a gravação logo no início: duas operações
/// que leem e depois gravam (ex.: checar capacidade e dar entrada) entram em fila, em vez de
/// uma delas falhar com "database is locked" no meio do caminho.
pub async fn transacao_escrita(pool: &Banco) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

pub fn agora_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn novo_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Reais (como chega no JSON) -> centavos (como fica no banco).
pub fn para_centavos(reais: f64) -> i64 {
    (reais * 100.0).round() as i64
}

/// Centavos -> reais (para o JSON).
pub fn de_centavos(centavos: i64) -> f64 {
    centavos as f64 / 100.0
}

// ---------- configurações gravadas no banco (tabela settings) ----------

pub async fn ler_config(pool: &Banco, chave: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(chave)
        .fetch_optional(pool)
        .await
}

pub async fn gravar_config(pool: &Banco, chave: &str, valor: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(chave)
    .bind(valor)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn apagar_config(pool: &Banco, chave: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(chave)
        .execute(pool)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn centavos_ida_e_volta() {
        assert_eq!(para_centavos(299.9), 29990);
        assert_eq!(para_centavos(0.1 + 0.2), 30); // 0.30000000000000004 não vira 30,000...01
        assert_eq!(de_centavos(29990), 299.9);
    }
}
