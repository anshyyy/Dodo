use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

/// Test-only counter for idempotency tests (PSP call count).
pub static PSP_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
pub struct ChargeRequest {
    pub card_token: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChargeResponse {
    Succeeded { psp_ref: Uuid },
    Failed { code: String },
}

pub fn router() -> Router {
    Router::new().route("/v1/charges", post(charge))
}

async fn charge(Json(req): Json<ChargeRequest>) -> Result<Json<ChargeResponse>, axum::http::StatusCode> {
    PSP_CALL_COUNT.fetch_add(1, Ordering::SeqCst);

    match req.card_token.as_str() {
        "tok_success" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse::Succeeded {
                psp_ref: Uuid::new_v4(),
            }))
        }
        "tok_insufficient_funds" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse::Failed {
                code: "insufficient_funds".into(),
            }))
        }
        "tok_card_declined" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse::Failed {
                code: "card_declined".into(),
            }))
        }
        "tok_timeout" => {
            sleep(Duration::from_secs(30)).await;
            Ok(Json(ChargeResponse::Succeeded {
                psp_ref: Uuid::new_v4(),
            }))
        }
        "tok_network_error" => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        _ => Ok(Json(ChargeResponse::Failed {
            code: "invalid_token".into(),
        })),
    }
}
