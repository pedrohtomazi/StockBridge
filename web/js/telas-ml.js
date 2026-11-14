/* Telas: Anúncios do Mercado Livre, Rendimento, Onde está (mapa por SKU) e Entrada em lote. */
import { api } from './api.js';
import {
    h, limpar, aviso, abrirModal, confirmar, vazio, carregando, campo,
    fmtMoeda, fmtNum, fmtPct, normalizar, baixarArquivo, paraCsv, diaBrasilia,
} from './util.js';
import { topo, carregar, enviar, acao, input, select, opcao, botao, tabela, kpi } from './telas.js';

const STATUS_ML = { active: 'Ativo', paused: 'Pausado', closed: 'Encerrado', under_review: 'Em revisão', inactive: 'Inativo' };

// ---------------------------------------------------------------------------
// ANÚNCIOS DO MERCADO LIVRE (admin)
// ---------------------------------------------------------------------------
export async function telaAnuncios(raiz, ctx) {
    const area = h('div');
    const recarregar = (atualizar = false) => desenharTela(atualizar);
    const btnAtualizar = botao('Atualizar do Mercado Livre', { onclick: () => recarregar(true) });
    limpar(raiz, topo('Anúncios do Mercado Livre', 'Os anúncios da sua conta, com SKU, vendas e visitas. Importe-os como produtos (casando pelo SKU).', btnAtualizar), area);

    async function desenharTela(atualizar) {
        await carregar(area, () => api.anunciosMl(atualizar), (dados) => {
            const anuncios = dados.anuncios;
            if (!anuncios.length) { area.append(vazio('Nenhum anúncio encontrado nesta conta.')); return; }
            const selecionados = new Set();
            const busca = input({ type: 'search', placeholder: 'Buscar título, SKU ou MLB…' });
            const filtro = select({},
                opcao('', 'Todos'), opcao('sem', 'Ainda não importados'), opcao('com', 'Já vinculados'),
                opcao('ativos', 'Só ativos'), opcao('var', 'Com variações'), opcao('full', 'No Full'));
            const todos = h('input', { type: 'checkbox', 'aria-label': 'Selecionar todos os visíveis' });
            const btnImportar = botao('Importar selecionados', { class: 'btn btn-primario', disabled: true });
            const contador = h('span', { class: 'sub pequeno' });
            const corpo = h('tbody');
            let visiveis = [];

            const totalVendidos = anuncios.reduce((s, a) => s + a.vendidos, 0);
            const totalVisitas = anuncios.reduce((s, a) => s + (a.visitas || 0), 0);
            const resumo = h('div', { class: 'kpis kpis-compactos' },
                kpi('Anúncios', fmtNum(anuncios.length), `${fmtNum(anuncios.filter((a) => a.status === 'active').length)} ativos`),
                kpi('Já vinculados', fmtNum(anuncios.filter((a) => a.vinculo).length), `${fmtNum(anuncios.filter((a) => !a.vinculo).length)} a importar`),
                kpi('Vendidos (total)', fmtNum(totalVendidos), 'desde a criação dos anúncios'),
                kpi('Visitas (total)', fmtNum(totalVisitas), totalVisitas ? `conversão ${fmtPct(totalVendidos / totalVisitas)}` : ''));

            function atualizarBotao() {
                btnImportar.disabled = selecionados.size === 0;
                contador.textContent = selecionados.size ? `${selecionados.size} selecionado(s)` : '';
            }

            function desenhar() {
                const termo = normalizar(busca.value);
                visiveis = anuncios.filter((a) => {
                    if (termo && !normalizar(`${a.titulo} ${a.sku || ''} ${a.id}`).includes(termo)) return false;
                    if (filtro.value === 'sem' && a.vinculo) return false;
                    if (filtro.value === 'com' && !a.vinculo) return false;
                    if (filtro.value === 'ativos' && a.status !== 'active') return false;
                    if (filtro.value === 'var' && !a.variacoes.length) return false;
                    if (filtro.value === 'full' && !a.full) return false;
                    return true;
                });
                limpar(corpo, visiveis.map((a) => {
                    const podeImportar = !a.vinculo && !a.variacoes.length;
                    const marca = h('input', { type: 'checkbox', disabled: !podeImportar, checked: selecionados.has(a.id), 'aria-label': `Selecionar ${a.titulo}` });
                    marca.addEventListener('change', () => { if (marca.checked) selecionados.add(a.id); else selecionados.delete(a.id); atualizarBotao(); });
                    let situacao;
                    if (a.vinculo) {
                        situacao = h('div', {}, h('span', { class: 'badge badge-entrada' }, 'Vinculado'),
                            h('div', { class: 'pequeno sub' }, `${a.vinculo.productSku} · ${fmtNum(a.vinculo.estoqueSistema)} no sistema`));
                    } else if (a.variacoes.length) {
                        situacao = h('span', { class: 'badge badge-saida', title: a.variacoes.map((v) => `${v.nome}: ${v.sku || 'sem SKU'}`).join('\n') }, `${a.variacoes.length} variações`);
                    } else if (a.produtoMesmoSku) {
                        situacao = h('span', { class: 'sub pequeno' }, `Vincula ao produto "${a.produtoMesmoSku}"`);
                    } else {
                        situacao = h('span', { class: 'sub pequeno' }, a.sku ? 'Cria produto novo' : 'Sem SKU: usa o código MLB');
                    }
                    return h('tr', {},
                        h('td', {}, marca),
                        h('td', {}, a.foto ? h('img', { src: a.foto, alt: '', class: 'miniatura', loading: 'lazy', referrerpolicy: 'no-referrer' }) : null),
                        h('td', {},
                            a.link ? h('a', { class: 'link', href: a.link, target: '_blank', rel: 'noopener noreferrer' }, a.titulo) : a.titulo,
                            h('div', { class: 'mono pequeno sub' }, a.id, a.full ? ' · Full' : '')),
                        h('td', { class: 'mono' }, a.sku || '—'),
                        h('td', { class: 'num' }, fmtMoeda(a.preco)),
                        h('td', { class: 'num' }, fmtNum(a.estoqueMl)),
                        h('td', { class: 'num' }, h('strong', {}, fmtNum(a.vendidos))),
                        h('td', { class: 'num' }, a.visitas == null ? '—' : fmtNum(a.visitas)),
                        h('td', {}, STATUS_ML[a.status] || a.status),
                        h('td', {}, situacao));
                }));
                if (!visiveis.length) corpo.append(h('tr', {}, h('td', { colspan: 10, class: 'vazio-celula' }, 'Nada no filtro.')));
                todos.checked = false;
            }

            todos.addEventListener('change', () => {
                visiveis.filter((a) => !a.vinculo && !a.variacoes.length).forEach((a) => { if (todos.checked) selecionados.add(a.id); else selecionados.delete(a.id); });
                desenhar();
                todos.checked = selecionados.size > 0;
                atualizarBotao();
            });
            busca.addEventListener('input', desenhar);
            filtro.addEventListener('change', desenhar);

            btnImportar.addEventListener('click', async () => {
                const ids = [...selecionados];
                if (!await confirmar(`Importar ${ids.length} anúncio(s)? Para cada um, o sistema cria o produto (ou usa o que já tem o mesmo SKU) e vincula. O estoque NÃO é enviado ao Mercado Livre agora: depois, lance as entradas nas estantes.`, { textoOk: 'Importar' })) return;
                btnImportar.disabled = true;
                try {
                    const r = await api.importarAnuncios(ids);
                    mostrarResultadoImportacao(r);
                    recarregar(false);
                } catch (e) {
                    aviso(e.message, 'erro');
                    btnImportar.disabled = false;
                }
            });

            area.append(resumo,
                h('div', { class: 'filtros' }, busca, filtro, btnImportar, contador),
                tabela([{ texto: '', classe: '' }, '', 'Anúncio', 'SKU', { texto: 'Preço', classe: 'num' }, { texto: 'Estoque ML', classe: 'num' },
                    { texto: 'Vendidos', classe: 'num' }, { texto: 'Visitas', classe: 'num' }, 'Status', 'No sistema'], corpo),
                h('p', { class: 'sub pequeno' }, `Dados lidos do Mercado Livre há ${dados.idadeSegundos < 60 ? 'menos de 1 minuto' : `${Math.round(dados.idadeSegundos / 60)} min`}. `,
                    'Anúncios com variações ainda não podem ser importados (o sistema controla um estoque por anúncio).'));
            corpo.parentElement.querySelector('thead th').append(todos);
            desenhar();
            atualizarBotao();
        });
    }
    recarregar(false);
}

