//! Testes da API de ponta a ponta (rodam com `cargo test`, sem rede e sem banco externo).
//! Cada teste cria um banco SQLite novo numa pasta temporária.

use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::PathBuf;
use stockbridge::{app, cripto::Cofre, db, rede::InfoRede, security::state::AppState};
use tower::ServiceExt;

const SENHA: &str = "senhaDeTeste123";

struct Ambiente {
    state: AppState,
    pasta: PathBuf,
}

impl Ambiente {
    async fn novo() -> Self {
        let pasta = std::env::temp_dir().join(format!("stockbridge-teste-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&pasta).unwrap();
        let pool = db::abrir(&pasta.join("teste.db")).await.unwrap();
        let info = InfoRede { nome: "teste".into(), porta: 3000, ca_pem: "-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n".into(), ..Default::default() };
        let state = AppState::new(pool, "x".repeat(48), Cofre::temporario(), pasta.clone(), info).unwrap();
        Self { state, pasta }
    }

    /// Router como se a requisição viesse do IP informado.
    fn app(&self, ip: [u8; 4]) -> Router {
        app::router(self.state.clone()).layer(MockConnectInfo(SocketAddr::from((ip, 50000))))
    }

    async fn chamar(&self, ip: [u8; 4], metodo: &str, caminho: &str, token: Option<&str>, corpo: Option<Value>) -> (StatusCode, String) {
        let mut req = Request::builder().method(metodo).uri(caminho);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let req = match corpo {
            Some(c) => req.header("Content-Type", "application/json").body(Body::from(c.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app(ip).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn ok(&self, metodo: &str, caminho: &str, token: &str, corpo: Option<Value>) -> Value {
        let (s, b) = self.chamar([10, 0, 0, 5], metodo, caminho, Some(token), corpo).await;
        assert!(s.is_success(), "{metodo} {caminho} -> {s}: {b}");
        serde_json::from_str(&b).unwrap_or(Value::Null)
    }

    async fn status(&self, metodo: &str, caminho: &str, token: Option<&str>, corpo: Option<Value>) -> StatusCode {
        self.chamar([10, 0, 0, 5], metodo, caminho, token, corpo).await.0
    }

    /// Cria o admin (pelo próprio servidor) e devolve o token.
    async fn admin(&self) -> String {
        let (s, b) = self
            .chamar([127, 0, 0, 1], "POST", "/api/setup", None, Some(json!({"email":"admin@empresa.com","password":SENHA})))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        serde_json::from_str::<Value>(&b).unwrap()["token"].as_str().unwrap().to_string()
    }

    async fn login(&self, email: &str, senha: &str) -> (StatusCode, String) {
        let (s, b) = self.chamar([10, 0, 0, 5], "POST", "/api/auth/login", None, Some(json!({"email":email,"password":senha}))).await;
        let token = serde_json::from_str::<Value>(&b).ok().and_then(|v| v["token"].as_str().map(String::from)).unwrap_or(b);
        (s, token)
    }

    async fn operador(&self, admin: &str) -> String {
        self.ok("POST", "/api/users", admin, Some(json!({"email":"op@empresa.com","password":SENHA,"role":"operador"}))).await;
        self.login("op@empresa.com", SENHA).await.1
    }

    /// Depósito + estante (capacidade informada) + produto. Devolve (estante, produto).
    async fn cenario(&self, admin: &str, capacidade: i64) -> (String, String) {
        let dep = self.ok("POST", "/api/warehouses", admin, Some(json!({"name":"Galpão"}))).await["id"].as_str().unwrap().to_string();
        let sufixo = &uuid::Uuid::new_v4().simple().to_string()[..6];
        let est = self
            .ok("POST", "/api/locations", admin, Some(json!({"warehouseId":dep,"sector":"A","rack":"1","level":"1",
                "fullCode":format!("A-1-{sufixo}"),"qrToken":format!("QR-{sufixo}"),"maxCapacity":capacidade}))).await["id"]
            .as_str().unwrap().to_string();
        let prod = self
            .ok("POST", "/api/products", admin, Some(json!({"name":"Monitor","sku":format!("MON-{sufixo}"),"cost":150,"price":299.9}))).await["id"]
            .as_str().unwrap().to_string();
        (est, prod)
    }

    async fn mover(&self, token: &str, prod: &str, est: &str, tipo: &str, qtd: i64) -> (StatusCode, String) {
        self.chamar([10, 0, 0, 5], "POST", "/api/movements", Some(token), Some(json!({"productId":prod,"locationId":est,"type":tipo,"quantity":qtd}))).await
    }
}

impl Drop for Ambiente {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.pasta);
    }
}

#[tokio::test]
async fn configuracao_inicial_so_no_servidor_e_uma_vez() {
    let a = Ambiente::novo().await;
    let (_, b) = a.chamar([10, 0, 0, 5], "GET", "/api/setup", None, None).await;
    assert_eq!(serde_json::from_str::<Value>(&b).unwrap()["needsSetup"], true);
    // De outro computador da rede: recusado (ninguém "chega antes" e vira admin)
    let (s, _) = a.chamar([192, 168, 0, 20], "POST", "/api/setup", None, Some(json!({"email":"x@y.com","password":SENHA}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    a.admin().await;
    let (s, _) = a.chamar([127, 0, 0, 1], "POST", "/api/setup", None, Some(json!({"email":"z@y.com","password":SENHA}))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(a.status("GET", "/api/health", None, None).await, StatusCode::OK);
}

#[tokio::test]
async fn permissoes() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let op = a.operador(&admin).await;
    assert_eq!(a.status("GET", "/api/movements", None, None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(a.status("GET", "/api/movements", Some("abc.def.ghi"), None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(a.status("GET", "/api/products", Some(&op), None).await, StatusCode::OK);
    for (m, c) in [("POST", "/api/products"), ("GET", "/api/users"), ("GET", "/api/meli/status"), ("GET", "/api/config"), ("POST", "/api/warehouses")] {
        assert_eq!(a.status(m, c, Some(&op), Some(json!({}))).await, StatusCode::FORBIDDEN, "{m} {c}");
    }
}

#[tokio::test]
async fn produtos_dinheiro_e_validacao() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    a.ok("POST", "/api/products", &admin, Some(json!({"name":"Cabo","sku":"CAB-1","cost":0.1,"price":0.2}))).await;
    let lista = a.ok("GET", "/api/products", &admin, None).await;
    assert_eq!(lista[0]["cost"], 0.1);
    assert_eq!(a.status("POST", "/api/products", Some(&admin), Some(json!({"name":"X","sku":"CAB-1","cost":1,"price":2}))).await, StatusCode::CONFLICT);
    assert_eq!(a.status("POST", "/api/products", Some(&admin), Some(json!({"name":"X","sku":"x'; DROP TABLE products;--","cost":1,"price":2}))).await, StatusCode::BAD_REQUEST);
    assert_eq!(a.status("POST", "/api/products", Some(&admin), Some(json!({"name":"X","sku":"NEG","cost":-5,"price":2}))).await, StatusCode::BAD_REQUEST);
    let grande = json!({"name": "a".repeat(20_000)});
    assert_eq!(a.status("POST", "/api/products", Some(&admin), Some(grande)).await, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn estante_exige_deposito_existente() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let s = a.status("POST", "/api/locations", Some(&admin), Some(json!({"warehouseId":"nao-existe","sector":"A","rack":"1","level":"1",
        "fullCode":"A-1","qrToken":"QR-A-1","maxCapacity":10}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn regras_de_movimentacao() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let op = a.operador(&admin).await;
    let (est, prod) = a.cenario(&admin, 100).await;
    assert_eq!(a.mover(&op, &prod, &est, "IN", -50).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, &prod, &est, "IN", 0).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, &prod, &est, "XYZ", 5).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, &prod, &est, "OUT", 10).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, &prod, &est, "IN", 500).await.0, StatusCode::BAD_REQUEST);
    let (s, b) = a.mover(&op, &prod, &est, "IN", 50).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&b).unwrap()["new_balance"], 50);
    assert_eq!(a.mover(&op, &prod, &est, "OUT", 20).await.0, StatusCode::OK);
    assert_eq!(a.mover(&op, &prod, &est, "OUT", 100).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, "nao-existe", &est, "IN", 1).await.0, StatusCode::BAD_REQUEST);

    // Estante inativa: não aceita entrada, mas deixa esvaziar
    let estante = a.ok("GET", "/api/locations", &admin, None).await[0].clone();
    let mut alt = estante.clone();
    alt["isActive"] = json!(false);
    a.ok("PUT", &format!("/api/locations/{est}"), &admin, Some(alt)).await;
    assert_eq!(a.mover(&op, &prod, &est, "IN", 1).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.mover(&op, &prod, &est, "OUT", 30).await.0, StatusCode::OK);
    // Estante vazia: produtos com saldo zero não aparecem
    assert_eq!(a.ok("GET", &format!("/api/locations/{est}/stock"), &op, None).await.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn historico_guarda_preco_da_epoca_e_quem_fez() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let op = a.operador(&admin).await;
    let (est, prod) = a.cenario(&admin, 100).await;
    a.mover(&op, &prod, &est, "IN", 5).await;
    a.ok("PUT", &format!("/api/products/{prod}"), &admin, Some(json!({"name":"Monitor","sku":"MON-NOVO","cost":999,"price":1999}))).await;
    let h = a.ok("GET", "/api/movements", &admin, None).await;
    assert_eq!(h[0]["price"], 299.9, "o preço do histórico não pode mudar");
    assert_eq!(h[0]["userEmail"], "op@empresa.com");
    assert!(h[0]["locationCode"].as_str().unwrap().starts_with("A-1-"));
}

#[tokio::test]
async fn capacidade_resiste_a_entradas_simultaneas() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let (est, prod) = a.cenario(&admin, 100).await;
    let mut tarefas = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let app = a.app([10, 0, 0, 5]);
        let corpo = json!({"productId":prod,"locationId":est,"type":"IN","quantity":10}).to_string();
        let auth = format!("Bearer {admin}");
        tarefas.spawn(async move {
            let req = Request::post("/api/movements").header("Authorization", auth).header("Content-Type", "application/json").body(Body::from(corpo)).unwrap();
            app.oneshot(req).await.unwrap().status()
        });
    }
    let mut ok = 0;
    while let Some(s) = tarefas.join_next().await {
        if s.unwrap() == StatusCode::OK {
            ok += 1;
        }
    }
    assert_eq!(ok, 10, "só 10 entradas de 10 cabem numa estante de 100");
    let estante = a.ok("GET", "/api/locations", &admin, None).await;
    assert_eq!(estante[0]["currentQuantity"], 100);
}

#[tokio::test]
async fn desativar_ou_trocar_senha_derruba_o_login() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let op = a.operador(&admin).await;
    let usuarios = a.ok("GET", "/api/users", &admin, None).await;
    let id_op = usuarios.as_array().unwrap().iter().find(|u| u["email"] == "op@empresa.com").unwrap()["id"].as_str().unwrap().to_string();

    // promover a admin vale na hora (papel lido do banco)
    a.ok("PUT", &format!("/api/users/{id_op}"), &admin, Some(json!({"role":"admin"}))).await;
    assert_eq!(a.status("GET", "/api/products", Some(&op), None).await, StatusCode::UNAUTHORIZED, "token antigo derrubado");
    let op = a.login("op@empresa.com", SENHA).await.1;
    assert_eq!(a.status("GET", "/api/users", Some(&op), None).await, StatusCode::OK);

    // desativar
    a.ok("PUT", &format!("/api/users/{id_op}"), &admin, Some(json!({"active":false}))).await;
    assert_eq!(a.status("GET", "/api/products", Some(&op), None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(a.login("op@empresa.com", SENHA).await.0, StatusCode::FORBIDDEN);

    // o admin não tira o próprio acesso
    let eu = a.ok("GET", "/api/auth/me", &admin, None).await;
    assert_eq!(a.status("PUT", &format!("/api/users/{}", eu["id"].as_str().unwrap()), Some(&admin), Some(json!({"active":false}))).await, StatusCode::BAD_REQUEST);

    // trocar a própria senha devolve token novo e derruba o antigo
    let novo = a.ok("POST", "/api/auth/password", &admin, Some(json!({"current":SENHA,"new":"outraSenhaLonga1"}))).await;
    assert_eq!(a.status("GET", "/api/users", Some(&admin), None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(a.status("GET", "/api/users", Some(novo["token"].as_str().unwrap()), None).await, StatusCode::OK);
}

#[tokio::test]
async fn bloqueio_de_login_e_por_computador() {
    let a = Ambiente::novo().await;
    a.admin().await;
    let corpo = json!({"email":"admin@empresa.com","password":"errada"});
    let mut ultimo = StatusCode::OK;
    for _ in 0..6 {
        ultimo = a.chamar([192, 168, 0, 66], "POST", "/api/auth/login", None, Some(corpo.clone())).await.0;
    }
    assert_eq!(ultimo, StatusCode::TOO_MANY_REQUESTS);
    // Outro computador continua entrando: o atacante não consegue bloquear o admin.
    let (s, _) = a.chamar([192, 168, 0, 10], "POST", "/api/auth/login", None, Some(json!({"email":"admin@empresa.com","password":SENHA}))).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn exclusoes_protegem_o_historico() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let (est, prod) = a.cenario(&admin, 100).await;
    a.mover(&admin, &prod, &est, "IN", 1).await;
    assert_eq!(a.status("DELETE", &format!("/api/products/{prod}"), Some(&admin), None).await, StatusCode::CONFLICT);
    assert_eq!(a.status("DELETE", &format!("/api/locations/{est}"), Some(&admin), None).await, StatusCode::CONFLICT);
    let dep = a.ok("GET", "/api/warehouses", &admin, None).await[0]["id"].as_str().unwrap().to_string();
    assert_eq!(a.status("DELETE", &format!("/api/warehouses/{dep}"), Some(&admin), None).await, StatusCode::CONFLICT);
    // produto sem histórico pode ser excluído
    let novo = a.ok("POST", "/api/products", &admin, Some(json!({"name":"Tmp","sku":"TMP-1","cost":1,"price":2}))).await;
    assert_eq!(a.status("DELETE", &format!("/api/products/{}", novo["id"].as_str().unwrap()), Some(&admin), None).await, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn mercado_livre_configuracao_e_status_da_sincronizacao() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let (est, prod) = a.cenario(&admin, 100).await;

    assert_eq!(a.status("GET", "/api/meli/authorize", Some(&admin), None).await, StatusCode::BAD_REQUEST, "sem app configurado");
    a.ok("PUT", "/api/config/meli", &admin, Some(json!({"appId":"123456","secret":"segredo-do-app","redirectUri":"https://google.com"}))).await;
    let cfg = a.ok("GET", "/api/config", &admin, None).await;
    assert_eq!(cfg["meli"]["secretConfigured"], true);
    assert!(!cfg.to_string().contains("segredo-do-app"), "o secret nunca volta para a tela");
    let guardado: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'meli_secret_enc'").fetch_one(&a.state.pool).await.unwrap();
    assert!(!guardado.contains("segredo"), "o secret fica cifrado no banco");

    let url = a.ok("GET", "/api/meli/authorize", &admin, None).await["url"].as_str().unwrap().to_string();
    assert!(url.contains("client_id=123456") && url.contains("state="));
    assert_eq!(a.status("GET", "/api/meli/callback?code=abc&state=inventado", None, None).await, StatusCode::BAD_REQUEST);

    assert_eq!(a.status("POST", "/api/meli/links", Some(&admin), Some(json!({"product_id":prod,"meli_item_id":"abc&x=1"}))).await, StatusCode::BAD_REQUEST);
    a.ok("POST", "/api/meli/links", &admin, Some(json!({"product_id":prod,"meli_item_id":"mlb123456789"}))).await;
    assert_eq!(a.status("POST", "/api/meli/links", Some(&admin), Some(json!({"product_id":prod,"meli_item_id":"MLB123456789"}))).await, StatusCode::CONFLICT);
    a.mover(&admin, &prod, &est, "IN", 3).await;

    // Sem conta conectada, a falha fica registrada no vínculo (antes só ia para o terminal).
    let mut erro = Value::Null;
    for _ in 0..40 {
        let v = a.ok("GET", "/api/meli/links", &admin, None).await;
        if v[0]["lastSyncOk"] == false {
            erro = v[0]["lastSyncError"].clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(erro.as_str().unwrap_or_default().contains("não conectada"), "erro registrado: {erro}");
    let status = a.ok("GET", "/api/meli/status", &admin, None).await;
    assert_eq!(status["configured"], true);
    assert_eq!(status["connected"], false);
}

#[tokio::test]
async fn backup_cria_copia_do_banco() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let r = a.ok("POST", "/api/config/backup/agora", &admin, None).await;
    let arquivo = PathBuf::from(r["arquivo"].as_str().unwrap());
    assert!(arquivo.exists());
    assert!(arquivo.starts_with(a.pasta.join("backups")));
    let cfg = a.ok("GET", "/api/config", &admin, None).await;
    assert!(cfg["backup"]["ultimo"].is_string());
}

#[tokio::test]
async fn entrada_em_lote_e_mapa_por_sku() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    let op = a.operador(&admin).await;
    let (_, _) = a.cenario(&admin, 10).await; // estante A-1-xxxx (cap. 10) + produto MON-xxxx
    let estante = a.ok("GET", "/api/locations", &admin, None).await[0].clone();
    let codigo = estante["fullCode"].as_str().unwrap().to_string();
    let qr = estante["qrToken"].as_str().unwrap().to_lowercase(); // maiúsculas/minúsculas não importam
    let sku = a.ok("GET", "/api/products", &admin, None).await[0]["sku"].as_str().unwrap().to_string();

    let lote = json!({"itens":[
        {"sku": sku, "estante": codigo, "quantidade": 4},
        {"sku": sku.to_lowercase(), "estante": qr, "quantidade": 3},
        {"sku": "NAO-EXISTE", "estante": codigo, "quantidade": 1},
        {"sku": sku, "estante": "ESTANTE-X", "quantidade": 1},
        {"sku": sku, "estante": codigo, "quantidade": 50}
    ]});
    assert_eq!(a.status("POST", "/api/movements/lote", Some(&op), Some(lote.clone())).await, StatusCode::FORBIDDEN);
    let r = a.ok("POST", "/api/movements/lote", &admin, Some(lote)).await;
    let ok: Vec<bool> = r.as_array().unwrap().iter().map(|l| l["ok"].as_bool().unwrap()).collect();
    assert_eq!(ok, vec![true, true, false, false, false], "{r}");
    assert!(r[2]["mensagem"].as_str().unwrap().contains("não cadastrado"));
    assert!(r[4]["mensagem"].as_str().unwrap().contains("Capacidade"));

    let mapa = a.ok("GET", &format!("/api/stock?q={}", &sku[..5]), &op, None).await;
    assert_eq!(mapa.as_array().unwrap().len(), 1);
    assert_eq!(mapa[0]["quantity"], 7);
    assert_eq!(mapa[0]["locationCode"], codigo);
    assert_eq!(a.ok("GET", "/api/stock?q=nada-disso", &op, None).await.as_array().unwrap().len(), 0);
}

/// Mercado Livre falso, com as respostas no formato da API real.
async fn mercado_livre_simulado() -> String {
    use axum::{Json as J, extract::Query as Q, routing::get as g};
    use std::collections::HashMap;
    let itens = json!({
        "MLB1": {"id":"MLB1","title":"Camiseta Preta","price":50.0,"available_quantity":12,"sold_quantity":40,"status":"active",
                 "permalink":"https://ml/MLB1","thumbnail":"http://http2.mlstatic.com/f1.jpg",
                 "attributes":[{"id":"SELLER_SKU","value_name":"CAM-01"}],"shipping":{"logistic_type":"cross_docking"},"variations":[]},
        "MLB2": {"id":"MLB2","title":"Tênis","price":199.0,"available_quantity":5,"sold_quantity":3,"status":"paused",
                 "shipping":{"logistic_type":"fulfillment"},"variations":[{"id":9,"available_quantity":5,"sold_quantity":3,
                 "attribute_combinations":[{"value_name":"42"}],"attributes":[{"id":"SELLER_SKU","value_name":"TEN-42"}]}]},
        "MLB3": {"id":"MLB3","title":"Boné sem SKU","price":30.0,"available_quantity":2,"sold_quantity":1,"status":"active",
                 "shipping":{},"variations":[]}
    });
    let itens2 = itens.clone();
    let app = Router::new()
        .route("/users/me", g(|| async { J(json!({"id": 777, "nickname": "LOJA"})) }))
        .route("/users/777/items/search", g(|| async { J(json!({"results":["MLB1","MLB2","MLB3"],"paging":{"total":3,"offset":0,"limit":50}})) }))
        .route("/items", g(move |Q(p): Q<HashMap<String, String>>| {
            let itens = itens2.clone();
            async move {
                J(Value::Array(p["ids"].split(',').map(|id| json!({"code":200,"body": itens[id]})).collect()))
            }
        }))
        .route("/visits/items", g(|| async { J(json!({"MLB1": 900, "MLB2": 50, "MLB3": 10})) }))
        .route("/orders/search", g(|| async {
            J(json!({"paging":{"total":2,"offset":0,"limit":50},"results":[
                {"status":"paid","date_created":"2026-11-10T10:00:00.000-03:00","order_items":[
                    {"item":{"id":"MLB1","title":"Camiseta Preta","seller_sku":"CAM-01"},"quantity":2,"unit_price":50.0,"sale_fee":8.0}]},
                {"status":"cancelled","date_created":"2026-11-11T10:00:00.000-03:00","order_items":[]}
            ]}))
        }));
    let _ = itens;
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endereco = format!("http://{}", ouvinte.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(ouvinte, app).await.unwrap() });
    endereco
}

#[tokio::test]
async fn anuncios_importacao_e_rendimento_do_mercado_livre() {
    let a = Ambiente::novo().await;
    let admin = a.admin().await;
    // Sem conta conectada: erro claro
    let (s, b) = a.chamar([10, 0, 0, 5], "GET", "/api/meli/anuncios", Some(&admin), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b.contains("não conectada"), "{b}");

    let url = mercado_livre_simulado().await;
    // SAFETY: só este teste usa a variável (as rotas de leitura do ML).
    unsafe { std::env::set_var("STOCKBRIDGE_MELI_API", &url) };
    let token = stockbridge::services::meli::TokenMeli { access_token: "T".into(), refresh_token: Some("R".into()), expires_in: 21600 };
    stockbridge::services::meli::salvar_credenciais(&a.state.pool, &a.state.cofre, &token).await.unwrap();
    a.ok("POST", "/api/products", &admin, Some(json!({"name":"Camiseta","sku":"cam-01","cost":20,"price":50}))).await;

    let r = a.ok("GET", "/api/meli/anuncios", &admin, None).await;
    let lista = r["anuncios"].as_array().unwrap();
    assert_eq!(lista.len(), 3);
    let mlb1 = lista.iter().find(|x| x["id"] == "MLB1").unwrap();
    assert_eq!(mlb1["sku"], "CAM-01");
    assert_eq!(mlb1["visitas"], 900);
    assert_eq!(mlb1["produtoMesmoSku"], "Camiseta", "achou o produto pelo SKU, sem diferenciar maiúsculas");
    assert_eq!(lista.iter().find(|x| x["id"] == "MLB2").unwrap()["full"], true);

    let imp = a.ok("POST", "/api/meli/importar", &admin, Some(json!({"ids":["MLB1","MLB2","MLB3","MLB999"]}))).await;
    let res: Vec<&str> = imp.as_array().unwrap().iter().map(|x| x["resultado"].as_str().unwrap()).collect();
    assert_eq!(res, vec!["vinculado", "ignorado", "criado", "erro"], "{imp}");
    let de_novo = a.ok("POST", "/api/meli/importar", &admin, Some(json!({"ids":["MLB1"]}))).await;
    assert_eq!(de_novo[0]["resultado"], "ja_vinculado");
    let produtos = a.ok("GET", "/api/products", &admin, None).await;
    assert!(produtos.as_array().unwrap().iter().any(|p| p["sku"] == "MLB3" && p["name"] == "Boné sem SKU" && p["price"] == 30.0));

    // Importar NÃO envia estoque (senão zeraria o anúncio): nenhuma sincronização registrada.
    let vinculos = a.ok("GET", "/api/meli/links", &admin, None).await;
    assert_eq!(vinculos.as_array().unwrap().len(), 2);
    assert!(vinculos.as_array().unwrap().iter().all(|v| v["lastSyncAt"].is_null()));

    let rend = a.ok("GET", "/api/meli/rendimento?dias=30", &admin, None).await;
    assert_eq!(rend["pedidos"], 1);
    assert_eq!(rend["cancelados"], 1);
    assert_eq!(rend["faturamento"], 100.0);
    assert_eq!(rend["tarifas"], 16.0);
    assert_eq!(rend["lucroEstimado"], 44.0, "100 - 16 de tarifa - 2 x 20 de custo");
    assert_eq!(a.status("GET", "/api/meli/rendimento?dias=0", Some(&admin), None).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn telas_e_cabecalhos() {
    let a = Ambiente::novo().await;
    let res = a.app([10, 0, 0, 5]).oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(res.headers()["content-security-policy"].to_str().unwrap().contains("default-src 'self'"));
    assert_eq!(res.headers()["x-frame-options"], "DENY");
    assert_eq!(a.status("GET", "/api/nao-existe", None, None).await, StatusCode::NOT_FOUND);
    let res = a.app([10, 0, 0, 5]).oneshot(Request::get("/api/rede/ca.crt").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
