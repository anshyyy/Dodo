use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, serde::Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, serde::Serialize)]
pub struct ErrorDetail {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: self.message,
                details: None,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

pub fn internal(msg: impl Into<String>) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        msg.into(),
    )
}

pub fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "not_found", msg.into())
}

pub fn conflict(code: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, code, msg.into())
}

pub fn unprocessable(code: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, msg.into())
}

pub fn unauthorized() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "invalid or missing API key",
    )
}

pub fn ok_json<T: serde::Serialize>(value: T) -> Response {
    Json(value).into_response()
}

pub fn json_status<T: serde::Serialize>(status: StatusCode, value: T) -> Response {
    (status, Json(value)).into_response()
}

pub fn empty_details() -> serde_json::Value {
    json!({})
}
