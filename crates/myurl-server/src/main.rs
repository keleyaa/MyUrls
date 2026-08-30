use std::{env, process::ExitCode};

use myurl_server::{AppConfig, run};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    let filter = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let Ok(config) = AppConfig::from_process_env() else {
        tracing::error!(event = "configuration_invalid", "configuration is invalid");
        return ExitCode::FAILURE;
    };
    if run(config).await.is_err() {
        tracing::error!(event = "application_stopped", "application stopped");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
