use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PspChargeRequest<'a> {
    pub card_token: &'a str,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PspChargeResponse {
    Succeeded { psp_ref: Uuid },
    Failed { code: String },
}

#[derive(Debug, thiserror::Error)]
pub enum PspError {
    #[error("network error")]
    Network,
    #[error("http {0}")]
    Http(u16),
    #[error("invalid response")]
    InvalidResponse,
}

pub struct PspClient {
    http: reqwest::Client,
    base_url: String,
}

impl PspClient {
    pub fn new(base_url: String, timeout_secs: u64) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("reqwest client");
        Self { http, base_url }
    }

    pub async fn charge(&self, card_token: &str) -> Result<PspChargeResponse, PspError> {
        let url = format!("{}/mock-psp/v1/charges", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&PspChargeRequest { card_token })
            .send()
            .await
            .map_err(|_| PspError::Network)?;

        if !resp.status().is_success() {
            return Err(PspError::Http(resp.status().as_u16()));
        }

        resp.json::<PspChargeResponse>()
            .await
            .map_err(|_| PspError::InvalidResponse)
    }
}
