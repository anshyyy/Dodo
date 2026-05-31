use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::OnceLock;

const OPENAPI_FALLBACK: &str = include_str!("../../openapi.yaml");
const SWAGGER_HTML: &str = include_str!("../../assets/swagger-ui.html");

static OPENAPI_SPEC: OnceLock<String> = OnceLock::new();

/// Prefer on-disk spec (Docker: `/app/openapi.yaml`) so OpenAPI edits don't require recompiling Rust.
fn openapi_spec() -> &'static str {
    OPENAPI_SPEC
        .get_or_init(|| {
            let candidates = [
                std::env::var("OPENAPI_SPEC_PATH").ok(),
                Some("/app/openapi.yaml".into()),
                Some("openapi.yaml".into()),
            ];
            for path in candidates.into_iter().flatten() {
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    return contents;
                }
            }
            OPENAPI_FALLBACK.to_string()
        })
        .as_str()
}

pub fn routes() -> Router {
    Router::new()
        .route("/docs", get(swagger_ui))
        .route("/api/openapi.yaml", get(openapi_yaml))
}

async fn openapi_yaml() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/yaml")],
        openapi_spec(),
    )
}

async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_HTML)
}
