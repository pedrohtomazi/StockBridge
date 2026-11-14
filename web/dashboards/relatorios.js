/*
 * StockBridge - relatórios (SheetJS).
 * Biblioteca: <script src="https://cdn.sheetjs.com/xlsx-0.20.3/package/dist/xlsx.full.min.js"></script>
 * Uso (index.html): StockBridgeReports.exportar("produtos", "xlsx") — os dados vêm de StockBridgeApp.dados().
 * Toda linha exportada traz a coluna "Origem dos dados"; dados de demonstração saem com prefixo DEMO_.
 */
const ORIGEM_DEMO = "Dados demonstrativos (não vêm da API)";
const PREFIXO_DEMO = "DEMO_";

function dataAtual() {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const dataHoraBR = (d) => (d instanceof Date ? d.toLocaleString("pt-BR") : "");

function origemMovimentos(ctx) {
    if (ctx.origemMov === "demo") return ORIGEM_DEMO;
    return `API GET /api/movements (carregado em ${dataHoraBR(ctx.carregadoEm)})`;
}

/* Cada função devolve { linhas, origem } onde origem é "api", "demo" ou null (sem dados). */
const GERADORES = {
    resumo(ctx) {
        if (!ctx.calc) return { linhas: [], origem: null };
        const r = ctx.calc.resumo;
        return { origem: ctx.origemMov, linhas: [{
            "Origem dos dados": origemMovimentos(ctx),
            "Faturamento bruto (calculado)": r.faturamento,
            "Custo das entradas (calculado)": r.custoEntradas,
            "Custo das unidades vendidas (calculado)": r.custoVendido,
            "Lucro bruto estimado (calculado)": r.lucroBruto,
            "Margem bruta (%)": r.margem ?? "",
            "ROI sobre custo vendido (%)": r.roi ?? "",
            "Entradas (un.)": r.entradas,
            "Saídas (un.)": r.saidas,
            "Itens em estoque (saldo calculado)": r.saldoItens,
            "Movimentações consideradas": r.movimentos,
            "Movimentações ignoradas": ctx.ignorados.length,
            "Produtos (SKUs)": r.produtos,
            "Observação": "Lucro bruto estimado: sem impostos, tarifas e frete. Veja a aba/lista de premissas."
        }] };
    },
    produtos(ctx) {
        if (!ctx.calc) return { linhas: [], origem: null };
        const o = origemMovimentos(ctx);
        return { origem: ctx.origemMov, linhas: ctx.calc.porProduto.map(p => ({
            "Origem dos dados": o,
            "SKU": p.sku, "Produto": p.produto,
            "Custo unitário (cadastro atual)": p.custoUnit, "Preço unitário (cadastro atual)": p.precoUnit,
            "Entradas (un.)": p.entradas, "Saídas (un.)": p.saidas, "Saldo calculado (un.)": p.saldo,
            "Faturamento bruto": p.faturamento, "Custo das entradas": p.custoEntradas,
            "Custo das unidades vendidas": p.custoVendido, "Lucro bruto estimado": p.lucroBruto,
            "Margem bruta (%)": p.margem ?? "", "ROI (%)": p.roi ?? ""
        })) };
    },
    movimentos(ctx) {
        if (!ctx.validos.length) return { linhas: [], origem: null };
        const o = origemMovimentos(ctx);
        return { origem: ctx.origemMov, linhas: ctx.validos.map(m => ({
            "Origem dos dados": o,
            "ID": m.id, "Data (UTC, como veio da API)": m.criadoEm ?? "", "Data/hora (Brasília)": StockBridgeCalc.dataHoraLocal(m.criadoEm), "Tipo": m.tipo === "IN" ? "Entrada" : "Saída",
            "SKU": m.sku, "Produto": m.produto, "Quantidade": m.quantidade,
            "Custo unitário": m.custo, "Preço unitário": m.preco,
            "Valor a custo": StockBridgeCalc.arred2(m.quantidade * m.custo),
            "Valor a preço": StockBridgeCalc.arred2(m.quantidade * m.preco)
        })) };
    },
    localizacoes(ctx) {
        const locais = ctx.locais;
        if (!locais.length) return { linhas: [], origem: null };
        const linhas = [];
        for (const l of locais) {
            const origem = ctx.origemLocais === "demo" ? ORIGEM_DEMO
                : `API GET /api/locations/{id}/stock (consultado em ${dataHoraBR(l.consultadoEm)})`;
            const base = { "Origem dos dados": origem, "ID da localização": l.id,
                "Capacidade máxima (informada manualmente)": l.capacidade ?? "" };
            const ocupacao = l.capacidade ? StockBridgeCalc.arred2((l.total / l.capacidade) * 100) : "";
            if (!l.itens.length) linhas.push({ ...base, "ID do produto": "", "Produto": "", "SKU": "", "Quantidade": 0, "Total na localização": l.total, "Ocupação (%)": ocupacao });
            for (const i of l.itens) linhas.push({ ...base, "ID do produto": i.product_id, "Produto": i.product_name, "SKU": i.sku,
                "Quantidade": i.quantity, "Total na localização": l.total, "Ocupação (%)": ocupacao });
        }
        return { linhas, origem: ctx.origemLocais };
    }
};

const NOMES_ABA = { resumo: "Resumo", produtos: "Por produto", movimentos: "Movimentações", localizacoes: "Estoque por localização" };
const NOMES_ARQ = { resumo: "resumo_geral", produtos: "analise_por_produto", movimentos: "historico_movimentacoes", localizacoes: "estoque_por_localizacao" };

function ajustarLarguraColunas(planilha) {
    if (!planilha["!ref"]) return;
    const ref = XLSX.utils.decode_range(planilha["!ref"]);
    const larguras = [];
    for (let c = ref.s.c; c <= ref.e.c; c++) {
        let maior = 10;
        for (let r = ref.s.r; r <= ref.e.r; r++) {
            const cel = planilha[XLSX.utils.encode_cell({ r, c })];
            if (cel && cel.v !== undefined) maior = Math.max(maior, String(cel.v).length + 2);
        }
        larguras.push({ wch: Math.min(maior, 40) });
    }
    planilha["!cols"] = larguras;
}

const contexto = () => window.StockBridgeApp.dados();

function exportarRelatorio(tipo, formato) {
    if (typeof XLSX === "undefined") { alert("SheetJS não foi carregado. Verifique o <script> da biblioteca."); return; }
    const { linhas, origem } = GERADORES[tipo](contexto());
    if (!linhas.length) {
        alert("Não há dados para este relatório. Carregue o histórico da API (ou use os dados de demonstração) e, para estoque por localização, consulte ao menos uma localização.");
        return;
    }
    const planilha = XLSX.utils.json_to_sheet(linhas);
    ajustarLarguraColunas(planilha);
    const prefixo = origem === "demo" ? PREFIXO_DEMO : "";
    if (formato === "csv") {
        const csv = XLSX.utils.sheet_to_csv(planilha, { FS: ";" });
        const url = URL.createObjectURL(new Blob(["﻿" + csv], { type: "text/csv;charset=utf-8;" }));
        const link = document.createElement("a");
        link.href = url; link.download = `${prefixo}${NOMES_ARQ[tipo]}_${dataAtual()}.csv`;
        document.body.appendChild(link); link.click(); link.remove(); URL.revokeObjectURL(url);
        return;
    }
    const wb = XLSX.utils.book_new();
    XLSX.utils.book_append_sheet(wb, planilha, NOMES_ABA[tipo]);
    XLSX.writeFile(wb, `${prefixo}${NOMES_ARQ[tipo]}_${dataAtual()}.xlsx`);
}

function exportarRelatorioCompleto() {
    if (typeof XLSX === "undefined") { alert("SheetJS não foi carregado."); return; }
    const ctx = contexto();
    const wb = XLSX.utils.book_new();
    let demo = false, algum = false;
    for (const tipo of ["resumo", "produtos", "movimentos", "localizacoes"]) {
        const { linhas, origem } = GERADORES[tipo](ctx);
        if (!linhas.length) continue;
        algum = true; demo = demo || origem === "demo";
        const planilha = XLSX.utils.json_to_sheet(linhas);
        ajustarLarguraColunas(planilha);
        XLSX.utils.book_append_sheet(wb, planilha, NOMES_ABA[tipo]);
    }
    if (!algum) { alert("Não há dados para exportar. Carregue o histórico da API (ou use os dados de demonstração)."); return; }
    const notas = [...StockBridgeCalc.PREMISSAS.map(t => ({ "Tipo": "Premissa de cálculo", "Descrição": t })),
        ...StockBridgeCalc.LIMITES_API.map(t => ({ "Tipo": "Limite da API", "Descrição": t }))];
    const aba = XLSX.utils.json_to_sheet(notas);
    aba["!cols"] = [{ wch: 22 }, { wch: 120 }];
    XLSX.utils.book_append_sheet(wb, aba, "Premissas e limites");
    XLSX.writeFile(wb, `${demo ? PREFIXO_DEMO : ""}stockbridge_relatorio_completo_${dataAtual()}.xlsx`);
}

window.StockBridgeReports = { exportar: exportarRelatorio, exportarCompleto: exportarRelatorioCompleto };
