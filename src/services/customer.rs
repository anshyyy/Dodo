use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{CustomerName, Email};
use crate::repository::customer::{self, Customer, CustomerDbError, CustomerListFilters, CustomerListPage};

pub const DEFAULT_CUSTOMER_LIST_LIMIT: i64 = 50;
pub const MAX_CUSTOMER_LIST_LIMIT: i64 = 100;

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
    #[error("invalid list pagination")]
    InvalidListPagination,
    #[error("business_id does not match authenticated tenant")]
    BusinessIdMismatch,
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

    pub async fn list(
        &self,
        authenticated_business_id: Uuid,
        filters: CustomerListFilters,
        limit: i64,
        offset: i64,
    ) -> Result<CustomerListPage, CustomerError> {
        if filters.business_id != authenticated_business_id {
            return Err(CustomerError::BusinessIdMismatch);
        }
        if limit < 1 || limit > MAX_CUSTOMER_LIST_LIMIT || offset < 0 {
            return Err(CustomerError::InvalidListPagination);
        }
        customer::list_page(&self.pool, &filters, limit, offset)
            .await
            .map_err(CustomerError::Internal)
    }
}
