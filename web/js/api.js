/* Cliente da API StockBridge (rotas em API.md). As telas e a API saem do MESMO servidor,
 * então as chamadas vão para "/api/..." na mesma origem. */

const CHAVE_SESSAO = 'stockbridge.sessao';
const TEMPO_LIMITE_MS = 20000;

export class ApiErro extends Error {
    constructor(status, mensagem) {
        super(mensagem);
        this.status = status;
    }
}

// ---------- sessão (token JWT) ----------
// Fica no sessionStorage: some ao fechar a aba (reduz o risco de token esquecido em PC compartilhado).
export const sessao = {
    obter() {
        try {
            const s = JSON.parse(sessionStorage.getItem(CHAVE_SESSAO) || 'null');
            if (!s || !s.token || !s.expiraEm || Date.now() >= s.expiraEm) return null;
            return s;
        } catch {
            return null;
        }
    },
    /** Guarda a resposta de login/troca de senha ({ token, role, email, expires_in }). */
    salvarLogin(r) {
        const s = { token: r.token, role: r.role, email: r.email, expiraEm: Date.now() + r.expires_in * 1000 };
        try { sessionStorage.setItem(CHAVE_SESSAO, JSON.stringify(s)); } catch { /* modo privado */ }
        return s;
    },
    limpar() {
        try { sessionStorage.removeItem(CHAVE_SESSAO); } catch { /* ignora */ }
    },
};

const MENSAGENS_PADRAO = {
    400: 'Dados inválidos.',
    401: 'Sessão expirada. Entre novamente.',
    403: 'Seu usuário não tem permissão para esta ação.',
    404: 'Não encontrado.',
    409: 'Já existe um registro com esses dados.',
    413: 'Dados grandes demais.',
    429: 'Muitas tentativas. Aguarde 15 minutos e tente novamente.',
    500: 'Erro interno do servidor. O detalhe aparece na janela do StockBridge no servidor.',
    503: 'O banco de dados do servidor não está respondendo.',
};

async function requisicao(metodo, caminho, corpo, { publica = false } = {}) {
    const cabecalhos = { Accept: 'application/json, text/plain' };
    if (corpo !== undefined) cabecalhos['Content-Type'] = 'application/json';
    const s = sessao.obter();
    if (s && !publica) cabecalhos.Authorization = `Bearer ${s.token}`;

    const controle = new AbortController();
    const timer = setTimeout(() => controle.abort(), TEMPO_LIMITE_MS);
    let resposta;
    try {
        resposta = await fetch(`/api${caminho}`, {
            method: metodo,
            headers: cabecalhos,
            body: corpo === undefined ? undefined : JSON.stringify(corpo),
            signal: controle.signal,
            cache: 'no-store',
        });
    } catch (e) {
        if (e.name === 'AbortError') throw new ApiErro(0, 'O servidor demorou demais para responder.');
        throw new ApiErro(0, 'Sem conexão com o servidor. Ele está ligado e com o StockBridge aberto?');
    } finally {
        clearTimeout(timer);
    }

    if (resposta.status === 401 && !publica) {
        sessao.limpar();
        window.dispatchEvent(new CustomEvent('stockbridge:sessao-expirada'));
    }
    if (!resposta.ok) {
        let texto = '';
        try { texto = (await resposta.text()).trim(); } catch { /* sem corpo */ }
        if (texto.startsWith('<') || texto.length > 500) texto = '';
        throw new ApiErro(resposta.status, texto || MENSAGENS_PADRAO[resposta.status] || `Erro HTTP ${resposta.status}.`);
    }
    if (resposta.status === 204 || resposta.status === 202) return null;
    const tipo = resposta.headers.get('content-type') || '';
    return tipo.includes('application/json') ? resposta.json() : resposta.text();
}

const get = (c, o) => requisicao('GET', c, undefined, o);
const post = (c, corpo, o) => requisicao('POST', c, corpo ?? {}, o);
const put = (c, corpo) => requisicao('PUT', c, corpo);
const del = (c) => requisicao('DELETE', c);
const seg = encodeURIComponent;

export const api = {
    // públicas
    saude: () => get('/health', { publica: true }),
    login: (email, password) => post('/auth/login', { email, password }, { publica: true }),
    setupStatus: () => get('/setup', { publica: true }),
    criarPrimeiroAdmin: (email, password) => post('/setup', { email, password }, { publica: true }),
    meliCallback: (code, state) => get(`/meli/callback?code=${seg(code)}&state=${seg(state)}`, { publica: true }),

    // qualquer usuário logado
    eu: () => get('/auth/me'),
    trocarMinhaSenha: (atual, nova) => post('/auth/password', { current: atual, new: nova }),
    produtos: () => get('/products'),
    depositos: () => get('/warehouses'),
    localizacoes: (qrToken) => get(qrToken ? `/locations?qrToken=${seg(qrToken)}` : '/locations'),
    estoqueDaLocalizacao: (id) => get(`/locations/${seg(id)}/stock`),
    movimentacoes: () => get('/movements'),
    criarMovimentacao: (dados) => post('/movements', dados),
    mapaEstoque: (q) => get(q ? `/stock?q=${seg(q)}` : '/stock'),

    // somente admin
    usuarios: () => get('/users'),
    criarUsuario: (dados) => post('/users', dados),
    alterarUsuario: (id, dados) => put(`/users/${seg(id)}`, dados),
    redefinirSenha: (id, password) => post(`/users/${seg(id)}/password`, { password }),
    criarProduto: (dados) => post('/products', dados),
    alterarProduto: (id, dados) => put(`/products/${seg(id)}`, dados),
    excluirProduto: (id) => del(`/products/${seg(id)}`),
    criarDeposito: (dados) => post('/warehouses', dados),
    alterarDeposito: (id, dados) => put(`/warehouses/${seg(id)}`, dados),
    excluirDeposito: (id) => del(`/warehouses/${seg(id)}`),
    criarLocalizacao: (dados) => post('/locations', dados),
    alterarLocalizacao: (id, dados) => put(`/locations/${seg(id)}`, dados),
    excluirLocalizacao: (id) => del(`/locations/${seg(id)}`),
    meliStatus: () => get('/meli/status'),
    meliAutorizar: () => get('/meli/authorize'),
    meliDesconectar: () => del('/meli/credentials'),
    meliVinculos: () => get('/meli/links'),
    criarVinculo: (dados) => post('/meli/links', dados),
    removerVinculo: (id) => del(`/meli/links/${seg(id)}`),
    sincronizarVinculo: (id) => post(`/meli/links/${seg(id)}/sync`),
    anunciosMl: (atualizar) => get(`/meli/anuncios${atualizar ? '?atualizar=true' : ''}`),
    importarAnuncios: (ids) => post('/meli/importar', { ids }),
    rendimento: (dias) => get(`/meli/rendimento?dias=${seg(dias)}`),
    entradaLote: (itens, sincronizar) => post('/movements/lote', { itens, sincronizar }),
    configuracao: () => get('/config'),
    salvarConfigMeli: (dados) => put('/config/meli', dados),
    salvarConfigBackup: (pasta) => put('/config/backup', { pasta }),
    backupAgora: () => post('/config/backup/agora'),
};
