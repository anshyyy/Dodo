#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerName(String);

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("name must be non-empty")]
pub struct InvalidCustomerName;

impl CustomerName {
    pub fn parse(raw: &str) -> Result<Self, InvalidCustomerName> {
        let s = raw.trim();
        if s.is_empty() || s.len() > 200 {
            return Err(InvalidCustomerName);
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
