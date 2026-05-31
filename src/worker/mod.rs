pub mod webhook_sign;

use crate::repository::webhook;
use crate::worker::webhook_sign::signature_header;
use sqlx::PgPool;
use std::time::{Duration, Instant};

pub fn spawn(pool: PgPool) {
    tracing::info!("webhook delivery worker started");
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&pool).await {
                tracing::error!(error = %e, "webhook worker tick failed");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn tick(pool: &PgPool) -> anyhow::Result<()> {
    let events = webhook::fetch_due(pool, 20).await?;
    if events.is_empty() {
        return Ok(());
    }

    tracing::debug!(count = events.len(), "webhook worker processing batch");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("dodo-invoice-service/webhook-delivery")
        .build()?;

    for ev in events {
        let endpoints = webhook::endpoints_for_business(pool, ev.business_id).await?;
        if endpoints.is_empty() {
            tracing::info!(
                webhook_event_id = %ev.id,
                business_id = %ev.business_id,
                event_type = %ev.event_type,
                "webhook event skipped (no endpoints registered)"
            );
            webhook::mark_delivered(pool, ev.id).await?;
            continue;
        }

        tracing::info!(
            webhook_event_id = %ev.id,
            business_id = %ev.business_id,
            event_type = %ev.event_type,
            attempt = ev.attempt_count + 1,
            endpoint_count = endpoints.len(),
            "webhook delivery started"
        );

        let body = ev.payload.to_string();
        let mut any_ok = false;
        for (endpoint_id, url, secret) in &endpoints {
            let sig = signature_header(secret, &body);
            let started = Instant::now();
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
                    tracing::info!(
                        webhook_event_id = %ev.id,
                        webhook_endpoint_id = %endpoint_id,
                        event_type = %ev.event_type,
                        url = %url,
                        http_status = r.status().as_u16(),
                        latency_ms = started.elapsed().as_millis() as u64,
                        "webhook delivered"
                    );
                }
                Ok(r) => {
                    tracing::warn!(
                        webhook_event_id = %ev.id,
                        webhook_endpoint_id = %endpoint_id,
                        event_type = %ev.event_type,
                        url = %url,
                        http_status = r.status().as_u16(),
                        latency_ms = started.elapsed().as_millis() as u64,
                        "webhook delivery rejected by receiver"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        webhook_event_id = %ev.id,
                        webhook_endpoint_id = %endpoint_id,
                        event_type = %ev.event_type,
                        url = %url,
                        error = %e,
                        latency_ms = started.elapsed().as_millis() as u64,
                        "webhook delivery request failed"
                    );
                }
            }
        }

        if any_ok {
            tracing::info!(
                webhook_event_id = %ev.id,
                event_type = %ev.event_type,
                "webhook event marked delivered"
            );
            webhook::mark_delivered(pool, ev.id).await?;
        } else {
            let next = ev.attempt_count + 1;
            tracing::warn!(
                webhook_event_id = %ev.id,
                event_type = %ev.event_type,
                next_attempt = next,
                "webhook delivery failed for all endpoints; scheduling retry"
            );
            webhook::schedule_retry(pool, ev.id, next).await?;
        }
    }
    Ok(())
}
