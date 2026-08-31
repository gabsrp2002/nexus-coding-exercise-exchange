use crate::engine::matcher::MatchingEngine;
use crate::feed::AccountId;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tracing::info;

#[derive(Deserialize)]
struct GetTradesQuery {
    symbol: Option<String>,
    account: Option<AccountId>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct GetBookQuery {
    symbol: Option<String>,
}

#[derive(Deserialize)]
struct GetAccountQuery {
    id: AccountId,
}

async fn get_trades(
    State(engine): State<Arc<Mutex<MatchingEngine>>>,
    Query(params): Query<GetTradesQuery>,
) -> Json<Value> {
    let engine = engine.lock().unwrap();
    let trades = engine.get_trades(params.symbol.as_deref(), params.account, params.limit);
    Json(json!(trades))
}

async fn get_book(
    State(engine): State<Arc<Mutex<MatchingEngine>>>,
    Query(params): Query<GetBookQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let engine = engine.lock().unwrap();
    if let Some(symbol) = params.symbol {
        let sym_upper = symbol.to_uppercase();
        match engine.get_book(&sym_upper) {
            Some(book) => Ok(Json(json!(book))),
            None => Err((
                StatusCode::NOT_FOUND,
                format!("Symbol '{}' not found", symbol),
            )),
        }
    } else {
        Ok(Json(json!(engine.get_all_books())))
    }
}

async fn get_accounts(State(engine): State<Arc<Mutex<MatchingEngine>>>) -> Result<Json<Value>, (StatusCode, String)> {
    let engine = engine.lock().unwrap();
    match engine.get_all_accounts() {
        Ok(accounts) => Ok(Json(json!(accounts))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e))),
    }
}

async fn get_account(
    State(engine): State<Arc<Mutex<MatchingEngine>>>,
    Query(params): Query<GetAccountQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let engine = engine.lock().unwrap();
    match engine.get_account_portfolio(params.id) {
        Ok(portfolio) => Ok(Json(json!(portfolio))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e))),
    }
}

async fn get_stats(State(engine): State<Arc<Mutex<MatchingEngine>>>) -> Json<Value> {
    let engine = engine.lock().unwrap();
    Json(json!(engine.get_stats()))
}

async fn health_check() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "matching-engine" }))
}

/// Starts the matching engine HTTP query server.
pub async fn run_server(engine: Arc<Mutex<MatchingEngine>>, port: u16) {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/trades", get(get_trades))
        .route("/book", get(get_book))
        .route("/accounts", get(get_accounts))
        .route("/account", get(get_account))
        .route("/stats", get(get_stats))
        .with_state(engine);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Matching Engine API server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
