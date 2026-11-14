# StockBridge API: rotas

**Endereço:** o mesmo das telas (ex.: `https://192.168.0.10:3000`). **Formato:** JSON (`Content-Type: application/json`).
Valores em dinheiro vão e voltam em **reais** (`299.9`); no banco ficam em centavos.

## Autenticação
1. `POST /api/auth/login` devolve um **token** (vale 8 horas).
2. Nas rotas 🔒, envie `Authorization: Bearer SEU_TOKEN`.
3. Papéis: **admin** (tudo) e **operador** (leitura e movimentações). 👑 = só admin.
4. A cada requisição o usuário é conferido no banco: desativar, trocar papel ou senha **derruba o token na hora** (401).

## Erros
Corpo em **texto puro**. `400` dado inválido/regra violada · `401` sem login ou sessão encerrada · `403` sem permissão ·
`404` não encontrado · `409` duplicado ou exclusão bloqueada · `413` corpo > 16 KB · `429` muitas tentativas de login ·
`500` erro interno (detalhe só na janela do servidor) · `503` banco indisponível.

---

## Públicas
| Rota | Resposta |
|---|---|
| `GET /api/health` | `200 API Rust Operante!` (confere o banco; `503` se ele não responde) |
| `POST /api/auth/login` `{email, password}` | `{token, role, email, expires_in}` · `401` e-mail ou senha errados · `403` usuário desativado · `429` 5 erros do mesmo computador com o mesmo e-mail (ou 20 do mesmo computador) em 15 min |
| `GET /api/setup` | `{needsSetup, local}`: se ainda não existe usuário e se a página foi aberta no próprio servidor |
| `POST /api/setup` `{email, password}` | cria o 1º admin e devolve o mesmo que o login. **Só no próprio servidor** (`403` vindo da rede) e só uma vez (`409`) |
| `GET /api/meli/callback?code=&state=` | conclui a conexão com o Mercado Livre; só aceita `state` gerado pelo sistema (uso único, 10 min) |
| `GET /api/rede/ca.crt` | certificado público da autoridade certificadora (para instalar em PCs/celulares) |

## Usuário logado 🔒
| Rota | Resposta |
|---|---|
| `GET /api/auth/me` | `{id, email, role}` |
| `POST /api/auth/password` `{current, new}` | troca a própria senha; devolve token novo (os outros logins da pessoa caem) |
| `GET /api/products` | `[{id, name, sku, cost, price, totalQuantity}]` (total em todas as estantes) |
| `GET /api/warehouses` | `[{id, name}]` |
| `GET /api/locations` (`?qrToken=` opcional) | `[{id, warehouseId, warehouseName, sector, rack, level, position, fullCode, qrToken, maxCapacity, isActive, currentQuantity}]` |
| `GET /api/locations/{id}/stock` | `[{product_id, product_name, sku, quantity}]` (só com saldo > 0) |
| `GET /api/movements` (`?limit=` até 10000, padrão 1000; `?from=`/`?to=` AAAA-MM-DD ou ISO, UTC) | `[{id, productId, productName, sku, locationId, locationCode, userEmail, type, quantity, cost, price, createdAt}]`. `cost`/`price` = **valores da época** |
| `POST /api/movements` `{productId, locationId, type: "IN"\|"OUT", quantity: 1..1000000}` | `{id, new_balance}` (saldo na estante). Erros 400: estante/produto não encontrado, estante inativa (entrada), capacidade excedida, estoque insuficiente |

| `GET /api/stock` (`?q=` SKU, nome ou estante) | `[{productId, productName, sku, locationId, locationCode, warehouseName, quantity}]`: onde está cada SKU (só saldo > 0) |

A checagem de capacidade e de saldo é à prova de concorrência (transação `BEGIN IMMEDIATE`).
Depois de gravar, o produto entra na fila de sincronização com o Mercado Livre (não atrasa a resposta).

