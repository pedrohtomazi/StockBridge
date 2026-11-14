//! StockBridge.exe
//!
//!   StockBridge.exe                 abre no modo escolhido na 1ª vez (servidor ou terminal)
//!   StockBridge.exe --servidor      força o modo servidor
//!   StockBridge.exe --terminal      força o modo terminal
//!   StockBridge.exe --reconfigurar  pergunta de novo se este computador é servidor ou terminal
//!   StockBridge.exe --porta 3001    outra porta (servidor)
//!   StockBridge.exe --dados C:\pasta   outra pasta de dados
//!   StockBridge.exe --sem-navegador    não abre o navegador sozinho

use std::{process::ExitCode, time::Duration};
use stockbridge::{
    config::{self, ConfigLocal, Modo},
    rede, servidor, terminal,
};

struct Args {
    servidor: bool,
    terminal: bool,
    reconfigurar: bool,
    sem_navegador: bool,
    sem_certificado: bool,
    dev_http: bool,
    porta: Option<u16>,
    dados: Option<String>,
}

fn ler_args() -> Result<Args, String> {
    let mut a = Args {
        servidor: false, terminal: false, reconfigurar: false, sem_navegador: false,
        sem_certificado: false, dev_http: false, porta: None, dados: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--servidor" => a.servidor = true,
            "--terminal" => a.terminal = true,
            "--reconfigurar" => a.reconfigurar = true,
            "--sem-navegador" => a.sem_navegador = true,
            // Desenvolvimento/testes (não documentados para a empresa):
            "--sem-instalar-certificado" => a.sem_certificado = true,
            "--dev-http" => a.dev_http = true,
            "--porta" => {
                let v = it.next().ok_or("--porta precisa de um número")?;
                a.porta = Some(v.parse().map_err(|_| format!("Porta inválida: {v}"))?);
            }
            "--dados" => a.dados = Some(it.next().ok_or("--dados precisa de uma pasta")?),
            "-h" | "--ajuda" | "--help" => {
                println!("{}", include_str!("main.rs").lines().skip(2).take(8).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            outro => return Err(format!("Opção desconhecida: {outro} (use --ajuda)")),
        }
    }
    if a.servidor && a.terminal {
        return Err("Use --servidor OU --terminal, não os dois.".into());
    }
    Ok(a)
}

async fn escolher_modo() -> Modo {
    println!("Primeira vez neste computador. Procurando um servidor StockBridge na rede...");
    if let Some((ip, a)) = rede::procurar_servidor(None, Duration::from_secs(3)).await {
        println!("Encontrei o servidor \"{}\" ({ip}).", a.nome);
        let r = config::perguntar("Usar ESTE computador como terminal (só acessa o servidor)? [S/n]: ");
        if !r.eq_ignore_ascii_case("n") {
            return Modo::Terminal;
        }
    } else {
        println!("Nenhum servidor StockBridge encontrado na rede.");
    }
    println!();
    println!("  [1] SERVIDOR: este computador guarda os dados. Precisa ficar ligado com o");
    println!("      StockBridge aberto. Só UM computador da empresa deve ser o servidor.");
    println!("  [2] TERMINAL: o servidor está em outro computador; este só acessa.");
    loop {
        match config::perguntar("Escolha 1 ou 2: ").as_str() {
            "1" => return Modo::Servidor,
            "2" => return Modo::Terminal,
            _ => println!("Digite 1 ou 2."),
        }
    }
}

async fn principal() -> Result<(), String> {
    let args = ler_args()?;
    let dados = config::pasta_de_dados(args.dados.as_deref());
    let mut cfg = ConfigLocal::carregar(&dados);
    if let Some(p) = args.porta {
        cfg.porta = p;
    }
    if args.reconfigurar {
        cfg.modo = None;
    }
    let modo = if args.servidor {
        Modo::Servidor
    } else if args.terminal {
        Modo::Terminal
    } else if let Some(m) = cfg.modo {
        m
    } else {
        escolher_modo().await
    };
    if cfg.modo != Some(modo) {
        cfg.modo = Some(modo);
        cfg.salvar(&dados)?;
    }
    match modo {
        Modo::Servidor => {
            let opcoes = servidor::Opcoes {
                dados: dados.clone(),
                porta: cfg.porta,
                abrir_navegador: !args.sem_navegador,
                instalar_certificado: !args.sem_certificado,
                dev_http: args.dev_http,
            };
            servidor::executar(opcoes, &mut cfg).await
        }
        Modo::Terminal => terminal::executar(&dados, &mut cfg).await,
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).with_target(false).init();
    // Um único provedor de criptografia para o HTTPS.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Falha ao iniciar: {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(principal()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!();
            eprintln!("ERRO: {e}");
            // Quem abriu com dois cliques precisa ver o erro antes da janela fechar.
            config::perguntar("Pressione Enter para fechar.");
            ExitCode::FAILURE
        }
    }
}
