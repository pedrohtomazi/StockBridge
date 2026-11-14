//! Rede local: HTTPS com certificado próprio e descoberta automática do servidor.
//!
//! CERTIFICADOS
//! Na 1ª execução o servidor cria uma "autoridade certificadora" (AC) só dele (`dados/ca.crt` e
//! `ca.key`). A cada abertura (e quando o IP muda) ele emite um certificado HTTPS para os IPs
//! ATUAIS da máquina, assinado por essa AC. Cada computador instala a AC uma vez; depois disso o
//! navegador confia no servidor mesmo que o IP mude todo dia.
//!
//! DESCOBERTA
//! O servidor escuta na porta UDP 47801. O terminal grita na rede "cadê o servidor StockBridge?"
//! (broadcast) e o servidor responde com a porta e o certificado da AC. Assim ninguém precisa
//! saber o IP do servidor.

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};
use tokio::net::UdpSocket;

pub const PORTA_DESCOBERTA: u16 = 47801;
const PERGUNTA: &[u8] = b"STOCKBRIDGE?ONDE-ESTA-O-SERVIDOR/1";

/// O que o servidor mostra na tela de configuração e anuncia na descoberta.
#[derive(Debug, Clone, Default)]
pub struct InfoRede {
    pub nome: String,
    pub porta: u16,
    pub enderecos: Vec<String>,
    pub ca_pem: String,
    pub ca_digital: String,
}

// ---------------------------------------------------------------------------
// Endereços da máquina
// ---------------------------------------------------------------------------

pub fn nome_maquina() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "servidor".to_string())
}

/// IPv4 desta máquina em redes locais (sem loopback e sem endereços automáticos 169.254.x.x).
pub fn ips_locais() -> Vec<Ipv4Addr> {
    let mut ips: BTreeSet<Ipv4Addr> = BTreeSet::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for i in interfaces {
            if let IpAddr::V4(v4) = i.ip()
                && !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() {
                    ips.insert(v4);
                }
        }
    }
    ips.into_iter().collect()
}

/// Endereços de broadcast das redes desta máquina (para procurar o servidor).
fn broadcasts() -> Vec<Ipv4Addr> {
    let mut lista: BTreeSet<Ipv4Addr> = BTreeSet::from([Ipv4Addr::BROADCAST]);
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for i in interfaces {
            if let if_addrs::IfAddr::V4(v4) = i.addr
                && let Some(b) = v4.broadcast
                    && !v4.ip.is_loopback() {
                        lista.insert(b);
                    }
        }
    }
    lista.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Certificados
// ---------------------------------------------------------------------------

/// Impressão digital SHA-256 do certificado (para conferir que é o mesmo servidor).
pub fn impressao_digital(pem: &str) -> String {
    let der = pem::parse_der(pem).unwrap_or_default();
    Sha256::digest(&der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

mod pem {
    use base64::{Engine, engine::general_purpose::STANDARD};
    pub fn parse_der(pem: &str) -> Option<Vec<u8>> {
        let corpo: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        STANDARD.decode(corpo.trim()).ok()
    }
}

/// Marca "certificado X instalado NESTE computador" (a pasta dados pode ser copiada para outro PC).
pub fn marca_instalacao(digital: &str) -> String {
    format!("{digital}|{}", nome_maquina())
}

pub struct AutoridadeCertificadora {
    pub pem: String,
    chave: KeyPair,
    nome: String,
}

fn parametros_ac(nome: &str) -> Result<CertificateParams, String> {
    let mut p = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, nome);
    dn.push(DnType::OrganizationName, "StockBridge");
    p.distinguished_name = dn;
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    let hoje = chrono::Utc::now().date_naive();
    use chrono::Datelike;
    p.not_before = rcgen::date_time_ymd(hoje.year() - 1, 1, 1);
    p.not_after = rcgen::date_time_ymd(hoje.year() + 20, 1, 1);
    Ok(p)
}

impl AutoridadeCertificadora {
    /// Carrega a AC de `dados/` ou cria uma nova na 1ª execução.
    pub fn carregar_ou_criar(dados: &Path) -> Result<Self, String> {
        let (arq_crt, arq_key, arq_nome) = (dados.join("ca.crt"), dados.join("ca.key"), dados.join("ca.nome"));
        if let (Ok(pem), Ok(chave), Ok(nome)) = (
            std::fs::read_to_string(&arq_crt),
            std::fs::read_to_string(&arq_key),
            std::fs::read_to_string(&arq_nome),
        ) {
            let chave = KeyPair::from_pem(&chave).map_err(|e| format!("ca.key inválida: {e}"))?;
            return Ok(Self { pem, chave, nome: nome.trim().to_string() });
        }
        // Nome único por instalação: duas empresas nunca têm a mesma AC.
        let nome = format!("StockBridge {} {}", nome_maquina(), &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let chave = KeyPair::generate().map_err(|e| e.to_string())?;
        let cert = parametros_ac(&nome)?.self_signed(&chave).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dados).map_err(|e| e.to_string())?;
        std::fs::write(&arq_crt, cert.pem()).map_err(|e| e.to_string())?;
        std::fs::write(&arq_key, chave.serialize_pem()).map_err(|e| e.to_string())?;
        std::fs::write(&arq_nome, &nome).map_err(|e| e.to_string())?;
        Ok(Self { pem: cert.pem(), chave, nome })
    }

    /// Certificado HTTPS para os IPs e nomes informados. Devolve (cert PEM, chave PEM).
    pub fn emitir_servidor(&self, ips: &[Ipv4Addr]) -> Result<(String, String), String> {
        // O emissor é reconstruído com o mesmo nome e a mesma chave: quem confia na AC
        // instalada confia neste certificado.
        let emissor = parametros_ac(&self.nome)?.self_signed(&self.chave).map_err(|e| e.to_string())?;
        let maquina = nome_maquina();
        let mut nomes = vec!["localhost".to_string(), maquina.clone(), format!("{maquina}.local")];
        nomes.dedup();
        let mut p = CertificateParams::new(nomes).map_err(|e| e.to_string())?;
        p.subject_alt_names.push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        for ip in ips {
            p.subject_alt_names.push(SanType::IpAddress(IpAddr::V4(*ip)));
        }
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, format!("StockBridge ({maquina})"));
        p.distinguished_name = dn;
        p.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        p.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
        let hoje = chrono::Utc::now();
        p.not_before = rcgen::date_time_ymd(
            chrono::Datelike::year(&hoje),
            chrono::Datelike::month(&hoje) as u8,
            1,
        );
        let fim = hoje + chrono::Duration::days(365);
        p.not_after = rcgen::date_time_ymd(chrono::Datelike::year(&fim), chrono::Datelike::month(&fim) as u8, 1);
        let chave = KeyPair::generate().map_err(|e| e.to_string())?;
        let cert = p.signed_by(&chave, &emissor, &self.chave).map_err(|e| e.to_string())?;
        // Envia a cadeia completa (servidor + AC).
        Ok((format!("{}{}", cert.pem(), self.pem), chave.serialize_pem()))
    }
}

