use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::error::{self, ApiError};
use crate::api::extractors::AuthBusiness;
use crate::api::AppState;
use crate::repository::customer::{self, CustomerListFilters, CustomerListPage};
use crate::services::invoice::NewLineItemInput;
use crate::services::payment::{PayError, PayOutcome};
use crate::services::webhook_endpoint::WebhookEndpointView;

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
    let c = state
        .customers
        .create(auth.business_id, &body.name, &body.email)
        .await?;
    Ok(Json(c))
}

fn default_customer_list_limit() -> i64 {
    crate::services::customer::DEFAULT_CUSTOMER_LIST_LIMIT
}

#[derive(Deserialize)]
struct ListCustomersQuery {
    business_id: Option<Uuid>,
    #[serde(default = "default_customer_list_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    email: Option<String>,
    name: Option<String>,
}

async fn list_customers(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Query(q): Query<ListCustomersQuery>,
) -> Result<Json<CustomerListPage>, ApiError> {
    let business_id = q.business_id.unwrap_or(auth.business_id);
    let filters = CustomerListFilters {
        business_id,
        email: q.email.filter(|s| !s.is_empty()),
        name: q.name.filter(|s| !s.is_empty()),
    };
    let page = state
        .customers
        .list(auth.business_id, filters, q.limit, q.offset)
        .await?;
    Ok(Json(page))
}

async fn get_customer(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<customer::Customer>, ApiError> {
    let c = state.customers.get(auth.business_id, id).await?;
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
) -> Result<Json<crate::repository::invoice::Invoice>, ApiError> {
    let items: Vec<NewLineItemInput> = body
        .line_items
        .into_iter()
        .map(|i| NewLineItemInput {
            description: i.description,
            quantity: i.quantity,
            unit_amount_cents: i.unit_amount_cents,
        })
        .collect();

    let inv = state
        .invoices
        .create(
            auth.business_id,
            body.customer_id,
            body.due_date,
            items,
            body.state.as_deref(),
        )
        .await?;

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
) -> Result<Json<Vec<crate::repository::invoice::Invoice>>, ApiError> {
    let list = state
        .invoices
        .list(auth.business_id, q.state.as_deref())
        .await?;
    Ok(Json(list))
}

async fn get_invoice(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::repository::invoice::Invoice>, ApiError> {
    let inv = state.invoices.get(auth.business_id, id).await?;
    Ok(Json(inv))
}

#[derive(Deserialize, serde::Serialize)]
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

async fn create_webhook(
    auth: AuthBusiness,
    State(state): State<AppState>,
    Json(body): Json<CreateWebhookRequest>,
) -> Result<Json<WebhookEndpointView>, ApiError> {
    let ep = state
        .webhooks
        .create(auth.business_id, &body.url, &body.secret)
        .await?;
    Ok(Json(ep))
}

async fn list_webhooks(
    auth: AuthBusiness,
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookEndpointView>>, ApiError> {
    let eps = state.webhooks.list(auth.business_id).await?;
    Ok(Json(eps))
}
