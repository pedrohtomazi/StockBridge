-- StockBridge: estrutura do banco (SQLite).
-- Regra de ouro: NUNCA edite uma migration que já rodou em algum computador.
-- Para mudar a estrutura, crie um arquivo novo, ex.: 0002_nome_da_mudanca.sql
--
-- Datas: texto ISO 8601 em UTC (ex.: 2026-11-13T18:30:00Z).
-- Dinheiro: inteiro em CENTAVOS (evita erro de arredondamento de número com vírgula).
-- STRICT: o SQLite recusa gravar um tipo errado na coluna.

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('admin', 'operador')),
    active        INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    -- Aumenta a cada troca de senha, papel ou desativação: derruba os logins antigos.
    token_version INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;

CREATE TABLE warehouses (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL
) STRICT;

CREATE TABLE products (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    sku         TEXT NOT NULL UNIQUE,
    cost_cents  INTEGER NOT NULL CHECK (cost_cents >= 0),
    price_cents INTEGER NOT NULL CHECK (price_cents >= 0)
) STRICT;

CREATE TABLE locations (
    id           TEXT PRIMARY KEY,
    -- Antes não havia chave estrangeira: dava para criar estante em depósito inexistente.
    warehouse_id TEXT NOT NULL REFERENCES warehouses(id),
    sector       TEXT NOT NULL,
    rack         TEXT NOT NULL,
    level        TEXT NOT NULL,
    position     TEXT,
    full_code    TEXT NOT NULL UNIQUE,
    qr_token     TEXT NOT NULL UNIQUE,
    max_capacity INTEGER NOT NULL CHECK (max_capacity > 0),
    is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
) STRICT;
CREATE INDEX locations_warehouse ON locations(warehouse_id);

CREATE TABLE stocks (
    product_id  TEXT NOT NULL REFERENCES products(id),
    location_id TEXT NOT NULL REFERENCES locations(id),
    quantity    INTEGER NOT NULL CHECK (quantity >= 0),
    PRIMARY KEY (product_id, location_id)
) STRICT;
CREATE INDEX stocks_location ON stocks(location_id);

CREATE TABLE movements (
    id               TEXT PRIMARY KEY,
    product_id       TEXT NOT NULL REFERENCES products(id),
    location_id      TEXT NOT NULL REFERENCES locations(id),
    user_id          TEXT NOT NULL REFERENCES users(id),
    type             TEXT NOT NULL CHECK (type IN ('IN', 'OUT')),
    quantity         INTEGER NOT NULL CHECK (quantity > 0),
    -- Custo e preço DA ÉPOCA: mudar o preço do produto não reescreve o histórico.
    unit_cost_cents  INTEGER NOT NULL,
    unit_price_cents INTEGER NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
) STRICT;
CREATE INDEX movements_created ON movements(created_at);
CREATE INDEX movements_product ON movements(product_id);
CREATE INDEX movements_location ON movements(location_id);

-- Uma única conta do Mercado Livre por instalação. Tokens guardados CIFRADOS.
CREATE TABLE meli_credentials (
    id                INTEGER PRIMARY KEY CHECK (id = 1),
    access_token_enc  TEXT NOT NULL,
    refresh_token_enc TEXT NOT NULL,
    expires_at        TEXT NOT NULL,
    needs_reconnect   INTEGER NOT NULL DEFAULT 0 CHECK (needs_reconnect IN (0, 1)),
    last_error        TEXT,
    updated_at        TEXT NOT NULL
) STRICT;

CREATE TABLE meli_links (
    id              TEXT PRIMARY KEY,
    product_id      TEXT NOT NULL REFERENCES products(id),
    meli_item_id    TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- Resultado da última sincronização (antes só aparecia no terminal).
    last_sync_at    TEXT,
    last_sync_ok    INTEGER CHECK (last_sync_ok IN (0, 1)),
    last_sync_qty   INTEGER,
    last_sync_error TEXT
) STRICT;
CREATE INDEX meli_links_product ON meli_links(product_id);

-- Configurações editadas pelas telas (credenciais do app do ML, pasta de backup...).
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
