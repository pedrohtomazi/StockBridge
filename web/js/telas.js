/* Telas do StockBridge. Cada tela recebe (raiz, ctx) e desenha seu conteúdo em `raiz`.
 * ctx = { usuario: { email, role }, admin: boolean, ir(rota) } */
import { api, ApiErro, sessao } from './api.js';
import {
    h, limpar, aviso, abrirModal, confirmar, barraOcupacao, vazio, carregando, campo,
    fmtMoeda, fmtNum, fmtPct, fmtDataHora, diaBrasilia, normalizar, baixarArquivo, paraCsv,
} from './util.js';

// ---------------------------------------------------------------------------
// Auxiliares comuns
// ---------------------------------------------------------------------------
export function topo(titulo, subtitulo, ...acoes) {
    return h('div', { class: 'tela-topo' },
        h('div', {}, h('h1', {}, titulo), subtitulo ? h('p', { class: 'sub' }, subtitulo) : null),
        acoes.filter(Boolean).length ? h('div', { class: 'tela-acoes' }, ...acoes) : null);
}

export function caixaErro(erro, tentarDeNovo) {
    return h('div', { class: 'erro-caixa', role: 'alert' },
        h('strong', {}, 'Não foi possível carregar.'),
        h('p', {}, erro?.message || String(erro)),
        tentarDeNovo ? h('button', { class: 'btn', type: 'button', onclick: tentarDeNovo }, 'Tentar de novo') : null);
}

/** Carrega dados mostrando "carregando" e, em caso de erro, uma caixa com "tentar de novo". */
export async function carregar(area, buscar, desenhar) {
    limpar(area, carregando());
    try {
        const dados = await buscar();
        limpar(area);
        desenhar(dados);
    } catch (e) {
        if (e instanceof ApiErro && e.status === 401) return; // o app já volta para o login
        limpar(area, caixaErro(e, () => carregar(area, buscar, desenhar)));
    }
}

/** Envia um formulário com o botão travado; mostra o erro da API se falhar. */
export async function enviar(form, acao) {
    const botao = form.querySelector('button[type=submit]');
    if (botao) botao.disabled = true;
    try {
        const r = await acao();
        return r === null ? true : r;
    } catch (e) {
        if (!(e instanceof ApiErro && e.status === 401)) aviso(e.message, 'erro');
        return undefined;
    } finally {
        if (botao) botao.disabled = false;
    }
}

/** Executa uma ação de botão (excluir, ativar...) mostrando erro se falhar. */
export async function acao(fn, mensagemOk) {
    try {
        await fn();
        if (mensagemOk) aviso(mensagemOk);
        return true;
    } catch (e) {
        if (!(e instanceof ApiErro && e.status === 401)) aviso(e.message, 'erro');
        return false;
    }
}

export const input = (attrs) => h('input', { class: 'entrada', ...attrs });
export const select = (attrs, ...opcoes) => h('select', { class: 'entrada', ...attrs }, ...opcoes);
export const opcao = (valor, texto, extra = {}) => h('option', { value: valor, ...extra }, texto);
export const botao = (texto, attrs = {}) => h('button', { type: 'button', class: 'btn', ...attrs }, texto);
export const tabela = (cabecalho, linhas, classe = '') =>
    h('div', { class: 'tabela-rolagem' },
        h('table', { class: `tabela ${classe}` },
            h('thead', {}, h('tr', {}, cabecalho.map((c) =>
                typeof c === 'string' ? h('th', {}, c) : h('th', { class: c.classe }, c.texto)))),
            linhas instanceof HTMLTableSectionElement ? linhas : h('tbody', {}, linhas)));

const tipoBadge = (tipo) => h('span', { class: `badge ${tipo === 'IN' ? 'badge-entrada' : 'badge-saida'}` },
    tipo === 'IN' ? 'Entrada' : 'Saída');

export function codigoLocal(l) {
    return [l.sector, l.rack, l.level, l.position].filter((p) => p && String(p).trim()).join(' · ');
}

export function kpi(rotulo, valor, detalhe) {
    return h('div', { class: 'kpi' }, h('span', { class: 'kpi-rotulo' }, rotulo),
        h('strong', { class: 'kpi-valor' }, valor), detalhe ? h('small', {}, detalhe) : null);
}

// ---------------------------------------------------------------------------
// PAINEL
// ---------------------------------------------------------------------------
export async function telaPainel(raiz, ctx) {
    const area = h('div');
    limpar(raiz, topo('Painel', `Olá, ${ctx.usuario.email}. Este é o resumo do depósito agora.`,
        botao('Registrar movimentação', { class: 'btn btn-primario', onclick: () => ctx.ir('movimentar') })), area);

    await carregar(area,
        () => Promise.all([
            api.produtos(), api.localizacoes(), api.movimentacoes(),
            ctx.admin ? api.meliStatus().catch(() => null) : Promise.resolve(null),
            ctx.admin ? api.meliVinculos().catch(() => []) : Promise.resolve([]),
        ]),
        ([produtos, locais, movs, meli, vinculos]) => {
            const unidades = produtos.reduce((s, p) => s + p.totalQuantity, 0);
            const valorCusto = produtos.reduce((s, p) => s + p.totalQuantity * p.cost, 0);
            const valorVenda = produtos.reduce((s, p) => s + p.totalQuantity * p.price, 0);
            const capacidade = locais.reduce((s, l) => s + l.maxCapacity, 0);
            const ocupado = locais.reduce((s, l) => s + l.currentQuantity, 0);
            const hoje = diaBrasilia(new Date());
            const movsHoje = movs.filter((m) => m.createdAt && diaBrasilia(m.createdAt) === hoje);
            const entradasHoje = movsHoje.filter((m) => m.type === 'IN').reduce((s, m) => s + m.quantity, 0);
            const saidasHoje = movsHoje.filter((m) => m.type === 'OUT').reduce((s, m) => s + m.quantity, 0);

            const cards = h('div', { class: 'kpis' },
                kpi('Produtos', fmtNum(produtos.length), `${fmtNum(produtos.filter((p) => p.totalQuantity === 0).length)} sem estoque`),
                kpi('Unidades em estoque', fmtNum(unidades), `em ${fmtNum(locais.filter((l) => l.currentQuantity > 0).length)} estantes`),
                kpi('Valor em estoque (custo)', fmtMoeda(valorCusto), `${fmtMoeda(valorVenda)} a preço de venda`),
                kpi('Ocupação geral', capacidade ? fmtPct(ocupado / capacidade) : '—', `${fmtNum(ocupado)} de ${fmtNum(capacidade)} vagas`),
                kpi('Movimentações hoje', fmtNum(movsHoje.length), `+${fmtNum(entradasHoje)} entradas · −${fmtNum(saidasHoje)} saídas`));

            const cheias = [...locais].filter((l) => l.maxCapacity > 0)
                .sort((a, b) => b.currentQuantity / b.maxCapacity - a.currentQuantity / a.maxCapacity).slice(0, 6);
            const blocoEstantes = h('section', { class: 'cartao' },
                h('div', { class: 'cartao-topo' }, h('h2', {}, 'Estantes mais cheias'),
                    h('a', { href: '#/estantes', class: 'link' }, 'Ver todas')),
                cheias.length
                    ? h('ul', { class: 'lista-ocupacao' }, cheias.map((l) =>
                        h('li', {}, h('span', { class: 'mono' }, l.fullCode), barraOcupacao(l.currentQuantity, l.maxCapacity))))
                    : vazio('Nenhuma estante cadastrada.'));

            const ultimas = movs.slice(0, 8);
            const blocoMovs = h('section', { class: 'cartao' },
                h('div', { class: 'cartao-topo' }, h('h2', {}, 'Últimas movimentações'),
                    h('a', { href: '#/historico', class: 'link' }, 'Histórico completo')),
                ultimas.length
                    ? h('ul', { class: 'lista-movs' }, ultimas.map((m) => h('li', {},
                        tipoBadge(m.type),
                        h('span', { class: 'mov-produto' }, m.productName, h('small', { class: 'mono' }, `${m.sku} · ${m.locationCode}`)),
                        h('span', { class: 'mov-qtd' }, `${m.type === 'IN' ? '+' : '−'}${fmtNum(m.quantity)}`),
                        h('small', { class: 'mov-data' }, fmtDataHora(m.createdAt)))))
                    : vazio('Nenhuma movimentação registrada ainda.'));

            const semEstoque = produtos.filter((p) => p.totalQuantity === 0).slice(0, 8);
            const blocoAlerta = h('section', { class: 'cartao' },
                h('div', { class: 'cartao-topo' }, h('h2', {}, 'Produtos sem estoque')),
                semEstoque.length
                    ? h('ul', { class: 'lista-simples' }, semEstoque.map((p) =>
                        h('li', {}, p.name, h('small', { class: 'mono' }, p.sku))))
                    : vazio(produtos.length ? 'Todos os produtos têm saldo.' : 'Nenhum produto cadastrado.'));

            const blocos = [blocoEstantes, blocoMovs, blocoAlerta];
            if (ctx.admin) {
                const falhas = vinculos.filter((v) => v.lastSyncOk === false).length;
                let estado;
                if (!meli) estado = h('p', { class: 'sub' }, 'Não foi possível consultar o status.');
                else if (!meli.configured) estado = h('p', {}, h('span', { class: 'ponto ponto-alerta' }), 'App do Mercado Livre não configurado.');
                else if (!meli.connected) estado = h('p', {}, h('span', { class: 'ponto ponto-alerta' }), 'Conta não conectada. As movimentações não chegam ao Mercado Livre.');
                else if (meli.needsReconnect) estado = h('p', {}, h('span', { class: 'ponto ponto-erro' }), 'A conexão expirou: reconecte a conta.');
                else if (!meli.autoRenew) estado = h('p', {}, h('span', { class: 'ponto ponto-alerta' }), `Conectada só até ${fmtDataHora(meli.expiresAt)}: falta a permissão offline_access no app.`);
                else estado = h('p', {}, h('span', { class: `ponto ${falhas ? 'ponto-alerta' : 'ponto-ok'}` }),
                    falhas ? `Conta conectada, mas ${falhas} anúncio(s) com falha na última sincronização.` : 'Conta conectada. Estoque sincronizado a cada movimentação.');
                blocos.push(h('section', { class: 'cartao' },
                    h('div', { class: 'cartao-topo' }, h('h2', {}, 'Mercado Livre'),
                        h('a', { href: '#/mercadolivre', class: 'link' }, 'Gerenciar')),
                    estado));
            }
            area.append(cards, h('div', { class: 'grade-2' }, ...blocos));
        });
}

