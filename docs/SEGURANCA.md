# Segurança do StockBridge

A camada de segurança original foi escrita pelo **Medina** (Argon2, JWT, permissões, validação, erros seguros,
`state` no OAuth, cabeçalhos). A versão 0.2 (SQLite, executável único, rede local com HTTPS) manteve essas peças
e corrigiu os riscos que estavam abertos. Tudo abaixo é coberto por `cargo test` (pasta `tests/`).

## O que mudou na versão 0.2

| Antes | Agora |
|---|---|
| Banco PostgreSQL no Docker com senha fixa `senha_secreta` e porta 5432 aberta para a rede | Banco SQLite num arquivo local. Não há porta de banco para atacar |
| Bloqueio de login **por e-mail**: qualquer pessoa trancava o admin errando a senha 5 vezes | Bloqueio **por computador (IP)**: 5 erros do mesmo IP com o mesmo e-mail, ou 20 do mesmo IP, em 15 min. Outro computador continua entrando |
| Token valia 8 h mesmo com o usuário removido ou rebaixado | Cada requisição confere o usuário no banco (`active` e `token_version`). Desativar, trocar papel ou senha derruba o login **na hora** |
| Sem como listar, desativar ou trocar senha de usuário | Rotas e tela de usuários; "Minha senha" para cada um |
| Tokens do Mercado Livre em texto puro no banco | Secret do app e tokens **cifrados** (ChaCha20-Poly1305) com a chave `dados/chave.bin`, separada do banco: um backup vazado não entrega a conta |
| Credenciais do Mercado Livre no `.env` | Tela Configurações; o Secret nunca volta para a tela |
| Primeiro admin pelo `.env` | Tela de primeiro acesso, aceita **só no próprio servidor** (127.0.0.1) e só uma vez. Ninguém da rede "chega antes" |
| HTTP: senha e token passavam abertos na rede | **HTTPS** com autoridade certificadora própria (veja abaixo) |
| CORS liberando uma origem | Sem CORS: telas e API saem do mesmo endereço, nenhum outro site é liberado |
| Página de relatórios sem login (401 em tudo) e com CSP nenhuma | Usa o token do sistema; CSP própria e restrita a ela |
| Telas sem Content-Security-Policy | CSP estrita (`default-src 'self'`, sem script inline) em todas as telas principais |
| Estante podia apontar para depósito inexistente | Chave estrangeira no banco |
| Dinheiro em ponto flutuante | Centavos inteiros |
| Histórico recalculado com o preço atual | Cada movimentação guarda custo e preço da época e quem fez |
| Sincronização com o ML fora de ordem, sem registro de falha, renovação de token concorrente | Fila única, um produto por vez; resultado gravado por anúncio; nova tentativa a cada 10 min; status "precisa reconectar" quando a renovação é recusada |
| Migrations rodando duas vezes (Docker + sqlx), compilação exigindo banco ligado | Só o sqlx roda as migrations; consultas checadas em tempo de execução: compila sem banco |
| Sem testes em Rust | 17 testes, inclusive 20 entradas simultâneas numa estante de capacidade 100 (passam exatamente 10) |

## Como cada peça funciona

### Senha (Argon2) e login sem entregar informação
O banco guarda só o hash Argon2id (lento de propósito, com sal aleatório). A mesma mensagem vale para "e-mail não existe"
e "senha errada", e um hash falso é calculado quando o e-mail não existe, para o tempo de resposta ser igual.
O Argon2 roda em outra thread (`spawn_blocking`) para não travar o servidor.

### Token (JWT) e revogação
O token leva o id do usuário e a `token_version` dele, assinados com `dados/jwt.secret` (gerado na 1ª execução).
O middleware `require_auth` confere a assinatura **e** o banco: usuário ativo e versão igual. O papel usado é o do banco,
não o da hora do login. Trocar senha, papel ou desativar soma 1 na versão.

### Permissões
Três grupos de rotas em `app.rs`: públicas, logado e admin. `require_auth` roda antes de `require_admin`.
Não é possível tirar o próprio acesso de admin nem ficar sem nenhum admin ativo.

