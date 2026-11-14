/* StockBridge - cliente da API para a página de relatórios (somente leitura).
 *   GET /api/health                  -> texto puro
 *   GET /api/movements               -> histórico (ver calculos.js)
 *   GET /api/locations/{id}/stock    -> [{ product_id, product_name, sku, quantity }]
 *
 * A página é servida pelo próprio servidor StockBridge: as chamadas vão para a mesma origem e usam
 * o token do login do sistema (guardado na aba pelo frontend principal). Antes, nenhuma chamada
 * levava o token e a API respondia 401 em tudo.
 */
(() => {
    const TIMEOUT_MS = 15000;
    const CHAVE_SESSAO = 'stockbridge.sessao';

    function token() {
        try {
            const s = JSON.parse(sessionStorage.getItem(CHAVE_SESSAO) || 'null');
            return s && s.token && Date.now() < s.expiraEm ? s.token : null;
        } catch {
            return null;
        }
    }

    const baseUrl = () => '';
    const descricaoBase = () => location.origin;

    async function buscar(caminho) {
        const controlador = new AbortController();
        const timer = setTimeout(() => controlador.abort(), TIMEOUT_MS);
        const cabecalhos = {};
        const t = token();
        if (t) cabecalhos.Authorization = `Bearer ${t}`;
        try {
            return await fetch(caminho, { signal: controlador.signal, headers: cabecalhos, cache: 'no-store' });
        } catch (erro) {
            if (erro.name === 'AbortError') throw new Error(`tempo esgotado após ${TIMEOUT_MS / 1000}s`);
            throw erro;
        } finally {
            clearTimeout(timer);
        }
    }

    async function exigirOk(resposta) {
        if (resposta.ok) return;
        const detalhe = await resposta.text();
        throw new Error(`HTTP ${resposta.status}${detalhe ? ` — ${detalhe}` : ''}`);
    }

    function dica(erro) {
        if (erro.message.startsWith('HTTP 401')) {
            return ' Entre no sistema (página inicial) e abra esta página de novo pelo menu "Relatórios".';
        }
        return '';
    }

    async function health() {
        const resposta = await buscar('/api/health');
        await exigirOk(resposta);
        return resposta.text();
    }

    async function listarMovimentos() {
        const resposta = await buscar('/api/movements');
        await exigirOk(resposta);
        const dados = await resposta.json();
        if (!Array.isArray(dados)) throw new Error('A resposta da API não está no formato esperado (lista de movimentações).');
        return dados;
    }

    async function estoqueLocalizacao(id) {
        const resposta = await buscar(`/api/locations/${encodeURIComponent(id)}/stock`);
        await exigirOk(resposta);
        const dados = await resposta.json();
        if (!Array.isArray(dados)) throw new Error('A resposta da API não está no formato esperado (lista de produtos).');
        return dados;
    }

    window.StockBridgeApi = { baseUrl, descricaoBase, dica, health, listarMovimentos, estoqueLocalizacao };
})();
