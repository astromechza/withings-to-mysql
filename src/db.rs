use anyhow::Result;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::MySqlPool;
use std::str::FromStr;

pub async fn connect(database_url: &str) -> Result<MySqlPool> {
    let opts = MySqlConnectOptions::from_str(database_url)?.timezone(Some("+00:00".into()));
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