function mostrarResultadoImportacao(resultados) {
    const rotulo = { criado: 'Criado', vinculado: 'Vinculado', ja_vinculado: 'Já vinculado', ignorado: 'Ignorado', erro: 'Erro' };
    const conta = (r) => resultados.filter((x) => x.resultado === r).length;
    abrirModal('Resultado da importação', h('div', {},
        h('p', {}, `${conta('criado')} produto(s) criado(s), ${conta('vinculado')} vinculado(s) a produtos existentes, ${conta('ignorado') + conta('erro') + conta('ja_vinculado')} não importado(s).`),
        h('div', { class: 'alerta' }, h('strong', {}, 'Próximos passos: '),
            '1) em Produtos, preencha o CUSTO dos produtos criados (o Mercado Livre não sabe quanto você pagou); ',
            '2) lance as quantidades nas estantes (Movimentar > Entrada em lote, colando do Excel: SKU, estante, quantidade). ',
            'Só depois disso o estoque passa a ser enviado ao Mercado Livre.'),
        tabela(['Anúncio', 'Resultado', 'Detalhe'], resultados.map((r) => h('tr', {},
            h('td', { class: 'mono' }, r.meliItemId),
            h('td', {}, h('span', { class: `badge ${['criado', 'vinculado'].includes(r.resultado) ? 'badge-entrada' : 'badge-saida'}` }, rotulo[r.resultado] || r.resultado)),
            h('td', { class: 'pequeno' }, r.mensagem))))), { larga: true });
}

