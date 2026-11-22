use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const SESSION_LIFETIME_SECS: i64 = 30 * 24 * 3600; // 30 days

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub logged_in: bool,
    pub username: Option<String>,
    #[serde(default = "default_exp")]
    pub exp: i64,
}

fn default_exp() -> i64 {
    Utc::now().timestamp() + SESSION_LIFETIME_SECS
}

impl Default for UserSession {
    fn default() -> Self {
        Self {
            logged_in: false,
            username: None,
            exp: default_exp(),
        }
    }
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

    let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload_b64.as_bytes());

    // Constant-time signature comparison to prevent timing attacks
    if mac.verify_slice(&sig_bytes).is_err() {
        return None;
    }

    let raw = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let session = serde_json::from_slice::<UserSession>(&raw).ok()?;

    // Check expiration timestamp
    let now = Utc::now().timestamp();
    if now > session.exp {
        return None;
    }

    Some(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_session_roundtrip() {
        let secret = "super-secret-key-123";
        let session = UserSession {
            logged_in: true,
            username: Some("testuser".to_string()),
            exp: Utc::now().timestamp() + 3600,
        };

        let signed = sign_session(&session, secret);
        let verified = verify_session(&signed, secret).expect("should verify");
        assert!(verified.logged_in);
        assert_eq!(verified.username.as_deref(), Some("testuser"));
    }

    #[test]
    fn test_expired_session_rejected() {
        let secret = "super-secret-key-123";
        let session = UserSession {
            logged_in: true,
            username: Some("testuser".to_string()),
            exp: Utc::now().timestamp() - 10,
        };

        let signed = sign_session(&session, secret);
        assert!(verify_session(&signed, secret).is_none());
    }

    #[test]
    fn test_tampered_signature_rejected() {
        let secret = "super-secret-key-123";
        let session = UserSession {
            logged_in: true,
            username: Some("testuser".to_string()),
            exp: Utc::now().timestamp() + 3600,
        };

        let signed = sign_session(&session, secret);
        let tampered = format!("{}x", signed);
        assert!(verify_session(&tampered, secret).is_none());

        let wrong_secret = "wrong-secret-key-456";
        assert!(verify_session(&signed, wrong_secret).is_none());
    }
}
