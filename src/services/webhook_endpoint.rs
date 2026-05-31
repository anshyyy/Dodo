use sqlx::PgPool;
use uuid::Uuid;

use crate::repository::webhook;

#[derive(Debug, Clone, serde::Serialize)]
pub struct WebhookEndpointView {
    pub id: Uuid,
    pub url: String,
    pub enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookEndpointError {
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct WebhookEndpointService {
    pool: PgPool,
}

impl WebhookEndpointService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        business_id: Uuid,
        url: &str,
        secret: &str,
    ) -> Result<WebhookEndpointView, WebhookEndpointError> {
        let ep = webhook::create(&self.pool, business_id, url, secret)
            .await
            .map_err(WebhookEndpointError::Internal)?;
        Ok(WebhookEndpointView {
            id: ep.id,
            url: ep.url,
            enabled: ep.enabled,
        })
    }

    pub async fn list(&self, business_id: Uuid) -> Result<Vec<WebhookEndpointView>, WebhookEndpointError> {
        let eps = webhook::list(&self.pool, business_id)
            .await
            .map_err(WebhookEndpointError::Internal)?;
        Ok(eps
            .into_iter()
            .map(|e| WebhookEndpointView {
                id: e.id,
                url: e.url,
                enabled: e.enabled,
            })
            .collect())
    }
}
