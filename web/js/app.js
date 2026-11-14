/* Inicialização: primeiro acesso, login, menu lateral, rotas (#/...) e controle da sessão. */
import { api, sessao, ApiErro } from './api.js';
import { h, limpar, $, aviso, campo } from './util.js';
import {
    telaPainel, telaMovimentar, telaProdutos, telaDepositos, telaEstantes,
    telaHistorico, telaMercadoLivre, telaUsuarios, telaConfiguracoes, abrirMinhaSenha,
} from './telas.js';
import { telaAnuncios, telaRendimento, telaLocalizar } from './telas-ml.js';

const ROTAS = [
    { id: 'painel', titulo: 'Painel', icone: '◧', tela: telaPainel },
    { id: 'movimentar', titulo: 'Movimentar', icone: '⇅', tela: telaMovimentar },
    { id: 'localizar', titulo: 'Onde está', icone: '⌕', tela: telaLocalizar },
    { id: 'produtos', titulo: 'Produtos', icone: '▤', tela: telaProdutos },
    { id: 'estantes', titulo: 'Estantes', icone: '▦', tela: telaEstantes },
    { id: 'depositos', titulo: 'Depósitos', icone: '⌂', tela: telaDepositos },
    { id: 'historico', titulo: 'Histórico', icone: '☰', tela: telaHistorico },
    { id: 'anuncios', titulo: 'Anúncios ML', icone: '◫', tela: telaAnuncios, admin: true },
    { id: 'rendimento', titulo: 'Rendimento', icone: '↗', tela: telaRendimento, admin: true },
    { id: 'mercadolivre', titulo: 'Conexão ML', icone: '⇄', tela: telaMercadoLivre, admin: true },
    { id: 'usuarios', titulo: 'Usuários', icone: '☺', tela: telaUsuarios, admin: true },
    { id: 'configuracoes', titulo: 'Configurações', icone: '⚙', tela: telaConfiguracoes, admin: true },
];

const raizApp = $('#app');
let ctx = null;
let timerExpiracao = null;
let timerSaude = null;

function marca(classe = '') {
    return h('div', { class: `marca ${classe}` }, h('span', { class: 'marca-icone' }, 'SB'), 'StockBridge');
}

