use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CustomerDbError {
    #[error("customer with this email already exists")]
    DuplicateEmail,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

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
) -> Result<Customer, CustomerDbError> {
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
    .map_err(|e| {
        if let sqlx::Error::Database(db) = &e {
            if db.code().as_deref() == Some("23505") {
                return CustomerDbError::DuplicateEmail;
            }
        }
        CustomerDbError::Database(e)
    })
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

#[derive(Debug, Clone)]
pub struct CustomerListFilters {
    pub business_id: Uuid,
    /// Case-insensitive substring match on email.
    pub email: Option<String>,
    /// Case-insensitive substring match on name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CustomerListPage {
    pub business_id: Uuid,
    pub items: Vec<Customer>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_next: bool,
    pub has_previous: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_offset: Option<i64>,
}

fn pagination_meta(total: i64, limit: i64, offset: i64) -> (bool, bool, Option<i64>, Option<i64>) {
    let has_previous = offset > 0;
    let has_next = offset + limit < total;
    let next_offset = has_next.then_some(offset + limit);
    let previous_offset = has_previous.then_some((offset - limit).max(0));
    (has_next, has_previous, next_offset, previous_offset)
}

fn ilike_contains(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

pub async fn list_page(
    pool: &PgPool,
    filters: &CustomerListFilters,
    limit: i64,
    offset: i64,
) -> anyhow::Result<CustomerListPage> {
    let email_pat = filters.email.as_deref().map(ilike_contains);
    let name_pat = filters.name.as_deref().map(ilike_contains);

    let total: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)::bigint
        FROM customers
        WHERE business_id = $1
          AND ($2::text IS NULL OR email ILIKE $2 ESCAPE '\')
          AND ($3::text IS NULL OR name ILIKE $3 ESCAPE '\')
        "#,
    )
    .bind(filters.business_id)
    .bind(email_pat.as_deref())
    .bind(name_pat.as_deref())
    .fetch_one(pool)
    .await?;

    let items = sqlx::query_as::<_, Customer>(
        r#"
        SELECT id, business_id, name, email, created_at
        FROM customers
        WHERE business_id = $1
          AND ($2::text IS NULL OR email ILIKE $2 ESCAPE '\')
          AND ($3::text IS NULL OR name ILIKE $3 ESCAPE '\')
        ORDER BY created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(filters.business_id)
    .bind(email_pat.as_deref())
    .bind(name_pat.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let (has_next, has_previous, next_offset, previous_offset) =
        pagination_meta(total.0, limit, offset);

    Ok(CustomerListPage {
        business_id: filters.business_id,
        items,
        total: total.0,
        limit,
        offset,
        has_next,
        has_previous,
        next_offset,
        previous_offset,
    })
}
