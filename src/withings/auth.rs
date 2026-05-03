use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute the signature for `getnonce`: HMAC-SHA256("getnonce,<client_id>,<timestamp>", key=secret).
pub fn sign_getnonce(client_id: &str, timestamp: i64, client_secret: &str) -> String {
    sign(client_secret, &format!("getnonce,{client_id},{timestamp}"))
}

/// Compute the signature for any signed action that follows the
/// `<action>,<client_id>,<nonce>` pattern (e.g. `requesttoken`).
pub fn sign_action(action: &str, client_id: &str, nonce: &str, client_secret: &str) -> String {
    sign(client_secret, &format!("{action},{client_id},{nonce}"))
}

fn sign(secret: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(Debug, Deserialize)]
pub struct EnvelopeNonce {
    pub status: i64,
    #[serde(default)]
    pub body: Option<NonceBody>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NonceBody {
    pub nonce: String,
}

#[derive(Debug, Deserialize)]
pub struct EnvelopeToken {
    pub status: i64,
    #[serde(default)]
    pub body: Option<TokenBody>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TokenBody {
    pub userid: serde_json::Value,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub scope: String,
    #[serde(default)]
    pub token_type: Option<String>,
}

/// Parse a Withings JSON envelope. Returns `body` on `status:0`, error otherwise.
pub fn parse_nonce(json: &str) -> Result<NonceBody> {
    let env: EnvelopeNonce = serde_json::from_str(json).context("parse nonce envelope")?;
    if env.status != 0 {
        bail!("withings status={} error={:?}", env.status, env.error);
    }
    env.body.context("nonce body missing")
}

pub fn parse_token(json: &str) -> Result<TokenBody> {
    let env: EnvelopeToken = serde_json::from_str(json).context("parse token envelope")?;
    if env.status != 0 {
        bail!("withings status={} error={:?}", env.status, env.error);
    }
    env.body.context("token body missing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_deterministic() {
        let s1 = sign_getnonce("client", 1_700_000_000, "secret");
        let s2 = sign_getnonce("client", 1_700_000_000, "secret");
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 64); // hex-encoded SHA-256 = 64 chars
    }

    #[test]
    fn signature_changes_with_inputs() {
        let a = sign_getnonce("client", 1, "secret");
        let b = sign_getnonce("client", 2, "secret");
        assert_ne!(a, b);
    }

    #[test]
    fn action_signature_format() {
        let s = sign_action("requesttoken", "cid", "nonce-x", "secret");
        let expected = sign("secret", "requesttoken,cid,nonce-x");
        assert_eq!(s, expected);
    }

    #[test]
    fn parse_nonce_ok() {
        let n = parse_nonce(r#"{"status":0,"body":{"nonce":"abc"}}"#).unwrap();
        assert_eq!(n.nonce, "abc");
    }

    #[test]
    fn parse_nonce_err_status() {
        let err = parse_nonce(r#"{"status":503,"error":"Invalid Params"}"#).unwrap_err();
        assert!(err.to_string().contains("status=503"));
    }

    #[test]
    fn parse_token_ok() {
        let t = parse_token(
            r#"{"status":0,"body":{"userid":"12","access_token":"a","refresh_token":"r","expires_in":10800,"scope":"x"}}"#,
        )
        .unwrap();
        assert_eq!(t.access_token, "a");
        assert_eq!(t.refresh_token, "r");
        assert_eq!(t.expires_in, 10800);
    }
}