/// Instala a AC como "confiável" para o usuário atual do Windows. O próprio Windows mostra um
/// aviso pedindo confirmação ("Sim"). Devolve false se não deu (ex.: a pessoa clicou "Não").
pub fn instalar_ac_no_windows(arquivo_crt: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("certutil")
            .args(["-user", "-addstore", "Root"])
            .arg(arquivo_crt)
            .stdout(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = arquivo_crt;
        false
    }
}

// ---------------------------------------------------------------------------
// Descoberta (UDP)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anuncio {
    pub app: String,
    pub nome: String,
    pub porta: u16,
    pub ca_pem: String,
}

/// Servidor: responde a quem procurar. Roda para sempre.
pub async fn responder_descoberta(anuncio: Anuncio) {
    let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, PORTA_DESCOBERTA)).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "⚠️  Descoberta automática desligada (porta UDP {PORTA_DESCOBERTA} ocupada: {e}). \
                 Os terminais vão precisar do IP digitado."
            );
            return;
        }
    };
    let resposta = serde_json::to_vec(&anuncio).unwrap_or_default();
    let mut buf = [0u8; 256];
    loop {
        if let Ok((n, origem)) = socket.recv_from(&mut buf).await
            && &buf[..n] == PERGUNTA {
                let _ = socket.send_to(&resposta, origem).await;
            }
    }
}

/// Terminal: procura o servidor. `alvo` = IP digitado à mão (pula o broadcast).
pub async fn procurar_servidor(alvo: Option<IpAddr>, espera: Duration) -> Option<(IpAddr, Anuncio)> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok()?;
    socket.set_broadcast(true).ok()?;
    let destinos: Vec<SocketAddr> = match alvo {
        Some(ip) => vec![SocketAddr::new(ip, PORTA_DESCOBERTA)],
        None => {
            let mut d: Vec<SocketAddr> =
                broadcasts().into_iter().map(|b| SocketAddr::new(IpAddr::V4(b), PORTA_DESCOBERTA)).collect();
            // servidor no próprio computador
            d.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORTA_DESCOBERTA));
            d
        }
    };
    let mut buf = vec![0u8; 8192];
    let fim = tokio::time::Instant::now() + espera;
    let mut proximo_envio = tokio::time::Instant::now();
    loop {
        if tokio::time::Instant::now() >= proximo_envio {
            for d in &destinos {
                let _ = socket.send_to(PERGUNTA, d).await;
            }
            proximo_envio = tokio::time::Instant::now() + Duration::from_millis(700);
        }
        let restante = fim.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            return None;
        }
        let espera_agora = restante.min(Duration::from_millis(700));
        if let Ok(Ok((n, origem))) = tokio::time::timeout(espera_agora, socket.recv_from(&mut buf)).await
            && let Ok(a) = serde_json::from_slice::<Anuncio>(&buf[..n])
                && a.app == "StockBridge" {
                    return Some((origem.ip(), a));
                }
    }
}
