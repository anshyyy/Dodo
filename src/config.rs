use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
    pub mock_psp_base_url: String,
    /// Max time the pay handler waits for PSP before returning 202
    pub pay_sync_wait_secs: u64,
    /// HTTP timeout when calling mock PSP from background task
    pub psp_http_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://dodo:dodo@localhost:5433/dodo".into()),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            mock_psp_base_url: env::var("MOCK_PSP_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            pay_sync_wait_secs: env::var("PAY_SYNC_WAIT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8),
            psp_http_timeout_secs: env::var("PSP_HTTP_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(35),
        })
    }
}
