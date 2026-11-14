//! StockBridge: controle de estoque de depósito integrado ao Mercado Livre.
//!
//! Um único executável. Em cada rede, UM computador roda como **servidor** (guarda o banco SQLite
//! e atende as telas por HTTPS); os demais rodam como **terminal** (acham o servidor sozinhos na
//! rede e abrem o navegador nele). Ver `servidor.rs` e `terminal.rs`.

pub mod app;
pub mod backup;
pub mod config;
pub mod cripto;
pub mod db;
pub mod handlers;
pub mod rede;
pub mod security;
pub mod servidor;
pub mod services;
pub mod terminal;
pub mod web;
