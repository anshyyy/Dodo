use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub business_id: Uuid,
    pub key_prefix: String,
}

pub fn hash_api_key(raw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(raw.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_api_key(raw: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(raw.as_bytes(), &parsed)
        .is_ok()
}

pub fn key_prefix(raw: &str) -> String {
    raw.chars().take(12).collect()
}

pub async fn resolve_business_by_raw_key(
    pool: &PgPool,
    raw_key: &str,
) -> anyhow::Result<Option<ApiKeyRecord>> {
    let rows = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT id, business_id, key_prefix, key_hash
        FROM api_keys
        WHERE revoked_at IS NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        if verify_api_key(raw_key, &row.key_hash) {
            return Ok(Some(ApiKeyRecord {
                id: row.id,
                business_id: row.business_id,
                key_prefix: row.key_prefix,
            }));
        }
    }
    Ok(None)
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    business_id: Uuid,
    key_prefix: String,
    key_hash: String,
}

pub async fn insert_key(
    pool: &PgPool,
    business_id: Uuid,
    raw_key: &str,
) -> anyhow::Result<ApiKeyRecord> {
    let id = Uuid::new_v4();
    let prefix = key_prefix(raw_key);
    let hash = hash_api_key(raw_key)?;
    sqlx::query(
        r#"
        INSERT INTO api_keys (id, business_id, key_prefix, key_hash)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(&prefix)
    .bind(&hash)
    .execute(pool)
    .await?;
    Ok(ApiKeyRecord {
        id,
        business_id,
        key_prefix: prefix,
    })
}

/// Demo tenant from migration seed (`Demo Business`); created with a random UUID if missing.
async fn demo_business_id(pool: &PgPool) -> anyhow::Result<Uuid> {
    const DEMO_NAME: &str = "Demo Business";
    if let Some((id,)) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM businesses WHERE name = $1 LIMIT 1",
    )
    .bind(DEMO_NAME)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }
    let (id,) = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO businesses (name) VALUES ($1) RETURNING id",
    )
    .bind(DEMO_NAME)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn ensure_demo_key(pool: &PgPool) -> anyhow::Result<()> {
    const DEMO_KEY: &str = "dodo_test_key_demo12345678901234567890";
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_keys")
        .fetch_one(pool)
        .await?;
    if count.0 == 0 {
        let business_id = demo_business_id(pool).await?;
        insert_key(pool, business_id, DEMO_KEY).await?;
        tracing::info!("Seeded demo API key (see README)");
    }
    Ok(())
}
