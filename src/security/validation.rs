//! Validação de entrada. Regra de ouro: NUNCA confiar no que vem do cliente.
//! Cada função devolve `Err((400, mensagem))` se algo estiver fora do permitido.

use crate::security::error::{ApiError, bad_request};

pub const MAX_QTD: i64 = 1_000_000;
const MAX_REAIS: f64 = 1_000_000_000.0;

pub fn texto(campo: &str, valor: &str, max: usize) -> Result<(), ApiError> {
    if valor.trim().is_empty() {
        return Err(bad_request(&format!("{campo} é obrigatório.")));
    }
    if valor.chars().count() > max {
        return Err(bad_request(&format!("{campo} excede {max} caracteres.")));
    }
    if valor.chars().any(|c| c.is_control()) {
        return Err(bad_request(&format!("{campo} contém caracteres inválidos.")));
    }
    Ok(())
}

/// Identificadores (UUIDs gerados pelo sistema) têm no máximo 64 caracteres.
pub fn id(campo: &str, valor: &str) -> Result<(), ApiError> {
    texto(campo, valor, 64)
}

pub fn dinheiro(campo: &str, valor: f64) -> Result<(), ApiError> {
    if !valor.is_finite() || !(0.0..=MAX_REAIS).contains(&valor) {
        return Err(bad_request(&format!("{campo} deve estar entre 0 e 1.000.000.000.")));
    }
    Ok(())
}

pub fn quantidade(campo: &str, valor: i64) -> Result<(), ApiError> {
    if !(1..=MAX_QTD).contains(&valor) {
        return Err(bad_request(&format!("{campo} deve estar entre 1 e 1.000.000.")));
    }
    Ok(())
}

pub fn sku(valor: &str) -> Result<(), ApiError> {
    texto("sku", valor, 64)?;
    if !valor.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        return Err(bad_request("sku aceita apenas letras, números, '-', '_' e '.'."));
    }
    Ok(())
}

/// E-mail: formato básico (algo@dominio.ext), sem espaços. Devolve em minúsculas.
pub fn email(valor: &str) -> Result<String, ApiError> {
    let e = valor.trim().to_lowercase();
    let invalido = || bad_request("E-mail inválido.");
    if e.len() < 5 || e.len() > 254 || e.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalido());
    }
    let (local, dominio) = e.split_once('@').ok_or_else(invalido)?;
    if local.is_empty()
        || dominio.contains('@')
        || !dominio.contains('.')
        || dominio.starts_with('.')
        || dominio.ends_with('.')
    {
        return Err(invalido());
    }
    Ok(e)
}

pub fn senha(valor: &str) -> Result<(), ApiError> {
    if !(10..=128).contains(&valor.chars().count()) {
        return Err(bad_request("A senha deve ter entre 10 e 128 caracteres."));
    }
    Ok(())
}

pub fn papel(valor: &str) -> Result<(), ApiError> {
    if valor != "admin" && valor != "operador" {
        return Err(bad_request("role deve ser \"admin\" ou \"operador\"."));
    }
    Ok(())
}

/// ID de anúncio: "ML" + letra do país + dígitos (ex.: MLB123456789). Devolve em maiúsculas.
pub fn anuncio_meli(valor: &str) -> Result<String, ApiError> {
    let s = valor.trim().to_uppercase();
    let b = s.as_bytes();
    let ok = (6..=20).contains(&s.len())
        && s.starts_with("ML")
        && b[2].is_ascii_uppercase()
        && b[3..].iter().all(|c| c.is_ascii_digit());
    if ok { Ok(s) } else { Err(bad_request("ID do anúncio inválido. Exemplo: MLB123456789.")) }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(email(" Admin@Empresa.com ").unwrap(), "admin@empresa.com");
        for ruim in ["sem-arroba", "a@b", "@empresa.com", "a@.com", "a b@empresa.com", "a@@b.com"] {
            assert!(email(ruim).is_err(), "{ruim} deveria ser recusado");
        }
    }

    #[test]
    fn anuncios() {
        assert_eq!(anuncio_meli("mlb123456").unwrap(), "MLB123456");
        assert!(anuncio_meli("MLB12a").is_err());
        assert!(anuncio_meli("abc&x=1").is_err());
    }
}
