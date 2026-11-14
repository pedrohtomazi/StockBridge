# O que mudou em relação ao frontend_adaptado_corrigido
- Dados fictícios saíram do HTML: sem dados carregados, os indicadores mostram "—".
- Novo: `GET /api/movements` alimenta cards, gráficos e relatórios (valores calculados, com premissas explícitas).
- Gráficos "por lote" viraram "por produto (SKU)"; novo gráfico de movimentações por dia.
- Ocupação das estantes: usa estoque real consultado + capacidade informada manualmente.
- Sincronização ML: marcada como indisponível (o backend não expõe status).
- Registros inválidos do histórico (tipo desconhecido, quantidade ≤ 0, custo/preço ausente) são ignorados e contados na tela.
- Modo demonstração opcional e explícito (selo DEMO, prefixo DEMO_ nos arquivos).
- Relatórios novos: histórico de movimentações, análise por produto; relatório completo com aba "Premissas e limites".
- Consulta de estoque com lista vazia não entra no gráfico (a API não diferencia local vazio de ID inexistente).

# Atualização (backend "Api 1.0", com CORS)
- Textos e dicas de CORS atualizados: o backend atual libera CORS; o endereço padrão passou a ser `http://localhost:3000` e o proxy `servidor-local.js` ficou opcional.
- Documentação com o nome real da pasta (`Dashboards_Exportacoes`) e com as pendências do repositório.
- Limites atualizados: vínculo produto↔anúncio (`POST /api/meli/links`) existe, mas nada o devolve; SKU é único no banco; item de CORS removido da lista de limites.
- Datas: dias e horários agora no horário de Brasília (America/Sao_Paulo); o relatório de movimentações traz as colunas "Data (UTC, como veio da API)" e "Data/hora (Brasília)".
