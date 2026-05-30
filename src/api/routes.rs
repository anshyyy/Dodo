use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::error::{self, ApiError};
use crate::api::extractors::AuthBusiness;
use crate::api::AppState;
use crate::domain::InvoiceState;
use crate::repository::{customer, invoice, webhook};
use crate::services::payment::{PayError, PayOutcome};

/// HTTP path prefix for versioned REST API (used in idempotency hashes).
pub const API_V1_PREFIX: &str = "/api/v1";

pub fn v1_routes(state: AppState) -> Router {
    Router::new()
        .route("/customers", post(create_customer).get(list_customers))
        .route("/customers/{id}", get(get_customer))
        .route("/invoices", post(create_invoice).get(list_invoices))
        .route("/invoices/{id}", get(get_invoice))
        .route("/invoices/{id}/pay", post(pay_invoice))
        .route(
            "/webhook_endpoints",
            post(create_webhook).get(list_webhooks),
        )
        .with_state(state)
}

#[derive(Deserialize)]
struct CreateCustomerRequest {
    name: String,
    email: String,
}

async fn create_customer(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Json(body): Json<CreateCustomerRequest>,
) -> Result<Json<customer::Customer>, ApiError> {
    let c = customer::create(&state.pool, auth.business_id, &body.name, &body.email)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate") {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "customer_exists",
                    "customer with this email already exists",
                )
            } else {
                error::internal(e.to_string())
            }
        })?;
    Ok(Json(c))
}

async fn list_customers(
    auth: AuthBusiness,
    State(state): State<AppState>,
) -> Result<Json<Vec<customer::Customer>>, ApiError> {
    let list = customer::list(&state.pool, auth.business_id)
        .await
        .map_err(|e| error::internal(e.to_string()))?;
    Ok(Json(list))
}

async fn get_customer(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<customer::Customer>, ApiError> {
    let c = customer::get(&state.pool, auth.business_id, id)
        .await
        .map_err(|e| error::internal(e.to_string()))?
        .ok_or_else(|| error::not_found("customer not found"))?;
    Ok(Json(c))
}

#[derive(Deserialize)]
struct LineItemInput {
    description: String,
    quantity: i32,
    unit_amount_cents: i64,
}

#[derive(Deserialize)]
struct CreateInvoiceRequest {
    customer_id: Uuid,
    due_date: NaiveDate,
    line_items: Vec<LineItemInput>,
    #[serde(default)]
    state: Option<String>,
}

async fn create_invoice(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Json(body): Json<CreateInvoiceRequest>,
) -> Result<Json<invoice::Invoice>, ApiError> {
    let cust = customer::get(&state.pool, auth.business_id, body.customer_id)
        .await
        .map_err(|e| error::internal(e.to_string()))?
        .ok_or_else(|| error::not_found("customer not found"))?;

    let initial_state = match body.state.as_deref() {
        Some("draft") => InvoiceState::Draft,
        None | Some("open") => InvoiceState::Open,
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "state must be open or draft",
            ))
        }
    };

    let items: Vec<invoice::NewLineItem> = body
        .line_items
        .into_iter()
        .map(|i| invoice::NewLineItem {
            description: i.description,
            quantity: i.quantity,
            unit_amount_cents: i.unit_amount_cents,
        })
        .collect();

    let inv = invoice::create(
        &state.pool,
        auth.business_id,
        cust.id,
        body.due_date,
        &items,
        initial_state,
    )
    .await
    .map_err(|e| error::internal(e.to_string()))?;

    let state_str = serde_json::to_value(inv.state)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "open".into());
    let payload =
        webhook::invoice_payload("invoice.created", inv.id, &state_str, inv.total_cents);
    webhook::enqueue(&state.pool, auth.business_id, "invoice.created", payload)
        .await
        .map_err(|e| error::internal(e.to_string()))?;

    Ok(Json(inv))
}

#[derive(Deserialize)]
struct ListInvoicesQuery {
    state: Option<String>,
}

