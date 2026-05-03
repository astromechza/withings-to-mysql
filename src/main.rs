#[tokio::main]
async fn main() -> anyhow::Result<()> {
    withings_to_mysql::run().await
}