// ---------------------------------------------------------------------------
// MOVIMENTAR (entrada / saída)
// ---------------------------------------------------------------------------
export async function telaMovimentar(raiz, ctx) {
    const area = h('div');
    const lote = ctx.admin ? botao('Entrada em lote (colar do Excel)', {
        onclick: async () => (await import('./telas-ml.js')).abrirEntradaLote(() => telaMovimentar(raiz, ctx)),
    }) : null;
    limpar(raiz, topo('Movimentar estoque', 'Registre entradas e saídas em uma estante. Bipe o QR Code da estante ou escolha na lista.', lote), area);

    await carregar(area, () => Promise.all([api.produtos(), api.localizacoes()]), ([produtos, locais]) => {
        let tipo = 'IN';
        let local = null;
        let estoqueLocal = [];
        const registradas = [];

        const textoLocal = (l) => `${l.fullCode} — ${fmtNum(l.currentQuantity)}/${fmtNum(l.maxCapacity)}${l.isActive ? '' : ' (inativa)'}`;
        const btnEntrada = botao('Entrada', { class: 'seg ativo', 'aria-pressed': 'true' });
        const btnSaida = botao('Saída', { class: 'seg', 'aria-pressed': 'false' });
        const qr = input({ placeholder: 'Bipe ou digite o QR (ex.: QR-B-10-2-1) e tecle Enter', autocomplete: 'off' });
        const selLocal = select({ required: true },
            opcao('', locais.length ? 'Escolha a estante…' : 'Nenhuma estante cadastrada'),
            locais.map((l) => opcao(l.id, textoLocal(l))));
        const selProduto = select({ required: true });
        const qtd = input({ type: 'number', min: 1, max: 1000000, step: 1, required: true, inputmode: 'numeric', placeholder: '0' });
        const infoLocal = h('div', { class: 'info-local' });
        const dicaQtd = h('small', { class: 'ajuda' });
        const submit = h('button', { type: 'submit', class: 'btn btn-primario btn-grande' }, 'Registrar entrada');
        const listaFeitas = h('div');

        function atualizarOpcao(l) {
            const op = [...selLocal.options].find((o) => o.value === l.id);
            if (op) op.textContent = textoLocal(l);
            else selLocal.append(opcao(l.id, textoLocal(l)));
        }

        function desenharProdutos() {
            const anterior = selProduto.value;
            if (tipo === 'OUT') {
                limpar(selProduto,
                    opcao('', !local ? 'Escolha a estante primeiro' : estoqueLocal.length ? 'Escolha o produto…' : 'Estante vazia'),
                    estoqueLocal.map((e) => opcao(e.product_id, `${e.product_name} (${e.sku}) — saldo ${fmtNum(e.quantity)}`)));
            } else {
                limpar(selProduto, opcao('', produtos.length ? 'Escolha o produto…' : 'Nenhum produto cadastrado'),
                    produtos.map((p) => opcao(p.id, `${p.name} (${p.sku})`)));
            }
            if ([...selProduto.options].some((o) => o.value === anterior)) selProduto.value = anterior;
            atualizarDica();
        }

        function atualizarDica() {
            if (!local) { dicaQtd.textContent = ''; return; }
            if (tipo === 'IN') {
                const livre = Math.max(local.maxCapacity - local.currentQuantity, 0);
                dicaQtd.textContent = local.isActive ? `Espaço livre na estante: ${fmtNum(livre)}` : 'Estante inativa: não aceita entradas.';
                qtd.max = Math.max(livre, 1);
            } else {
                const item = estoqueLocal.find((e) => e.product_id === selProduto.value);
                dicaQtd.textContent = item ? `Saldo deste produto na estante: ${fmtNum(item.quantity)}` : '';
                qtd.max = item ? Math.max(item.quantity, 1) : 1000000;
            }
        }

        function desenharInfoLocal() {
            if (!local) { limpar(infoLocal); return; }
            limpar(infoLocal,
                h('div', { class: 'info-local-topo' },
                    h('strong', { class: 'mono' }, local.fullCode),
                    h('span', { class: 'sub' }, `${local.warehouseName} · ${codigoLocal(local)}`),
                    local.isActive ? null : h('span', { class: 'badge badge-saida' }, 'Inativa')),
                barraOcupacao(local.currentQuantity, local.maxCapacity),
                estoqueLocal.length
                    ? h('ul', { class: 'lista-simples compacta' }, estoqueLocal.map((e) =>
                        h('li', {}, e.product_name, h('small', { class: 'mono' }, `${e.sku} · ${fmtNum(e.quantity)} un.`))))
                    : h('p', { class: 'sub' }, 'Estante vazia.'));
        }

        async function escolherLocal(id) {
            local = locais.find((l) => l.id === id) || null;
            selLocal.value = local ? local.id : '';
            estoqueLocal = [];
            desenharInfoLocal();
            desenharProdutos();
            if (!local) return;
            limpar(infoLocal, carregando('Consultando a estante…'));
            try {
                estoqueLocal = await api.estoqueDaLocalizacao(local.id);
            } catch (e) {
                aviso(e.message, 'erro');
            }
            desenharInfoLocal();
            desenharProdutos();
        }

        async function recarregarLocal() {
            try {
                const [atual] = await api.localizacoes(local.qrToken);
                if (atual) {
                    const i = locais.findIndex((l) => l.id === atual.id);
                    if (i >= 0) locais[i] = atual; else locais.push(atual);
                    atualizarOpcao(atual);
                }
            } catch { /* mantém o que já tem */ }
        }

        function mudarTipo(novo) {
            tipo = novo;
            btnEntrada.classList.toggle('ativo', tipo === 'IN');
            btnSaida.classList.toggle('ativo', tipo === 'OUT');
            btnEntrada.setAttribute('aria-pressed', String(tipo === 'IN'));
            btnSaida.setAttribute('aria-pressed', String(tipo === 'OUT'));
            submit.textContent = tipo === 'IN' ? 'Registrar entrada' : 'Registrar saída';
            submit.classList.toggle('btn-saida', tipo === 'OUT');
            desenharProdutos();
        }

        btnEntrada.addEventListener('click', () => mudarTipo('IN'));
        btnSaida.addEventListener('click', () => mudarTipo('OUT'));
        selLocal.addEventListener('change', () => escolherLocal(selLocal.value));
        selProduto.addEventListener('change', atualizarDica);
        qr.addEventListener('keydown', async (e) => {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            const valor = qr.value.trim();
            if (!valor) return;
            try {
                const [achado] = await api.localizacoes(valor);
                if (!achado) { aviso(`Nenhuma estante com o QR "${valor}".`, 'erro'); return; }
                const i = locais.findIndex((l) => l.id === achado.id);
                if (i >= 0) locais[i] = achado; else locais.push(achado);
                atualizarOpcao(achado);
                await escolherLocal(achado.id);
                qr.value = '';
                selProduto.focus();
            } catch (err) {
                aviso(err.message, 'erro');
            }
        });

        const form = h('form', { class: 'cartao form-mov', novalidate: true },
            h('div', { class: 'segmentado', role: 'group', 'aria-label': 'Tipo de movimentação' }, btnEntrada, btnSaida),
            campo('QR Code da estante', qr),
            campo('Estante', selLocal),
            infoLocal,
            campo('Produto', selProduto),
            campo('Quantidade', qtd, dicaQtd),
            submit,
            h('p', { class: 'sub pequeno' }, 'Se o produto tiver anúncio vinculado, o Mercado Livre recebe o estoque total em segundo plano.'));

        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const quantidade = Number(qtd.value);
            if (!local) return aviso('Escolha a estante.', 'erro');
            if (!selProduto.value) return aviso('Escolha o produto.', 'erro');
            if (!Number.isInteger(quantidade) || quantidade < 1 || quantidade > 1000000) {
                return aviso('A quantidade deve ser um número inteiro entre 1 e 1.000.000.', 'erro');
            }
            const nomeProduto = selProduto.selectedOptions[0]?.textContent.split(' — ')[0] || '';
            const r = await enviar(form, () => api.criarMovimentacao({
                productId: selProduto.value, locationId: local.id, type: tipo, quantity: quantidade,
            }));
            if (!r) return;
            aviso(`${tipo === 'IN' ? 'Entrada' : 'Saída'} registrada. Saldo na estante: ${fmtNum(r.new_balance)}.`);
            registradas.unshift({ tipo, nomeProduto, quantidade, estante: local.fullCode, saldo: r.new_balance });
            desenharFeitas();
            qtd.value = '';
            await recarregarLocal();
            await escolherLocal(local.id);
        });

        function desenharFeitas() {
            limpar(listaFeitas, registradas.length
                ? h('section', { class: 'cartao' }, h('h2', {}, 'Registradas agora'),
                    h('ul', { class: 'lista-movs' }, registradas.slice(0, 10).map((m) => h('li', {},
                        tipoBadge(m.tipo),
                        h('span', { class: 'mov-produto' }, m.nomeProduto, h('small', { class: 'mono' }, m.estante)),
                        h('span', { class: 'mov-qtd' }, `${m.tipo === 'IN' ? '+' : '−'}${fmtNum(m.quantidade)}`),
                        h('small', { class: 'mov-data' }, `saldo ${fmtNum(m.saldo)}`)))))
                : null);
        }

        desenharProdutos();
        area.append(h('div', { class: 'grade-mov' }, form, listaFeitas));
        qr.focus();
    });
}

