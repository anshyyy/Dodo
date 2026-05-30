//! Integration tests require PostgreSQL:
//! `docker compose up -d postgres` then `DATABASE_URL=postgres://dodo:dodo@localhost:5432/dodo cargo test --test integration_tests`

use dodo_invoice_service::mock_psp::PSP_CALL_COUNT;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use uuid::Uuid;

const API_KEY: &str = "dodo_test_key_demo12345678901234567890";

struct TestEnv {
    client: Client,
    base: String,
    pool: sqlx::PgPool,
}

async fn setup() -> TestEnv {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://dodo:dodo@localhost:5433/dodo".into());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("connect postgres (start docker compose postgres)");

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    dodo_invoice_service::repository::api_key::ensure_demo_key(&pool)
        .await
        .unwrap();

    let config = dodo_invoice_service::config::Config {
        database_url: database_url.clone(),
        listen_addr: "127.0.0.1:0".into(),
        mock_psp_base_url: String::new(), // set after bind
        pay_sync_wait_secs: 8,
        psp_http_timeout_secs: 35,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut config = config;
    config.mock_psp_base_url = format!("http://{addr}");
    config.listen_addr = addr.to_string();

    let state = dodo_invoice_service::api::build_state(pool.clone(), config.clone());
    let app = dodo_invoice_service::api::build_router(state, &config);
    dodo_invoice_service::worker::spawn(pool.clone());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    TestEnv {
        client: Client::new(),
        base: format!("http://{addr}"),
        pool,
    }
}

fn auth_headers(idempotency: Option<&str>) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("X-Api-Key", API_KEY.parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());
    if let Some(k) = idempotency {
        h.insert("Idempotency-Key", k.parse().unwrap());
    }
    h
}

async fn create_customer(env: &TestEnv) -> Value {
    let resp = env
        .client
        .post(format!("{}/api/v1/customers", env.base))
        .headers(auth_headers(None))
        .json(&json!({"name": "Ada", "email": format!("ada-{}@test.com", Uuid::new_v4())}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    resp.json().await.unwrap()
}

async fn create_invoice(env: &TestEnv, customer_id: Uuid) -> Value {
    let resp = env
        .client
        .post(format!("{}/api/v1/invoices", env.base))
        .headers(auth_headers(None))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2026-12-31",
            "line_items": [{"description": "Widget", "quantity": 2, "unit_amount_cents": 1500}]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    resp.json().await.unwrap()
}

async fn pay(
    env: &TestEnv,
    invoice_id: Uuid,
    idem: &str,
    token: &str,
) -> (StatusCode, Value) {
    let resp = env
        .client
        .post(format!("{}/api/v1/invoices/{invoice_id}/pay", env.base))
        .headers(auth_headers(Some(idem)))
        .json(&json!({"card_token": token}))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));
    (status, body)
}

#[tokio::test]
async fn concurrent_pay_only_one_succeeds() {
    let env = setup().await;
    PSP_CALL_COUNT.store(0, Ordering::SeqCst);

    let customer = create_customer(&env).await;
    let invoice = create_invoice(&env, customer["id"].as_str().unwrap().parse().unwrap()).await;
    let invoice_id: Uuid = invoice["id"].as_str().unwrap().parse().unwrap();

    let mut handles = Vec::new();
    for i in 0..8 {
        let env = TestEnv {
            client: env.client.clone(),
            base: env.base.clone(),
            pool: env.pool.clone(),
        };
        let idem = format!("concurrent-{i}-{}", Uuid::new_v4());
        handles.push(tokio::spawn(async move {
            pay(&env, invoice_id, &idem, "tok_success").await
        }));
    }

    let mut success_count = 0;
    for h in handles {
        let (status, body) = h.await.unwrap();
        if status.is_success() {
            if body["payment_attempt"]["status"] == "succeeded" {
                success_count += 1;
            }
        }
    }

    let succeeded: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM payment_attempts WHERE invoice_id = $1 AND status = 'succeeded'",
    )
    .bind(invoice_id)
    .fetch_one(&env.pool)
    .await
    .unwrap();

    let state: (String,) =
        sqlx::query_as("SELECT state::text FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&env.pool)
            .await
            .unwrap();

    assert_eq!(succeeded.0, 1, "exactly one succeeded attempt in DB");
    assert_eq!(state.0, "paid");
    assert!(success_count >= 1);
}

#[tokio::test]
async fn idempotency_replays_without_second_psp_call() {
    let env = setup().await;
    PSP_CALL_COUNT.store(0, Ordering::SeqCst);

    let customer = create_customer(&env).await;
    let invoice = create_invoice(&env, customer["id"].as_str().unwrap().parse().unwrap()).await;
    let invoice_id: Uuid = invoice["id"].as_str().unwrap().parse().unwrap();
    let idem = format!("idem-{}", Uuid::new_v4());

    let (s1, b1) = pay(&env, invoice_id, &idem, "tok_success").await;
    let (s2, b2) = pay(&env, invoice_id, &idem, "tok_success").await;

    assert!(s1.is_success());
    assert!(s2.is_success());
    assert_eq!(b1, b2);
    assert_eq!(PSP_CALL_COUNT.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn psp_network_error_does_not_corrupt_invoice() {
    let env = setup().await;
    let customer = create_customer(&env).await;
    let invoice = create_invoice(&env, customer["id"].as_str().unwrap().parse().unwrap()).await;
    let invoice_id: Uuid = invoice["id"].as_str().unwrap().parse().unwrap();

    let start = Instant::now();
    let (status, body) = pay(
        &env,
        invoice_id,
        &format!("fail-{}", Uuid::new_v4()),
        "tok_network_error",
    )
    .await;
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(status.is_success());
    assert_eq!(body["payment_attempt"]["status"], "failed");
    assert_eq!(body["invoice_state"], "open");

    let state: (String,) =
        sqlx::query_as("SELECT state::text FROM invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_one(&env.pool)
            .await
            .unwrap();
    assert_eq!(state.0, "open");
}
