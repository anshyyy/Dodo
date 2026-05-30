pub mod docs;
pub mod error;
pub mod extractors;
pub mod routes;

use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;

use crate::config::Config;
use crate::services::{payment::PaymentService, psp_client::PspClient};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub payments: Arc<PaymentService>,
}

pub fn build_router(state: AppState, _config: &Config) -> Router {
    let psp_router = crate::mock_psp::router();

    Router::new()
        .merge(docs::routes())
        .nest("/api/v1", routes::v1_routes(state))
        .nest("/mock-psp", psp_router)
        .route("/", axum::routing::get(root))
        .route("/health", axum::routing::get(health))
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "service": "dodo-invoice-service",
        "health": "/health",
        "api_v1": "/api/v1",
        "docs": "/docs",
        "mock_psp": "/mock-psp/v1/charges",
    }))
}

async fn health() -> &'static str {
    "ok"
}

pub fn build_state(pool: PgPool, config: Config) -> AppState {
    let psp = Arc::new(PspClient::new(
        config.mock_psp_base_url.clone(),
        config.psp_http_timeout_secs,
    ));
    let payments = Arc::new(PaymentService::new(pool.clone(), psp, config));
    AppState { pool, payments }
}
