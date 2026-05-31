//! Library entrypoint for integration tests and shared wiring.

pub mod api;
pub mod config;
pub mod domain;
pub mod logging;
pub mod mock_psp;
pub mod repository;
pub mod services;
pub mod worker;

use config::Config;

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    logging::init();

    let config = Config::from_env()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    repository::api_key::ensure_demo_key(&pool).await?;

    worker::spawn(pool.clone());

    let state = api::build_state(pool, config.clone());
    let app = api::build_router(state, &config);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);
    axum::serve(listener, app).await?;
    Ok(())
    
}
