use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{CustomerName, Email};
use crate::repository::customer::{self, Customer, CustomerDbError};

#[derive(Debug, thiserror::Error)]
pub enum CustomerError {
    #[error("invalid email address")]
    InvalidEmail,
    #[error("name must be non-empty")]
    InvalidName,
    #[error("customer with this email already exists")]
    DuplicateEmail,
    #[error("customer not found")]
    NotFound,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct CustomerService {
    pool: PgPool,
}

impl CustomerService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        business_id: Uuid,
        name: &str,
        email: &str,
    ) -> Result<Customer, CustomerError> {
        let name = CustomerName::parse(name).map_err(|_| CustomerError::InvalidName)?;
        let email = Email::parse(email).map_err(|_| CustomerError::InvalidEmail)?;

        customer::create(
            &self.pool,
            business_id,
            name.as_str(),
            email.as_str(),
        )
        .await
        .map_err(|e| match e {
            CustomerDbError::DuplicateEmail => CustomerError::DuplicateEmail,
            CustomerDbError::Database(err) => CustomerError::Internal(err.into()),
        })
    }

    pub async fn get(&self, business_id: Uuid, id: Uuid) -> Result<Customer, CustomerError> {
        customer::get(&self.pool, business_id, id)
            .await
            .map_err(CustomerError::Internal)?
            .ok_or(CustomerError::NotFound)
    }

    pub async fn list(&self, business_id: Uuid) -> Result<Vec<Customer>, CustomerError> {
        customer::list(&self.pool, business_id)
            .await
            .map_err(CustomerError::Internal)
    }
}
