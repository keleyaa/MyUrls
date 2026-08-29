#![cfg(feature = "test-support")]

use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use myurl_server::{
    AppConfig, LinkStore, ShortLinkService, build_app, build_app_with_static,
    ip::fingerprint_ip,
    testing::{FakeTurnstile, MemoryLinkStore},
};
use time::{OffsetDateTime, format_description::well_known::Iso8601};
use tower::ServiceExt;
use uuid::Uuid;

const PUBLIC_BASE_URL: &str = "http://localhost:3000";
const IP_HASH_SECRET: &str = "0123456789abcdef0123456789abcdef";

fn test_config() -> AppConfig {
    AppConfig::from_env([
        ("NODE_ENV", "test"),
        ("APP_PORT", "3000"),
        ("PUBLIC_BASE_URL", PUBLIC_BASE_URL),
        ("IP_HASH_SECRET", IP_HASH_SECRET),
        ("REDIS_URL", "redis://127.0.0.1:6379/0"),
        ("TURNSTILE_ENABLED", "false"),
        ("TURNSTILE_MODE", "test"),
        ("TEST_STORE", "memory"),
    ])
    .expect("test configuration is valid")
}

fn test_dependencies(
    config: &AppConfig,
) -> (Arc<dyn LinkStore>, MemoryLinkStore, Arc<ShortLinkService>) {
    let memory_store = MemoryLinkStore::from_config(config, Arc::new(OffsetDateTime::now_utc))
        .expect("test configuration enables the memory store");
    let store: Arc<dyn LinkStore> = Arc::new(memory_store.clone());
    let service = Arc::new(ShortLinkService::with_defaults(
        config.clone(),
        Arc::clone(&store),
        Arc::new(FakeTurnstile::new()),
    ));
    (store, memory_store, service)
}

fn static_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("myurl-http-test-{}", Uuid::new_v4()));
    fs::create_dir_all(root.join("assets")).expect("static assets directory can be created");
    fs::write(root.join("index.html"), "<main>MyUrls</main>").expect("index can be written");
    fs::write(root.join("assets/app.js"), "console.log('myurls')").expect("asset can be written");
    root
}

#[tokio::test]
async fn static_fallback_preserves_problem_details_for_unknown_api_routes() {
    let config = test_config();
    let (store, _, service) = test_dependencies(&config);
    let root = static_root();
    let app = build_app_with_static(config, store, service, root.clone());

    for path in ["/api", "/api/missing"] {
        let api_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("API request is valid"),
            )
            .await
            .expect("router responds");
        assert_eq!(api_response.status(), StatusCode::NOT_FOUND, "path: {path}");
        assert_eq!(
            api_response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "application/problem+json"
            )),
            "path: {path}"
        );
        let api_body = to_bytes(api_response.into_body(), usize::MAX)
            .await
            .expect("problem response body is readable");
        assert!(
            std::str::from_utf8(&api_body)
                .expect("problem response is UTF-8")
                .contains("\"code\":\"invalid_request\""),
            "path: {path}"
        );
    }

    let asset_response = app
        .oneshot(
            Request::builder()
                .uri("/assets/app.js")
                .body(Body::empty())
                .expect("asset request is valid"),
        )
        .await
        .expect("router responds");
    assert_eq!(asset_response.status(), StatusCode::OK);
    let asset_body = to_bytes(asset_response.into_body(), usize::MAX)
        .await
        .expect("asset response body is readable");
    assert_eq!(asset_body.as_ref(), b"console.log('myurls')");

    fs::remove_dir_all(root).expect("temporary static root can be removed");
}

#[tokio::test]
async fn connect_info_peer_address_is_used_for_create_rate_limit_fingerprint() {
    let config = test_config();
    let (store, memory_store, service) = test_dependencies(&config);
    let app = build_app(config.clone(), store, service);
    let peer = SocketAddr::from(([203, 0, 113, 9], 43123));
    let service = app
        .into_make_service_with_connect_info::<SocketAddr>()
        .oneshot(peer)
        .await
        .expect("connection service is created");

    let response = service
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/links")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, PUBLIC_BASE_URL)
                .body(Body::from(r#"{"url":"https://example.com"}"#))
                .expect("create request is valid"),
        )
        .await
        .expect("router responds");
    assert_eq!(response.status(), StatusCode::CREATED);

    let fingerprint = fingerprint_ip(&config.ip_hash_secret, "203.0.113.9");
    let date = OffsetDateTime::now_utc()
        .format(&Iso8601::DATE)
        .expect("UTC date can be formatted");
    assert!(
        memory_store
            .create_counter_ttl_seconds(&fingerprint, &date)
            .expect("memory store is readable")
            .is_some()
    );
}
