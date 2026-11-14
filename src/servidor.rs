//! Modo SERVIDOR: guarda o banco e atende as telas por HTTPS para todos os computadores da rede.

use axum_server::{Handle, tls_rustls::RustlsConfig};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

use crate::{
    app, backup,
    config::{self, ConfigLocal},
    cripto::Cofre,
    db,
    rede::{self, Anuncio, AutoridadeCertificadora, InfoRede},
    security::state::AppState,
};

pub struct Opcoes {
    pub dados: PathBuf,
    pub porta: u16,
    pub abrir_navegador: bool,
    /// Não pede ao Windows para instalar o certificado (testes automatizados).
    pub instalar_certificado: bool,
    /// DESENVOLVIMENTO: HTTP sem criptografia e SÓ neste computador (127.0.0.1), sem descoberta.
    pub dev_http: bool,
}

fn enderecos(ips: &[std::net::Ipv4Addr], porta: u16) -> Vec<String> {
    ips.iter().map(|ip| format!("https://{ip}:{porta}")).collect()
}

/// Espera o pedido de parada: Ctrl+C ou fechar a janela do console.
pub async fn sinal_de_parada() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(windows)]
    let fechar_janela = async {
        if let Ok(mut s) = tokio::signal::windows::ctrl_close() {
            s.recv().await;
        }
    };
    #[cfg(not(windows))]
    let fechar_janela = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = fechar_janela => {}
    }
}

/// Modo de desenvolvimento: HTTP puro, preso a 127.0.0.1 (ninguém da rede alcança).
async fn servir_dev_http(op: Opcoes, state: AppState, pool: db::Banco) -> Result<(), String> {
    let endereco = SocketAddr::from(([127, 0, 0, 1], op.porta));
    let listener = tokio::net::TcpListener::bind(endereco)
        .await
        .map_err(|e| format!("Não foi possível abrir a porta {}: {e}", op.porta))?;
    println!("MODO DESENVOLVIMENTO: http://127.0.0.1:{} (sem HTTPS, só este computador)", op.porta);
    let r = axum::serve(listener, app::router(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(sinal_de_parada())
        .await;
    pool.close().await;
    r.map_err(|e| format!("Falha no servidor: {e}"))
}

pub async fn executar(op: Opcoes, cfg: &mut ConfigLocal) -> Result<(), String> {
    println!("StockBridge - modo SERVIDOR (este computador guarda os dados)");
    println!("Pasta de dados: {}", op.dados.display());

    let pool = db::abrir(&op.dados.join("stockbridge.db")).await?;
    let cofre = Cofre::carregar_ou_criar(&op.dados)?;
    let jwt = config::segredo_persistente(&op.dados, "jwt.secret")?;
    let ac = AutoridadeCertificadora::carregar_ou_criar(&op.dados)?;
    let digital = rede::impressao_digital(&ac.pem);

    // Instala a AC neste computador (1ª vez), para o navegador local não reclamar.
    let marca = rede::marca_instalacao(&digital);
    if op.instalar_certificado && !op.dev_http && cfg.ca_instalada.as_deref() != Some(marca.as_str()) {
        println!();
        println!("O Windows vai perguntar se você quer instalar o certificado do StockBridge.");
        println!("Confira que o nome começa com \"StockBridge\" e clique em SIM.");
        if rede::instalar_ac_no_windows(&op.dados.join("ca.crt")) {
            cfg.ca_instalada = Some(marca);
            cfg.salvar(&op.dados)?;
            println!("Certificado instalado.");
        } else {
            println!("Certificado NÃO instalado: o navegador vai mostrar um aviso de segurança.");
            println!("Dá para instalar depois pela tela Configurações > Rede.");
        }
        println!();
    }

    let ips = rede::ips_locais();
    let (cert, chave) = ac.emitir_servidor(&ips)?;
    let tls = RustlsConfig::from_pem(cert.into_bytes(), chave.into_bytes())
        .await
        .map_err(|e| format!("Falha ao preparar o HTTPS: {e}"))?;

    let nome = rede::nome_maquina();
    let info = InfoRede {
        nome: nome.clone(),
        porta: op.porta,
        enderecos: enderecos(&ips, op.porta),
        ca_pem: ac.pem.clone(),
        ca_digital: digital,
    };
    let state = AppState::new(pool.clone(), jwt, cofre, op.dados.clone(), info)?;

    if op.dev_http {
        return servir_dev_http(op, state, pool).await;
    }

    // Descoberta: os terminais acham este servidor sozinhos.
    tokio::spawn(rede::responder_descoberta(Anuncio {
        app: "StockBridge".into(),
        nome: nome.clone(),
        porta: op.porta,
        ca_pem: ac.pem.clone(),
    }));

    // O IP mudou (ex.: roteador deu outro)? Emite um certificado novo sem reiniciar.
    {
        let (tls, state) = (tls.clone(), state.clone());
        let mut ips = ips.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let novos = rede::ips_locais();
                if novos == ips {
                    continue;
                }
                match ac.emitir_servidor(&novos) {
                    Ok((c, k)) => match tls.reload_from_pem(c.into_bytes(), k.into_bytes()).await {
                        Ok(()) => {
                            let lista = enderecos(&novos, state.rede.read().map(|r| r.porta).unwrap_or(0));
                            println!("O endereço deste computador mudou. Agora: {}", lista.join("  "));
                            if let Ok(mut r) = state.rede.write() {
                                r.enderecos = lista;
                            }
                            ips = novos;
                        }
                        Err(e) => eprintln!("Falha ao atualizar o certificado: {e}"),
                    },
                    Err(e) => eprintln!("Falha ao emitir o certificado: {e}"),
                }
            }
        });
    }

    backup::iniciar_agendador(pool.clone(), op.dados.clone());

    let handle = Handle::new();
    {
        let h = handle.clone();
        tokio::spawn(async move {
            sinal_de_parada().await;
            println!("Encerrando...");
            h.graceful_shutdown(Some(Duration::from_secs(5)));
        });
    }

    let precisa_setup: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.unwrap_or(1);
    println!();
    println!("============================================================");
    println!(" StockBridge no ar. NÃO feche esta janela enquanto usarem o sistema.");
    println!(" Neste computador:  https://localhost:{}", op.porta);
    for e in enderecos(&ips, op.porta) {
        println!(" Na rede:           {e}");
    }
    println!(" Nos outros computadores, abra o StockBridge.exe (modo terminal).");
    if precisa_setup == 0 {
        println!(" PRIMEIRO ACESSO: crie o administrador na página que vai abrir.");
    }
    println!("============================================================");

    if op.abrir_navegador {
        let url = format!("https://localhost:{}/", op.porta);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            config::abrir_navegador(&url);
        });
    }

    let endereco = SocketAddr::from(([0, 0, 0, 0], op.porta));
    let resultado = axum_server::bind_rustls(endereco, tls)
        .handle(handle)
        .serve(app::router(state).into_make_service_with_connect_info::<SocketAddr>())
        .await;
    pool.close().await;
    resultado.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!(
                "A porta {} já está em uso. O StockBridge já está aberto neste computador? \
                 (Feche a outra janela ou use --porta 3001.)",
                op.porta
            )
        } else {
            format!("Falha no servidor: {e}")
        }
    })
}
