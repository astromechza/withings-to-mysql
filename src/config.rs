use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub database_url: String,
    pub backfill_days: i64,
    pub user_tz: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            client_id: std::env::var("WITHINGS_CLIENT_ID")
                .context("WITHINGS_CLIENT_ID required")?,
            client_secret: std::env::var("WITHINGS_CLIENT_SECRET")
                .context("WITHINGS_CLIENT_SECRET required")?,
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL required")?,
            backfill_days: std::env::var("WITHINGS_BACKFILL_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            user_tz: std::env::var("WITHINGS_USER_TZ").unwrap_or_else(|_| "UTC".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_defaults() {
        let c = Config {
            client_id: "x".into(),
            client_secret: "y".into(),
            database_url: "mysql://u:p@h/db".into(),
            backfill_days: 30,
            user_tz: "UTC".into(),
        };
        assert_eq!(c.backfill_days, 30);
        assert_eq!(c.user_tz, "UTC");
    }
}
