use anyhow::{Context, Result};

use crate::db;
use crate::state;

pub async fn run() -> Result<()> {
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let pool = db::connect(&database_url).await?;

    match state::load_tokens(&pool).await? {
        None => println!("No tokens in DB. Run `exchange` first."),
        Some(mut t) => {
            t.access_token  = redact(&t.access_token);
            t.refresh_token = redact(&t.refresh_token);
            let c = state::load_cursors(&pool).await?;
            println!("Tokens:");
            println!("  userid:        {}", t.userid);
            println!("  scope:         {}", t.scope);
            println!("  access_token:  {}", t.access_token);
            println!("  refresh_token: {}", t.refresh_token);
            println!("  expires_at:    {}", t.expires_at);
            println!("Cursors:");
            println!("  measure:  {}", c.measure);
            println!("  activity: {}", c.activity);
            println!("  sleep:    {}", c.sleep);
            println!("  workouts: {}", c.workouts);
            println!("  intraday: {}", c.intraday);
        }
    }
    Ok(())
}

fn redact(s: &str) -> String {
    if s.len() < 8 {
        return "***".into();
    }
    format!("{}…{} ({} chars)", &s[..4], &s[s.len() - 4..], s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_long() {
        let r = redact("abcdefghijklmnop");
        assert!(r.contains("16 chars"));
        assert!(r.starts_with("abcd"));
    }

    #[test]
    fn redacts_short() {
        assert_eq!(redact("ab"), "***");
    }
}
