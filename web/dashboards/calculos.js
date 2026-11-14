/*
 * StockBridge - cálculo dos indicadores a partir do histórico de movimentações.
 * Funções puras (sem DOM), usadas por app.js, chart.js e relatorios.js.
 *
 * Fonte: GET /api/movements -> [{ id, productName, sku, type, quantity, cost, price, createdAt }]
 *   - type: "IN" (entrada) ou "OUT" (saída)
 *   - cost/price: cadastro ATUAL do produto (JOIN em products), não o valor da época
 *   - createdAt: texto "AAAA-MM-DDTHH:MM:SSZ" em UTC (pode ser null); os dias são convertidos para America/Sao_Paulo
 */
(function (raiz) {
    const TIPOS = ['IN', 'OUT'];
    const num = v => (v === null || v === undefined || v === '' ? NaN : Number(v));
    const arred2 = n => Math.round((Number(n) + Number.EPSILON) * 100) / 100;

    /* ---- datas: createdAt vem em UTC; os dias e horários exibidos usam o horário de Brasília ---- */
    const FUSO = 'America/Sao_Paulo';
    const fmtPartes = new Intl.DateTimeFormat('en-US', { timeZone: FUSO, year: 'numeric', month: '2-digit', day: '2-digit' });
    const fmtHora = new Intl.DateTimeFormat('pt-BR', { timeZone: FUSO, dateStyle: 'short', timeStyle: 'medium' });

    function diaLocal(texto) {
        if (!texto) return 'sem data';
        if (/^\d{4}-\d{2}-\d{2}$/.test(String(texto))) return String(texto); // data sem hora: já é o dia
        const d = new Date(texto);
        if (Number.isNaN(d.getTime())) {
            const m = /^(\d{4}-\d{2}-\d{2})/.exec(String(texto));
            return m ? m[1] : 'sem data';
        }
        const p = {};
        for (const parte of fmtPartes.formatToParts(d)) p[parte.type] = parte.value;
        return `${p.year}-${p.month}-${p.day}`;
    }

    function dataHoraLocal(texto) {
        if (!texto) return '';
        if (/^\d{4}-\d{2}-\d{2}$/.test(String(texto))) return String(texto);
        const d = new Date(texto);
        return Number.isNaN(d.getTime()) ? String(texto) : fmtHora.format(d);
    }

    const PREMISSAS = [
        'Saída (OUT) é tratada como venda e entrada (IN) como compra/reposição. A API não distingue venda de outras saídas (perda, transferência, devolução).',
        'Faturamento bruto = soma de (quantidade das saídas × preço do produto).',
        'Custo das entradas = soma de (quantidade das entradas × custo do produto). Custo das unidades vendidas = soma de (quantidade das saídas × custo).',
        'Lucro bruto estimado = faturamento − custo das unidades vendidas. Não inclui impostos, tarifas do Mercado Livre, frete nem outras despesas, por isso NÃO é lucro líquido.',
        'ROI = lucro bruto estimado ÷ custo das unidades vendidas. Margem bruta = lucro bruto estimado ÷ faturamento.',
        'Custo e preço vêm do cadastro atual do produto, não do valor da época da movimentação: se o preço mudar, todo o histórico é recalculado com o novo valor.',
        'Itens em estoque = soma das entradas − soma das saídas do histórico. Só vale se toda alteração de estoque passou por /api/movements.',
        'Produtos são agrupados por SKU (único no banco), pois a rota de histórico não devolve o ID do produto.',
        'Datas: o createdAt chega em UTC (sufixo Z; assume-se servidor PostgreSQL em UTC, que é o padrão do container). Antes de agrupar por dia, a data é convertida para o horário de Brasília (America/Sao_Paulo).',
        'Registros com tipo diferente de IN/OUT, quantidade não inteira ou ≤ 0, ou custo/preço ausente ou negativo são ignorados e contados.'
    ];

    const LIMITES_API = [
        'Lotes: a API não tem a noção de lote. Por isso os gráficos de faturamento, lucro e ROI agora são por produto (SKU).',
        'Lucro líquido, impostos e tarifas: não há dados na API; só é possível estimar o lucro bruto.',
        'Sincronização com o Mercado Livre: o backend tem o callback de autorização (OAuth), o vínculo produto↔anúncio (POST /api/meli/links, que só grava) e uma função interna de atualização de estoque que nenhuma rota chama; não existe consulta de vínculos nem status ou taxa de sincronização.',
        'Capacidade das estantes: o maxCapacity é gravado ao criar a localização, mas nenhuma rota de consulta o devolve; aqui ele precisa ser informado manualmente.',
        'Localizações: não existe rota para listar localizações, e o histórico de movimentações não informa a localização. É preciso conhecer o UUID de cada uma.',
        'Produtos: não existe GET de produtos; produtos sem movimentação não aparecem. O vínculo com o anúncio do Mercado Livre é gravado, mas nenhuma rota o devolve; categoria, lote e localização do produto não existem na API.'
    ];

    function validarMovimento(m) {
        if (!m || typeof m !== 'object') return 'registro inválido';
        if (!TIPOS.includes(m.type)) return `tipo desconhecido (${m.type})`;
        const q = num(m.quantity);
        if (!Number.isInteger(q) || q <= 0) return `quantidade inválida (${m.quantity})`;
        const c = num(m.cost), p = num(m.price);
        if (!Number.isFinite(c) || !Number.isFinite(p)) return 'custo ou preço ausente';
        if (c < 0 || p < 0) return 'custo ou preço negativo';
        if (!m.sku) return 'SKU ausente';
        return null;
    }

    function normalizarMovimentos(lista) {
        const validos = [], ignorados = [];
        if (!Array.isArray(lista)) return { validos, ignorados: [{ indice: -1, motivo: 'a resposta não é uma lista' }] };
        lista.forEach((m, indice) => {
            const motivo = validarMovimento(m);
            if (motivo) { ignorados.push({ indice, id: m && m.id, motivo }); return; }
            validos.push({
                id: m.id, produto: String(m.productName == null ? '' : m.productName), sku: String(m.sku),
                tipo: m.type, quantidade: Number(m.quantity), custo: Number(m.cost), preco: Number(m.price),
                criadoEm: typeof m.createdAt === 'string' && m.createdAt ? m.createdAt : null
            });
        });
        return { validos, ignorados };
    }

    function calcularIndicadores(movs) {
        const produtos = new Map(), dias = new Map();
        const t = { entradas: 0, saidas: 0, faturamento: 0, custoEntradas: 0, custoVendido: 0 };
        for (const m of movs) {
            let p = produtos.get(m.sku);
            if (!p) {
                p = { sku: m.sku, produto: m.produto, custoUnit: m.custo, precoUnit: m.preco,
                    entradas: 0, saidas: 0, faturamento: 0, custoEntradas: 0, custoVendido: 0 };
                produtos.set(m.sku, p);
            }
            const dia = diaLocal(m.criadoEm);
            let d = dias.get(dia);
            if (!d) { d = { dia, entradas: 0, saidas: 0 }; dias.set(dia, d); }
            if (m.tipo === 'IN') {
                p.entradas += m.quantidade; d.entradas += m.quantidade; t.entradas += m.quantidade;
                p.custoEntradas += m.quantidade * m.custo; t.custoEntradas += m.quantidade * m.custo;
            } else {
                p.saidas += m.quantidade; d.saidas += m.quantidade; t.saidas += m.quantidade;
                p.faturamento += m.quantidade * m.preco; t.faturamento += m.quantidade * m.preco;
                p.custoVendido += m.quantidade * m.custo; t.custoVendido += m.quantidade * m.custo;
            }
        }
        const derivar = o => {
            const lucro = o.faturamento - o.custoVendido;
            return {
                ...o,
                faturamento: arred2(o.faturamento), custoEntradas: arred2(o.custoEntradas), custoVendido: arred2(o.custoVendido),
                lucroBruto: arred2(lucro),
                roi: o.custoVendido > 0 ? arred2((lucro / o.custoVendido) * 100) : null,
                margem: o.faturamento > 0 ? arred2((lucro / o.faturamento) * 100) : null,
                saldo: o.entradas - o.saidas
            };
        };
        const porProduto = Array.from(produtos.values()).map(derivar)
            .sort((a, b) => b.faturamento - a.faturamento || a.sku.localeCompare(b.sku));
        const porDia = Array.from(dias.values()).sort((a, b) =>
            a.dia === 'sem data' ? 1 : b.dia === 'sem data' ? -1 : a.dia.localeCompare(b.dia));
        const r = derivar(t);
        return {
            resumo: {
                faturamento: r.faturamento, custoEntradas: r.custoEntradas, custoVendido: r.custoVendido,
                lucroBruto: r.lucroBruto, roi: r.roi, margem: r.margem,
                entradas: t.entradas, saidas: t.saidas, saldoItens: t.entradas - t.saidas,
                movimentos: movs.length, produtos: produtos.size
            },
            porProduto, porDia
        };
    }

    const api = { PREMISSAS, LIMITES_API, FUSO, normalizarMovimentos, calcularIndicadores, validarMovimento, arred2, diaLocal, dataHoraLocal };
    raiz.StockBridgeCalc = api;
    if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : globalThis);
