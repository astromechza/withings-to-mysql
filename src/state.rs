use anyhow::Result;
use sqlx::MySqlPool;

pub use crate::withings::client::Tokens;

#[derive(Debug, Clone, Default)]
pub struct Cursors {
    pub measure:  i64,
    pub activity: i64,
    pub sleep:    i64,
    pub workouts: i64,
    pub intraday: i64,
}

async fn get_val(pool: &MySqlPool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value_text FROM state WHERE key_name = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(v,)| v))
}

async fn set_val(pool: &MySqlPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO state (key_name, value_text) VALUES (?, ?)
         ON DUPLICATE KEY UPDATE value_text = VALUES(value_text)",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_tokens(pool: &MySqlPool) -> Result<Option<Tokens>> {
    let access  = get_val(pool, "token.access_token").await?;
    let refresh = get_val(pool, "token.refresh_token").await?;
    let expires = get_val(pool, "token.expires_at").await?;
    let userid  = get_val(pool, "token.userid").await?;
    let scope   = get_val(pool, "token.scope").await?;
    match (access, refresh, expires, userid) {
        (Some(a), Some(r), Some(e), Some(u)) => Ok(Some(Tokens {
            access_token:  a,
            refresh_token: r,
            expires_at:    e.parse().unwrap_or(0),
            userid:        u,
            scope:         scope.unwrap_or_default(),
        })),
        _ => Ok(None),
    }
}

pub async fn save_tokens(pool: &MySqlPool, t: &Tokens) -> Result<()> {
    set_val(pool, "token.access_token",  &t.access_token).await?;
    set_val(pool, "token.refresh_token", &t.refresh_token).await?;
    set_val(pool, "token.expires_at",    &t.expires_at.to_string()).await?;
    set_val(pool, "token.userid",        &t.userid).await?;
    set_val(pool, "token.scope",         &t.scope).await?;
    Ok(())
}

pub async fn load_cursors(pool: &MySqlPool) -> Result<Cursors> {
    async fn fetch(pool: &MySqlPool, key: &str) -> Result<i64> {
        Ok(get_val(pool, key).await?.and_then(|s| s.parse().ok()).unwrap_or(0))
    }
    Ok(Cursors {
        measure:  fetch(pool, "cursor.measure").await?,
        activity: fetch(pool, "cursor.activity").await?,
        sleep:    fetch(pool, "cursor.sleep").await?,
        workouts: fetch(pool, "cursor.workouts").await?,
        intraday: fetch(pool, "cursor.intraday").await?,
    })
}

pub async fn save_cursors(pool: &MySqlPool, c: &Cursors) -> Result<()> {
    set_val(pool, "cursor.measure",  &c.measure.to_string()).await?;
    set_val(pool, "cursor.activity", &c.activity.to_string()).await?;
    set_val(pool, "cursor.sleep",    &c.sleep.to_string()).await?;
    set_val(pool, "cursor.workouts", &c.workouts.to_string()).await?;
    set_val(pool, "cursor.intraday", &c.intraday.to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_default_empty() {
        let t = Tokens::default();
        assert!(t.access_token.is_empty());
        assert_eq!(t.expires_at, 0);
    }

    #[test]
    fn cursors_default_zero() {
        let c = Cursors::default();
        assert_eq!(c.measure, 0);
        assert_eq!(c.intraday, 0);
    }
}
