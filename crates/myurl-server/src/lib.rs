use std::net::SocketAddr;

use axum::{Router, routing::get};

pub mod config;
pub mod domain;
pub mod error;
pub mod http;
pub mod ip;
pub mod ports;
pub mod redis;
pub mod service;
pub mod turnstile;

pub use config::{AppConfig, ConfigError};
pub use error::{
    AppError, Challenge, ChallengeError, ChallengeProvider, ChallengeValidationError, DomainError,
    ErrorCode, ResponseMetadata, RuntimeError, StoreError,
};
pub use http::build_app;
pub use ports::{ChallengeVerifier, CreateCounts, CreateResult, LinkStore};
pub use redis::RedisLinkStore;
pub use service::{
    Clock, CreateLinkContext, CreateLinkRequest, ResolveLinkContext, ResolveLinkRequest,
    ShortCodeGenerator, ShortLinkService,
};
pub use turnstile::CloudflareTurnstileVerifier;

/// Binds the configured listener and serves the temporary liveness application.
///
/// Task 14 replaces this transitional path once it can construct the runtime
/// store and challenge verifier before calling [`build_app`].
pub async fn run(config: AppConfig) -> Result<(), AppError> {
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::runtime)?;

    let liveness_app = Router::new().route("/health/live", get(|| async { "ok" }));
    axum::serve(listener, liveness_app)
        .await
        .map_err(AppError::runtime)
}

#[cfg(feature = "test-support")]
pub mod testing;
