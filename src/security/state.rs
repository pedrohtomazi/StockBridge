use axum::{extract::FromRef, http::StatusCode};
use std::{
    collections::HashMap,
    net::IpAddr,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{
    cripto::Cofre,
    db::Banco,
    rede::InfoRede,
    security::{auth::hash_password, error::ApiError},
    services::sync::Sincronizador,
};

/// Erros seguidos do MESMO computador com o MESMO e-mail até bloquear.
const MAX_FALHAS_EMAIL_IP: u32 = 5;
/// Erros seguidos do mesmo computador (qualquer e-mail) até bloquear.
const MAX_FALHAS_IP: u32 = 20;
const JANELA_BLOQUEIO: Duration = Duration::from_secs(15 * 60);
const VALIDADE_STATE_OAUTH: Duration = Duration::from_secs(10 * 60);

/// Estado compartilhado da aplicação.
#[derive(Clone)]
pub struct AppState {
    pub pool: Banco,
    pub jwt_secret: Arc<Vec<u8>>,
    pub cofre: Arc<Cofre>,
    pub sync: Sincronizador,
    pub dados: Arc<PathBuf>,
    /// Endereços, porta e certificado: mostrados na tela de configuração.
    pub rede: Arc<RwLock<InfoRede>>,
    /// Contagem de senhas erradas. A chave inclui o IP de quem tentou: antes era só o e-mail,
    /// e qualquer pessoa conseguia bloquear o admin errando a senha 5 vezes de propósito.
    login_falhas: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    oauth_states: Arc<Mutex<HashMap<String, Instant>>>,
    /// Hash falso: o login demora o MESMO tempo com e-mail inexistente (não revela quem existe).
    pub hash_falso: Arc<String>,
    /// Anúncios lidos do Mercado Livre (guardados 5 min para não consultar a cada clique).
    pub cache_anuncios: CacheAnuncios,
}

pub type CacheAnuncios = Arc<tokio::sync::Mutex<Option<(Instant, Vec<crate::services::meli_leitura::Anuncio>)>>>;

impl FromRef<AppState> for Banco {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

impl AppState {
    pub fn new(
        pool: Banco,
        jwt_secret: String,
        cofre: Cofre,
        dados: PathBuf,
        rede: InfoRede,
    ) -> Result<Self, String> {
        if jwt_secret.len() < 32 {
            return Err("O segredo do JWT precisa ter pelo menos 32 caracteres.".to_string());
        }
        let cofre = Arc::new(cofre);
        let sync = Sincronizador::iniciar(pool.clone(), cofre.clone());
        Ok(Self {
            pool,
            jwt_secret: Arc::new(jwt_secret.into_bytes()),
            cofre,
            sync,
            dados: Arc::new(dados),
            rede: Arc::new(RwLock::new(rede)),
            login_falhas: Arc::new(Mutex::new(HashMap::new())),
            oauth_states: Arc::new(Mutex::new(HashMap::new())),
            hash_falso: Arc::new(hash_password("senha-falsa-apenas-para-igualar-o-tempo")?),
            cache_anuncios: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    // ---------- limite de tentativas de login ----------

    fn chaves(ip: IpAddr, email: &str) -> [(String, u32); 2] {
        [(format!("ip|{ip}"), MAX_FALHAS_IP), (format!("ip-email|{ip}|{email}"), MAX_FALHAS_EMAIL_IP)]
    }

    pub fn login_permitido(&self, ip: IpAddr, email: &str) -> Result<(), ApiError> {
        let mut mapa = self.login_falhas.lock().unwrap_or_else(|e| e.into_inner());
        for (chave, limite) in Self::chaves(ip, email) {
            if matches!(mapa.get(&chave), Some((_, desde)) if desde.elapsed() > JANELA_BLOQUEIO) {
                mapa.remove(&chave);
            }
            if let Some((falhas, _)) = mapa.get(&chave)
                && *falhas >= limite {
                    return Err((
                        StatusCode::TOO_MANY_REQUESTS,
                        "Muitas tentativas erradas deste computador. Aguarde 15 minutos e tente novamente."
                            .to_string(),
                    ));
                }
        }
        Ok(())
    }

    pub fn registrar_falha_login(&self, ip: IpAddr, email: &str) {
        let mut mapa = self.login_falhas.lock().unwrap_or_else(|e| e.into_inner());
        if mapa.len() > 10_000 {
            mapa.retain(|_, (_, desde)| desde.elapsed() <= JANELA_BLOQUEIO);
        }
        for (chave, _) in Self::chaves(ip, email) {
            let entrada = mapa.entry(chave).or_insert((0, Instant::now()));
            if entrada.1.elapsed() > JANELA_BLOQUEIO {
                *entrada = (0, Instant::now());
            }
            entrada.0 += 1;
        }
    }

    pub fn limpar_falhas_login(&self, ip: IpAddr, email: &str) {
        let mut mapa = self.login_falhas.lock().unwrap_or_else(|e| e.into_inner());
        mapa.remove(&format!("ip-email|{ip}|{email}"));
    }

    // ---------- `state` do OAuth (proteção contra CSRF) ----------

    pub fn novo_state_oauth(&self) -> String {
        let valor = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let mut mapa = self.oauth_states.lock().unwrap_or_else(|e| e.into_inner());
        mapa.retain(|_, criado| criado.elapsed() <= VALIDADE_STATE_OAUTH);
        mapa.insert(valor.clone(), Instant::now());
        valor
    }

    /// Só vale UMA vez e só por 10 minutos.
    pub fn consumir_state_oauth(&self, valor: &str) -> bool {
        let mut mapa = self.oauth_states.lock().unwrap_or_else(|e| e.into_inner());
        match mapa.remove(valor) {
            Some(criado) => criado.elapsed() <= VALIDADE_STATE_OAUTH,
            None => false,
        }
    }
}
