use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, sqlx::Type)]
#[sqlx(type_name = "payment_attempt_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentAttemptStatus {
    Processing,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub status: PaymentAttemptStatus,
    pub failure_code: Option<String>,
    pub psp_ref: Option<Uuid>,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
}

pub fn fingerprint_card_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn insert_processing(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
    idempotency_key: &str,
    card_token: &str,
) -> Result<PaymentAttempt, sqlx::Error> {
    let id = Uuid::new_v4();
    let fp = fingerprint_card_token(card_token);
    sqlx::query(
        r#"
        INSERT INTO payment_attempts (id, invoice_id, status, idempotency_key, card_token_fingerprint)
        VALUES ($1, $2, 'processing', $3, $4)
        "#,
    )
    .bind(id)
    .bind(invoice_id)
    .bind(idempotency_key)
    .bind(fp)
    .execute(&mut **tx)
    .await?;

    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM payment_attempts WHERE id = $1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;

    Ok(PaymentAttempt {
        id,
        invoice_id,
        status: PaymentAttemptStatus::Processing,
        failure_code: None,
        psp_ref: None,
        idempotency_key: idempotency_key.to_string(),
        created_at,
    })
}

pub async fn mark_succeeded(pool: &PgPool, attempt_id: Uuid, psp_ref: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE payment_attempts
        SET status = 'succeeded', psp_ref = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(psp_ref)
    .bind(attempt_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_failed(
    pool: &PgPool,
    attempt_id: Uuid,
    failure_code: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE payment_attempts
        SET status = 'failed', failure_code = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(failure_code)
    .bind(attempt_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct AttemptRow {
    id: Uuid,
    invoice_id: Uuid,
    status: PaymentAttemptStatus,
    failure_code: Option<String>,
    psp_ref: Option<Uuid>,
    idempotency_key: String,
    created_at: DateTime<Utc>,
}

fn row_to_attempt(r: AttemptRow) -> PaymentAttempt {
    PaymentAttempt {
        id: r.id,
        invoice_id: r.invoice_id,
        status: r.status,
        failure_code: r.failure_code,
        psp_ref: r.psp_ref,
        idempotency_key: r.idempotency_key,
        created_at: r.created_at,
    }
}

pub async fn get(pool: &PgPool, attempt_id: Uuid) -> anyhow::Result<Option<PaymentAttempt>> {
    let row = sqlx::query_as::<_, AttemptRow>(
        r#"
        SELECT id, invoice_id, status, failure_code, psp_ref, idempotency_key, created_at
        FROM payment_attempts
        WHERE id = $1
        "#,
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_attempt))
}

pub async fn find_by_idempotency(
    pool: &PgPool,
    invoice_id: Uuid,
    idempotency_key: &str,
) -> anyhow::Result<Option<PaymentAttempt>> {
    let row = sqlx::query_as::<_, AttemptRow>(
        r#"
        SELECT id, invoice_id, status, failure_code, psp_ref, idempotency_key, created_at
        FROM payment_attempts
        WHERE invoice_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(invoice_id)
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_attempt))
}
