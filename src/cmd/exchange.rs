use anyhow::{Context, Result};

use crate::db;
use crate::state;
use crate::withings::client::{Tokens, WithingsClient};

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn run(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<()> {
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let pool = db::connect(&database_url).await?;

    let http = reqwest::Client::builder()
        .user_agent(format!("withings-to-mysql/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let client = WithingsClient::new(
        http,
        client_id.into(),
        client_secret.into(),
        Tokens::default(),
    );
    let body = client.exchange_code(code, redirect_uri).await?;

    let tokens = Tokens {
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_at: now_secs() + body.expires_in,
        userid: userid_str(&body.userid),
        scope: body.scope,
    };
    state::save_tokens(&pool, &tokens).await?;
    eprintln!("Tokens saved to DB (userid={})", tokens.userid);
    Ok(())
}

fn userid_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => v.to_string(),
    }
}
