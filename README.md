# StockBridge

Controle de estoque de depósito (WMS) com sincronização automática com o Mercado Livre.
**Um único executável** (`StockBridge.exe`): API em Rust (Axum), banco **SQLite** e telas web, tudo embutido.
Não precisa de Docker, PostgreSQL, Node, Python nem arquivo `.env`.

- Instalação na empresa (passo a passo para quem vai usar): [`docs/INSTALACAO.md`](docs/INSTALACAO.md)
- Rotas da API: [`API.md`](API.md)
- Como a segurança funciona e o que ainda falta: [`docs/SEGURANCA.md`](docs/SEGURANCA.md)

---

## Como o sistema roda

```
PC servidor (1 por empresa)                         Outros PCs (quantos quiserem)
┌──────────────────────────────────────┐           ┌──────────────────────────────┐
│ StockBridge.exe  (modo servidor)     │  HTTPS    │ StockBridge.exe (modo terminal)│
│  ├─ API + telas  https://IP:3000     │◄──────────│  acha o servidor sozinho e    │
│  ├─ dados/stockbridge.db (SQLite)    │           │  abre o navegador nele        │
│  ├─ backup diário, certificados      │  UDP 47801│                               │
│  └─ responde "onde está o servidor?" │◄──────────│                               │
└──────────────┬───────────────────────┘           └──────────────────────────────┘
               │ (só se houver internet)
               ▼
        API do Mercado Livre
```

- **Mesmo `.exe` nos dois papéis.** Na 1ª execução ele procura um servidor na rede: se achar, sugere ser **terminal**; senão, pergunta se este computador será o **servidor**. A escolha fica em `dados/config.json`.
- **IP que muda todo dia não é problema:** o terminal procura o servidor por broadcast UDP (porta 47801) a cada abertura, e o servidor emite um certificado novo quando o IP dele muda.
- **HTTPS sem aviso:** o servidor cria uma autoridade certificadora própria (`dados/ca.crt`). Cada computador a instala uma vez (o Windows pede confirmação) e o navegador passa a confiar.
- **Sem internet**, o depósito continua funcionando; só a sincronização com o Mercado Livre espera (ela tenta de novo a cada 10 minutos).

### Telas
Painel · Movimentar (com entrada em lote colada do Excel) · **Onde está** (cada SKU em quais prateleiras) · Produtos ·
Estantes · Depósitos · Histórico · **Anúncios ML** (lê a conta, importa anúncios como produtos pelo SKU) ·
**Rendimento** (faturamento, tarifas e lucro dos pedidos do ML) · Conexão ML · Usuários · Configurações · Relatórios.

Integração com o Mercado Livre: **lê** anúncios, visitas e pedidos; **envia** o estoque total do produto a cada
movimentação. Vendas no ML ainda não dão baixa automática no estoque do sistema.

Opções do executável: `--servidor`, `--terminal`, `--reconfigurar`, `--porta 3001`, `--dados C:\pasta`, `--sem-navegador`, `--ajuda`.

### Pasta `dados/` (ao lado do .exe, no servidor)

| Arquivo | O que é | Se perder |
|---|---|---|
| `stockbridge.db` | o banco (estoque, usuários, histórico) | perde tudo: restaure um backup |
| `chave.bin` | chave que cifra o Secret e os tokens do Mercado Livre | precisa reconfigurar o app e reconectar a conta |
| `jwt.secret` | assina os logins | todo mundo precisa entrar de novo |
| `ca.crt`, `ca.key`, `ca.nome` | autoridade certificadora do HTTPS | os terminais avisam que o certificado mudou e pedem para confiar de novo |
| `backups/` | cópias diárias do banco (as 14 últimas) | |
| `config.json` | modo (servidor/terminal) e porta | o .exe pergunta de novo |

**Backup completo** = copiar a pasta `dados/` inteira. A tela Configurações permite mandar o backup diário para outra pasta (pendrive, outro PC).

---

## Desenvolvimento

Precisa só do **Rust** (https://rustup.rs) e, no Windows, do *Visual Studio Build Tools* com "Desenvolvimento para desktop com C++".

```bash
cargo test                      # 17 testes (unidade + API de ponta a ponta), sem banco externo
cargo run -- --servidor --dev-http --dados ./dados-dev
```

`--dev-http` sobe em **http://127.0.0.1:3000** sem HTTPS e só nesta máquina (útil para testar no navegador sem instalar certificado).
Em modo desenvolvimento as telas são lidas direto da pasta `web/`: edite e recarregue o navegador, sem recompilar.

**Gerar o executável para entregar:**
```bash
cargo build --release
```
Sai em `target/release/StockBridge.exe` (~13 MB, já com telas e SQLite dentro). É só copiar esse arquivo.

**Mudar o banco:** crie um arquivo novo em `migrations/` (ex.: `0002_nome.sql`). Nunca edite uma migration que já rodou em algum computador; elas rodam sozinhas ao abrir o programa.

### Estrutura

```text
stockbridge/
├── Cargo.toml
├── migrations/0001_inicial.sql   # estrutura do banco SQLite
├── web/                          # telas (embutidas no .exe)
│   ├── index.html, css/, js/     # sistema principal
│   └── dashboards/               # página de gráficos e relatórios Excel
├── tests/api.rs                  # testes de ponta a ponta
└── src/
    ├── main.rs                   # escolhe servidor ou terminal
    ├── servidor.rs / terminal.rs # os dois modos
    ├── rede.rs                   # certificados HTTPS e descoberta na rede
    ├── app.rs                    # rotas e permissões
    ├── db.rs, backup.rs, cripto.rs, config.rs, web.rs
    ├── security/                 # login (Argon2 + JWT), permissões, validação, erros, cabeçalhos
    ├── handlers/                 # uma rota por função
    └── services/                 # Mercado Livre e fila de sincronização
```

### Versão anterior (PostgreSQL + Docker)
Esta versão troca o PostgreSQL por SQLite e **não importa dados** de um banco PostgreSQL antigo. Se já existe estoque cadastrado na versão anterior, ele precisa ser recadastrado (ou migrado com um script à parte).
