//! Comunicação com o Mercado Livre: autorização (OAuth), renovação do token e atualização de estoque.
//!
//! As credenciais do app (App ID, Secret e URI de redirecionamento) vêm da tela de configuração
//! e ficam no banco. O Secret e os tokens da conta são guardados CIFRADOS (ver `cripto.rs`).

use chrono::{DateTime, Duration as Dur, Utc};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use url::form_urlencoded;

use crate::{cripto::Cofre, db::{self, Banco}};

pub const CHAVE_APP_ID: &str = "meli_app_id";
pub const CHAVE_SECRET: &str = "meli_secret_enc";
pub const CHAVE_REDIRECT: &str = "meli_redirect_uri";
pub const REDIRECT_PADRAO: &str = "https://google.com";

fn url_token() -> String {
    format!("{}/oauth/token", super::meli_leitura::base())
}
fn url_itens() -> String {
    format!("{}/items", super::meli_leitura::base())
}
const URL_AUTORIZACAO: &str = "https://auth.mercadolivre.com.br/authorization";

pub struct AppMeli {
    pub app_id: String,
    pub secret: String,
    pub redirect_uri: String,
}

#[derive(Deserialize, Debug)]
pub struct TokenMeli {
    pub access_token: String,
    /// Só vem quando o app tem a permissão "offline_access" no DevCenter.
    /// Sem ele, a conexão vale ~6 horas e depois precisa ser refeita.
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}

pub const AVISO_SEM_RENOVACAO: &str = "O Mercado Livre não enviou o token de renovação: o app está sem a permissão \
    offline_access. Ative-a no DevCenter (permissões do app) e reconecte a conta; sem ela, a conexão expira em ~6 horas.";

/// Lê a resposta do token. Em caso de erro, diz QUAIS campos vieram, nunca os valores (são segredos).
pub fn ler_resposta_token(texto: &str) -> Result<TokenMeli, String> {
    serde_json::from_str::<TokenMeli>(texto).map_err(|e| {
        let campos = serde_json::from_str::<serde_json::Value>(texto)
            .ok()
            .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(", ")))
            .unwrap_or_else(|| "resposta não é JSON".to_string());
        format!("Resposta inesperada do Mercado Livre ({e}). Campos recebidos: {campos}")
    })
}

/// Cliente HTTP com TIMEOUT (sem ele, um Mercado Livre lento travava a fila para sempre).
fn http() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Erro ao criar cliente HTTP: {e}"))
}

