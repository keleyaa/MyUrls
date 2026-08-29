use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use thiserror::Error;

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
pub use http::{build_app, build_app_with_static};
pub use ports::{ChallengeVerifier, CreateCounts, CreateResult, LinkStore};
pub use redis::RedisLinkStore;
pub use service::{
    Clock, CreateLinkContext, CreateLinkRequest, ResolveLinkContext, ResolveLinkRequest,
    ShortCodeGenerator, ShortLinkService,
};
pub use turnstile::CloudflareTurnstileVerifier;

#[derive(Debug, Error)]
enum StartupError {
    #[error("static assets are unavailable")]
    StaticAssetsUnavailable,
    #[error("server shutdown timed out")]
    ShutdownTimedOut,
    #[cfg(not(feature = "test-support"))]
    #[error("test adapter support is unavailable")]
    TestAdapterSupportUnavailable,
}

/// Constructs runtime dependencies, hosts the web application, and closes its store on shutdown.
pub async fn run(config: AppConfig) -> Result<(), AppError> {
    let web_root = web_root()?;
    let store = build_store(&config).await?;
    let verifier = build_challenge_verifier(&config)?;
    let service = Arc::new(ShortLinkService::with_defaults(
        config.clone(),
        Arc::clone(&store),
        verifier,
    ));
    let app = build_app_with_static(config.clone(), Arc::clone(&store), service, web_root);
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(AppError::runtime)?;

    serve_with_shutdown(listener, app, store, config.shutdown_timeout_ms).await
}

async fn serve_with_shutdown(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    store: Arc<dyn LinkStore>,
    shutdown_timeout_ms: u64,
) -> Result<(), AppError> {
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = shutdown_receiver.await;
        })
        .await
    });

    tokio::select! {
        result = &mut server => {
            let server_result = join_server(result);
            let close_result = close_store_until(
                &store,
                tokio::time::Instant::now() + Duration::from_millis(shutdown_timeout_ms),
            ).await;
            combine_shutdown_results(server_result, close_result)
        }
        () = shutdown_signal() => {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(shutdown_timeout_ms);
            let _ = shutdown_sender.send(());
            let (server_result, close_result) = tokio::join!(
                await_server_until(&mut server, deadline),
                close_store_until(&store, deadline),
            );
            combine_shutdown_results(server_result, close_result)
        }
    }
}

async fn await_server_until(
    server: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
    deadline: tokio::time::Instant,
) -> Result<(), AppError> {
    match tokio::time::timeout_at(deadline, &mut *server).await {
        Ok(result) => join_server(result),
        Err(_) => {
            tracing::error!(event = "server_shutdown_timed_out");
            server.abort();
            let _ = server.await;
            Err(AppError::runtime(StartupError::ShutdownTimedOut))
        }
    }
}

fn combine_shutdown_results(
    server_result: Result<(), AppError>,
    close_result: Result<(), AppError>,
) -> Result<(), AppError> {
    match (server_result, close_result) {
        (Err(server_error), _) => Err(server_error),
        (Ok(()), Err(store_error)) => Err(store_error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn join_server(
    result: Result<Result<(), std::io::Error>, tokio::task::JoinError>,
) -> Result<(), AppError> {
    result
        .map_err(AppError::runtime)?
        .map_err(AppError::runtime)
}

async fn close_store_until(
    store: &Arc<dyn LinkStore>,
    deadline: tokio::time::Instant,
) -> Result<(), AppError> {
    match tokio::time::timeout_at(deadline, store.close()).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => {
            tracing::error!(event = "store_shutdown_failed");
            Err(AppError::Store(StoreError::unavailable()))
        }
    }
}

async fn build_store(config: &AppConfig) -> Result<Arc<dyn LinkStore>, AppError> {
    #[cfg(feature = "test-support")]
    if config.test_store.is_some() {
        let clock: Clock = Arc::new(time::OffsetDateTime::now_utc);
        let store =
            testing::MemoryLinkStore::from_config(config, clock).map_err(AppError::runtime)?;
        return Ok(Arc::new(store));
    }

    #[cfg(not(feature = "test-support"))]
    if config.test_store.is_some() {
        return Err(AppError::runtime(
            StartupError::TestAdapterSupportUnavailable,
        ));
    }

    RedisLinkStore::connect(&config.redis_url, config.redis_timeout_ms)
        .await
        .map(|store| Arc::new(store) as Arc<dyn LinkStore>)
        .map_err(AppError::from)
}

fn build_challenge_verifier(config: &AppConfig) -> Result<Arc<dyn ChallengeVerifier>, AppError> {
    #[cfg(feature = "test-support")]
    if config.turnstile.mode == config::TurnstileMode::Test {
        return Ok(Arc::new(testing::FakeTurnstile::new()));
    }

    #[cfg(not(feature = "test-support"))]
    if config.turnstile.mode == config::TurnstileMode::Test {
        return Err(AppError::runtime(
            StartupError::TestAdapterSupportUnavailable,
        ));
    }

    Ok(Arc::new(CloudflareTurnstileVerifier::new(config)))
}

fn web_root() -> Result<PathBuf, AppError> {
    let root = env::var_os("WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/web/dist"));
    if root.is_dir() && root.join("index.html").is_file() {
        Ok(root)
    } else {
        Err(AppError::runtime(StartupError::StaticAssetsUnavailable))
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}

#[cfg(feature = "test-support")]
pub mod testing;
