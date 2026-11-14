use axum::{
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

/// Política das telas: só carregam arquivos do próprio servidor e só falam com ele.
/// (As fotos dos anúncios vêm do servidor de imagens do Mercado Livre, mlstatic.com.)
const CSP_TELAS: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https://*.mlstatic.com; \
    connect-src 'self'; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
/// A API só devolve JSON/texto: nada pode ser carregado a partir dela.
const CSP_API: &str = "default-src 'none'; frame-ancestors 'none'";
/// Página antiga de relatórios: usa Chart.js e SheetJS da internet e scripts inline.
/// Política mais aberta SÓ para ela; as telas principais continuam com a estrita.
const CSP_RELATORIOS: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net \
    https://cdn.sheetjs.com; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; \
    base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

/// Cabeçalhos de segurança em TODA resposta.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let eh_api = req.uri().path().starts_with("/api/");
    let eh_relatorios = req.uri().path().starts_with("/dashboards/");
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    if eh_api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP_API));
    } else {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        let csp = if eh_relatorios { CSP_RELATORIOS } else { CSP_TELAS };
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(csp));
    }
    res
}