/// Monta "a=1&b=2" com encode correto em cada valor (um `code` com & ou = não injeta campos).
fn form(pares: &[(&str, &str)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (chave, valor) in pares {
        s.append_pair(chave, valor);
    }
    s.finish()
}

fn resumir(texto: &str) -> String {
    let t: String = texto.chars().filter(|c| !c.is_control()).take(300).collect();
    if t.is_empty() { "(sem detalhes)".into() } else { t }
}

pub async fn carregar_app(pool: &Banco, cofre: &Cofre) -> Result<Option<AppMeli>, String> {
    let app_id = db::ler_config(pool, CHAVE_APP_ID).await.map_err(|e| e.to_string())?;
    let secret = db::ler_config(pool, CHAVE_SECRET).await.map_err(|e| e.to_string())?;
    let redirect = db::ler_config(pool, CHAVE_REDIRECT).await.map_err(|e| e.to_string())?;
    match (app_id, secret) {
        (Some(app_id), Some(secret)) if !app_id.is_empty() => Ok(Some(AppMeli {
            app_id,
            secret: cofre.decifrar(&secret)?,
            redirect_uri: redirect.unwrap_or_else(|| REDIRECT_PADRAO.to_string()),
        })),
        _ => Ok(None),
    }
}

pub async fn exigir_app(pool: &Banco, cofre: &Cofre) -> Result<AppMeli, String> {
    carregar_app(pool, cofre)
        .await?
        .ok_or_else(|| "Configure o App ID e o Secret do Mercado Livre (tela Configurações).".to_string())
}

pub fn url_autorizacao(app: &AppMeli, state: &str) -> String {
    let query = form(&[
        ("response_type", "code"),
        ("client_id", &app.app_id),
        ("redirect_uri", &app.redirect_uri),
        ("state", state),
    ]);
    format!("{URL_AUTORIZACAO}?{query}")
}

async fn pedir_token(corpo: String) -> Result<TokenMeli, (Option<StatusCode>, String)> {
    let res = http()
        .map_err(|e| (None, e))?
        .post(url_token())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(corpo)
        .send()
        .await
        .map_err(|e| (None, format!("Sem conexão com o Mercado Livre: {e}")))?;
    let status = res.status();
    let texto = res.text().await.unwrap_or_default();
    if status.is_success() {
        ler_resposta_token(&texto).map_err(|e| (Some(status), e))
    } else {
        Err((Some(status), format!("Mercado Livre recusou ({status}): {}", resumir(&texto))))
    }
}

/// Troca o `code` (que volta da autorização) pelos tokens.
pub async fn trocar_codigo(app: &AppMeli, code: &str) -> Result<TokenMeli, String> {
    pedir_token(form(&[
        ("grant_type", "authorization_code"),
        ("client_id", &app.app_id),
        ("client_secret", &app.secret),
        ("code", code),
        ("redirect_uri", &app.redirect_uri),
    ]))
    .await
    .map_err(|(_, e)| e)
}

/// Grava os tokens. Sem `refresh_token` na resposta, guarda vazio e deixa o aviso registrado.
pub async fn salvar_credenciais(pool: &Banco, cofre: &Cofre, t: &TokenMeli) -> Result<(), String> {
    let expira = (Utc::now() + Dur::seconds(t.expires_in)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let renovacao = t.refresh_token.as_deref().unwrap_or("");
    let aviso = if renovacao.is_empty() { Some(AVISO_SEM_RENOVACAO) } else { None };
    sqlx::query(
        r#"INSERT INTO meli_credentials (id, access_token_enc, refresh_token_enc, expires_at, needs_reconnect, last_error, updated_at)
           VALUES (1, ?, ?, ?, 0, ?, ?)
           ON CONFLICT(id) DO UPDATE SET access_token_enc = excluded.access_token_enc,
               refresh_token_enc = excluded.refresh_token_enc, expires_at = excluded.expires_at,
               needs_reconnect = 0, last_error = excluded.last_error, updated_at = excluded.updated_at"#,
    )
    .bind(cofre.cifrar(&t.access_token)?)
    .bind(cofre.cifrar(renovacao)?)
    .bind(expira)
    .bind(aviso)
    .bind(db::agora_iso())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

async fn registrar_erro(pool: &Banco, erro: &str, reconectar: bool) {
    let _ = sqlx::query(
        "UPDATE meli_credentials SET last_error = ?, needs_reconnect = MAX(needs_reconnect, ?), updated_at = ? WHERE id = 1",
    )
    .bind(erro)
    .bind(reconectar as i64)
    .bind(db::agora_iso())
    .execute(pool)
    .await;
}

/// Devolve um token de acesso válido, renovando se faltar menos de 5 minutos.
/// Se o Mercado Livre recusar a renovação (conta desconectada, permissão revogada), marca
/// `needs_reconnect` e a tela passa a pedir para reconectar. Antes, o status dizia
/// "conectado" mesmo com a renovação falhando.
pub async fn token_valido(pool: &Banco, cofre: &Cofre) -> Result<String, String> {
    let linha: Option<(String, String, String, i64)> = sqlx::query_as(
        "SELECT access_token_enc, refresh_token_enc, expires_at, needs_reconnect FROM meli_credentials WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let (acesso, renovacao, expira, reconectar) =
        linha.ok_or_else(|| "Conta do Mercado Livre não conectada.".to_string())?;
    if reconectar == 1 {
        return Err("A conexão com o Mercado Livre expirou. Reconecte a conta.".into());
    }
    let expira = DateTime::parse_from_rfc3339(&expira).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
    if expira > Utc::now() + Dur::minutes(5) {
        return cofre.decifrar(&acesso);
    }

    tracing::info!("Token do Mercado Livre vencendo: renovando...");
    let renovacao = cofre.decifrar(&renovacao)?;
    if renovacao.is_empty() {
        registrar_erro(pool, AVISO_SEM_RENOVACAO, true).await;
        return Err(AVISO_SEM_RENOVACAO.to_string());
    }
    let app = exigir_app(pool, cofre).await?;
    match pedir_token(form(&[
        ("grant_type", "refresh_token"),
        ("client_id", &app.app_id),
        ("client_secret", &app.secret),
        ("refresh_token", &renovacao),
    ]))
    .await
    {
        Ok(t) => {
            salvar_credenciais(pool, cofre, &t).await?;
            tracing::info!("Token do Mercado Livre renovado.");
            Ok(t.access_token)
        }
        Err((status, erro)) => {
            // 400/401 = renovação recusada de vez. Outros (rede, 5xx) = tentar de novo depois.
            let definitivo = matches!(status, Some(StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED));
            registrar_erro(pool, &erro, definitivo).await;
            Err(erro)
        }
    }
}

/// Atualiza a quantidade disponível de UM anúncio.
pub async fn atualizar_anuncio(token: &str, item_id: &str, quantidade: i64) -> Result<(), String> {
    let res = http()?
        .put(format!("{}/{item_id}", url_itens()))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "available_quantity": quantidade }))
        .send()
        .await
        .map_err(|e| format!("Sem conexão com o Mercado Livre: {e}"))?;
    if res.status().is_success() {
        return Ok(());
    }
    let status = res.status();
    let texto = res.text().await.unwrap_or_default();
    let dica = if texto.contains("variations") {
        " (anúncio com variações: o sistema ainda não atualiza variações)"
    } else {
        ""
    };
    Err(format!("Mercado Livre recusou ({status}){dica}: {}", resumir(&texto)))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn resposta_com_e_sem_refresh_token() {
        let com = ler_resposta_token(r#"{"access_token":"APP_USR-a","token_type":"Bearer","expires_in":21600,"scope":"offline_access read write","user_id":123,"refresh_token":"TG-r"}"#).unwrap();
        assert_eq!(com.refresh_token.as_deref(), Some("TG-r"));
        let sem = ler_resposta_token(r#"{"access_token":"APP_USR-a","token_type":"Bearer","expires_in":21600,"scope":"read write","user_id":123}"#).unwrap();
        assert!(sem.refresh_token.is_none());
        // Erro diz os campos, nunca os valores
        let erro = ler_resposta_token(r#"{"token":"SEGREDO","expires_in":1}"#).unwrap_err();
        assert!(erro.contains("access_token") && erro.contains("expires_in, token") && !erro.contains("SEGREDO"), "{erro}");
    }
}
