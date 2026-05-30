use crate::config::Config;
use crate::domain::{InvoiceEvent, InvoiceState};
use crate::repository::{
    idempotency::{self, request_hash},
    invoice,
    payment::{self, PaymentAttemptStatus},
    webhook,
};
use crate::services::psp_client::{PspClient, PspChargeResponse, PspError};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PayResponseBody {
    pub payment_attempt: payment::PaymentAttempt,
    pub invoice_state: InvoiceState,
}

pub enum PayOutcome {
    Completed {
        status: u16,
        body: PayResponseBody,
    },
    Replay {
        status: i32,
        body: Value,
    },
}

pub struct PaymentService {
    pool: PgPool,
    psp: Arc<PspClient>,
    config: Config,
}

impl PaymentService {
    pub fn new(pool: PgPool, psp: Arc<PspClient>, config: Config) -> Self {
        Self { pool, psp, config }
    }

    pub async fn pay(
        &self,
        business_id: Uuid,
        invoice_id: Uuid,
        idempotency_key: &str,
        card_token: &str,
        request_path: &str,
        request_body_raw: &str,
    ) -> anyhow::Result<Result<PayOutcome, PayError>> {
        let hash = request_hash("POST", request_path, request_body_raw);

        if let Some(stored) = idempotency::get(&self.pool, business_id, idempotency_key).await? {
            if stored.request_hash != hash {
                return Ok(Err(PayError::IdempotencyMismatch));
            }
            return Ok(Ok(PayOutcome::Replay {
                status: stored.response_status,
                body: stored.response_body,
            }));
        }

        let mut tx = self.pool.begin().await?;
        let locked = invoice::lock_for_update(&mut tx, business_id, invoice_id).await?;
        let Some((_id, state, total_cents)) = locked else {
            return Ok(Err(PayError::InvoiceNotFound));
        };

        if state == InvoiceState::Paid {
            return Ok(Err(PayError::AlreadyPaid));
        }
        if state != InvoiceState::Open {
            return Ok(Err(PayError::InvalidState(state)));
        }

        if let Some(existing) =
            payment::find_by_idempotency(&self.pool, invoice_id, idempotency_key).await?
        {
            tx.commit().await?;
            let state = invoice::get_state(&self.pool, business_id, invoice_id)
                .await?
                .unwrap_or(state);
            let body = PayResponseBody {
                payment_attempt: existing,
                invoice_state: state,
            };
            let val = serde_json::to_value(&body)?;
            return Ok(Ok(PayOutcome::Replay {
                status: 200,
                body: val,
            }));
        }

        let attempt = match payment::insert_processing(
            &mut tx,
            invoice_id,
            idempotency_key,
            card_token,
        )
        .await
        {
            Ok(a) => a,
            Err(e) if is_unique_violation_sqlx(&e) => {
                tx.rollback().await?;
                return Ok(Err(PayError::ConcurrentPay));
            }
            Err(e) => return Err(e.into()),
        };
        tx.commit().await?;

        let pool = self.pool.clone();
        let psp = self.psp.clone();
        let config = self.config.clone();
        let attempt_id = attempt.id;
        let token = card_token.to_string();
        let idem_key = idempotency_key.to_string();
        let path = request_path.to_string();
        let body_raw = request_body_raw.to_string();
        let hash_clone = hash.clone();

        let handle = tokio::spawn(async move {
            process_psp_result(
                &pool,
                &psp,
                business_id,
                invoice_id,
                attempt_id,
                total_cents,
                &token,
                &idem_key,
                &path,
                &body_raw,
                &hash_clone,
                config,
            )
            .await
        });

        let sync_wait =
            tokio::time::Duration::from_secs(self.config.pay_sync_wait_secs);
        match tokio::time::timeout(sync_wait, handle).await {
            Ok(Ok(Ok(outcome))) => Ok(Ok(outcome)),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(join)) => Err(anyhow::anyhow!("task join: {join}")),
            Err(_) => {
                let attempt = payment::get(&self.pool, attempt_id)
                    .await?
                    .unwrap_or(attempt);
                Ok(Ok(PayOutcome::Completed {
                    status: 202,
                    body: PayResponseBody {
                        payment_attempt: attempt,
                        invoice_state: InvoiceState::Open,
                    },
                }))
            }
        }
    }

    pub async fn wait_for_attempt(
        &self,
        attempt_id: Uuid,
        max_wait_secs: u64,
    ) -> anyhow::Result<Option<payment::PaymentAttempt>> {
        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_secs(max_wait_secs);
        loop {
            if let Some(a) = payment::get(&self.pool, attempt_id).await? {
                if a.status != PaymentAttemptStatus::Processing {
                    return Ok(Some(a));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(payment::get(&self.pool, attempt_id).await?);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PayError {
    #[error("invoice not found")]
    InvoiceNotFound,
    #[error("invoice already paid")]
    AlreadyPaid,
    #[error("invalid invoice state")]
    InvalidState(InvoiceState),
    #[error("idempotency key reused with different body")]
    IdempotencyMismatch,
    #[error("payment already in progress")]
    ConcurrentPay,
}

async fn process_psp_result(
    pool: &PgPool,
    psp: &PspClient,
    business_id: Uuid,
    invoice_id: Uuid,
    attempt_id: Uuid,
    total_cents: i64,
    card_token: &str,
    idempotency_key: &str,
    request_path: &str,
    request_body_raw: &str,
    req_hash: &str,
    _config: Config,
) -> anyhow::Result<PayOutcome> {
    let psp_result = psp.charge(card_token).await;

    match psp_result {
        Ok(PspChargeResponse::Succeeded { psp_ref }) => {
            let mut tx = pool.begin().await?;
            let locked = invoice::lock_for_update(&mut tx, business_id, invoice_id).await?;
            let Some((_id, state, _)) = locked else {
                return Err(anyhow::anyhow!("invoice missing"));
            };

            if state == InvoiceState::Paid {
                sqlx::query(
                    r#"
                    UPDATE payment_attempts
                    SET status = 'succeeded', psp_ref = $1, updated_at = NOW()
                    WHERE id = $2
                    "#,
                )
                .bind(psp_ref)
                .bind(attempt_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
            } else if state == InvoiceState::Open {
                let next = state.apply(InvoiceEvent::PaymentSucceeded)?;
                let updated =
                    invoice::transition_state(&mut tx, invoice_id, InvoiceState::Open, next)
                        .await?;
                if !updated {
                    tx.rollback().await?;
                } else {
                    sqlx::query(
                        r#"
                        UPDATE payment_attempts
                        SET status = 'succeeded', psp_ref = $1, updated_at = NOW()
                        WHERE id = $2
                        "#,
                    )
                    .bind(psp_ref)
                    .bind(attempt_id)
                    .execute(&mut *tx)
                    .await?;
                    let payload = webhook::invoice_payload(
                        "invoice.paid",
                        invoice_id,
                        "paid",
                        total_cents,
                    );
                    webhook::enqueue_in_tx(&mut tx, business_id, "invoice.paid", payload).await?;
                    tx.commit().await?;
                }
            }

            let attempt = payment::get(pool, attempt_id).await?.unwrap();
            let inv_state = invoice::get_state(pool, business_id, invoice_id)
                .await?
                .unwrap_or(InvoiceState::Open);
            let body = PayResponseBody {
                payment_attempt: attempt,
                invoice_state: inv_state,
            };
            store_idempotency_if_absent(
                pool,
                business_id,
                idempotency_key,
                req_hash,
                200,
                &body,
                request_path,
                request_body_raw,
            )
            .await?;
            Ok(PayOutcome::Completed {
                status: 200,
                body,
            })
        }
        Ok(PspChargeResponse::Failed { code }) => {
            payment::mark_failed(pool, attempt_id, &code).await?;
            let payload = webhook::invoice_payload(
                "invoice.payment_failed",
                invoice_id,
                "open",
                total_cents,
            );
            webhook::enqueue(pool, business_id, "invoice.payment_failed", payload).await?;

            let attempt = payment::get(pool, attempt_id).await?.unwrap();
            let body = PayResponseBody {
                payment_attempt: attempt,
                invoice_state: InvoiceState::Open,
            };
            store_idempotency_if_absent(
                pool,
                business_id,
                idempotency_key,
                req_hash,
                200,
                &body,
                request_path,
                request_body_raw,
            )
            .await?;
            Ok(PayOutcome::Completed {
                status: 200,
                body,
            })
        }
        Err(PspError::Network) | Err(PspError::Http(_)) => {
            payment::mark_failed(pool, attempt_id, "network_error").await?;
            let payload = webhook::invoice_payload(
                "invoice.payment_failed",
                invoice_id,
                "open",
                total_cents,
            );
            webhook::enqueue(pool, business_id, "invoice.payment_failed", payload).await?;

            let attempt = payment::get(pool, attempt_id).await?.unwrap();
            let body = PayResponseBody {
                payment_attempt: attempt,
                invoice_state: InvoiceState::Open,
            };
            store_idempotency_if_absent(
                pool,
                business_id,
                idempotency_key,
                req_hash,
                200,
                &body,
                request_path,
                request_body_raw,
            )
            .await?;
            Ok(PayOutcome::Completed {
                status: 200,
                body,
            })
        }
        Err(PspError::InvalidResponse) => {
            payment::mark_failed(pool, attempt_id, "invalid_psp_response").await?;
            Err(anyhow::anyhow!("invalid psp response"))
        }
    }
}

fn is_unique_violation_sqlx(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.constraint().is_some())
}

async fn store_idempotency_if_absent(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
    req_hash: &str,
    status: i32,
    body: &PayResponseBody,
    _path: &str,
    _raw: &str,
) -> anyhow::Result<()> {
    if idempotency::get(pool, business_id, key)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let val = serde_json::to_value(body)?;
    let mut tx = pool.begin().await?;
    if let Err(e) = idempotency::insert(&mut tx, business_id, key, req_hash, status, &val).await {
        tracing::debug!("idempotency insert skipped: {e}");
    } else {
        tx.commit().await?;
    }
    Ok(())
}
