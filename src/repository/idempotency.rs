use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StoredIdempotency {
    pub response_status: i32,
    pub response_body: Value,
    pub request_hash: String,
}

pub fn request_hash(method: &str, path: &str, body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(method.as_bytes());
    hasher.update(b"|");
    hasher.update(path.as_bytes());
    hasher.update(b"|");
    hasher.update(body.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn get(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
) -> anyhow::Result<Option<StoredIdempotency>> {
    let row = sqlx::query_as::<_, IdemRow>(
        r#"
        SELECT request_hash, response_status, response_body
        FROM idempotency_keys
        WHERE business_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(business_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| StoredIdempotency {
        request_hash: r.request_hash,
        response_status: r.response_status,
        response_body: r.response_body,
    }))
}

#[derive(sqlx::FromRow)]
struct IdemRow {
    request_hash: String,
    response_status: i32,
    response_body: Value,
}

pub async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    key: &str,
    req_hash: &str,
    response_status: i32,
    response_body: &Value,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO idempotency_keys (id, business_id, idempotency_key, request_hash, response_status, response_body)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(key)
    .bind(req_hash)
    .bind(response_status)
    .bind(response_body)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
