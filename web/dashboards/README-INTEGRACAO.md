# StockBridge — Dashboards e Exportações (pasta `Dashboards_Exportacoes`)

Painel de indicadores, gráficos e relatórios (Excel/CSV) do StockBridge. Só lê da API; não altera o backend.

## Arquivos
- `index.html`: página (sem dados fictícios embutidos).
- `calculos.js`: cálculo dos indicadores a partir do histórico de movimentações, conversão de datas para o horário de Brasília, premissas e limites.
- `api.js`: cliente das rotas de leitura: `GET /api/health`, `GET /api/movements`, `GET /api/locations/{id}/stock`.
- `chart.js`: 5 gráficos (faturamento, lucro e ROI por produto; movimentações por dia; ocupação das estantes consultadas).
- `relatorios.js`: exportação Excel/CSV (resumo, por produto, histórico, estoque por localização, completo).
- `app.js`: estado e ligação entre as partes.
- `demo.js`: dados fictícios, usados só no botão "Usar dados de demonstração".
- `servidor-local.js`: servidor estático com proxy `/api` → API (opcional).
- `vendor/LEIAME.txt`: Chart.js offline (opcional).

## Como rodar
1. Siga o README do repositório para subir o banco e a API (`cargo run`, porta 3000).
2. Abra o dashboard de uma destas formas:
   - **Simples:** abra `index.html` no navegador (ou por qualquer servidor estático). O endereço padrão da API é `http://localhost:3000` e funciona porque o backend atual libera CORS.
   - **Com o servidor local (opcional):** `cd Dashboards_Exportacoes && node servidor-local.js` (Node 18+, sem dependências) e abra `http://localhost:8080`. Deixe "Endereço da API" vazio para usar o proxy. Só é necessário com versões antigas da API, sem CORS.
3. Clique em "Carregar histórico da API". Para estoque por localização, informe o UUID (e, se quiser ver ocupação, a capacidade máxima).

## O que é real, calculado e indisponível
- **Dado da API:** movimentações (tipo, quantidade, SKU, produto, custo, preço, data) e estoque por localização.
- **Calculado no frontend** (premissas na página e na aba "Premissas e limites" do relatório completo): faturamento bruto, custo das entradas, lucro bruto estimado, ROI, margem, saldo de itens, resumos por produto e por dia.
- **Informado manualmente:** capacidade máxima da estante (a API grava, mas nenhuma rota devolve).
- **Indisponível na API:** lotes, lucro líquido (impostos/tarifas), status de sincronização com o Mercado Livre, listagem de localizações e de produtos.
- **Demonstração:** só aparece após clicar no botão; tudo fica marcado DEMO e os arquivos saem com prefixo `DEMO_`.

## Datas e fuso horário
`createdAt` chega em UTC (sufixo `Z`; assume-se servidor PostgreSQL em UTC, padrão do container). O gráfico "por dia" e a coluna "Data/hora (Brasília)" dos relatórios usam o horário de `America/Sao_Paulo`; a coluna "Data (UTC, como veio da API)" mantém o valor original.

## Observações sobre o backend (somente leitura; nada foi alterado)
- O backend atual tem CORS (`CorsLayer` liberando qualquer origem).
- O schema está em `202610310000_init.sql`; `sku` é único (agrupar por SKU é seguro).
- `custo` e `preço` vêm do cadastro atual do produto: alterar o preço recalcula todo o histórico.
- `GET /api/movements` não informa a localização nem o ID do produto.
- A função `update_item_stock` (Mercado Livre) não é chamada por nenhuma rota; `POST /api/meli/links` só grava o vínculo.
- Pendências conhecidas do repositório (a corrigir pelo grupo): `migrations/` ausente para o `sqlx::migrate!`, coluna `"isActive"` usada no `INSERT` de localizações mas ausente do SQL, e README sem a lista de rotas.
- Sugestões que dependeriam do grupo: devolver `maxCapacity` e o código da localização, criar listagem de localizações/produtos/vínculos, devolver `locationId` e o preço da época nas movimentações.