// ---------------------------------------------------------------------------
// RENDIMENTO (admin)
// ---------------------------------------------------------------------------
export async function telaRendimento(raiz) {
    const area = h('div');
    const periodo = select({ 'aria-label': 'Período' }, opcao('7', 'Últimos 7 dias'), opcao('30', 'Últimos 30 dias'),
        opcao('90', 'Últimos 90 dias'), opcao('365', 'Últimos 12 meses'));
    periodo.value = '30';
    periodo.addEventListener('change', () => desenharTela());
    limpar(raiz, topo('Rendimento', 'Vendas do Mercado Livre no período (pedidos pagos), com tarifas e lucro estimado.', periodo), area);

    async function desenharTela() {
        await carregar(area, () => api.rendimento(periodo.value), (r) => {
            const cobertura = r.faturamento > 0 ? r.faturamentoComCusto / r.faturamento : 0;
            area.append(h('div', { class: 'kpis' },
                kpi('Faturamento', fmtMoeda(r.faturamento), `${fmtNum(r.pedidos)} pedidos · ${fmtNum(r.unidades)} unidades`),
                kpi('Ticket médio', fmtMoeda(r.ticketMedio), `${fmtNum(r.cancelados)} cancelado(s) fora da conta`),
                kpi('Tarifas do Mercado Livre', fmtMoeda(r.tarifas), r.faturamento ? `${fmtPct(r.tarifas / r.faturamento)} do faturamento` : ''),
                kpi('Lucro estimado', fmtMoeda(r.lucroEstimado), r.faturamento
                    ? `considera ${fmtPct(cobertura)} do faturamento (produtos com custo)` : '')));

            if (r.truncado) area.append(h('div', { class: 'alerta' }, 'Período com muitos pedidos: foram considerados os 3.000 mais recentes. Escolha um período menor para o total exato.'));
            if (r.faturamento && cobertura < 0.999) {
                area.append(h('p', { class: 'sub pequeno' }, 'O lucro só inclui anúncios vinculados a produtos com custo preenchido. Frete e impostos não estão descontados.'));
            }

            // Gráfico de barras por dia (sem bibliotecas). Dias sem venda entram com zero.
            const porDia = new Map(r.porDia.map((d) => [d.dia, d]));
            const dias = [];
            for (let d = new Date(`${r.de}T12:00:00Z`); d <= new Date(`${r.ate}T12:00:00Z`); d.setUTCDate(d.getUTCDate() + 1)) {
                const chave = d.toISOString().slice(0, 10);
                dias.push(porDia.get(chave) || { dia: chave, pedidos: 0, unidades: 0, faturamento: 0 });
            }
            const max = Math.max(1, ...dias.map((d) => d.faturamento));
            area.append(h('section', { class: 'cartao' }, h('h2', {}, 'Faturamento por dia'),
                r.porDia.length
                    ? h('div', { class: 'grafico-barras', role: 'img', 'aria-label': 'Faturamento por dia' }, dias.map((d) =>
                        h('div', { class: 'barra-dia', title: `${d.dia.split('-').reverse().join('/')}: ${fmtMoeda(d.faturamento)} · ${d.pedidos} pedido(s)` },
                            h('div', { class: 'barra-valor', style: { height: `${Math.max(2, (d.faturamento / max) * 100)}%` } }),
                            h('span', { class: 'barra-rotulo' }, d.dia.slice(8, 10)))))
                    : vazio('Nenhuma venda no período.')));

            const exportar = botao('Exportar CSV', {
                onclick: () => {
                    const linhas = [['Anúncio', 'Título', 'SKU', 'Unidades', 'Faturamento', 'Tarifas', 'Custo', 'Lucro estimado']];
                    r.porAnuncio.forEach((a) => linhas.push([a.meliItemId, a.titulo, a.sku || '', a.unidades, a.faturamento, a.tarifas, a.custoTotal ?? '', a.lucroEstimado ?? '']));
                    baixarArquivo(`rendimento_${r.de}_a_${r.ate}.csv`, paraCsv(linhas));
                },
            });
            area.append(h('section', { class: 'cartao' },
                h('div', { class: 'cartao-topo' }, h('h2', {}, 'Por anúncio'), exportar),
                r.porAnuncio.length
                    ? tabela(['Anúncio', 'SKU', { texto: 'Unid.', classe: 'num' }, { texto: 'Faturamento', classe: 'num' },
                        { texto: 'Tarifas', classe: 'num' }, { texto: 'Custo', classe: 'num' }, { texto: 'Lucro est.', classe: 'num' }],
                    r.porAnuncio.map((a) => h('tr', {},
                        h('td', {}, a.titulo, h('div', { class: 'mono pequeno sub' }, a.meliItemId)),
                        h('td', { class: 'mono' }, a.sku || '—'),
                        h('td', { class: 'num' }, fmtNum(a.unidades)),
                        h('td', { class: 'num' }, fmtMoeda(a.faturamento)),
                        h('td', { class: 'num' }, fmtMoeda(a.tarifas)),
                        h('td', { class: 'num' }, a.custoTotal == null ? h('span', { class: 'sub', title: 'Sem produto vinculado ou custo não preenchido' }, '—') : fmtMoeda(a.custoTotal)),
                        h('td', { class: `num ${a.lucroEstimado < 0 ? 'negativo' : ''}` }, a.lucroEstimado == null ? '—' : fmtMoeda(a.lucroEstimado)))))
                    : vazio('Nenhuma venda no período.')));
            area.append(h('p', { class: 'sub pequeno' }, `Período: ${r.de.split('-').reverse().join('/')} a ${r.ate.split('-').reverse().join('/')} (horário de Brasília).`));
        });
    }
    desenharTela();
}

