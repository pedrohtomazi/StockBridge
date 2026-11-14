/* Utilitários de interface: criação segura de elementos, formatação, avisos e janelas.
 * Todo texto vindo da API entra na página como TEXTO (nunca como HTML), o que evita XSS. */

export const $ = (seletor, raiz = document) => raiz.querySelector(seletor);

/** Cria um elemento. Ex.: h('button', { class: 'btn', onclick: fn }, 'Salvar') */
export function h(tag, attrs = {}, ...filhos) {
    const el = document.createElement(tag);
    for (const [chave, valor] of Object.entries(attrs || {})) {
        if (valor === null || valor === undefined || valor === false) continue;
        if (chave.startsWith('on') && typeof valor === 'function') {
            el.addEventListener(chave.slice(2), valor);
        } else if (chave === 'class') {
            el.className = valor;
        } else if (chave === 'style' && typeof valor === 'object') {
            Object.assign(el.style, valor);
        } else if (chave === 'value') {
            el.value = valor;
        } else if (valor === true) {
            el.setAttribute(chave, '');
        } else {
            el.setAttribute(chave, String(valor));
        }
    }
    anexar(el, filhos);
    return el;
}

function anexar(el, filhos) {
    for (const f of filhos.flat(Infinity)) {
        if (f === null || f === undefined || f === false) continue;
        el.append(f instanceof Node ? f : document.createTextNode(String(f)));
    }
}

export function limpar(el, ...filhos) {
    el.replaceChildren();
    anexar(el, filhos);
    return el;
}

// ---------- formatação (pt-BR) ----------
const moeda = new Intl.NumberFormat('pt-BR', { style: 'currency', currency: 'BRL' });
const numero = new Intl.NumberFormat('pt-BR');
const pct = new Intl.NumberFormat('pt-BR', { style: 'percent', maximumFractionDigits: 1 });
const FUSO = 'America/Sao_Paulo';

export const fmtMoeda = (v) => (Number.isFinite(v) ? moeda.format(v) : '—');
export const fmtNum = (v) => (Number.isFinite(v) ? numero.format(v) : '—');
export const fmtPct = (v) => (Number.isFinite(v) ? pct.format(v) : '—');

export function fmtDataHora(iso) {
    if (!iso) return '—';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString('pt-BR', { timeZone: FUSO, dateStyle: 'short', timeStyle: 'short' });
}

/** Data (AAAA-MM-DD) no horário de Brasília. Usada nos filtros por período. */
export function diaBrasilia(isoOuData) {
    const d = isoOuData instanceof Date ? isoOuData : new Date(isoOuData);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleDateString('en-CA', { timeZone: FUSO });
}

export function normalizar(texto) {
    return String(texto ?? '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

// ---------- avisos rápidos ----------
export function aviso(mensagem, tipo = 'ok') {
    let area = $('#avisos');
    if (!area) {
        area = h('div', { id: 'avisos', role: 'status', 'aria-live': 'polite' });
        document.body.append(area);
    }
    const item = h('div', { class: `aviso aviso-${tipo}` }, mensagem);
    area.append(item);
    setTimeout(() => item.classList.add('saindo'), tipo === 'erro' ? 6000 : 3500);
    setTimeout(() => item.remove(), tipo === 'erro' ? 6400 : 3900);
}

// ---------- janelas (dialog) ----------
export function abrirModal(titulo, conteudo, { larga = false } = {}) {
    const fechar = () => d.close();
    const d = h('dialog', { class: `modal${larga ? ' modal-larga' : ''}` },
        h('header', { class: 'modal-topo' },
            h('h3', {}, titulo),
            h('button', { type: 'button', class: 'btn-icone', 'aria-label': 'Fechar', onclick: fechar }, '×')),
        h('div', { class: 'modal-corpo' }, conteudo));
    // O evento nativo "close" nem sempre dispara (ex.: aba em segundo plano), então
    // d.close() é substituído para remover a janela na hora e avisar com "fechou".
    let fechado = false;
    const aoFechar = () => {
        if (fechado) return;
        fechado = true;
        d.remove();
        d.dispatchEvent(new Event('fechou'));
    };
    const fecharNativo = d.close.bind(d);
    d.close = () => { if (d.open) fecharNativo(); aoFechar(); };
    d.addEventListener('close', aoFechar); // tecla Esc
    d.addEventListener('click', (e) => { if (e.target === d) fechar(); });
    document.body.append(d);
    d.showModal();
    return d;
}

export function confirmar(mensagem, { textoOk = 'Confirmar', perigo = false } = {}) {
    return new Promise((resolver) => {
        let resposta = false;
        const corpo = h('div', {},
            h('p', {}, mensagem),
            h('div', { class: 'acoes' },
                h('button', { type: 'button', class: 'btn', onclick: () => d.close() }, 'Cancelar'),
                h('button', {
                    type: 'button', class: `btn ${perigo ? 'btn-perigo' : 'btn-primario'}`,
                    onclick: () => { resposta = true; d.close(); },
                }, textoOk)));
        const d = abrirModal('Confirmar', corpo);
        d.addEventListener('fechou', () => resolver(resposta));
    });
}

// ---------- pequenos componentes ----------
export function barraOcupacao(atual, maximo) {
    const fracao = maximo > 0 ? Math.min(atual / maximo, 1) : 0;
    const nivel = fracao >= 0.9 ? 'alta' : fracao >= 0.7 ? 'media' : 'baixa';
    return h('div', { class: 'ocupacao', title: `${fmtNum(atual)} de ${fmtNum(maximo)} (${fmtPct(fracao)})` },
        h('div', { class: 'ocupacao-trilho' },
            h('div', { class: `ocupacao-barra ${nivel}`, style: { width: `${(fracao * 100).toFixed(1)}%` } })),
        h('span', { class: 'ocupacao-texto' }, `${fmtNum(atual)} / ${fmtNum(maximo)}`));
}

export function vazio(texto, ...extra) {
    return h('div', { class: 'vazio' }, h('p', {}, texto), ...extra);
}

export function carregando(texto = 'Carregando…') {
    return h('div', { class: 'carregando' }, h('span', { class: 'giro', 'aria-hidden': 'true' }), texto);
}

export function campo(rotulo, controle, ajuda) {
    const id = controle.id || `c-${Math.random().toString(36).slice(2, 9)}`;
    controle.id = id;
    return h('div', { class: 'campo' },
        h('label', { for: id }, rotulo),
        controle,
        ajuda ? h('small', { class: 'ajuda' }, ajuda) : null);
}

/** Baixa um texto como arquivo (gerado no próprio navegador). */
export function baixarArquivo(nome, conteudo, tipo = 'text/csv;charset=utf-8') {
    const blob = new Blob([conteudo], { type: tipo });
    const url = URL.createObjectURL(blob);
    const a = h('a', { href: url, download: nome });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** CSV com ";" (abre direto no Excel em português) e BOM para acentos. */
export function paraCsv(linhas) {
    const celula = (v) => {
        if (typeof v === 'number') return Number.isFinite(v) ? String(v).replace('.', ',') : '';
        let s = v === null || v === undefined ? '' : String(v);
        if (/^[=+\-@\t\r]/.test(s)) s = `'${s}`; // evita fórmula maliciosa ao abrir no Excel
        return /[";\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    return '﻿' + linhas.map((l) => l.map(celula).join(';')).join('\r\n');
}