// ---------------------------------------------------------------------------
// PRODUTOS
// ---------------------------------------------------------------------------
export async function telaProdutos(raiz, ctx) {
    const area = h('div');
    const recarregar = () => telaProdutos(raiz, ctx);
    limpar(raiz, topo('Produtos', 'Catálogo com o estoque total de cada produto (soma de todas as estantes).',
        ctx.admin ? botao('Novo produto', { class: 'btn btn-primario', onclick: () => formProduto(recarregar) }) : null), area);

    await carregar(area, () => api.produtos(), (produtos) => {
        if (!produtos.length) {
            area.append(vazio('Nenhum produto cadastrado.', ctx.admin ? botao('Cadastrar o primeiro', { class: 'btn btn-primario', onclick: () => formProduto(recarregar) }) : null));
            return;
        }
        const busca = input({ type: 'search', placeholder: 'Buscar por nome ou SKU…' });
        const soComEstoque = h('input', { type: 'checkbox' });
        const corpo = h('tbody');
        const rodape = h('p', { class: 'sub pequeno' });

        function desenhar() {
            const termo = normalizar(busca.value);
            const lista = produtos.filter((p) => (!termo || normalizar(`${p.name} ${p.sku}`).includes(termo))
                && (!soComEstoque.checked || p.totalQuantity > 0));
            limpar(corpo, lista.map((p) => {
                const margem = p.price > 0 ? (p.price - p.cost) / p.price : NaN;
                return h('tr', {},
                    h('td', {}, p.name),
                    h('td', { class: 'mono' }, p.sku),
                    h('td', { class: 'num' }, fmtMoeda(p.cost)),
                    h('td', { class: 'num' }, fmtMoeda(p.price)),
                    h('td', { class: `num ${margem < 0 ? 'negativo' : ''}` }, fmtPct(margem)),
                    h('td', { class: 'num' }, h('strong', {}, fmtNum(p.totalQuantity))),
                    h('td', { class: 'num' }, fmtMoeda(p.totalQuantity * p.cost)),
                    h('td', { class: 'acoes-linha' },
                        p.totalQuantity > 0 ? botao('Onde está?', { class: 'btn btn-pequeno', onclick: () => ondeEsta(p) }) : null,
                        ctx.admin ? botao('Editar', { class: 'btn btn-pequeno', onclick: () => formProduto(recarregar, p) }) : null,
                        ctx.admin ? botao('Excluir', {
                            class: 'btn btn-pequeno btn-perigo-texto',
                            onclick: async () => {
                                if (!await confirmar(`Excluir o produto "${p.name}"? Só é possível se ele nunca foi movimentado.`, { textoOk: 'Excluir', perigo: true })) return;
                                if (await acao(() => api.excluirProduto(p.id), 'Produto excluído.')) recarregar();
                            },
                        }) : null));
            }));
            if (!lista.length) corpo.append(h('tr', {}, h('td', { colspan: 8, class: 'vazio-celula' }, 'Nada encontrado.')));
            rodape.textContent = `${fmtNum(lista.length)} de ${fmtNum(produtos.length)} produtos`;
        }
        busca.addEventListener('input', desenhar);
        soComEstoque.addEventListener('change', desenhar);

        area.append(h('div', { class: 'filtros' }, busca, h('label', { class: 'check' }, soComEstoque, 'Só com estoque')),
            tabela(['Produto', 'SKU', { texto: 'Custo', classe: 'num' }, { texto: 'Preço', classe: 'num' },
                { texto: 'Margem', classe: 'num' }, { texto: 'Estoque', classe: 'num' }, { texto: 'Valor (custo)', classe: 'num' }, ''], corpo),
            rodape);
        desenhar();
    });
}

/** Estantes onde o produto tem saldo. */
async function ondeEsta(produto) {
    const corpo = h('div', {}, carregando('Procurando nas estantes…'));
    abrirModal(`Onde está: ${produto.name}`, corpo);
    try {
        const achados = (await api.mapaEstoque(produto.sku)).filter((l) => l.productId === produto.id)
            .sort((a, b) => b.quantity - a.quantity);
        limpar(corpo, achados.length
            ? tabela(['Estante', 'Depósito', { texto: 'Quantidade', classe: 'num' }],
                achados.map((a) => h('tr', {}, h('td', { class: 'mono' }, a.locationCode), h('td', {}, a.warehouseName),
                    h('td', { class: 'num' }, fmtNum(a.quantity)))))
            : vazio('Nenhuma estante com saldo deste produto.'));
    } catch (e) {
        limpar(corpo, caixaErro(e));
    }
}

