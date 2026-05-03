use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod cmd;
pub mod config;
pub mod db;
pub mod state;
pub mod withings;

#[derive(Parser, Debug)]
#[command(name = "withings-to-mysql", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Print OAuth consent URL (no DB needed).
    AuthUrl {
        #[arg(long, env = "WITHINGS_CLIENT_ID")]
        client_id: String,
        #[arg(long)]
        redirect_uri: String,
        #[arg(long, default_value = "user.metrics,user.activity,user.info")]
        scope: String,
        #[arg(long)]
        state: Option<String>,
    },
    /// Exchange auth code for tokens, store in DB.
    Exchange {
        #[arg(long, env = "WITHINGS_CLIENT_ID")]
        client_id: String,
        #[arg(long, env = "WITHINGS_CLIENT_SECRET")]
        client_secret: String,
        #[arg(long)]
        redirect_uri: String,
        code: String,
    },
    /// Sync all Withings endpoints to MySQL.
    Sync,
    /// Print state from DB (tokens redacted).
    DumpState,
}

pub async fn run() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    match cli.command {
        Cmd::AuthUrl {
            client_id,
            redirect_uri,
            scope,
            state,
        } => cmd::auth_url::run(&client_id, &redirect_uri, &scope, state.as_deref()),
        Cmd::Exchange {
            client_id,
            client_secret,
            redirect_uri,
            code,
        } => cmd::exchange::run(&client_id, &client_secret, &redirect_uri, &code).await,
        Cmd::Sync => cmd::sync::run().await,
        Cmd::DumpState => cmd::dump_state::run().await,
    }
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
}