## Admin 👑
| Rota | Observação |
|---|---|
| `GET /api/users` | `[{id, email, role, active, createdAt}]` |
| `POST /api/users` (ou `/api/auth/register`) `{email, password, role}` | senha 10–128; `409` e-mail já existe |
| `PUT /api/users/{id}` `{role?, active?}` | `204`. Não deixa tirar o próprio acesso de admin nem ficar sem admin ativo (`400`) |
| `POST /api/users/{id}/password` `{password}` | `204`. A pessoa é deslogada |
| `POST /api/products` · `PUT /api/products/{id}` `{name, sku, cost, price}` | `409` SKU repetido |
| `DELETE /api/products/{id}` | `409` se já teve movimentação ou tem anúncio vinculado |
| `POST /api/warehouses` · `PUT /api/warehouses/{id}` `{name}` | |
| `DELETE /api/warehouses/{id}` | `409` se tem estantes |
| `POST /api/locations` `{warehouseId, sector, rack, level, position?, fullCode, qrToken, maxCapacity}` | `400` depósito inexistente · `409` código ou QR repetido |
| `PUT /api/locations/{id}` (mesmos campos + `isActive`) | capacidade não pode ficar menor que o já guardado |
| `DELETE /api/locations/{id}` | `409` se já teve movimentação (desative em vez de excluir) |
| `GET /api/meli/status` | `{configured, connected, expiresAt, needsReconnect, lastError}` (nunca devolve tokens) |
| `GET /api/meli/authorize` | `{url}` com `state` de uso único. `400` se o app não foi configurado |
| `DELETE /api/meli/credentials` | desconecta a conta |
| `GET /api/meli/links` | `[{id, productId, meliItemId, productName, sku, totalQuantity, createdAt, lastSyncAt, lastSyncOk, lastSyncQty, lastSyncError}]` |
| `POST /api/meli/links` `{product_id, meli_item_id}` | `MLB123...`; `409` anúncio já vinculado. **Não** envia estoque na hora (envia na próxima movimentação ou no "sincronizar") |
| `GET /api/meli/anuncios` (`?atualizar=true` ignora o cache de 5 min) | `{anuncios: [{id, titulo, preco, estoqueMl, vendidos, status, link, foto, sku, full, variacoes: [{id, nome, sku, estoqueMl, vendidos}], visitas, vinculo: {linkId, productId, productName, productSku, estoqueSistema} \| null, produtoMesmoSku}], idadeSegundos}`. Lido da conta do Mercado Livre |
| `POST /api/meli/importar` `{ids: ["MLB..."]}` | `[{meliItemId, resultado: criado\|vinculado\|ja_vinculado\|ignorado\|erro, mensagem}]`. Casa pelo SKU (sem diferenciar maiúsculas); sem SKU usa o código MLB. Anúncios com variações são ignorados. Produto criado com custo 0. **Não** envia estoque |
| `GET /api/meli/rendimento?dias=30` (1–365) | `{de, ate, pedidos, cancelados, unidades, faturamento, tarifas, ticketMedio, lucroEstimado, faturamentoComCusto, porDia: [{dia, pedidos, unidades, faturamento}], porAnuncio: [{meliItemId, titulo, sku, unidades, faturamento, tarifas, custoTotal, lucroEstimado}], truncado}`. Pedidos pagos do período; cancelados ficam fora; tarifas = `sale_fee` × quantidade (sem frete); lucro só dos anúncios com custo |
| `POST /api/movements/lote` `{itens: [{sku, estante, quantidade}], sincronizar}` | `[{linha, ok, mensagem}]`. Cada linha é uma entrada independente; `estante` = código ou QR. Até 5.000 linhas / 512 KB |
| `DELETE /api/meli/links/{id}` · `POST /api/meli/links/{id}/sync` | remover / sincronizar agora (`202`) |
| `GET /api/config` | `{meli: {appId, secretConfigured, redirectUri}, backup: {pasta, pastaPadrao, personalizada, ultimo, ultimoErro}, rede: {nome, porta, enderecos, caFingerprint}}` |
| `PUT /api/config/meli` `{appId, secret?, redirectUri}` | `secret` vazio mantém o atual. Fica cifrado no banco |
| `PUT /api/config/backup` `{pasta?}` | vazio = pasta padrão; testa se dá para gravar |
| `POST /api/config/backup/agora` | `{arquivo}` |