function formProduto(aoSalvar, produto) {
    const nome = input({ required: true, maxlength: 200, placeholder: 'Ex.: Monitor Gamer 24"', value: produto?.name });
    const sku = input({ required: true, maxlength: 64, placeholder: 'Ex.: MON-GAM-24', pattern: '[A-Za-z0-9._\\-]+', autocapitalize: 'characters', value: produto?.sku });
    const custo = input({ type: 'number', min: 0, max: 1000000000, step: '0.01', required: true, inputmode: 'decimal', placeholder: '0,00', value: produto?.cost });
    const preco = input({ type: 'number', min: 0, max: 1000000000, step: '0.01', required: true, inputmode: 'decimal', placeholder: '0,00', value: produto?.price });
    const margem = h('small', { class: 'ajuda' });
    const calcMargem = () => {
        const c = Number(custo.value), p = Number(preco.value);
        margem.textContent = p > 0 && custo.value !== '' ? `Margem bruta: ${fmtPct((p - c) / p)} (${fmtMoeda(p - c)} por unidade)` : '';
    };
    custo.addEventListener('input', calcMargem);
    preco.addEventListener('input', calcMargem);
    calcMargem();

    const form = h('form', { class: 'form-grade' },
        campo('Nome', nome),
        campo('SKU', sku, 'Letras, números, "-", "_" e "." (único).'),
        h('div', { class: 'linha-2' }, campo('Custo (R$)', custo), campo('Preço de venda (R$)', preco)),
        margem,
        produto ? h('p', { class: 'sub pequeno' }, 'Mudar custo ou preço não altera o histórico: cada movimentação guarda os valores da época.') : null,
        h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => d.close() }),
            h('button', { type: 'submit', class: 'btn btn-primario' }, produto ? 'Salvar' : 'Cadastrar produto')));
    const d = abrirModal(produto ? 'Editar produto' : 'Novo produto', form);
    nome.focus();

    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        if (!form.reportValidity()) return;
        const dados = { name: nome.value.trim(), sku: sku.value.trim(), cost: Number(custo.value), price: Number(preco.value) };
        const r = await enviar(form, () => produto ? api.alterarProduto(produto.id, dados) : api.criarProduto(dados));
        if (!r) return;
        aviso(produto ? 'Produto atualizado.' : 'Produto cadastrado.');
        d.close();
        aoSalvar();
    });
}

// ---------------------------------------------------------------------------
// DEPÓSITOS
// ---------------------------------------------------------------------------
export async function telaDepositos(raiz, ctx) {
    const area = h('div');
    const recarregar = () => telaDepositos(raiz, ctx);
    limpar(raiz, topo('Depósitos', 'Galpões onde ficam as estantes.'), area);

    await carregar(area, () => Promise.all([api.depositos(), api.localizacoes()]), ([depositos, locais]) => {
        if (ctx.admin) {
            const nome = input({ required: true, maxlength: 120, placeholder: 'Nome do depósito (ex.: Galpão Central)' });
            const form = h('form', { class: 'form-inline cartao' }, nome,
                h('button', { type: 'submit', class: 'btn btn-primario' }, 'Adicionar depósito'));
            form.addEventListener('submit', async (e) => {
                e.preventDefault();
                if (!nome.value.trim()) return aviso('Informe o nome do depósito.', 'erro');
                const r = await enviar(form, () => api.criarDeposito({ name: nome.value.trim() }));
                if (r) { aviso('Depósito cadastrado.'); recarregar(); }
            });
            area.append(form);
        }
        if (!depositos.length) { area.append(vazio('Nenhum depósito cadastrado.')); return; }

        area.append(h('div', { class: 'grade-cards' }, depositos.map((d) => {
            const dele = locais.filter((l) => l.warehouseId === d.id);
            const cap = dele.reduce((s, l) => s + l.maxCapacity, 0);
            const ocup = dele.reduce((s, l) => s + l.currentQuantity, 0);
            return h('section', { class: 'cartao' },
                h('h2', {}, d.name),
                h('p', { class: 'sub' }, `${fmtNum(dele.length)} estantes`),
                cap ? barraOcupacao(ocup, cap) : h('p', { class: 'sub' }, 'Sem estantes ainda.'),
                h('div', { class: 'acoes-linha' },
                    h('a', { class: 'link', href: `#/estantes?deposito=${encodeURIComponent(d.id)}` }, 'Ver estantes'),
                    ctx.admin ? botao('Renomear', {
                        class: 'btn btn-pequeno',
                        onclick: () => {
                            const novo = input({ required: true, maxlength: 120, value: d.name });
                            const f = h('form', { class: 'form-grade' }, campo('Nome', novo),
                                h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => m.close() }),
                                    h('button', { type: 'submit', class: 'btn btn-primario' }, 'Salvar')));
                            const m = abrirModal('Renomear depósito', f);
                            f.addEventListener('submit', async (e) => {
                                e.preventDefault();
                                if (await enviar(f, () => api.alterarDeposito(d.id, { name: novo.value.trim() }))) { m.close(); aviso('Depósito renomeado.'); recarregar(); }
                            });
                        },
                    }) : null,
                    ctx.admin ? botao('Excluir', {
                        class: 'btn btn-pequeno btn-perigo-texto',
                        onclick: async () => {
                            if (!await confirmar(`Excluir o depósito "${d.name}"? Só é possível se ele não tiver estantes.`, { textoOk: 'Excluir', perigo: true })) return;
                            if (await acao(() => api.excluirDeposito(d.id), 'Depósito excluído.')) recarregar();
                        },
                    }) : null));
        })));
    });
}

// ---------------------------------------------------------------------------
// ESTANTES (localizações)
// ---------------------------------------------------------------------------
export async function telaEstantes(raiz, ctx, params) {
    const area = h('div');
    let depositosCache = [];
    const recarregar = () => telaEstantes(raiz, ctx, params);
    limpar(raiz, topo('Estantes', 'Localizações físicas (setor, rua, nível, posição) com capacidade e ocupação.',
        ctx.admin ? botao('Nova estante', { class: 'btn btn-primario', onclick: () => formEstante(depositosCache, recarregar) }) : null), area);

    await carregar(area, () => Promise.all([api.localizacoes(), api.depositos()]), ([locais, depositos]) => {
        depositosCache = depositos;
        const filtroDep = select({}, opcao('', 'Todos os depósitos'), depositos.map((d) => opcao(d.id, d.name)));
        if (params.get('deposito')) filtroDep.value = params.get('deposito');
        const busca = input({ type: 'search', placeholder: 'Buscar código, setor ou QR…' });
        const filtroOcup = select({}, opcao('', 'Qualquer situação'), opcao('vazias', 'Vazias'),
            opcao('livres', 'Com espaço'), opcao('cheias', '90% ou mais'), opcao('inativas', 'Inativas'));
        const corpo = h('tbody');
        const rodape = h('p', { class: 'sub pequeno' });

        function desenhar() {
            const termo = normalizar(busca.value);
            const lista = locais.filter((l) => {
                if (filtroDep.value && l.warehouseId !== filtroDep.value) return false;
                if (termo && !normalizar(`${l.fullCode} ${l.qrToken} ${codigoLocal(l)}`).includes(termo)) return false;
                const f = l.maxCapacity ? l.currentQuantity / l.maxCapacity : 0;
                if (filtroOcup.value === 'vazias' && l.currentQuantity > 0) return false;
                if (filtroOcup.value === 'livres' && (l.currentQuantity >= l.maxCapacity || !l.isActive)) return false;
                if (filtroOcup.value === 'cheias' && f < 0.9) return false;
                if (filtroOcup.value === 'inativas' && l.isActive) return false;
                return true;
            });
            const abrir = (l) => detalheEstante(l, ctx, depositosCache, recarregar);
            limpar(corpo, lista.map((l) => h('tr', { class: 'clicavel', tabindex: 0, onclick: () => abrir(l), onkeydown: (e) => { if (e.key === 'Enter') abrir(l); } },
                h('td', { class: 'mono' }, h('strong', {}, l.fullCode)),
                h('td', {}, l.warehouseName),
                h('td', {}, codigoLocal(l)),
                h('td', { class: 'mono pequeno' }, l.qrToken),
                h('td', {}, barraOcupacao(l.currentQuantity, l.maxCapacity)),
                h('td', {}, l.isActive ? h('span', { class: 'badge badge-entrada' }, 'Ativa') : h('span', { class: 'badge badge-saida' }, 'Inativa')))));
            if (!lista.length) corpo.append(h('tr', {}, h('td', { colspan: 6, class: 'vazio-celula' }, locais.length ? 'Nada encontrado.' : 'Nenhuma estante cadastrada.')));
            rodape.textContent = `${fmtNum(lista.length)} de ${fmtNum(locais.length)} estantes · clique em uma linha para ver o conteúdo${ctx.admin ? ' e editar' : ''}`;
        }
        [filtroDep, filtroOcup].forEach((el) => el.addEventListener('change', desenhar));
        busca.addEventListener('input', desenhar);

        area.append(h('div', { class: 'filtros' }, busca, filtroDep, filtroOcup),
            tabela(['Código', 'Depósito', 'Setor · Rua · Nível · Posição', 'QR', 'Ocupação', 'Situação'], corpo),
            rodape);
        desenhar();
    });
}

