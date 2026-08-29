use std::{error::Error, fmt, net::SocketAddr};

use axum::{Router, routing::get};

/// Temporary process configuration until full environment parsing is added.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub port: u16,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { port: 3000 }
    }
}

/// Opaque application error placeholder for the initial server scaffold.
#[derive(Debug)]
pub struct AppError;

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("application error")
    }
}

impl Error for AppError {}

/// Builds the temporary HTTP application used to verify service liveness.
pub fn build_app() -> Router {
    Router::new().route("/health/live", get(|| async { "ok" }))
}

/// Binds the configured listener and serves the temporary application.
pub async fn run(config: AppConfig) -> Result<(), AppError> {
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|_| AppError)?;

    axum::serve(listener, build_app())
        .await
        .map_err(|_| AppError)
}

pub mod testing {}
