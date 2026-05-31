/// Normalized email address (trimmed, lowercased).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid email address")]
pub struct InvalidEmail;

impl Email {
    pub fn parse(raw: &str) -> Result<Self, InvalidEmail> {
        let s = raw.trim();
        if s.is_empty() || s.len() > 254 {
            return Err(InvalidEmail);
        }
        if s.chars().any(|c| c.is_whitespace()) {
            return Err(InvalidEmail);
        }
        let Some((local, domain)) = s.split_once('@') else {
            return Err(InvalidEmail);
        };
        if local.is_empty() || local.len() > 64 {
            return Err(InvalidEmail);
        }
        if domain.is_empty() || domain.starts_with('.') || domain.ends_with('.') {
            return Err(InvalidEmail);
        }
        if !domain.contains('.') {
            return Err(InvalidEmail);
        }
        let valid_local = local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(c));
        let valid_domain = domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c));
        if !valid_local || !valid_domain {
            return Err(InvalidEmail);
        }
        if domain
            .rsplit('.')
            .next()
            .is_none_or(|tld| tld.len() < 2)
        {
            return Err(InvalidEmail);
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