// ---------------------------------------------------------------------------
// ONDE ESTÁ (mapa do estoque por SKU)
// ---------------------------------------------------------------------------
export async function telaLocalizar(raiz, ctx) {
    const area = h('div');
    limpar(raiz, topo('Onde está', 'Procure pelo SKU (ou nome, ou código da estante) e veja em quais prateleiras está cada item.',
        ctx.admin ? botao('Entrada em lote', { class: 'btn btn-primario', onclick: () => abrirEntradaLote(() => telaLocalizar(raiz, ctx)) }) : null), area);

    await carregar(area, () => api.mapaEstoque(), (linhas) => {
        const busca = input({ type: 'search', placeholder: 'SKU, nome do produto ou código da estante…', autocomplete: 'off' });
        const corpo = h('tbody');
        const rodape = h('p', { class: 'sub pequeno' });
        const exportar = botao('Exportar CSV', {
            onclick: () => baixarArquivo(`onde-esta_${diaBrasilia(new Date())}.csv`, paraCsv([
                ['SKU', 'Produto', 'Depósito', 'Estante', 'Quantidade'],
                ...filtradas().map((l) => [l.sku, l.productName, l.warehouseName, l.locationCode, l.quantity])])),
        });
        const filtradas = () => {
            const termo = normalizar(busca.value);
            return linhas.filter((l) => !termo || normalizar(`${l.sku} ${l.productName} ${l.locationCode}`).includes(termo));
        };
        function desenhar() {
            const lista = filtradas();
            // agrupa por SKU para mostrar o total
            const totais = new Map();
            lista.forEach((l) => totais.set(l.sku, (totais.get(l.sku) || 0) + l.quantity));
            let anterior = null;
            limpar(corpo, lista.slice(0, 1000).map((l) => {
                const primeira = l.sku !== anterior;
                anterior = l.sku;
                return h('tr', { class: primeira ? 'grupo-inicio' : '' },
                    h('td', { class: 'mono' }, primeira ? h('strong', {}, l.sku) : ''),
                    h('td', {}, primeira ? l.productName : ''),
                    h('td', { class: 'mono' }, h('strong', {}, l.locationCode)),
                    h('td', {}, l.warehouseName),
                    h('td', { class: 'num' }, fmtNum(l.quantity)),
                    h('td', { class: 'num sub' }, primeira ? fmtNum(totais.get(l.sku)) : ''));
            }));
            if (!lista.length) corpo.append(h('tr', {}, h('td', { colspan: 6, class: 'vazio-celula' }, linhas.length ? 'Nada encontrado.' : 'Nenhum item guardado nas estantes ainda.')));
            rodape.textContent = `${fmtNum(totais.size)} SKU(s) em ${fmtNum(lista.length)} posição(ões)${lista.length > 1000 ? ' (mostrando 1.000; refine a busca)' : ''}`;
        }
        busca.addEventListener('input', desenhar);
        area.append(h('div', { class: 'filtros' }, busca, exportar),
            tabela(['SKU', 'Produto', 'Estante', 'Depósito', { texto: 'Qtd.', classe: 'num' }, { texto: 'Total do SKU', classe: 'num' }], corpo),
            rodape);
        desenhar();
        busca.focus();
    });
}

