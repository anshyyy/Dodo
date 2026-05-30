use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

/// OpenAPI spec embedded at compile time (rebuild after editing openapi.yaml).
const OPENAPI_YAML: &str = include_str!("../../openapi.yaml");

const SWAGGER_HTML: &str = include_str!("../../assets/swagger-ui.html");

pub fn routes() -> Router {
    Router::new()
        .route("/docs", get(swagger_ui))
        .route("/api/openapi.yaml", get(openapi_yaml))
}

async fn openapi_yaml() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/yaml")],
        OPENAPI_YAML,
    )
}

async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_HTML)
}
