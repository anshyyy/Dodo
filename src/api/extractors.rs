use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use uuid::Uuid;

use crate::api::{error::ApiError, AppState};
use crate::repository::api_key;

#[derive(Debug, Clone)]
pub struct AuthBusiness {
    pub business_id: Uuid,
    pub api_key_id: Uuid,
}

impl FromRequestParts<AppState> for AuthBusiness {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let key = extract_api_key(parts).ok_or_else(crate::api::error::unauthorized)?;
        let record = api_key::resolve_business_by_raw_key(&state.pool, &key)
            .await
            .map_err(|e| crate::api::error::internal(e.to_string()))?
            .ok_or_else(crate::api::error::unauthorized)?;
        
        Ok(AuthBusiness {
            business_id: record.business_id,
            api_key_id: record.id,
        })
    }
}

fn extract_api_key(parts: &Parts) -> Option<String> {
    if let Some(v) = parts.headers.get("X-Api-Key") {
        return v.to_str().ok().map(|s| s.to_string());
    }
    if let Some(v) = parts.headers.get(axum::http::header::AUTHORIZATION) {
        let s = v.to_str().ok()?;
        if let Some(token) = s.strip_prefix("Bearer ") {
            return Some(token.to_string());
        }
    }
    None
}
