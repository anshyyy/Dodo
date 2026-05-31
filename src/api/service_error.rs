use axum::http::StatusCode;

use crate::api::error::{self, ApiError};
use crate::services::customer::CustomerError;
use crate::services::invoice::InvoiceError;
use crate::services::webhook_endpoint::WebhookEndpointError;

impl From<CustomerError> for ApiError {
    fn from(value: CustomerError) -> Self {
        match value {
            CustomerError::InvalidEmail => ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_email",
                "email must be a valid address",
            ),
            CustomerError::InvalidName => ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_name",
                "name must be non-empty",
            ),
            CustomerError::DuplicateEmail => error::conflict(
                "customer_exists",
                "customer with this email already exists",
            ),
            CustomerError::NotFound => error::not_found("customer not found"),
            CustomerError::Internal(e) => error::internal(e.to_string()),
        }
    }
}

impl From<InvoiceError> for ApiError {
    fn from(value: InvoiceError) -> Self {
        match value {
            InvoiceError::CustomerNotFound => error::not_found("customer not found"),
            InvoiceError::InvalidCreateState => ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "state must be open or draft",
            ),
            InvoiceError::InvalidListStateFilter => ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "unknown state filter",
            ),
            InvoiceError::NotFound => error::not_found("invoice not found"),
            InvoiceError::Internal(e) => error::internal(e.to_string()),
        }
    }
}

impl From<WebhookEndpointError> for ApiError {
    fn from(value: WebhookEndpointError) -> Self {
        match value {
            WebhookEndpointError::Internal(e) => error::internal(e.to_string()),
        }
    }
}
