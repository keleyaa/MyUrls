use std::process::ExitCode;

use myurl_server::{AppConfig, run};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

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
