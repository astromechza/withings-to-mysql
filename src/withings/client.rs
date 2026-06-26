use anyhow::{anyhow, Context, Result};
use reqwest::Client as HttpClient;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use super::auth::{parse_nonce, parse_token, sign_action, sign_getnonce, TokenBody};

const TOKEN_HOST: &str = "https://wbsapi.withings.net";
const SIGNATURE_PATH: &str = "/v2/signature";
const OAUTH2_PATH: &str = "/v2/oauth2";
const ACCOUNT_HOST: &str = "https://account.withings.com";

#[derive(Debug, Clone, Default)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) at which the access token expires.
    pub expires_at: i64,
    pub scope: String,
    pub userid: String,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Build the HTTP client used for all Withings API calls.
///
/// Binds the local socket to the IPv4 unspecified address (`0.0.0.0`) which
/// forces every outbound connection to use IPv4. `wbsapi.withings.net`
/// publishes both A and AAAA records, but our deployment environment (microk8s)
/// has no IPv6 egress, so a connect to the AAAA address blocks until ETIMEDOUT.
/// The musl-static build has no Happy Eyeballs / RFC 6724 IPv4 preference, so
/// the resolver order alone decides the family and the job fails intermittently.
/// Binding to a v4 socket makes any v6 destination fail-fast instead of hanging.
pub fn build_http_client() -> reqwest::Result<HttpClient> {
    HttpClient::builder()
        .user_agent(format!("withings-to-mysql/{}", env!("CARGO_PKG_VERSION")))
        .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        .build()
}

#[derive(Clone)]
pub struct WithingsClient {
    http: HttpClient,
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
    pub tokens: Arc<Mutex<Tokens>>,
}

impl WithingsClient {
    pub fn new(http: HttpClient, client_id: String, client_secret: String, tokens: Tokens) -> Self {
        Self {
            http,
            client_id,
            client_secret,
            base_url: TOKEN_HOST.to_string(),
            tokens: Arc::new(Mutex::new(tokens)),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn snapshot_tokens(&self) -> Tokens {
        self.tokens.lock().unwrap().clone()
    }

    pub async fn ensure_fresh_token(&self, now: i64, leeway_secs: i64) -> Result<String> {
        let snap = self.snapshot_tokens();
        if now + leeway_secs < snap.expires_at {
            return Ok(snap.access_token);
        }
        let new = self.refresh_token().await?;
        Ok(new.access_token)
    }

    async fn get_nonce(&self) -> Result<String> {
        let ts = now_secs();
        let sig = sign_getnonce(&self.client_id, ts, &self.client_secret);
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, SIGNATURE_PATH))
            .form(&[
                ("action", "getnonce"),
                ("client_id", self.client_id.as_str()),
                ("timestamp", &ts.to_string()),
                ("signature", &sig),
            ])
            .send()
            .await
            .context("getnonce http")?;
        let text = resp.text().await.context("getnonce body")?;
        Ok(parse_nonce(&text)?.nonce)
    }

    pub async fn refresh_token(&self) -> Result<TokenBody> {
        let nonce = self.get_nonce().await?;
        let sig = sign_action("requesttoken", &self.client_id, &nonce, &self.client_secret);
        let refresh = self.snapshot_tokens().refresh_token;
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, OAUTH2_PATH))
            .form(&[
                ("action", "requesttoken"),
                ("client_id", self.client_id.as_str()),
                ("nonce", &nonce),
                ("signature", &sig),
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh),
            ])
            .send()
            .await
            .context("requesttoken http")?;
        let text = resp.text().await.context("requesttoken body")?;
        let body = parse_token(&text)?;
        let now = now_secs();
        let mut t = self.tokens.lock().unwrap();
        t.access_token = body.access_token.clone();
        t.refresh_token = body.refresh_token.clone();
        t.expires_at = now + body.expires_in;
        t.scope = body.scope.clone();
        t.userid = userid_to_string(&body.userid);
        Ok(body)
    }

    pub async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenBody> {
        let nonce = self.get_nonce().await?;
        let sig = sign_action("requesttoken", &self.client_id, &nonce, &self.client_secret);
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, OAUTH2_PATH))
            .form(&[
                ("action", "requesttoken"),
                ("client_id", self.client_id.as_str()),
                ("nonce", &nonce),
                ("signature", &sig),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .context("exchange http")?;
        let text = resp.text().await.context("exchange body")?;
        parse_token(&text)
    }

    pub async fn post_data(&self, path: &str, params: &[(&str, String)]) -> Result<String> {
        let now = now_secs();
        let access = self.ensure_fresh_token(now, 300).await?;
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&access)
            .form(params)
            .send()
            .await
            .with_context(|| format!("data http {path}"))?;
        if resp.status().as_u16() == 401 {
            tracing::warn!("data API returned 401; refreshing once");
            let new = self.refresh_token().await?;
            let resp2 = self
                .http
                .post(format!("{}{}", self.base_url, path))
                .bearer_auth(&new.access_token)
                .form(params)
                .send()
                .await
                .with_context(|| format!("data http retry {path}"))?;
            if !resp2.status().is_success() {
                return Err(anyhow!("retry status {}", resp2.status()));
            }
            return Ok(resp2.text().await?);
        }
        if !resp.status().is_success() {
            return Err(anyhow!("status {}", resp.status()));
        }
        Ok(resp.text().await?)
    }
}

