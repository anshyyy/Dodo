use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Customer {
    pub id: Uuid,
    pub business_id: Uuid,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    business_id: Uuid,
    name: &str,
    email: &str,
) -> anyhow::Result<Customer> {
    let id = Uuid::new_v4();
    sqlx::query_as::<_, Customer>(
        r#"
        INSERT INTO customers (id, business_id, name, email)
        VALUES ($1, $2, $3, $4)
        RETURNING id, business_id, name, email, created_at
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(name)
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn get(pool: &PgPool, business_id: Uuid, id: Uuid) -> anyhow::Result<Option<Customer>> {
    sqlx::query_as::<_, Customer>(
        r#"
        SELECT id, business_id, name, email, created_at
        FROM customers
        WHERE id = $1 AND business_id = $2
        "#,
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn list(pool: &PgPool, business_id: Uuid) -> anyhow::Result<Vec<Customer>> {
    sqlx::query_as::<_, Customer>(
        r#"
        SELECT id, business_id, name, email, created_at
        FROM customers
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(business_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}
