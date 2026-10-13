use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserSession {
    pub logged_in: bool,
    pub username: Option<String>,
}

pub fn sign_session(session: &UserSession, secret: &str) -> String {
    let json_bytes = serde_json::to_vec(session).unwrap_or_default();
    let payload_b64 = URL_SAFE_NO_PAD.encode(json_bytes);

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload_b64.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{}.{}", payload_b64, sig)
}

pub fn verify_session(cookie_val: &str, secret: &str) -> Option<UserSession> {
    let parts: Vec<&str> = cookie_val.split('.').collect();
    if parts.len() != 2 {
        return None;
    }

    let payload_b64 = parts[0];
    let sig_b64 = parts[1];

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload_b64.as_bytes());
    let expected_sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    if sig_b64 != expected_sig {
        return None;
    }

    let raw = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    serde_json::from_slice::<UserSession>(&raw).ok()
}