pub fn authorize_url(client_id: &str, redirect_uri: &str, scope: &str, state: &str) -> String {
    let mut url = url::Url::parse(&format!("{ACCOUNT_HOST}/oauth2_user/authorize2")).expect("url");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("scope", scope)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    url.into()
}

fn userid_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn v4_bound_client_reaches_v4_endpoint() {
        // build_http_client binds to 0.0.0.0 (IPv4). Verify that an ordinary
        // request to a v4 endpoint still works — i.e. forcing IPv4 doesn't break
        // normal connectivity. wiremock binds on 127.0.0.1, so this exercises the
        // v4 path the production fix relies on.
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/ping"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("pong"))
            .mount(&mock)
            .await;
        let http = build_http_client().expect("client builds");
        let body = http
            .get(format!("{}/ping", mock.uri()))
            .send()
            .await
            .expect("request succeeds")
            .text()
            .await
            .expect("body");
        assert_eq!(body, "pong");
    }

    #[test]
    fn authorize_url_has_expected_params() {
        let u = authorize_url("CID", "https://example/cb", "user.metrics", "abc");
        assert!(u.contains("client_id=CID"));
        assert!(u.contains("scope=user.metrics"));
        assert!(u.contains("redirect_uri=https%3A%2F%2Fexample%2Fcb"));
        assert!(u.contains("state=abc"));
        assert!(u.contains("response_type=code"));
    }

    #[test]
    fn userid_string_or_number() {
        assert_eq!(userid_to_string(&serde_json::json!("12")), "12");
        assert_eq!(userid_to_string(&serde_json::json!(12)), "12");
    }

    #[tokio::test]
    async fn refresh_token_updates_state() {
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v2/signature"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"status":0,"body":{"nonce":"n1"}}"#),
            )
            .mount(&mock)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v2/oauth2"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                r#"{"status":0,"body":{"userid":"42","access_token":"newA","refresh_token":"newR","expires_in":3600,"scope":"user.metrics"}}"#,
            ))
            .mount(&mock)
            .await;
        let client = WithingsClient::new(
            HttpClient::new(),
            "CID".into(),
            "SECRET".into(),
            Tokens {
                access_token: "old".into(),
                refresh_token: "oldR".into(),
                expires_at: 0,
                scope: String::new(),
                userid: String::new(),
            },
        )
        .with_base_url(mock.uri());
        let body = client.refresh_token().await.unwrap();
        assert_eq!(body.access_token, "newA");
        let snap = client.snapshot_tokens();
        assert_eq!(snap.access_token, "newA");
        assert_eq!(snap.refresh_token, "newR");
        assert_eq!(snap.userid, "42");
        assert!(snap.expires_at > 0);
    }

    #[tokio::test]
    async fn data_request_refreshes_on_401() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mock = wiremock::MockServer::start().await;
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        let counter_data = counter.clone();
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/measure"))
            .respond_with(move |_: &wiremock::Request| {
                let n = counter_data.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    wiremock::ResponseTemplate::new(401)
                } else {
                    wiremock::ResponseTemplate::new(200)
                        .set_body_string(r#"{"status":0,"body":{"measuregrps":[]}}"#)
                }
            })
            .mount(&mock)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v2/signature"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"status":0,"body":{"nonce":"n"}}"#),
            )
            .mount(&mock)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v2/oauth2"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                r#"{"status":0,"body":{"userid":"1","access_token":"A2","refresh_token":"R2","expires_in":3600,"scope":""}}"#,
            ))
            .mount(&mock)
            .await;
        let client = WithingsClient::new(
            HttpClient::new(),
            "CID".into(),
            "SECRET".into(),
            Tokens {
                access_token: "A".into(),
                refresh_token: "R".into(),
                expires_at: i64::MAX,
                scope: String::new(),
                userid: String::new(),
            },
        )
        .with_base_url(mock.uri());
        let body = client.post_data("/measure", &[]).await.unwrap();
        assert!(body.contains("measuregrps"));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "data endpoint should be hit twice"
        );
    }
}