async function detalheEstante(l, ctx, depositos, aoMudar) {
    const corpo = h('div', {}, carregando());
    const d = abrirModal(`Estante ${l.fullCode}`, corpo, { larga: true });
    try {
        const itens = await api.estoqueDaLocalizacao(l.id);
        const acoesAdmin = ctx.admin ? h('div', { class: 'acoes' },
            botao(l.isActive ? 'Desativar' : 'Ativar', {
                onclick: async () => {
                    const dados = { ...l, isActive: !l.isActive };
                    if (await acao(() => api.alterarLocalizacao(l.id, dados), l.isActive ? 'Estante desativada: não recebe mais entradas.' : 'Estante ativada.')) { d.close(); aoMudar(); }
                },
            }),
            botao('Editar', { onclick: () => { d.close(); formEstante(depositos, aoMudar, l); } }),
            botao('Excluir', {
                class: 'btn btn-perigo-texto',
                onclick: async () => {
                    if (!await confirmar(`Excluir a estante ${l.fullCode}? Só é possível se ela nunca teve movimentação; senão, desative-a.`, { textoOk: 'Excluir', perigo: true })) return;
                    if (await acao(() => api.excluirLocalizacao(l.id), 'Estante excluída.')) { d.close(); aoMudar(); }
                },
            })) : null;
        limpar(corpo,
            h('dl', { class: 'detalhes' },
                h('dt', {}, 'Depósito'), h('dd', {}, l.warehouseName),
                h('dt', {}, 'Posição'), h('dd', {}, codigoLocal(l)),
                h('dt', {}, 'QR Code'), h('dd', { class: 'mono' }, l.qrToken),
                h('dt', {}, 'Situação'), h('dd', {}, l.isActive ? 'Ativa' : 'Inativa (não recebe entradas)'),
                h('dt', {}, 'Ocupação'), h('dd', {}, barraOcupacao(l.currentQuantity, l.maxCapacity))),
            itens.length
                ? tabela(['Produto', 'SKU', { texto: 'Quantidade', classe: 'num' }],
                    itens.map((i) => h('tr', {},
                        h('td', {}, i.product_name), h('td', { class: 'mono' }, i.sku), h('td', { class: 'num' }, fmtNum(i.quantity)))))
                : vazio('Estante vazia.'),
            acoesAdmin);
    } catch (e) {
        limpar(corpo, caixaErro(e));
    }
}

function formEstante(depositos, aoSalvar, estante) {
    if (!depositos.length) {
        aviso('Cadastre um depósito antes de criar estantes.', 'erro');
        return;
    }
    const dep = select({ required: true }, depositos.map((d) => opcao(d.id, d.name)));
    if (estante) dep.value = estante.warehouseId;
    const setor = input({ required: true, maxlength: 20, placeholder: 'B', value: estante?.sector });
    const rua = input({ required: true, maxlength: 20, placeholder: '10', value: estante?.rack });
    const nivel = input({ required: true, maxlength: 20, placeholder: '2', value: estante?.level });
    const posicao = input({ maxlength: 20, placeholder: '1 (opcional)', value: estante?.position || '' });
    const codigo = input({ required: true, maxlength: 60, value: estante?.fullCode });
    const qrTok = input({ required: true, maxlength: 128, value: estante?.qrToken });
    const cap = input({ type: 'number', min: Math.max(1, estante?.currentQuantity || 1), max: 1000000, step: 1, required: true, inputmode: 'numeric', placeholder: '100', value: estante?.maxCapacity });
    let codigoEditado = !!estante, qrEditado = !!estante;
    const sugerir = () => {
        const partes = [setor, rua, nivel, posicao].map((i) => i.value.trim()).filter(Boolean);
        if (!codigoEditado) codigo.value = partes.join('-').toUpperCase();
        if (!qrEditado) qrTok.value = codigo.value ? `QR-${codigo.value}` : '';
    };
    [setor, rua, nivel, posicao].forEach((i) => i.addEventListener('input', sugerir));
    codigo.addEventListener('input', () => { codigoEditado = true; if (!qrEditado) qrTok.value = codigo.value ? `QR-${codigo.value}` : ''; });
    qrTok.addEventListener('input', () => { qrEditado = true; });

    const form = h('form', { class: 'form-grade' },
        campo('Depósito', dep),
        h('div', { class: 'linha-4' }, campo('Setor', setor), campo('Rua', rua), campo('Nível', nivel), campo('Posição', posicao)),
        h('div', { class: 'linha-2' },
            campo('Código da estante', codigo, estante ? 'Único.' : 'Único. Sugerido a partir dos campos acima.'),
            campo('Conteúdo do QR Code', qrTok, 'Único. É o que o leitor bipa.')),
        campo('Capacidade máxima (itens)', cap, estante
            ? `Soma de todos os produtos que cabem nela. Hoje ela tem ${fmtNum(estante.currentQuantity)} itens.`
            : 'Soma de todos os produtos que cabem nela (1 a 1.000.000).'),
        h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => d.close() }),
            h('button', { type: 'submit', class: 'btn btn-primario' }, estante ? 'Salvar' : 'Cadastrar estante')));
    const d = abrirModal(estante ? `Editar estante ${estante.fullCode}` : 'Nova estante', form, { larga: true });
    setor.focus();

    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        if (!form.reportValidity()) return;
        const dados = {
            warehouseId: dep.value, sector: setor.value.trim(), rack: rua.value.trim(), level: nivel.value.trim(),
            position: posicao.value.trim() || null, fullCode: codigo.value.trim(), qrToken: qrTok.value.trim(),
            maxCapacity: Number(cap.value), isActive: estante ? estante.isActive : true,
        };
        const r = await enviar(form, () => estante ? api.alterarLocalizacao(estante.id, dados) : api.criarLocalizacao(dados));
        if (!r) return;
        aviso(estante ? 'Estante atualizada.' : 'Estante cadastrada.');
        d.close();
        aoSalvar();
    });
}

