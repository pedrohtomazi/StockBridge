//! Cofre: cifra o "secret" do app do Mercado Livre e os tokens da conta antes de gravar no banco.
//!
//! A chave fica em `dados/chave.bin`, separada do banco. Assim, um backup do banco que vaze
//! (pendrive perdido, cópia por e-mail) não entrega o acesso à conta do Mercado Livre.
//! Quem tiver acesso total a este computador consegue as duas coisas: isso vale para qualquer
//! programa de desktop e não há como evitar.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng},
};
use std::path::Path;

pub struct Cofre {
    cifra: ChaCha20Poly1305,
}

impl Cofre {
    pub fn carregar_ou_criar(dados: &Path) -> Result<Self, String> {
        let arquivo = dados.join("chave.bin");
        let chave = match std::fs::read_to_string(&arquivo)
            .ok()
            .and_then(|t| B64.decode(t.trim()).ok())
            .filter(|b| b.len() == 32)
        {
            Some(bytes) => *Key::from_slice(&bytes),
            None => {
                if arquivo.exists() {
                    return Err(format!(
                        "O arquivo {} está corrompido. Restaure-o do backup: sem ele não dá para ler \
                         as credenciais do Mercado Livre.",
                        arquivo.display()
                    ));
                }
                let nova = ChaCha20Poly1305::generate_key(&mut OsRng);
                std::fs::create_dir_all(dados).map_err(|e| e.to_string())?;
                std::fs::write(&arquivo, B64.encode(nova))
                    .map_err(|e| format!("Não foi possível gravar {}: {e}", arquivo.display()))?;
                nova
            }
        };
        Ok(Self { cifra: ChaCha20Poly1305::new(&chave) })
    }

    /// Chave aleatória só em memória (testes).
    pub fn temporario() -> Self {
        Self { cifra: ChaCha20Poly1305::new(&ChaCha20Poly1305::generate_key(&mut OsRng)) }
    }

    /// Texto -> base64(nonce || cifrado)
    pub fn cifrar(&self, texto: &str) -> Result<String, String> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let cifrado = self
            .cifra
            .encrypt(&nonce, texto.as_bytes())
            .map_err(|_| "falha ao cifrar".to_string())?;
        let mut tudo = nonce.to_vec();
        tudo.extend_from_slice(&cifrado);
        Ok(B64.encode(tudo))
    }

    pub fn decifrar(&self, guardado: &str) -> Result<String, String> {
        let bytes = B64.decode(guardado).map_err(|_| "dado cifrado inválido".to_string())?;
        if bytes.len() < 12 {
            return Err("dado cifrado inválido".into());
        }
        let (nonce, cifrado) = bytes.split_at(12);
        let aberto = self
            .cifra
            .decrypt(Nonce::from_slice(nonce), cifrado)
            .map_err(|_| "não foi possível decifrar (a chave.bin foi trocada?)".to_string())?;
        String::from_utf8(aberto).map_err(|_| "dado cifrado inválido".into())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cifra_e_decifra() {
        let c = Cofre::temporario();
        let guardado = c.cifrar("APP_USR-segredo").unwrap();
        assert!(!guardado.contains("segredo"));
        assert_eq!(c.decifrar(&guardado).unwrap(), "APP_USR-segredo");
        assert!(Cofre::temporario().decifrar(&guardado).is_err());
    }
}
