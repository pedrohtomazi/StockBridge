//! Backup automático do banco: 1 por dia, guarda os 14 últimos.
//!
//! A pasta pode ser trocada na tela de configuração (ex.: pendrive ou pasta de outro computador).
//! Se a pasta escolhida não estiver disponível, o backup vai para a pasta padrão e o erro aparece na tela.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::db::{self, Banco};

pub const CHAVE_PASTA: &str = "backup_dir";
pub const CHAVE_ULTIMO: &str = "backup_ultimo";
pub const CHAVE_ERRO: &str = "backup_erro";
const GUARDAR: usize = 14;

pub fn pasta_padrao(dados: &Path) -> PathBuf {
    dados.join("backups")
}

pub async fn pasta_configurada(pool: &Banco, dados: &Path) -> PathBuf {
    match db::ler_config(pool, CHAVE_PASTA).await {
        Ok(Some(p)) if !p.trim().is_empty() => PathBuf::from(p),
        _ => pasta_padrao(dados),
    }
}

/// Confere se dá para gravar na pasta (cria se não existir).
pub fn testar_pasta(pasta: &Path) -> Result<(), String> {
    std::fs::create_dir_all(pasta).map_err(|e| format!("Não foi possível usar a pasta {}: {e}", pasta.display()))?;
    let teste = pasta.join(".stockbridge-teste");
    std::fs::write(&teste, b"ok").map_err(|e| format!("Sem permissão para gravar em {}: {e}", pasta.display()))?;
    let _ = std::fs::remove_file(teste);
    Ok(())
}

fn arquivos(pasta: &Path) -> Vec<PathBuf> {
    let mut lista: Vec<PathBuf> = std::fs::read_dir(pasta)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("stockbridge-") && n.to_string_lossy().ends_with(".db"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    lista.sort(); // nome tem data e hora: ordem alfabética = ordem cronológica
    lista
}

/// Cópia consistente do banco (VACUUM INTO funciona com o sistema em uso).
pub async fn copiar(pool: &Banco, pasta: &Path) -> Result<PathBuf, String> {
    testar_pasta(pasta)?;
    let destino = pasta.join(format!("stockbridge-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    sqlx::query("VACUUM INTO ?")
        .bind(destino.to_string_lossy().to_string())
        .execute(pool)
        .await
        .map_err(|e| format!("Falha ao copiar o banco: {e}"))?;
    let mut todos = arquivos(pasta);
    while todos.len() > GUARDAR {
        let _ = std::fs::remove_file(todos.remove(0));
    }
    Ok(destino)
}

/// Faz o backup na pasta configurada (ou na padrão, se a configurada falhar) e registra o resultado.
pub async fn executar(pool: &Banco, dados: &Path) -> Result<PathBuf, String> {
    let pasta = pasta_configurada(pool, dados).await;
    let padrao = pasta_padrao(dados);
    let (resultado, aviso) = match copiar(pool, &pasta).await {
        Ok(a) => (Ok(a), None),
        Err(e) if pasta != padrao => match copiar(pool, &padrao).await {
            Ok(a) => {
                let aviso = format!("{e} O backup foi feito na pasta padrão ({}).", padrao.display());
                (Ok(a), Some(aviso))
            }
            Err(e2) => {
                let erro = format!("{e} / {e2}");
                (Err(erro.clone()), Some(erro))
            }
        },
        Err(e) => (Err(e.clone()), Some(e)),
    };
    if resultado.is_ok() {
        let _ = db::gravar_config(pool, CHAVE_ULTIMO, &db::agora_iso()).await;
    }
    match &aviso {
        Some(a) => {
            tracing::warn!("Backup: {a}");
            let _ = db::gravar_config(pool, CHAVE_ERRO, a).await;
        }
        None => {
            let _ = db::apagar_config(pool, CHAVE_ERRO).await;
        }
    }
    resultado
}

fn tem_backup_recente(pasta: &Path) -> bool {
    arquivos(pasta).iter().any(|a| {
        std::fs::metadata(a)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|q| q.elapsed().ok())
            .map(|idade| idade < Duration::from_secs(20 * 3600))
            .unwrap_or(false)
    })
}

/// Confere a cada 3 horas; faz backup se o último tiver mais de 20 horas.
pub fn iniciar_agendador(pool: Banco, dados: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await; // não competir com a abertura
        loop {
            let pasta = pasta_configurada(&pool, &dados).await;
            if !tem_backup_recente(&pasta) && !tem_backup_recente(&pasta_padrao(&dados)) {
                match executar(&pool, &dados).await {
                    Ok(a) => println!("💾 Backup criado: {}", a.display()),
                    Err(e) => eprintln!("⚠️  Backup: {e}"),
                }
            }
            tokio::time::sleep(Duration::from_secs(3 * 3600)).await;
        }
    });
}