async fn list_invoices(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Query(q): Query<ListInvoicesQuery>,
) -> Result<Json<Vec<invoice::Invoice>>, ApiError> {
    let filter = q.state.as_deref().map(parse_state).transpose()?;
    let list = invoice::list(&state.pool, auth.business_id, filter)
        .await
        .map_err(|e| error::internal(e.to_string()))?;
    Ok(Json(list))
}

async fn get_invoice(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<invoice::Invoice>, ApiError> {
    let inv = invoice::get(&state.pool, auth.business_id, id)
        .await
        .map_err(|e| error::internal(e.to_string()))?
        .ok_or_else(|| error::not_found("invoice not found"))?;
    Ok(Json(inv))
}

#[derive(Deserialize, Serialize)]
struct PayRequest {
    card_token: String,
}

async fn pay_invoice(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<PayRequest>,
) -> Result<Response, ApiError> {
    let idem = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "missing_idempotency_key",
                "Idempotency-Key header is required",
            )
        })?;

    let path = format!("{API_V1_PREFIX}/invoices/{id}/pay");
    let raw_body = serde_json::to_string(&body).map_err(|e| error::internal(e.to_string()))?;

    let result = state
        .payments
        .pay(
            auth.business_id,
            id,
            idem,
            &body.card_token,
            &path,
            &raw_body,
        )
        .await
        .map_err(|e| error::internal(e.to_string()))?;

    match result {
        Ok(PayOutcome::Replay { status, body }) => Ok(error::json_status(
            StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK),
            body,
        )),
        Ok(PayOutcome::Completed { status, body }) => Ok(error::json_status(
            StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
            body,
        )),
        Err(PayError::InvoiceNotFound) => Err(error::not_found("invoice not found")),
        Err(PayError::AlreadyPaid) => Err(error::conflict(
            "invoice_already_paid",
            "invoice is already paid",
        )),
        Err(PayError::InvalidState(s)) => Err(error::conflict(
            "invoice_invalid_state",
            format!("cannot pay invoice in state {:?}", s),
        )),
        Err(PayError::IdempotencyMismatch) => Err(error::unprocessable(
            "idempotency_mismatch",
            "idempotency key reused with different request body",
        )),
        Err(PayError::ConcurrentPay) => Err(error::conflict(
            "payment_in_progress",
            "another payment is already in progress for this invoice",
        )),
    }
}

#[derive(Deserialize)]
struct CreateWebhookRequest {
    url: String,
    secret: String,
}

#[derive(Serialize)]
struct WebhookEndpointResponse {
    id: Uuid,
    url: String,
    enabled: bool,
}

async fn create_webhook(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Json(body): Json<CreateWebhookRequest>,
) -> Result<Json<WebhookEndpointResponse>, ApiError> {
    let ep = webhook::create(&state.pool, auth.business_id, &body.url, &body.secret)
        .await
        .map_err(|e| error::internal(e.to_string()))?;
    Ok(Json(WebhookEndpointResponse {
        id: ep.id,
        url: ep.url,
        enabled: ep.enabled,
    }))
}

async fn list_webhooks(
    auth: AuthBusiness,
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookEndpointResponse>>, ApiError> {
    let eps = webhook::list(&state.pool, auth.business_id)
        .await
        .map_err(|e| error::internal(e.to_string()))?;
    Ok(Json(
        eps.into_iter()
            .map(|e| WebhookEndpointResponse {
                id: e.id,
                url: e.url,
                enabled: e.enabled,
            })
            .collect(),
    ))
}

fn parse_state(s: &str) -> Result<InvoiceState, ApiError> {
    match s {
        "draft" => Ok(InvoiceState::Draft),
        "open" => Ok(InvoiceState::Open),
        "paid" => Ok(InvoiceState::Paid),
        "void" => Ok(InvoiceState::Void),
        "uncollectible" => Ok(InvoiceState::Uncollectible),
        _ => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "unknown state filter",
        )),
    }
}
