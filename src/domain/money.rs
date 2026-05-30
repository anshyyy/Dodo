/// Line total in USD minor units (cents). Uses checked arithmetic — no floats.
pub fn compute_line_total(quantity: i32, unit_amount_cents: i64) -> Result<i64, MoneyError> {
    if quantity <= 0 {
        return Err(MoneyError::InvalidQuantity);
    }
    if unit_amount_cents < 0 {
        return Err(MoneyError::NegativeAmount);
    }
    let q = i64::from(quantity);
    q.checked_mul(unit_amount_cents)
        .ok_or(MoneyError::Overflow)
}

pub fn sum_line_totals(totals: impl IntoIterator<Item = i64>) -> Result<i64, MoneyError> {
    totals
        .into_iter()
        .try_fold(0i64, |acc, t| acc.checked_add(t).ok_or(MoneyError::Overflow))
}

#[derive(Debug, thiserror::Error)]
pub enum MoneyError {
    #[error("invalid quantity")]
    InvalidQuantity,
    #[error("negative amount")]
    NegativeAmount,
    #[error("overflow")]
    Overflow,
}