// ---------------------------------------------------------------------------
// Primeiro acesso: criar o administrador
// ---------------------------------------------------------------------------
function telaPrimeiroAcesso(local) {
    pararTimers();
    document.title = 'Primeiro acesso · StockBridge';
    if (!local) {
        limpar(raizApp, h('main', { class: 'login' }, h('div', { class: 'login-cartao' },
            marca('marca-grande'),
            h('p', { class: 'sub' }, 'O sistema ainda não foi configurado.'),
            h('p', {}, 'Por segurança, o administrador é criado no ', h('strong', {}, 'próprio computador servidor'),
                '. Vá até ele: o navegador abre sozinho quando o StockBridge é iniciado (ou acesse https://localhost:3000 lá).'),
            h('button', { type: 'button', class: 'btn', onclick: iniciar }, 'Já configurei, tentar de novo'))));
        return;
    }
    const email = h('input', { class: 'entrada', type: 'email', required: true, autocomplete: 'username', maxlength: 254 });
    const senha = h('input', { class: 'entrada', type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
    const senha2 = h('input', { class: 'entrada', type: 'password', required: true, minlength: 10, maxlength: 128, autocomplete: 'new-password' });
    const erro = h('p', { class: 'erro-login', role: 'alert' });
    const criar = h('button', { type: 'submit', class: 'btn btn-primario btn-grande' }, 'Criar administrador e entrar');
    const form = h('form', { class: 'login-cartao' },
        marca('marca-grande'),
        h('p', { class: 'sub' }, 'Bem-vindo! Crie o primeiro administrador do sistema.'),
        campo('E-mail', email),
        campo('Senha', senha, '10 a 128 caracteres. Guarde em local seguro.'),
        campo('Repita a senha', senha2),
        erro, criar);
    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        erro.textContent = '';
        if (!form.reportValidity()) return;
        if (senha.value !== senha2.value) { erro.textContent = 'As senhas não conferem.'; return; }
        criar.disabled = true;
        try {
            sessao.salvarLogin(await api.criarPrimeiroAdmin(email.value.trim(), senha.value));
            iniciarApp();
            setTimeout(() => aviso('Administrador criado. Próximos passos: Configurações (Mercado Livre e backup), depois depósitos, estantes e produtos.'), 300);
        } catch (err) {
            erro.textContent = err.message;
        } finally {
            criar.disabled = false;
        }
    });
    limpar(raizApp, h('main', { class: 'login' }, form));
    email.focus();
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------
function telaLogin(mensagem, tipo = 'erro') {
    pararTimers();
    document.title = 'Entrar · StockBridge';
    const email = h('input', { class: 'entrada', type: 'email', required: true, autocomplete: 'username', maxlength: 254 });
    const senha = h('input', { class: 'entrada', type: 'password', required: true, autocomplete: 'current-password', maxlength: 128 });
    const erro = h('p', { class: `erro-login ${tipo === 'info' ? 'info' : ''}`, role: 'alert' }, mensagem || '');
    const entrar = h('button', { type: 'submit', class: 'btn btn-primario btn-grande' }, 'Entrar');
    const status = h('p', { class: 'status-api' }, h('span', { class: 'ponto' }), 'Verificando o servidor…');

    const form = h('form', { class: 'login-cartao' },
        marca('marca-grande'),
        h('p', { class: 'sub' }, 'Controle de estoque integrado ao Mercado Livre'),
        campo('E-mail', email),
        campo('Senha', senha),
        erro, entrar, status);

    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        erro.textContent = '';
        erro.classList.remove('info');
        if (!email.value.trim() || !senha.value) { erro.textContent = 'Informe e-mail e senha.'; return; }
        entrar.disabled = true;
        entrar.textContent = 'Entrando…';
        try {
            sessao.salvarLogin(await api.login(email.value.trim(), senha.value));
            iniciarApp();
        } catch (err) {
            erro.textContent = err instanceof ApiErro && err.status === 401 ? 'E-mail ou senha incorretos.' : err.message;
            senha.select();
        } finally {
            entrar.disabled = false;
            entrar.textContent = 'Entrar';
        }
    });

    limpar(raizApp, h('main', { class: 'login' }, form));
    email.focus();

    api.saude()
        .then(() => limpar(status, h('span', { class: 'ponto ponto-ok' }), 'Servidor online'))
        .catch((e) => limpar(status, h('span', { class: 'ponto ponto-erro' }), e.status === 0
            ? 'Servidor fora do ar. O StockBridge está aberto no computador servidor?'
            : `Servidor com problema: ${e.message}`));
}

