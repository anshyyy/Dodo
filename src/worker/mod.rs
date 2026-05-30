pub mod webhook_sign;

use crate::repository::webhook;
use crate::worker::webhook_sign::signature_header;
use sqlx::PgPool;
use std::time::Duration;

pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&pool).await {
                tracing::error!("webhook worker error: {e:#}");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn tick(pool: &PgPool) -> anyhow::Result<()> {
    let events = webhook::fetch_due(pool, 20).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    for ev in events {
        let endpoints = webhook::endpoints_for_business(pool, ev.business_id).await?;
        if endpoints.is_empty() {
            webhook::mark_delivered(pool, ev.id).await?;
            continue;
        }

        let body = ev.payload.to_string();
        let mut any_ok = false;
        for (_id, url, secret) in &endpoints {
            let sig = signature_header(secret, &body);
            let resp = client
                .post(url)
                .header("Content-Type", "application/json")
                .header("X-Dodo-Signature", sig)
                .header("X-Dodo-Event-Type", &ev.event_type)
                .body(body.clone())
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    any_ok = true;
                }
                Ok(r) => {
                    tracing::warn!(
                        "webhook delivery failed status {} to {url}",
                        r.status()
                    );
                }
                Err(e) => {
                    tracing::warn!("webhook delivery error to {url}: {e}");
                }
            }
        }

        if any_ok {
            webhook::mark_delivered(pool, ev.id).await?;
        } else {
            let next = ev.attempt_count + 1;
            webhook::schedule_retry(pool, ev.id, next).await?;
        }
    }
    Ok(())
}