// ---------------------------------------------------------------------------
// HISTÓRICO DE MOVIMENTAÇÕES
// ---------------------------------------------------------------------------
export async function telaHistorico(raiz) {
    const area = h('div');
    limpar(raiz, topo('Histórico de movimentações', 'As 1.000 mais recentes. Horários de Brasília.',
        h('a', { class: 'btn', href: '/dashboards/index.html' }, 'Gráficos e relatórios Excel')), area);

    await carregar(area, () => api.movimentacoes(), (movs) => {
        const busca = input({ type: 'search', placeholder: 'Produto, SKU, estante ou usuário…' });
        const tipo = select({}, opcao('', 'Entradas e saídas'), opcao('IN', 'Só entradas'), opcao('OUT', 'Só saídas'));
        const de = input({ type: 'date', 'aria-label': 'De' });
        const ate = input({ type: 'date', 'aria-label': 'Até' });
        const exportar = botao('Exportar CSV', { class: 'btn' });
        const resumo = h('div', { class: 'kpis kpis-compactos' });
        const corpo = h('tbody');
        let filtradas = movs;

        function desenhar() {
            const termo = normalizar(busca.value);
            filtradas = movs.filter((m) => {
                if (tipo.value && m.type !== tipo.value) return false;
                if (termo && !normalizar(`${m.productName} ${m.sku} ${m.locationCode} ${m.userEmail}`).includes(termo)) return false;
                const dia = diaBrasilia(m.createdAt);
                if (de.value && dia < de.value) return false;
                if (ate.value && dia > ate.value) return false;
                return true;
            });
            const ent = filtradas.filter((m) => m.type === 'IN');
            const sai = filtradas.filter((m) => m.type === 'OUT');
            const soma = (l, f) => l.reduce((s, m) => s + f(m), 0);
            const faturamento = soma(sai, (m) => m.quantity * m.price);
            const custoSaidas = soma(sai, (m) => m.quantity * m.cost);
            limpar(resumo,
                kpi('Entradas', `+${fmtNum(soma(ent, (m) => m.quantity))}`, `${fmtNum(ent.length)} registros`),
                kpi('Saídas', `−${fmtNum(soma(sai, (m) => m.quantity))}`, `${fmtNum(sai.length)} registros`),
                kpi('Faturamento estimado', fmtMoeda(faturamento), 'saídas × preço da época'),
                kpi('Lucro bruto estimado', fmtMoeda(faturamento - custoSaidas), 'saídas × (preço − custo)'));
            limpar(corpo, filtradas.slice(0, 500).map((m) => h('tr', {},
                h('td', { class: 'nowrap' }, fmtDataHora(m.createdAt)),
                h('td', {}, tipoBadge(m.type)),
                h('td', {}, m.productName, h('div', { class: 'mono pequeno sub' }, m.sku)),
                h('td', { class: 'mono' }, m.locationCode),
                h('td', { class: 'num' }, fmtNum(m.quantity)),
                h('td', { class: 'num' }, fmtMoeda(m.quantity * (m.type === 'OUT' ? m.price : m.cost))),
                h('td', { class: 'pequeno' }, m.userEmail))));
            if (!filtradas.length) corpo.append(h('tr', {}, h('td', { colspan: 7, class: 'vazio-celula' }, movs.length ? 'Nada no filtro.' : 'Nenhuma movimentação ainda.')));
            if (filtradas.length > 500) corpo.append(h('tr', {}, h('td', { colspan: 7, class: 'vazio-celula' }, `Mostrando 500 de ${fmtNum(filtradas.length)}. Use os filtros ou exporte o CSV.`)));
        }
        busca.addEventListener('input', desenhar);
        [tipo, de, ate].forEach((el) => el.addEventListener('change', desenhar));
        exportar.addEventListener('click', () => {
            const linhas = [['Data/hora (Brasília)', 'Data (UTC)', 'Tipo', 'Produto', 'SKU', 'Estante', 'Quantidade',
                'Custo unit. (época)', 'Preço unit. (época)', 'Usuário']];
            filtradas.forEach((m) => linhas.push([fmtDataHora(m.createdAt), m.createdAt, m.type === 'IN' ? 'Entrada' : 'Saída',
                m.productName, m.sku, m.locationCode, m.quantity, m.cost, m.price, m.userEmail]));
            baixarArquivo(`movimentacoes_${diaBrasilia(new Date())}.csv`, paraCsv(linhas));
        });

        area.append(h('div', { class: 'filtros' }, busca, tipo, h('label', { class: 'rotulo-inline' }, 'De', de),
            h('label', { class: 'rotulo-inline' }, 'Até', ate), exportar),
            resumo,
            h('p', { class: 'sub pequeno' }, 'Os valores usam o custo e o preço registrados no momento de cada movimentação.'),
            tabela(['Quando', 'Tipo', 'Produto', 'Estante', { texto: 'Qtd.', classe: 'num' }, { texto: 'Valor', classe: 'num' }, 'Usuário'], corpo));
        desenhar();
    });
}

