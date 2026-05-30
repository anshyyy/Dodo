use crate::domain::{InvoiceState, compute_line_total, sum_line_totals};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LineItem {
    pub id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
    pub line_total_cents: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Invoice {
    pub id: Uuid,
    pub business_id: Uuid,
    pub customer_id: Uuid,
    pub state: InvoiceState,
    pub due_date: NaiveDate,
    pub total_cents: i64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub line_items: Vec<LineItem>,
}

#[derive(Debug, Clone)]
pub struct NewLineItem {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(sqlx::FromRow)]
struct InvoiceHeader {
    id: Uuid,
    business_id: Uuid,
    customer_id: Uuid,
    state: InvoiceState,
    due_date: NaiveDate,
    total_cents: i64,
    currency: String,
    created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    business_id: Uuid,
    customer_id: Uuid,
    due_date: NaiveDate,
    items: &[NewLineItem],
    initial_state: InvoiceState,
) -> anyhow::Result<Invoice> {
    let mut tx = pool.begin().await?;
    let invoice = create_in_tx(&mut tx, business_id, customer_id, due_date, items, initial_state)
        .await?;
    tx.commit().await?;
    Ok(invoice)
}

pub async fn create_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    customer_id: Uuid,
    due_date: NaiveDate,
    items: &[NewLineItem],
    initial_state: InvoiceState,
) -> anyhow::Result<Invoice> {
    if items.is_empty() {
        anyhow::bail!("invoice must have at least one line item");
    }

    let line_totals: Vec<i64> = items
        .iter()
        .map(|i| compute_line_total(i.quantity, i.unit_amount_cents))
        .collect::<Result<Vec<_>, _>>()?;
    let total_cents = sum_line_totals(line_totals.iter().copied())?;

    let invoice_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO invoices (id, business_id, customer_id, state, due_date, total_cents)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(invoice_id)
    .bind(business_id)
    .bind(customer_id)
    .bind(initial_state)
    .bind(due_date)
    .bind(total_cents)
    .execute(&mut **tx)
    .await?;

    let mut line_items = Vec::with_capacity(items.len());
    for (item, &line_total) in items.iter().zip(line_totals.iter()) {
        let line_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO invoice_line_items (id, invoice_id, description, quantity, unit_amount_cents, line_total_cents)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(line_id)
        .bind(invoice_id)
        .bind(&item.description)
        .bind(item.quantity)
        .bind(item.unit_amount_cents)
        .bind(line_total)
        .execute(&mut **tx)
        .await?;
        line_items.push(LineItem {
            id: line_id,
            description: item.description.clone(),
            quantity: item.quantity,
            unit_amount_cents: item.unit_amount_cents,
            line_total_cents: line_total,
        });
    }

    let created_at: DateTime<Utc> = sqlx::query_scalar("SELECT created_at FROM invoices WHERE id = $1")
        .bind(invoice_id)
        .fetch_one(&mut **tx)
        .await?;

    Ok(Invoice {
        id: invoice_id,
        business_id,
        customer_id,
        state: initial_state,
        due_date,
        total_cents,
        currency: "USD".into(),
        created_at,
        line_items,
    })
}

pub async fn get(pool: &PgPool, business_id: Uuid, id: Uuid) -> anyhow::Result<Option<Invoice>> {
    let header = sqlx::query_as::<_, InvoiceHeader>(
        r#"
        SELECT id, business_id, customer_id, state, due_date, total_cents, currency, created_at
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(pool)
    .await?;

    let Some(h) = header else {
        return Ok(None);
    };

    let lines = fetch_line_items(pool, h.id).await?;
    Ok(Some(header_to_invoice(h, lines)))
}

pub async fn list(
    pool: &PgPool,
    business_id: Uuid,
    state_filter: Option<InvoiceState>,
) -> anyhow::Result<Vec<Invoice>> {
    let headers = if let Some(state) = state_filter {
        sqlx::query_as::<_, InvoiceHeader>(
            r#"
            SELECT id, business_id, customer_id, state, due_date, total_cents, currency, created_at
            FROM invoices
            WHERE business_id = $1 AND state = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(business_id)
        .bind(state)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, InvoiceHeader>(
            r#"
            SELECT id, business_id, customer_id, state, due_date, total_cents, currency, created_at
            FROM invoices
            WHERE business_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(business_id)
        .fetch_all(pool)
        .await?
    };

    let mut out = Vec::new();
    for h in headers {
        let lines = fetch_line_items(pool, h.id).await?;
        out.push(header_to_invoice(h, lines));
    }
    Ok(out)
}

fn header_to_invoice(h: InvoiceHeader, line_items: Vec<LineItem>) -> Invoice {
    Invoice {
        id: h.id,
        business_id: h.business_id,
        customer_id: h.customer_id,
        state: h.state,
        due_date: h.due_date,
        total_cents: h.total_cents,
        currency: h.currency,
        created_at: h.created_at,
        line_items,
    }
}

async fn fetch_line_items(pool: &PgPool, invoice_id: Uuid) -> anyhow::Result<Vec<LineItem>> {
    let rows = sqlx::query_as::<_, LineItemRow>(
        r#"
        SELECT id, description, quantity, unit_amount_cents, line_total_cents
        FROM invoice_line_items
        WHERE invoice_id = $1
        "#,
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| LineItem {
            id: r.id,
            description: r.description,
            quantity: r.quantity,
            unit_amount_cents: r.unit_amount_cents,
            line_total_cents: r.line_total_cents,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct LineItemRow {
    id: Uuid,
    description: String,
    quantity: i32,
    unit_amount_cents: i64,
    line_total_cents: i64,
}

pub async fn lock_for_update(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    invoice_id: Uuid,
) -> anyhow::Result<Option<(Uuid, InvoiceState, i64)>> {
    let row = sqlx::query_as::<_, LockRow>(
        r#"
        SELECT id, state, total_cents
        FROM invoices
        WHERE id = $1 AND business_id = $2
        FOR UPDATE
        "#,
    )
    .bind(invoice_id)
    .bind(business_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| (r.id, r.state, r.total_cents)))
}

#[derive(sqlx::FromRow)]
struct LockRow {
    id: Uuid,
    state: InvoiceState,
    total_cents: i64,
}

pub async fn transition_state(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
    expected: InvoiceState,
    next: InvoiceState,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE invoices
        SET state = $1, updated_at = NOW()
        WHERE id = $2 AND state = $3
        "#,
    )
    .bind(next)
    .bind(invoice_id)
    .bind(expected)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_state(
    pool: &PgPool,
    business_id: Uuid,
    invoice_id: Uuid,
) -> anyhow::Result<Option<InvoiceState>> {
    let row: Option<(InvoiceState,)> = sqlx::query_as(
        r#"
        SELECT state FROM invoices WHERE id = $1 AND business_id = $2
        "#,
    )
    .bind(invoice_id)
    .bind(business_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}
