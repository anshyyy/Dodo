use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub business_id: Uuid,
    pub url: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    business_id: Uuid,
    url: &str,
    secret: &str,
) -> anyhow::Result<WebhookEndpoint> {
    let id = Uuid::new_v4();
    sqlx::query_as::<_, WebhookEndpoint>(
        r#"
        INSERT INTO webhook_endpoints (id, business_id, url, secret)
        VALUES ($1, $2, $3, $4)
        RETURNING id, business_id, url, enabled, created_at
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(url)
    .bind(secret)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn list(pool: &PgPool, business_id: Uuid) -> anyhow::Result<Vec<WebhookEndpoint>> {
    sqlx::query_as::<_, WebhookEndpoint>(
        r#"
        SELECT id, business_id, url, enabled, created_at
        FROM webhook_endpoints
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(business_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn enqueue_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO webhook_events (id, business_id, event_type, payload)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(event_type)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    tracing::info!(
        webhook_event_id = %id,
        business_id = %business_id,
        event_type = %event_type,
        "webhook event enqueued"
    );
    Ok(id)
}

pub async fn enqueue(
    pool: &PgPool,
    business_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO webhook_events (id, business_id, event_type, payload)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await?;
    tracing::info!(
        webhook_event_id = %id,
        business_id = %business_id,
        event_type = %event_type,
        "webhook event enqueued"
    );
    Ok(())
}

pub fn invoice_payload(event_type: &str, invoice_id: Uuid, state: &str, total_cents: i64) -> Value {
    json!({
        "type": event_type,
        "data": {
            "invoice_id": invoice_id,
            "state": state,
            "total_cents": total_cents,
        }
    })
}

#[derive(Debug)]
pub struct PendingWebhook {
    pub id: Uuid,
    pub business_id: Uuid,
    pub event_type: String,
    pub payload: Value,
    pub attempt_count: i32,
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    id: Uuid,
    business_id: Uuid,
    event_type: String,
    payload: Value,
    attempt_count: i32,
}

pub async fn fetch_due(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<PendingWebhook>> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, PendingRow>(
        r#"
        SELECT id, business_id, event_type, payload, attempt_count
        FROM webhook_events
        WHERE status = 'pending' AND next_attempt_at <= NOW()
        ORDER BY next_attempt_at
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| PendingWebhook {
            id: r.id,
            business_id: r.business_id,
            event_type: r.event_type,
            payload: r.payload,
            attempt_count: r.attempt_count,
        })
        .collect())
}

pub async fn mark_delivered(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE webhook_events SET status = 'delivered' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn schedule_retry(pool: &PgPool, id: Uuid, attempt_count: i32) -> anyhow::Result<()> {
    const MAX_ATTEMPTS: i32 = 5;
    const BACKOFF_SECS: [i64; 5] = [60, 300, 1800, 7200, 86400];
    if attempt_count >= MAX_ATTEMPTS {
        sqlx::query(
            "UPDATE webhook_events SET status = 'dead', attempt_count = $1 WHERE id = $2",
        )
        .bind(attempt_count)
        .bind(id)
        .execute(pool)
        .await?;
        tracing::error!(
            webhook_event_id = %id,
            attempt_count = attempt_count,
            "webhook event marked dead after max delivery attempts"
        );
    } else {
        let delay = BACKOFF_SECS[(attempt_count - 1).max(0) as usize];
        sqlx::query(
            r#"
            UPDATE webhook_events
            SET attempt_count = $1, next_attempt_at = NOW() + ($2 || ' seconds')::interval
            WHERE id = $3
            "#,
        )
        .bind(attempt_count)
        .bind(delay)
        .bind(id)
        .execute(pool)
        .await?;
        tracing::info!(
            webhook_event_id = %id,
            attempt_count = attempt_count,
            retry_after_secs = delay,
            "webhook event scheduled for retry"
        );
    }
    Ok(())
}

pub async fn endpoints_for_business(
    pool: &PgPool,
    business_id: Uuid,
) -> anyhow::Result<Vec<(Uuid, String, String)>> {
    let rows = sqlx::query_as::<_, EndpointRow>(
        r#"
        SELECT id, url, secret FROM webhook_endpoints
        WHERE business_id = $1 AND enabled = TRUE
        "#,
    )
    .bind(business_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, r.url, r.secret))
        .collect())
}

#[derive(sqlx::FromRow)]
struct EndpointRow {
    id: Uuid,
    url: String,
    secret: String,
}
