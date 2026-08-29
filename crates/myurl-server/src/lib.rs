use std::net::SocketAddr;

use axum::{Router, routing::get};

pub mod config;
pub mod error;
pub mod ports;

pub use config::{AppConfig, ConfigError};
pub use error::{
    AppError, Challenge, ChallengeError, ChallengeProvider, ChallengeValidationError, DomainError,
    ErrorCode, ResponseMetadata, RuntimeError, StoreError,
};
pub use ports::{ChallengeVerifier, CreateCounts, CreateResult, LinkStore};

/// Builds the temporary HTTP application used to verify service liveness.
pub fn build_app() -> Router {
    Router::new().route("/health/live", get(|| async { "ok" }))
}

/// Binds the configured listener and serves the temporary application.
pub async fn run(config: AppConfig) -> Result<(), AppError> {
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::runtime)?;

    axum::serve(listener, build_app())
        .await
        .map_err(AppError::runtime)
}

pub mod testing {}
