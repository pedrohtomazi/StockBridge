/*
 * StockBridge - orquestração do dashboard: estado, cards, painéis e ligação com api.js / calculos.js / chart.js.
 * Nenhum valor é inventado: sem dados carregados, os indicadores mostram "—".
 */
(() => {
    const Api = window.StockBridgeApi, Calc = window.StockBridgeCalc, Demo = window.StockBridgeDemo, Charts = window.StockBridgeCharts;
    const el = id => document.getElementById(id);
    const fmtInt = n => Number(n).toLocaleString('pt-BR');

    const estado = {
        origemMov: null,      // 'api' | 'demo' | null  (movimentações e indicadores)
        origemLocais: null,   // 'api' | 'demo' | null  (estoque por localização)
        movimentos: [], validos: [], ignorados: [], calc: null, carregadoEm: null,
        locais: new Map()
    };
    let seqMov = 0, seqLoc = 0;

    const listaLocais = () => Array.from(estado.locais.values());
    const dataHora = d => (d ? d.toLocaleString('pt-BR') : '');

    function atualizarSelos() {
        document.querySelectorAll('[data-origem]').forEach(s => {
            const origem = s.dataset.origem === 'mov' ? estado.origemMov : estado.origemLocais;
            if (origem === 'demo') { s.className = 'selo selo-demo'; s.textContent = 'DEMO'; }
            else if (origem === 'api') { s.className = 'selo selo-real'; s.textContent = s.dataset.tipo === 'calc' ? 'CALCULADO · API' : 'DADOS DA API'; }
            else { s.className = 'selo selo-nd'; s.textContent = 'SEM DADOS'; }
        });
    }

    function atualizarBanner() {
        const b = el('bannerModo');
        if (estado.origemMov === 'demo' || estado.origemLocais === 'demo') {
            b.className = 'aviso aviso-demo';
            b.innerHTML = '<strong>MODO DEMONSTRAÇÃO.</strong> Os números abaixo são fictícios (arquivo demo.js) e <strong>não vêm da API</strong>. Arquivos exportados saem com prefixo <code>DEMO_</code>.';
        } else if (estado.origemMov === 'api') {
            b.className = 'aviso aviso-info';
            b.innerHTML = '<strong>Dados da API, indicadores calculados.</strong> Faturamento, custo, lucro bruto, ROI e saldo são <em>calculados</em> a partir do histórico de movimentações, com as premissas listadas ao final da página. Lucro líquido e sincronização com o Mercado Livre não estão disponíveis na API.';
        } else {
            b.className = 'aviso aviso-nd';
            b.innerHTML = '<strong>Nenhum dado carregado.</strong> Clique em "Carregar histórico da API" (com a API rodando) ou em "Usar dados de demonstração".';
        }
    }

    function atualizarCards() {
        const r = estado.calc && estado.calc.resumo;
        el('faturamento').textContent = r ? Charts.moedaBR(r.faturamento) : '—';
        el('custo').textContent = r ? Charts.moedaBR(r.custoEntradas) : '—';
        el('lucro').textContent = r ? Charts.moedaBR(r.lucroBruto) : '—';
        el('roi').textContent = r ? Charts.percentualBR(r.roi) : '—';
        el('estoque').textContent = r ? fmtInt(r.saldoItens) : '—';
    }

    function textoIgnorados() {
        if (!estado.ignorados.length) return '';
        const amostra = estado.ignorados.slice(0, 3).map(i => i.motivo).join('; ');
        return ` ${estado.ignorados.length} registro(s) ignorado(s): ${amostra}${estado.ignorados.length > 3 ? '…' : ''}.`;
    }

    function renderizar() {
        atualizarSelos(); atualizarBanner(); atualizarCards();
        Charts.renderizar({ calc: estado.calc, locais: listaLocais() });
    }

    function recalcular() {
        const { validos, ignorados } = Calc.normalizarMovimentos(estado.movimentos);
        estado.validos = validos; estado.ignorados = ignorados;
        estado.calc = validos.length ? Calc.calcularIndicadores(validos) : null;
    }

    /* ---------- conexão e histórico ---------- */
    async function verificarApi() {
        el('apiStatus').textContent = 'Conectando...';
        try {
            const texto = await Api.health();
            el('apiStatus').textContent = `API acessível em ${Api.descricaoBase()}: ${texto}`;
        } catch (erro) {
            el('apiStatus').textContent = `Não foi possível acessar a API em ${Api.descricaoBase()} (${erro.message}). Confirme se ela está rodando.${Api.dica(erro)}`;
        }
    }

    async function carregarMovimentos() {
        const minha = ++seqMov;
        el('movStatus').textContent = 'Carregando histórico...';
        try {
            const dados = await Api.listarMovimentos();
            if (minha !== seqMov) return;
            estado.movimentos = dados; estado.origemMov = 'api'; estado.carregadoEm = new Date();
            if (estado.origemLocais === 'demo') { estado.locais.clear(); estado.origemLocais = null; }
            recalcular(); renderizar();
            el('movStatus').textContent = dados.length
                ? `Histórico carregado em ${dataHora(estado.carregadoEm)}: ${dados.length} movimentação(ões), ${estado.validos.length} considerada(s).${textoIgnorados()}`
                : 'A API retornou histórico vazio: ainda não há movimentações registradas.';
        } catch (erro) {
            if (minha !== seqMov) return;
            el('movStatus').textContent = `Falha ao carregar o histórico: ${erro.message}.${Api.dica(erro)} Os dados exibidos anteriormente foram mantidos.`;
        }
    }

    function usarDemo() {
        seqMov++; seqLoc++;
        estado.movimentos = Demo.movimentos.map(m => ({ ...m })); estado.origemMov = 'demo'; estado.carregadoEm = new Date();
        estado.locais.clear();
        for (const l of Demo.locais) {
            estado.locais.set(l.id, { id: l.id, itens: l.itens.map(i => ({ ...i })), total: l.itens.reduce((s, i) => s + i.quantity, 0),
                capacidade: l.capacidade, consultadoEm: new Date() });
        }
        estado.origemLocais = 'demo';
        recalcular(); renderizar();
        el('movStatus').textContent = 'Dados de demonstração carregados (fictícios).';
        el('estoqueStatus').textContent = 'Localizações de demonstração carregadas (fictícias).';
    }

    function limpar() {
        seqMov++; seqLoc++;
        Object.assign(estado, { origemMov: null, origemLocais: null, movimentos: [], validos: [], ignorados: [], calc: null, carregadoEm: null });
        estado.locais.clear();
        renderizarTabela([]);
        el('movStatus').textContent = 'Dados limpos.';
        el('estoqueStatus').textContent = '';
        renderizar();
    }

    /* ---------- estoque por localização ---------- */
    function celula(valor) {
        const td = document.createElement('td');
        td.style.padding = '8px'; td.style.borderTop = '1px solid #e2e8f0';
        td.textContent = valor == null ? '' : String(valor);
        return td;
    }

    function renderizarTabela(itens) {
        const tbody = el('corpoEstoqueApi');
        tbody.replaceChildren();
        if (!itens.length) {
            const tr = document.createElement('tr'), td = document.createElement('td');
            td.colSpan = 4; td.style.padding = '8px'; td.textContent = 'Nenhum produto para exibir.';
            tr.appendChild(td); tbody.appendChild(tr); return;
        }
        for (const i of itens) {
            const tr = document.createElement('tr');
            for (const v of [i.product_id, i.product_name, i.sku, i.quantity]) tr.appendChild(celula(v));
            tbody.appendChild(tr);
        }
    }

    async function consultarEstoque(evento) {
        evento.preventDefault();
        const id = el('locationId').value.trim();
        if (!id) return;
        const bruto = el('capacidadeManual').value.trim();
        const capacidade = bruto === '' ? null : Number(bruto);
        if (capacidade !== null && (!Number.isInteger(capacidade) || capacidade <= 0)) {
            el('estoqueStatus').textContent = 'Capacidade máxima inválida: informe um número inteiro maior que zero ou deixe em branco.';
            return;
        }
        const minha = ++seqLoc;
        el('estoqueStatus').textContent = 'Consultando estoque...';
        try {
            const dados = await Api.estoqueLocalizacao(id);
            if (minha !== seqLoc) return;
            const validos = dados.filter(i => i && Number.isFinite(Number(i.quantity)));
            const total = validos.reduce((s, i) => s + Number(i.quantity), 0);
            renderizarTabela(validos);
            // Lista vazia pode ser ID inexistente (a API não diferencia): não entra no gráfico nem nos relatórios.
            if (validos.length) {
                if (estado.origemLocais === 'demo') estado.locais.clear();
                estado.origemLocais = 'api';
                estado.locais.set(id, { id, itens: validos, total, capacidade, consultadoEm: new Date() });
                renderizar();
            }
            const aviso = validos.length !== dados.length ? ` ${dados.length - validos.length} item(ns) com quantidade inválida foram ignorados.` : '';
            el('estoqueStatus').textContent = validos.length
                ? `Consulta concluída: ${validos.length} produto(s), ${fmtInt(total)} unidade(s) na localização.${aviso}`
                : 'A API retornou lista vazia. Isso vale tanto para localização sem produtos quanto para ID inexistente: a API não diferencia os dois casos. Confira o ID.';
        } catch (erro) {
            if (minha !== seqLoc) return;
            el('estoqueStatus').textContent = `Falha ao consultar: ${erro.message}. Verifique o ID da localização e a API.${Api.dica(erro)}`;
        }
    }

    /* ---------- inicialização ---------- */
    function preencherListas() {
        const li = t => { const e = document.createElement('li'); e.textContent = t; return e; };
        el('listaPremissas').replaceChildren(...Calc.PREMISSAS.map(li));
        el('listaLimites').replaceChildren(...Calc.LIMITES_API.map(li));
    }

    document.addEventListener('DOMContentLoaded', () => {
        preencherListas();
        el('btnVerificarApi').addEventListener('click', verificarApi);
        el('btnCarregarMovimentos').addEventListener('click', carregarMovimentos);
        el('btnDemo').addEventListener('click', usarDemo);
        el('btnLimpar').addEventListener('click', limpar);
        el('formEstoqueLocalizacao').addEventListener('submit', consultarEstoque);
        renderizar();
        verificarApi();
    });

    window.StockBridgeApp = {
        dados: () => ({
            origemMov: estado.origemMov, origemLocais: estado.origemLocais, calc: estado.calc, validos: estado.validos,
            ignorados: estado.ignorados, carregadoEm: estado.carregadoEm, locais: listaLocais()
        })
    };
})();