### Validação em duas camadas
O Rust valida tudo o que chega (`security/validation.rs`) e o SQLite repete as regras com `CHECK`, chaves estrangeiras
e tabelas `STRICT`. Uma entrada com quantidade negativa, tipo inventado ou SKU com aspas é recusada nas duas camadas.

### Concorrência
Movimentações usam `BEGIN IMMEDIATE`: duas entradas na mesma estante entram em fila, então a checagem de capacidade
e de saldo não pode ser "furada". A saída usa `UPDATE ... WHERE quantity >= ?`: o saldo nunca fica negativo.

### Erros que não vazam
Detalhes técnicos vão para a janela do servidor; o cliente vê "Erro interno do servidor." Erros esperados viram
códigos úteis (409 SKU repetido, 400 produto inexistente).

### HTTPS na rede local
Na 1ª execução o servidor cria uma autoridade certificadora (AC) única daquela instalação (`dados/ca.crt` + `ca.key`,
restrita a emitir certificados finais). A cada abertura, e quando o IP muda, ele emite um certificado para os IPs atuais,
`localhost` e o nome do computador. Cada computador instala a AC uma vez (o Windows pede confirmação) e o navegador
confia. O terminal recebe a AC pela descoberta na 1ª vez, mostra a impressão digital e, nas próximas, exige que seja a mesma.

### Descoberta na rede
O servidor responde na porta UDP 47801 só a uma pergunta exata, com nome, porta e certificado **público** da AC.
Nada sigiloso trafega por ali.

### OAuth do Mercado Livre
`GET /api/meli/authorize` gera um `state` aleatório de uso único (10 min). O callback público só aceita esse `state`:
sem isso, alguém mandaria ao admin um link com o `code` da conta dele e o sistema passaria a atualizar a conta errada.
O formulário da troca de token usa encode correto e o cliente HTTP tem timeout de 15 s.

### Cabeçalhos
Toda resposta leva `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`.
Telas: CSP `default-src 'self'` sem script inline. API: CSP `default-src 'none'` e `Cache-Control: no-store`.
O corpo das requisições é limitado a 16 KB. As telas montam tudo com `textContent` (nunca `innerHTML`), e o CSV
exportado neutraliza fórmulas (`=`, `+`, `-`, `@`).

## Riscos que continuam (e o que fazer)

1. **A pasta `dados/` do servidor é a joia da coroa.** Quem copiar `ca.key` consegue se passar pelo servidor para os
   computadores que confiam na AC; quem copiar `chave.bin` + o banco lê os tokens do Mercado Livre. Proteja o acesso ao
   computador servidor (senha do Windows, sem compartilhar a pasta).
2. **Confiança no primeiro uso.** Na 1ª conexão de um terminal, um impostor na mesma rede poderia responder antes do
   servidor. Para conferir, compare a impressão digital mostrada no terminal com a da tela Configurações > Rede.
3. **Operador vê custo e preço** dos produtos. Se isso for sigiloso, é preciso filtrar esses campos por papel.
4. **O `state` do OAuth e o contador de tentativas de login ficam na memória**: se o servidor reiniciar no meio da
   conexão com o Mercado Livre, é preciso gerar o link de novo.
5. **Retorno do OAuth por um site de terceiros** (padrão `https://google.com`): o `code` aparece na barra de endereços
   e no histórico daquele site. Ele é de uso único e inútil sem o Secret, mas o ideal é uma página de retorno própria.
6. **Página de relatórios** (`/dashboards/`) carrega Chart.js e SheetJS da internet e usa scripts inline, por isso tem
   uma CSP mais aberta só para ela. Para fechar: baixar as bibliotecas para `web/dashboards/vendor/` e mover os scripts inline.
7. **Executável sem assinatura digital**: o Windows (SmartScreen) avisa "editor desconhecido". Para distribuir a várias
   empresas, assine com um certificado de assinatura de código.
8. **Anúncios com variações** no Mercado Livre não são atualizados (a API do ML exige atualizar cada variação). O erro
   aparece na tela de Mercado Livre.
9. **`cargo audit`** (vulnerabilidades em dependências) ainda não roda automaticamente.
