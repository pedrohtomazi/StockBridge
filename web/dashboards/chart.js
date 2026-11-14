/*
 * StockBridge - gráficos (Chart.js).
 * Recebe { calc, locais } montado por app.js:
 *   calc   = resultado de StockBridgeCalc.calcularIndicadores (ou null)
 *   locais = estoques consultados por localização [{ id, total, capacidade|null, itens }]
 * Elementos esperados: #graficoFaturamento, #graficoLucro, #graficoROI, #graficoMovimentos, #graficoEstoque,
 * cada um dentro de .grafico-area, junto de um <p class="sem-dados">.
 */
let graficosStockBridge = {};

const valorValido = (valor) =>
    valor !== null && valor !== undefined && valor !== "" && Number.isFinite(Number(valor));

const moedaBR = (valor) =>
    !valorValido(valor) ? "—" : Number(valor).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });

const percentualBR = (valor) =>
    !valorValido(valor) ? "—" : `${Number(valor).toLocaleString("pt-BR", {
        minimumFractionDigits: 2, maximumFractionDigits: 2 })}%`;

const dataBR = (dia) => {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dia);
    return m ? `${m[3]}/${m[2]}/${m[1]}` : dia;
};

const IDS_GRAFICOS = ["graficoFaturamento", "graficoLucro", "graficoROI", "graficoMovimentos", "graficoEstoque"];

function destruirGrafico(chave) {
    if (graficosStockBridge[chave]) {
        graficosStockBridge[chave].destroy();
        delete graficosStockBridge[chave];
    }
}

function mensagemGrafico(canvasId, texto) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return false;
    const area = canvas.parentElement;
    const aviso = area.querySelector(".sem-dados");
    if (texto) {
        if (aviso) { aviso.textContent = texto; aviso.hidden = false; }
        canvas.style.display = "none";
    } else {
        if (aviso) aviso.hidden = true;
        canvas.style.display = "";
    }
    return true;
}

function criar(chave, canvasId, config) {
    destruirGrafico(chave);
    graficosStockBridge[chave] = new Chart(document.getElementById(canvasId), config);
}

const opcoesBase = (extra = {}) => ({ responsive: true, maintainAspectRatio: false, ...extra });

function graficoPorProduto(chave, canvasId, calc, campo, rotulo, cor, formatar, vazio) {
    destruirGrafico(chave);
    const lista = calc ? calc.porProduto.filter(p => valorValido(p[campo])).slice(0, 10) : [];
    if (!lista.length) { mensagemGrafico(canvasId, vazio); return; }
    mensagemGrafico(canvasId, null);
    criar(chave, canvasId, {
        type: "bar",
        data: {
            labels: lista.map(p => p.sku),
            datasets: [{ label: rotulo, data: lista.map(p => p[campo]), backgroundColor: cor, borderRadius: 6 }]
        },
        options: opcoesBase({
            plugins: {
                legend: { display: false },
                tooltip: { callbacks: {
                    title: itens => lista[itens[0].dataIndex].produto || lista[itens[0].dataIndex].sku,
                    label: c => ` ${rotulo}: ${formatar(c.raw)}`
                } }
            },
            scales: { y: { beginAtZero: true, ticks: { callback: v => formatar(v) } } }
        })
    });
}

function graficoMovimentos(calc) {
    destruirGrafico("movimentos");
    const dias = calc ? calc.porDia : [];
    if (!dias.length) { mensagemGrafico("graficoMovimentos", "Sem dados. Carregue o histórico de movimentações."); return; }
    mensagemGrafico("graficoMovimentos", null);
    criar("movimentos", "graficoMovimentos", {
        type: "bar",
        data: {
            labels: dias.map(d => dataBR(d.dia)),
            datasets: [
                { label: "Entradas (un.)", data: dias.map(d => d.entradas), backgroundColor: "#0d9488", borderRadius: 4 },
                { label: "Saídas (un.)", data: dias.map(d => d.saidas), backgroundColor: "#ea580c", borderRadius: 4 }
            ]
        },
        options: opcoesBase({ scales: { y: { beginAtZero: true, ticks: { precision: 0 } } } })
    });
}

function graficoOcupacao(locais) {
    destruirGrafico("estoque");
    const lista = locais || [];
    if (!lista.length) { mensagemGrafico("graficoEstoque", "Nenhuma localização consultada. Consulte uma localização abaixo."); return; }
    mensagemGrafico("graficoEstoque", null);
    criar("estoque", "graficoEstoque", {
        type: "bar",
        data: {
            labels: lista.map(l => (l.id.length > 12 ? l.id.slice(0, 8) + "…" : l.id)),
            datasets: [
                { label: "Itens armazenados", data: lista.map(l => l.total), backgroundColor: "#0d9488", borderRadius: 5 },
                { label: "Espaço livre (capacidade informada)", data: lista.map(l => l.capacidade ? Math.max(l.capacidade - l.total, 0) : null),
                  backgroundColor: "#cbd5e1", borderRadius: 5 }
            ]
        },
        options: opcoesBase({
            scales: { x: { stacked: true }, y: { stacked: true, beginAtZero: true, ticks: { precision: 0 } } },
            plugins: { tooltip: { callbacks: {
                title: itens => lista[itens[0].dataIndex].id,
                afterBody: itens => {
                    const l = lista[itens[0].dataIndex];
                    return l.capacidade
                        ? `Ocupação: ${percentualBR((l.total / l.capacidade) * 100)} (capacidade ${l.capacidade}, informada manualmente)`
                        : "Capacidade não informada";
                }
            } } }
        })
    });
}

function renderizarDashboard(ctx) {
    if (typeof Chart === "undefined") {
        IDS_GRAFICOS.forEach(id => mensagemGrafico(id,
            "Gráfico indisponível: a biblioteca Chart.js não foi carregada (sem internet ou CDN bloqueada). Veja vendor/LEIAME.txt."));
        return;
    }
    const calc = ctx && ctx.calc;
    const vazio = "Sem dados. Carregue o histórico de movimentações.";
    graficoPorProduto("faturamento", "graficoFaturamento", calc, "faturamento", "Faturamento bruto", "#2563eb", moedaBR, vazio);
    graficoPorProduto("lucro", "graficoLucro", calc, "lucroBruto", "Lucro bruto estimado", "#7c3aed", moedaBR, vazio);
    graficoPorProduto("roi", "graficoROI", calc, "roi", "ROI", "#ea580c", percentualBR,
        calc ? "Sem ROI calculável (não há saídas com custo maior que zero)." : vazio);
    graficoMovimentos(calc);
    graficoOcupacao(ctx && ctx.locais);
}

window.StockBridgeCharts = { renderizar: renderizarDashboard, moedaBR, percentualBR, valorValido, dataBR };
