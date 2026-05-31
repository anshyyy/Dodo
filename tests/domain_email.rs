//! Unit tests for domain email validation (no DB required).

use dodo_invoice_service::domain::Email;

#[test]
fn rejects_placeholder_and_malformed() {
    assert!(Email::parse("string").is_err());
    assert!(Email::parse("@example.com").is_err());
    assert!(Email::parse("user@").is_err());
    assert!(Email::parse("user@domain").is_err());
}

#[test]
fn accepts_common_addresses() {
    let e = Email::parse("Jane.Doe@Example.COM").unwrap();
    assert_eq!(e.as_str(), "jane.doe@example.com");
}