// ---------------------------------------------------------------------------
// MERCADO LIVRE (admin)
// ---------------------------------------------------------------------------
export async function telaMercadoLivre(raiz, ctx) {
    const area = h('div');
    const recarregar = () => telaMercadoLivre(raiz, ctx);
    limpar(raiz, topo('Mercado Livre', 'Conexão da conta e vínculo entre produtos e anúncios.'), area);

    await carregar(area, () => Promise.all([api.meliStatus(), api.meliVinculos(), api.produtos()]), ([status, vinculos, produtos]) => {
        // ----- conexão -----
        let situacao;
        if (!status.configured) {
            situacao = h('div', { class: 'alerta' },
                h('strong', {}, 'Falta configurar o app do Mercado Livre. '),
                'Cadastre o App ID e o Secret (criados no DevCenter da conta da empresa) em ',
                h('a', { href: '#/configuracoes', class: 'link' }, 'Configurações'), '.');
        } else if (!status.connected) {
            situacao = h('p', {}, h('span', { class: 'ponto ponto-alerta' }), 'Nenhuma conta conectada.');
        } else if (status.needsReconnect) {
            situacao = h('div', { class: 'erro-caixa' }, h('strong', {}, 'A conexão expirou ou foi revogada. Reconecte a conta abaixo.'),
                status.lastError ? h('p', { class: 'pequeno' }, status.lastError) : null);
        } else if (!status.autoRenew) {
            situacao = h('div', {},
                h('p', {}, h('span', { class: 'ponto ponto-alerta' }), 'Conta conectada, ',
                    h('strong', {}, `mas só até ${fmtDataHora(status.expiresAt)}.`)),
                h('div', { class: 'alerta' },
                    h('strong', {}, 'Sem renovação automática. '),
                    'O app está sem a permissão "offline_access". No DevCenter do Mercado Livre, abra o app, ative o acesso offline ',
                    '(offline_access) nas permissões, salve e reconecte a conta aqui. Sem isso, será preciso reconectar a cada ~6 horas.'));
        } else {
            situacao = h('p', {}, h('span', { class: 'ponto ponto-ok' }), 'Conta conectada. ',
                h('span', { class: 'sub' }, `Token válido até ${fmtDataHora(status.expiresAt)} (renovado automaticamente).`),
                status.lastError ? h('span', { class: 'sub pequeno' }, ` Último aviso: ${status.lastError}`) : null);
        }
        const blocoConexao = h('section', { class: 'cartao' }, h('h2', {}, 'Conexão'), situacao);

        if (status.configured) {
            const passo1 = h('div', { class: 'passo' });
            const urlColada = h('textarea', { class: 'entrada', rows: 3, placeholder: 'https://www.google.com/?code=TG-...&state=...' });
            const code = input({ placeholder: 'TG-...', autocomplete: 'off' });
            const state = input({ placeholder: 'state', autocomplete: 'off' });
            urlColada.addEventListener('input', () => {
                const { code: c, state: s } = lerRetorno(urlColada.value);
                if (c) code.value = c;
                if (s) state.value = s;
            });
            const gerar = botao(status.connected ? 'Gerar link para reconectar' : 'Gerar link de autorização', {
                class: `btn ${status.connected && !status.needsReconnect ? '' : 'btn-primario'}`,
                onclick: async () => {
                    gerar.disabled = true;
                    try {
                        const { url } = await api.meliAutorizar();
                        limpar(passo1,
                            h('p', {}, 'Link gerado (vale 10 minutos, uma vez só, e se perde se o servidor reiniciar):'),
                            h('a', { href: url, target: '_blank', rel: 'noopener noreferrer', class: 'btn btn-primario' }, 'Abrir o Mercado Livre em nova aba ↗'),
                            h('p', { class: 'sub pequeno' }, 'Entre na conta DONA dos anúncios e clique em Permitir. Você vai cair na página de retorno cadastrada no app (ex.: Google). Copie o endereço inteiro da barra e cole abaixo.'));
                    } catch (e) {
                        aviso(e.message, 'erro');
                    } finally {
                        gerar.disabled = false;
                    }
                },
            });
            limpar(passo1, gerar);
            const formConectar = h('form', { class: 'form-grade' },
                h('ol', { class: 'passos' },
                    h('li', {}, h('strong', {}, 'Gere o link e autorize no Mercado Livre'), passo1),
                    h('li', {}, h('strong', {}, 'Cole o endereço da página de retorno'),
                        campo('Endereço completo', urlColada, 'O code e o state são extraídos sozinhos.'),
                        h('div', { class: 'linha-2' }, campo('code', code), campo('state', state))),
                    h('li', {}, h('strong', {}, 'Conclua'), h('div', {}, h('button', { type: 'submit', class: 'btn btn-primario' }, 'Concluir conexão')))));
            formConectar.addEventListener('submit', async (e) => {
                e.preventDefault();
                if (!code.value.trim() || !state.value.trim()) return aviso('Informe o code e o state.', 'erro');
                const r = await enviar(formConectar, () => api.meliCallback(code.value.trim(), state.value.trim()));
                if (r === undefined) {
                    // O link e o código valem UMA vez: depois de uma falha, só um link novo funciona.
                    urlColada.value = code.value = state.value = '';
                    limpar(passo1, gerar, h('p', { class: 'alerta pequeno' },
                        'Este link já foi usado. Corrija o que a mensagem de erro indicou e gere um link novo.'));
                    return;
                }
                aviso('Integração concluída! Os anúncios vinculados serão sincronizados agora.');
                recarregar();
            });
            const precisaConectar = !status.connected || status.needsReconnect;
            blocoConexao.append(h('details', { class: 'sanfona', open: precisaConectar },
                h('summary', {}, precisaConectar ? 'Conectar a conta' : 'Trocar a conta conectada'), formConectar));
            if (status.connected) {
                blocoConexao.append(h('div', { class: 'acoes' }, botao('Desconectar conta', {
                    class: 'btn btn-perigo-texto',
                    onclick: async () => {
                        if (!await confirmar('Desconectar a conta do Mercado Livre? O estoque dos anúncios deixa de ser atualizado até conectar de novo.', { textoOk: 'Desconectar', perigo: true })) return;
                        if (await acao(() => api.meliDesconectar(), 'Conta desconectada.')) recarregar();
                    },
                })));
            }
        }

        // ----- vínculos -----
        const selProd = select({ required: true }, opcao('', 'Escolha o produto…'), produtos.map((p) => opcao(p.id, `${p.name} (${p.sku})`)));
        const item = input({ required: true, placeholder: 'MLB123456789', maxlength: 20, autocapitalize: 'characters' });
        const formVinc = h('form', { class: 'form-inline' }, selProd, item, h('button', { type: 'submit', class: 'btn btn-primario' }, 'Vincular'));
        formVinc.addEventListener('submit', async (e) => {
            e.preventDefault();
            const id = item.value.trim().toUpperCase();
            if (!selProd.value) return aviso('Escolha o produto.', 'erro');
            if (!/^ML[A-Z]\d+$/.test(id) || id.length < 6 || id.length > 20) return aviso('ID do anúncio inválido. Exemplo: MLB123456789.', 'erro');
            const r = await enviar(formVinc, () => api.criarVinculo({ product_id: selProd.value, meli_item_id: id }));
            if (r) { aviso('Anúncio vinculado. A sincronização começa agora.'); recarregar(); }
        });

        const situacaoSync = (v) => {
            if (v.lastSyncOk === true) return h('span', { class: 'badge badge-entrada', title: fmtDataHora(v.lastSyncAt) }, `Enviado: ${fmtNum(v.lastSyncQty)} un.`);
            if (v.lastSyncOk === false) return h('span', {}, h('span', { class: 'badge badge-saida' }, 'Falhou'), h('div', { class: 'pequeno sub erro-sync' }, v.lastSyncError || ''));
            return h('span', { class: 'sub pequeno' }, 'Aguardando');
        };

        const blocoVinculos = h('section', { class: 'cartao' },
            h('h2', {}, 'Produtos vinculados a anúncios'),
            h('p', { class: 'sub' }, 'Um produto pode ter vários anúncios; um anúncio pertence a um só produto. O Mercado Livre recebe o estoque TOTAL do produto. Falhas são tentadas de novo a cada 10 minutos.'),
            formVinc,
            vinculos.length
                ? tabela(['Anúncio', 'Produto', { texto: 'Estoque atual', classe: 'num' }, 'Última sincronização', ''],
                    vinculos.map((v) => h('tr', {},
                        h('td', { class: 'mono' }, h('a', { class: 'link', href: `https://produto.mercadolivre.com.br/${v.meliItemId.slice(0, 3)}-${v.meliItemId.slice(3)}`, target: '_blank', rel: 'noopener noreferrer' }, v.meliItemId)),
                        h('td', {}, v.productName, h('div', { class: 'mono pequeno sub' }, v.sku)),
                        h('td', { class: 'num' }, fmtNum(v.totalQuantity)),
                        h('td', {}, situacaoSync(v)),
                        h('td', { class: 'acoes-linha' },
                            botao('Sincronizar', { class: 'btn btn-pequeno', onclick: async () => { if (await acao(() => api.sincronizarVinculo(v.id), 'Sincronização agendada. Atualize a página em alguns segundos.')) setTimeout(recarregar, 2500); } }),
                            botao('Remover', {
                                class: 'btn btn-pequeno btn-perigo-texto',
                                onclick: async () => {
                                    if (!await confirmar(`Remover o vínculo de ${v.meliItemId} com "${v.productName}"? O anúncio deixa de ser atualizado pelo sistema.`, { textoOk: 'Remover', perigo: true })) return;
                                    if (await acao(() => api.removerVinculo(v.id), 'Vínculo removido.')) recarregar();
                                },
                            })))))
                : vazio('Nenhum anúncio vinculado ainda.'));

        area.append(blocoConexao, blocoVinculos);
    });
}

/** Extrai code e state de uma URL colada (ou de um texto "code=...&state=..."). */
export function lerRetorno(texto) {
    const t = String(texto || '').trim();
    let busca = '';
    try { busca = new URL(t).search; } catch { busca = t.includes('?') ? t.slice(t.indexOf('?')) : t; }
    const p = new URLSearchParams(busca.replace(/^\?/, ''));
    return { code: p.get('code') || '', state: p.get('state') || '' };
}

// ---------------------------------------------------------------------------
// USUÁRIOS (admin)
// ---------------------------------------------------------------------------
export async function telaUsuarios(raiz, ctx) {
    const area = h('div');
    const recarregar = () => telaUsuarios(raiz, ctx);
    limpar(raiz, topo('Usuários', 'Quem acessa o sistema. Desativar, trocar o papel ou a senha desconecta a pessoa na hora.'), area);

    await carregar(area, () => api.usuarios(), (usuarios) => {
        const email = input({ type: 'email', required: true, maxlength: 254, autocomplete: 'off', placeholder: 'operador@empresa.com' });
        const senha = input({ type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
        const papel = select({ required: true }, opcao('operador', 'Operador: consulta e movimenta'), opcao('admin', 'Administrador: tudo'));
        const form = h('form', { class: 'cartao form-grade' },
            h('h2', {}, 'Novo usuário'),
            h('div', { class: 'linha-2' }, campo('E-mail', email), campo('Senha inicial', senha, '10 a 128 caracteres. Passe para a pessoa por um canal seguro.')),
            campo('Papel', papel),
            h('div', { class: 'acoes' }, h('button', { type: 'submit', class: 'btn btn-primario' }, 'Criar usuário')));
        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            if (!form.reportValidity()) return;
            const r = await enviar(form, () => api.criarUsuario({ email: email.value.trim(), password: senha.value, role: papel.value }));
            if (r) { aviso('Usuário criado.'); recarregar(); }
        });

        const linhas = usuarios.map((u) => {
            const souEu = u.email === ctx.usuario.email;
            const selPapel = select({ 'aria-label': `Papel de ${u.email}`, disabled: souEu }, opcao('operador', 'Operador'), opcao('admin', 'Administrador'));
            selPapel.value = u.role;
            selPapel.addEventListener('change', async () => {
                if (!await acao(() => api.alterarUsuario(u.id, { role: selPapel.value }), 'Papel alterado.')) selPapel.value = u.role;
                else recarregar();
            });
            return h('tr', {},
                h('td', {}, u.email, souEu ? h('span', { class: 'badge badge-entrada', style: { marginLeft: '.4rem' } }, 'você') : null),
                h('td', {}, selPapel),
                h('td', {}, u.active ? h('span', { class: 'badge badge-entrada' }, 'Ativo') : h('span', { class: 'badge badge-saida' }, 'Desativado')),
                h('td', { class: 'nowrap pequeno' }, fmtDataHora(u.createdAt)),
                h('td', { class: 'acoes-linha' },
                    souEu ? null : botao(u.active ? 'Desativar' : 'Reativar', {
                        class: `btn btn-pequeno ${u.active ? 'btn-perigo-texto' : ''}`,
                        onclick: async () => {
                            if (u.active && !await confirmar(`Desativar ${u.email}? A pessoa é desconectada na hora e não consegue mais entrar.`, { textoOk: 'Desativar', perigo: true })) return;
                            if (await acao(() => api.alterarUsuario(u.id, { active: !u.active }), u.active ? 'Usuário desativado.' : 'Usuário reativado.')) recarregar();
                        },
                    }),
                    souEu ? null : botao('Redefinir senha', { class: 'btn btn-pequeno', onclick: () => formRedefinirSenha(u) })));
        });

        area.append(h('div', { class: 'grade-mov' }, form,
            h('section', { class: 'cartao' }, h('h2', {}, `Usuários (${usuarios.length})`),
                tabela(['E-mail', 'Papel', 'Situação', 'Criado em', ''], linhas),
                h('p', { class: 'sub pequeno' }, 'Para trocar a sua própria senha, use "Minha senha" no menu.'))));
    });
}