// ---------------------------------------------------------------------------
// ENTRADA EM LOTE (admin): colar do Excel
// ---------------------------------------------------------------------------
/** Lê linhas "SKU <tab ou ;> estante <tab ou ;> quantidade". Ignora cabeçalho e linhas vazias. */
export function lerLinhasLote(texto) {
    return String(texto || '').split(/\r?\n/).map((l) => l.trim()).filter(Boolean).map((l, i) => {
        const partes = l.split(/\t|;/).map((p) => p.trim());
        const qtd = Number(String(partes[2] || '').replace(/\./g, '').replace(',', '.'));
        return { original: l, linha: i + 1, sku: partes[0] || '', estante: partes[1] || '', quantidade: qtd };
    }).filter((l, i) => !(i === 0 && !Number.isFinite(l.quantidade))); // 1ª linha sem número = cabeçalho
}

export function abrirEntradaLote(aoTerminar) {
    const texto = h('textarea', { class: 'entrada', rows: 10, placeholder: 'SKU\tESTANTE\tQUANTIDADE\nCAM-01\tA-01-1\t12\nCAM-01\tB-10-2-1\t5\nBON-03\tA-01-2\t30' });
    const sincronizar = h('input', { type: 'checkbox' });
    const previa = h('div');
    const btn = h('button', { type: 'submit', class: 'btn btn-primario', disabled: true }, 'Registrar entradas');
    let itens = [];

    texto.addEventListener('input', () => {
        itens = lerLinhasLote(texto.value);
        const invalidas = itens.filter((i) => !i.sku || !i.estante || !Number.isInteger(i.quantidade) || i.quantidade < 1);
        btn.disabled = !itens.length || invalidas.length > 0;
        limpar(previa, itens.length
            ? h('p', { class: invalidas.length ? 'negativo pequeno' : 'sub pequeno' },
                invalidas.length
                    ? `${invalidas.length} linha(s) com problema (precisa de SKU, estante e quantidade inteira): ${invalidas.slice(0, 5).map((i) => `"${i.original}"`).join(', ')}`
                    : `${itens.length} linha(s) prontas · ${fmtNum(itens.reduce((s, i) => s + i.quantidade, 0))} unidades no total`)
            : null);
    });

    const form = h('form', { class: 'form-grade' },
        h('p', { class: 'sub pequeno' }, 'Copie do Excel três colunas, nesta ordem: SKU, código da estante (ou o conteúdo do QR) e quantidade, e cole abaixo. Cada linha vira uma ENTRADA, com as mesmas regras de capacidade.'),
        campo('Linhas', texto),
        previa,
        h('label', { class: 'check' }, sincronizar, 'Enviar o novo estoque ao Mercado Livre agora'),
        h('p', { class: 'sub pequeno' }, 'Na carga inicial, deixe desmarcado até lançar TODAS as estantes de cada SKU; depois use "Sincronizar" na tela Mercado Livre (ou a próxima movimentação envia).'),
        h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => d.close() }), btn));
    const d = abrirModal('Entrada em lote', form, { larga: true });
    texto.focus();

    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        const r = await enviar(form, () => api.entradaLote(itens.map(({ sku, estante, quantidade }) => ({ sku, estante, quantidade })), sincronizar.checked));
        if (!r) return;
        const ok = r.filter((x) => x.ok).length;
        const falhas = r.filter((x) => !x.ok);
        limpar(form,
            h('p', {}, h('strong', {}, `${ok} de ${r.length} entradas registradas.`)),
            falhas.length
                ? h('div', {}, h('p', { class: 'negativo' }, `${falhas.length} linha(s) não entraram:`),
                    tabela(['Linha', 'Conteúdo', 'Motivo'], falhas.map((f) => h('tr', {},
                        h('td', { class: 'num' }, f.linha), h('td', { class: 'mono pequeno' }, itens[f.linha - 1]?.original || ''), h('td', { class: 'pequeno' }, f.mensagem)))),
                    h('p', { class: 'sub pequeno' }, 'Corrija só essas linhas e cole de novo (as que entraram já estão gravadas).'))
                : null,
            h('div', { class: 'acoes' }, botao('Fechar', { class: 'btn btn-primario', onclick: () => d.close() })));
        if (ok) aviso(`${ok} entrada(s) registrada(s).`);
        if (aoTerminar) aoTerminar();
    });
}
