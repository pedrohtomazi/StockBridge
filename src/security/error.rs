use axum::http::StatusCode;
use std::fmt::Display;

/// Tipo de erro usado por todos os handlers: (código HTTP, mensagem para o cliente).
pub type ApiError = (StatusCode, String);

/// Erro inesperado. O DETALHE vai para o log do servidor; o cliente só vê texto genérico.
pub fn internal<E: Display>(e: E) -> ApiError {
    tracing::error!("erro interno: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Erro interno do servidor.".to_string())
}

pub fn bad_request(msg: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

pub fn conflito(msg: &str) -> ApiError {
    (StatusCode::CONFLICT, msg.to_string())
}

pub fn nao_encontrado(msg: &str) -> ApiError {
    (StatusCode::NOT_FOUND, msg.to_string())
}

/// Traduz erros do banco para respostas seguras e úteis.
pub fn db_error(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(d) = &e {
        if d.is_foreign_key_violation() {
            return bad_request("Referência inválida: produto, estante ou depósito inexistente.");
        }
        if d.is_unique_violation() {
            return conflito("Já existe um registro com esses dados.");
        }
        if d.is_check_violation() {
            return bad_request("Os dados enviados violam as regras do sistema.");
        }
    }
    internal(e)
}