function formRedefinirSenha(u) {
    const senha = input({ type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
    const form = h('form', { class: 'form-grade' },
        campo(`Nova senha para ${u.email}`, senha, 'A pessoa é desconectada e passa a entrar com esta senha.'),
        h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => d.close() }),
            h('button', { type: 'submit', class: 'btn btn-primario' }, 'Redefinir')));
    const d = abrirModal('Redefinir senha', form);
    senha.focus();
    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        if (!form.reportValidity()) return;
        if (await enviar(form, () => api.redefinirSenha(u.id, senha.value))) { aviso('Senha redefinida.'); d.close(); }
    });
}

/** "Minha senha" (qualquer usuário). */
export function abrirMinhaSenha() {
    const atual = input({ type: 'password', required: true, autocomplete: 'current-password' });
    const nova = input({ type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
    const nova2 = input({ type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
    const form = h('form', { class: 'form-grade' },
        campo('Senha atual', atual),
        h('div', { class: 'linha-2' }, campo('Nova senha', nova, '10 a 128 caracteres.'), campo('Repita a nova senha', nova2)),
        h('p', { class: 'sub pequeno' }, 'Outras sessões abertas com o seu usuário serão desconectadas.'),
        h('div', { class: 'acoes' }, botao('Cancelar', { onclick: () => d.close() }),
            h('button', { type: 'submit', class: 'btn btn-primario' }, 'Trocar senha')));
    const d = abrirModal('Minha senha', form);
    atual.focus();
    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        if (!form.reportValidity()) return;
        if (nova.value !== nova2.value) return aviso('As senhas novas não conferem.', 'erro');
        const r = await enviar(form, () => api.trocarMinhaSenha(atual.value, nova.value));
        if (!r) return;
        sessao.salvarLogin(r);
        aviso('Senha trocada.');
        d.close();
    });
}

// ---------------------------------------------------------------------------
// CONFIGURAÇÕES (admin)
// ---------------------------------------------------------------------------
export async function telaConfiguracoes(raiz, ctx) {
    const area = h('div');
    const recarregar = () => telaConfiguracoes(raiz, ctx);
    limpar(raiz, topo('Configurações', 'Mercado Livre, backup e acesso pela rede.'), area);

    await carregar(area, () => api.configuracao(), (cfg) => {
        // ----- Mercado Livre -----
        const appId = input({ required: true, inputmode: 'numeric', maxlength: 40, value: cfg.meli.appId, placeholder: '1234567890123456' });
        const secret = input({ type: 'password', autocomplete: 'off', maxlength: 200,
            placeholder: cfg.meli.secretConfigured ? '•••••••• (já gravado; preencha só para trocar)' : 'Secret do app' });
        const redirect = input({ required: true, maxlength: 300, value: cfg.meli.redirectUri });
        const formMeli = h('form', { class: 'cartao form-grade' },
            h('h2', {}, 'App do Mercado Livre'),
            h('p', { class: 'sub pequeno' }, 'Crie o app no DevCenter do Mercado Livre com a conta da empresa (dona dos anúncios) e copie os dados aqui. O Secret fica guardado cifrado e nunca volta para a tela.'),
            h('div', { class: 'linha-2' }, campo('App ID', appId), campo('Secret', secret)),
            campo('URI de redirecionamento', redirect, 'Exatamente a mesma cadastrada no app do DevCenter.'),
            h('div', { class: 'acoes' }, h('button', { type: 'submit', class: 'btn btn-primario' }, 'Salvar')));
        formMeli.addEventListener('submit', async (e) => {
            e.preventDefault();
            if (!formMeli.reportValidity()) return;
            const r = await enviar(formMeli, () => api.salvarConfigMeli({ appId: appId.value.trim(), secret: secret.value.trim() || null, redirectUri: redirect.value.trim() }));
            if (r) { aviso('Configuração do Mercado Livre salva. Agora conecte a conta na tela Mercado Livre.'); recarregar(); }
        });

        // ----- Backup -----
        const pasta = input({ maxlength: 400, value: cfg.backup.personalizada ? cfg.backup.pasta : '', placeholder: cfg.backup.pastaPadrao });
        const fazerAgora = botao('Fazer backup agora');
        fazerAgora.addEventListener('click', async () => {
            fazerAgora.disabled = true;
            try {
                const r = await api.backupAgora();
                aviso(`Backup criado: ${r.arquivo}`);
                recarregar();
            } catch (e) {
                aviso(e.message, 'erro');
            } finally {
                fazerAgora.disabled = false;
            }
        });
        const formBackup = h('form', { class: 'cartao form-grade' },
            h('h2', {}, 'Backup'),
            h('p', { class: 'sub pequeno' }, 'O sistema copia o banco uma vez por dia e guarda as 14 últimas cópias. Prefira uma pasta FORA deste computador (pendrive, HD externo ou pasta de outro computador da rede): se o disco falhar, o backup não vai junto.'),
            campo('Pasta do backup', pasta, `Vazio = pasta padrão (${cfg.backup.pastaPadrao}). Ex.: E:\\Backup StockBridge ou \\\\OUTRO-PC\\Backups`),
            h('p', { class: 'pequeno' }, 'Último backup: ', h('strong', {}, cfg.backup.ultimo ? fmtDataHora(cfg.backup.ultimo) : 'nenhum ainda')),
            cfg.backup.ultimoErro ? h('div', { class: 'alerta' }, cfg.backup.ultimoErro) : null,
            h('div', { class: 'acoes' }, fazerAgora, h('button', { type: 'submit', class: 'btn btn-primario' }, 'Salvar pasta')));
        formBackup.addEventListener('submit', async (e) => {
            e.preventDefault();
            const r = await enviar(formBackup, () => api.salvarConfigBackup(pasta.value.trim() || null));
            if (r) { aviso('Pasta de backup salva.'); recarregar(); }
        });

        // ----- Rede -----
        const blocoRede = h('section', { class: 'cartao' },
            h('h2', {}, 'Acesso pela rede'),
            h('p', {}, 'Nos outros computadores, basta abrir o ', h('strong', {}, 'StockBridge.exe'), ' (modo terminal): ele encontra este servidor sozinho, mesmo que o IP mude.'),
            h('dl', { class: 'detalhes' },
                h('dt', {}, 'Servidor'), h('dd', {}, cfg.rede.nome),
                h('dt', {}, 'Endereços'), h('dd', {}, cfg.rede.enderecos.length
                    ? h('ul', { class: 'lista-enderecos' }, cfg.rede.enderecos.map((e) => h('li', { class: 'mono' }, e)))
                    : 'Nenhuma rede encontrada (só funciona neste computador).'),
                h('dt', {}, 'Certificado'), h('dd', { class: 'mono pequeno quebra' }, cfg.rede.caFingerprint)),
            h('p', { class: 'sub pequeno' }, 'Celular ou computador sem o .exe: baixe e instale o certificado abaixo uma vez (no Windows: abrir o arquivo > Instalar certificado > Usuário atual > "Autoridades de Certificação Raiz Confiáveis"). Depois, acesse um dos endereços acima.'),
            h('div', { class: 'acoes' }, h('a', { class: 'btn', href: '/api/rede/ca.crt', download: 'StockBridge-certificado.crt' }, 'Baixar certificado')));

        area.append(h('div', { class: 'grade-2' }, formMeli, formBackup), blocoRede);
    });
}