// ---------------------------------------------------------------------------
// Aplicação logada
// ---------------------------------------------------------------------------
function iniciarApp() {
    const s = sessao.obter();
    if (!s) { telaLogin(); return; }
    ctx = { usuario: { email: s.email, role: s.role }, admin: s.role === 'admin', ir: (rota) => { location.hash = `#/${rota}`; } };

    const rotasVisiveis = ROTAS.filter((r) => !r.admin || ctx.admin);
    const menu = h('nav', { class: 'menu', 'aria-label': 'Principal' },
        rotasVisiveis.map((r) => h('a', { href: `#/${r.id}`, 'data-rota': r.id },
            h('span', { class: 'menu-icone', 'aria-hidden': 'true' }, r.icone), r.titulo)),
        h('a', { href: '/dashboards/index.html' }, h('span', { class: 'menu-icone', 'aria-hidden': 'true' }, '▥'), 'Relatórios'));
    const saude = () => h('span', { class: 'saude', title: 'Situação do servidor' }, h('span', { class: 'ponto' }));
    const btnMenu = h('button', { type: 'button', class: 'btn-icone so-celular', 'aria-label': 'Abrir menu', onclick: () => document.body.classList.toggle('menu-aberto') }, '☰');

    const lateral = h('aside', { class: 'lateral' },
        h('div', { class: 'marca' }, h('span', { class: 'marca-icone' }, 'SB'), 'StockBridge', saude()),
        menu,
        h('div', { class: 'lateral-rodape' },
            h('div', { class: 'usuario' },
                h('strong', {}, ctx.usuario.email),
                h('small', {}, ctx.admin ? 'Administrador' : 'Operador')),
            h('div', { class: 'usuario-acoes' },
                h('button', { type: 'button', class: 'btn btn-pequeno', onclick: abrirMinhaSenha }, 'Minha senha'),
                h('button', { type: 'button', class: 'btn btn-pequeno', onclick: sair }, 'Sair'))));

    const conteudo = h('main', { class: 'conteudo', id: 'conteudo', tabindex: '-1' });
    limpar(raizApp,
        h('div', { class: 'layout' },
            h('header', { class: 'barra-celular so-celular' }, btnMenu, marca(), saude()),
            lateral,
            h('div', { class: 'veu so-celular', onclick: () => document.body.classList.remove('menu-aberto') }),
            conteudo));

    // Sai sozinho quando o token vence (8 h)
    clearTimeout(timerExpiracao);
    timerExpiracao = setTimeout(() => telaLogin('Sua sessão expirou. Entre novamente.'), Math.max(s.expiraEm - Date.now(), 0));
    verificarSaude();
    clearInterval(timerSaude);
    timerSaude = setInterval(verificarSaude, 60000);

    if (!location.hash || location.hash === '#/' || location.hash === '#') location.hash = '#/painel';
    else rotear();
}

async function verificarSaude() {
    let ok = false;
    try { await api.saude(); ok = true; } catch { ok = false; }
    document.querySelectorAll('.saude .ponto').forEach((p) => {
        p.className = `ponto ${ok ? 'ponto-ok' : 'ponto-erro'}`;
        p.parentElement.title = ok ? 'Servidor online' : 'Servidor fora do ar';
    });
}

function rotear() {
    if (!ctx || !sessao.obter()) { iniciar(); return; }
    const [caminho, consulta = ''] = location.hash.replace(/^#\/?/, '').split('?');
    let rota = ROTAS.find((r) => r.id === caminho) || ROTAS[0];
    if (rota.admin && !ctx.admin) {
        aviso('Essa área é só para administradores.', 'erro');
        rota = ROTAS[0];
        history.replaceState(null, '', `#/${rota.id}`);
    }
    document.title = `${rota.titulo} · StockBridge`;
    document.querySelectorAll('.menu a[data-rota]').forEach((a) => {
        const ativo = a.dataset.rota === rota.id;
        a.classList.toggle('ativo', ativo);
        if (ativo) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
    });
    document.body.classList.remove('menu-aberto');
    window.scrollTo(0, 0);
    rota.tela($('#conteudo'), ctx, new URLSearchParams(consulta));
}

function sair() {
    sessao.limpar();
    ctx = null;
    history.replaceState(null, '', location.pathname);
    telaLogin('Você saiu do sistema.', 'info');
}

function pararTimers() {
    clearTimeout(timerExpiracao);
    clearInterval(timerSaude);
}

/** Decide a primeira tela: primeiro acesso, login ou o sistema. */
async function iniciar() {
    if (sessao.obter()) { iniciarApp(); return; }
    try {
        const st = await api.setupStatus();
        if (st.needsSetup) { telaPrimeiroAcesso(st.local); return; }
    } catch { /* servidor fora: a tela de login mostra o aviso */ }
    telaLogin();
}

window.addEventListener('hashchange', rotear);
window.addEventListener('stockbridge:sessao-expirada', () => {
    if (ctx) { ctx = null; telaLogin('Sua sessão expirou ou foi encerrada. Entre novamente.'); }
});

iniciar();
