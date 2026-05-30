use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn sign_payload(secret: &str, body: &str) -> (i64, String) {
    let t = Utc::now().timestamp();
    let signed_content = format!("{t}.{body}");
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(signed_content.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    (t, sig)
}

pub fn signature_header(secret: &str, body: &str) -> String {
    let (t, v1) = sign_payload(secret, body);
    format!("t={t},v1={v1}")
}
