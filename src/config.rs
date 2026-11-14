//! Arquivos locais do programa: pasta de dados, `config.json` (modo servidor/terminal) e segredos.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PORTA_PADRAO: u16 = 3000;
const ARQUIVO_CONFIG: &str = "config.json";

/// Pasta onde ficam banco, certificados, backups e segredos.
/// Padrão: pasta "dados" ao lado do executável. Pode ser trocada com --dados ou STOCKBRIDGE_DADOS.
pub fn pasta_de_dados(argumento: Option<&str>) -> PathBuf {
    if let Some(p) = argumento.filter(|p| !p.trim().is_empty()) {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("STOCKBRIDGE_DADOS")
        && !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|pasta| pasta.join("dados")))
        .unwrap_or_else(|| PathBuf::from("dados"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modo {
    Servidor,
    Terminal,
}

/// Configuração deste computador (não vai para o banco: o terminal nem tem banco).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigLocal {
    pub modo: Option<Modo>,
    #[serde(default = "porta_padrao")]
    pub porta: u16,
    /// Terminal: IP (ou nome) do servidor, se a busca automática for bloqueada na rede.
    #[serde(default)]
    pub servidor_manual: Option<String>,
    /// Terminal: impressão digital do certificado do servidor aceito na 1ª vez.
    #[serde(default)]
    pub ca_confiavel: Option<String>,
    /// Este computador já instalou o certificado no Windows?
    #[serde(default)]
    pub ca_instalada: Option<String>,
}

fn porta_padrao() -> u16 {
    PORTA_PADRAO
}

impl Default for ConfigLocal {
    fn default() -> Self {
        Self { modo: None, porta: PORTA_PADRAO, servidor_manual: None, ca_confiavel: None, ca_instalada: None }
    }
}

impl ConfigLocal {
    pub fn carregar(dados: &Path) -> Self {
        std::fs::read_to_string(dados.join(ARQUIVO_CONFIG))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn salvar(&self, dados: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dados)
            .map_err(|e| format!("Não foi possível criar a pasta {}: {e}", dados.display()))?;
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dados.join(ARQUIVO_CONFIG), texto)
            .map_err(|e| format!("Não foi possível gravar {ARQUIVO_CONFIG}: {e}"))
    }
}

/// Lê um segredo guardado em arquivo; se não existir, gera um aleatório e salva.
pub fn segredo_persistente(dados: &Path, nome_arquivo: &str) -> Result<String, String> {
    std::fs::create_dir_all(dados)
        .map_err(|e| format!("Não foi possível criar a pasta {}: {e}", dados.display()))?;
    let arquivo = dados.join(nome_arquivo);
    if let Ok(conteudo) = std::fs::read_to_string(&arquivo) {
        let valor = conteudo.trim().to_string();
        if valor.len() >= 32 {
            return Ok(valor);
        }
    }
    // 64 caracteres hexadecimais aleatórios (sorteados pelo sistema operacional)
    let novo = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    std::fs::write(&arquivo, &novo)
        .map_err(|e| format!("Não foi possível gravar {}: {e}", arquivo.display()))?;
    Ok(novo)
}

/// Pergunta no console e devolve a resposta (sem o Enter).
pub fn perguntar(texto: &str) -> String {
    use std::io::Write;
    print!("{texto}");
    let _ = std::io::stdout().flush();
    let mut linha = String::new();
    let _ = std::io::stdin().read_line(&mut linha);
    linha.trim().to_string()
}

/// Abre um endereço no navegador padrão.
pub fn abrir_navegador(url: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if r.is_err() {
        println!("Abra no navegador: {url}");
    }
}
