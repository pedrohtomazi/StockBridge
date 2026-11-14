//! Modo TERMINAL: acha o servidor na rede, confia no certificado dele e abre o navegador.
//! Este computador não guarda dados do estoque.

use std::{net::IpAddr, path::Path, time::Duration};

use crate::{
    config::{self, ConfigLocal},
    rede::{self, Anuncio},
};

async fn resolver(texto: &str) -> Option<IpAddr> {
    if let Ok(ip) = texto.parse::<IpAddr>() {
        return Some(ip);
    }
    tokio::net::lookup_host((texto, rede::PORTA_DESCOBERTA))
        .await
        .ok()?
        .find(|a| a.is_ipv4())
        .map(|a| a.ip())
}

/// Confere o certificado do servidor (na 1ª vez, aceita e instala; depois, exige que seja o mesmo).
fn confiar(dados: &Path, cfg: &mut ConfigLocal, a: &Anuncio) -> Result<(), String> {
    let digital = rede::impressao_digital(&a.ca_pem);
    match cfg.ca_confiavel.as_deref() {
        Some(d) if d == digital => {
            if cfg.ca_instalada.as_deref() == Some(rede::marca_instalacao(&digital).as_str()) {
                return Ok(());
            }
        }
        Some(_) => {
            println!();
            println!("ATENÇÃO: o certificado deste servidor é DIFERENTE do que este computador conhecia.");
            println!("Isso é normal só se o servidor foi reinstalado ou trocado de computador.");
            println!("Se ninguém mexeu no servidor, NÃO continue e avise o responsável.");
            let r = config::perguntar("Confiar no novo servidor? (s/N): ");
            if !r.eq_ignore_ascii_case("s") {
                return Err("Conexão cancelada.".into());
            }
        }
        None => {}
    }
    let arquivo = dados.join("servidor-ca.crt");
    std::fs::create_dir_all(dados).map_err(|e| e.to_string())?;
    std::fs::write(&arquivo, &a.ca_pem).map_err(|e| format!("Não foi possível gravar o certificado: {e}"))?;
    println!();
    println!("Servidor \"{}\". Impressão digital do certificado:", a.nome);
    println!("  {digital}");
    println!("(Se quiser conferir: é a mesma da tela Configurações > Rede, no servidor.)");
    println!();
    println!("O Windows vai perguntar se você quer instalar o certificado do StockBridge.");
    println!("Confira que o nome começa com \"StockBridge\" e clique em SIM.");
    cfg.ca_confiavel = Some(digital.clone());
    if rede::instalar_ac_no_windows(&arquivo) {
        cfg.ca_instalada = Some(rede::marca_instalacao(&digital));
        println!("Certificado instalado.");
    } else {
        println!("Certificado NÃO instalado: o navegador vai mostrar um aviso de segurança.");
    }
    cfg.salvar(dados)
}

pub async fn executar(dados: &Path, cfg: &mut ConfigLocal) -> Result<(), String> {
    println!("StockBridge - modo TERMINAL (este computador acessa o servidor da rede)");
    loop {
        println!("Procurando o servidor StockBridge na rede...");
        let mut achado = None;
        if let Some(manual) = cfg.servidor_manual.clone()
            && let Some(ip) = resolver(&manual).await {
                achado = rede::procurar_servidor(Some(ip), Duration::from_secs(2)).await;
            }
        if achado.is_none() {
            achado = rede::procurar_servidor(None, Duration::from_secs(4)).await;
        }

        if let Some((ip, anuncio)) = achado {
            confiar(dados, cfg, &anuncio)?;
            let host = if ip.is_loopback() { "localhost".to_string() } else { ip.to_string() };
            let url = format!("https://{host}:{}/", anuncio.porta);
            println!("Servidor encontrado: {} em {url}", anuncio.nome);
            config::abrir_navegador(&url);
            println!("Sistema aberto no navegador. Esta janela fecha sozinha.");
            tokio::time::sleep(Duration::from_secs(6)).await;
            return Ok(());
        }

        println!();
        println!("Servidor não encontrado. Confira:");
        println!("  - o computador servidor está ligado e com o StockBridge aberto?");
        println!("  - os dois computadores estão na mesma rede (mesmo roteador/Wi-Fi)?");
        println!("  - no servidor, o Firewall do Windows liberou o StockBridge em 'Redes privadas'?");
        let r = config::perguntar("Digite o IP do servidor (ex.: 192.168.0.10), Enter para tentar de novo, ou S para sair: ");
        if r.eq_ignore_ascii_case("s") {
            return Ok(());
        }
        if !r.is_empty() {
            cfg.servidor_manual = Some(r);
            cfg.salvar(dados)?;
        }
    }
}
