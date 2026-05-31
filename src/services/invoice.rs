use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::InvoiceState;
use crate::repository::{customer, invoice, webhook};

#[derive(Debug, Clone)]
pub struct NewLineItemInput {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum InvoiceError {
    #[error("customer not found")]
    CustomerNotFound,
    #[error("state must be open or draft")]
    InvalidCreateState,
    #[error("unknown state filter")]
    InvalidListStateFilter,
    #[error("invoice not found")]
    NotFound,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct InvoiceService {
    pool: PgPool,
}

impl InvoiceService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        business_id: Uuid,
        customer_id: Uuid,
        due_date: NaiveDate,
        line_items: Vec<NewLineItemInput>,
        initial_state: Option<&str>,
    ) -> Result<invoice::Invoice, InvoiceError> {
        let cust = customer::get(&self.pool, business_id, customer_id)
            .await
            .map_err(|e| InvoiceError::Internal(e.into()))?
            .ok_or(InvoiceError::CustomerNotFound)?;

        let initial_state = InvoiceState::from_create_option(initial_state)
            .map_err(|_| InvoiceError::InvalidCreateState)?;

        let items: Vec<invoice::NewLineItem> = line_items
            .into_iter()
            .map(|i| invoice::NewLineItem {
                description: i.description,
                quantity: i.quantity,
                unit_amount_cents: i.unit_amount_cents,
            })
            .collect();

        let inv = invoice::create(
            &self.pool,
            business_id,
            cust.id,
            due_date,
            &items,
            initial_state,
        )
        .await
        .map_err(InvoiceError::Internal)?;

        let state_str = serde_json::to_value(inv.state)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "open".into());
        let payload =
            webhook::invoice_payload("invoice.created", inv.id, &state_str, inv.total_cents);
        webhook::enqueue(&self.pool, business_id, "invoice.created", payload)
            .await
            .map_err(InvoiceError::Internal)?;

        Ok(inv)
    }

    pub async fn list(
        &self,
        business_id: Uuid,
        state_filter: Option<&str>,
    ) -> Result<Vec<invoice::Invoice>, InvoiceError> {
        let filter = match state_filter {
            None => None,
            Some(s) => Some(
                InvoiceState::from_filter_str(s).map_err(|_| InvoiceError::InvalidListStateFilter)?,
            ),
        };
        invoice::list(&self.pool, business_id, filter)
            .await
            .map_err(InvoiceError::Internal)
    }

    pub async fn get(
        &self,
        business_id: Uuid,
        id: Uuid,
    ) -> Result<invoice::Invoice, InvoiceError> {
        invoice::get(&self.pool, business_id, id)
            .await
            .map_err(InvoiceError::Internal)?
            .ok_or(InvoiceError::NotFound)
    }
}
