#![cfg(feature = "test-support")]

use std::{collections::BTreeMap, fs, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use myurl_server::{
    AppConfig, ChallengeError, ChallengeVerifier, LinkStore, ShortLinkService, build_app,
    build_app_with_static,
    ip::fingerprint_ip,
    testing::{FakeTurnstile, FakeTurnstileOutcome, MemoryLinkStore, StoreFailures},
};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Iso8601};
use tower::ServiceExt;
use uuid::Uuid;

const PUBLIC_BASE_URL: &str = "http://localhost:3000";
const IP_HASH_SECRET: &str = "0123456789abcdef0123456789abcdef";

struct SlowChallengeVerifier;

#[async_trait::async_trait]
impl ChallengeVerifier for SlowChallengeVerifier {
    async fn verify(&self, _token: &str) -> Result<bool, ChallengeError> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(true)
    }
}

fn test_config() -> AppConfig {
    test_config_with(&[])
}

fn test_config_with(overrides: &[(&str, &str)]) -> AppConfig {
    let mut environment = [
        ("NODE_ENV", "test"),
        ("APP_PORT", "3000"),
        ("PUBLIC_BASE_URL", PUBLIC_BASE_URL),
        ("IP_HASH_SECRET", IP_HASH_SECRET),
        ("REDIS_URL", "redis://127.0.0.1:6379/0"),
        ("TURNSTILE_ENABLED", "false"),
        ("TURNSTILE_MODE", "test"),
        ("TEST_STORE", "memory"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect::<BTreeMap<_, _>>();
    for (name, value) in overrides {
        environment.insert((*name).to_owned(), (*value).to_owned());
    }

    AppConfig::from_env(environment).expect("test configuration is valid")
}

fn test_app(config: AppConfig, verifier: FakeTurnstile) -> (Router, MemoryLinkStore) {
    test_app_with_verifier(config, Arc::new(verifier))
}

fn test_app_with_verifier(
    config: AppConfig,
    verifier: Arc<dyn ChallengeVerifier>,
) -> (Router, MemoryLinkStore) {
    let memory_store = MemoryLinkStore::from_config(&config, Arc::new(OffsetDateTime::now_utc))
        .expect("test configuration enables the memory store");
    let store: Arc<dyn LinkStore> = Arc::new(memory_store.clone());
    let service = Arc::new(ShortLinkService::with_defaults(
        config.clone(),
        Arc::clone(&store),
        verifier,
    ));
    (build_app(config, store, service), memory_store)
}

fn static_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("myurl-http-test-{}", Uuid::new_v4()));
    fs::create_dir_all(root.join("assets")).expect("static assets directory can be created");
    fs::write(root.join("index.html"), "<main>MyUrls</main>").expect("index can be written");
    fs::write(root.join("robots.txt"), "User-agent: *\nDisallow:\n")
        .expect("robots file can be written");
    fs::write(root.join("sitemap.xml"), "<urlset></urlset>").expect("sitemap file can be written");
    fs::write(root.join("assets/app.js"), "console.log('myurls')").expect("asset can be written");
    root
}

fn create_request(body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/links")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, PUBLIC_BASE_URL)
        .body(body.into())
        .expect("create request is valid")
}

async fn call(app: &Router, request: Request<Body>) -> Response {
    app.clone().oneshot(request).await.expect("router responds")
}

async fn response_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body is readable");
    serde_json::from_slice(&body).expect("response body is JSON")
}

async fn assert_problem(response: Response, status: StatusCode, code: &str) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static(
            "application/problem+json"
        ))
    );
    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("problem response has request ID")
        .to_str()
        .expect("request ID is text")
        .to_owned();
    assert!(!request_id.is_empty());

    let body = response_json(response).await;
    assert_eq!(body["status"], status.as_u16());
    assert_eq!(body["code"], code);
    assert_eq!(body["requestId"], request_id);
    assert_eq!(body["type"], format!("{PUBLIC_BASE_URL}/problems/{code}"));
    assert!(body.get("detail").is_none());
    body
}

async fn close(store: &MemoryLinkStore) {
    store.close().await.expect("test store closes");
}

#[tokio::test]
async fn creates_links_with_configured_origin_while_ignoring_host() {
    let (app, store) = test_app(test_config(), FakeTurnstile::new());
    let response = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/links")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_BASE_URL)
            .header(header::HOST, "attacker.example")
            .header("x-request-id", "accepted-id")
            .body(Body::from(r#"{"url":"https://example.com/path"}"#))
            .expect("create request is valid"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.headers().get("x-request-id"),
        Some(&header::HeaderValue::from_static("accepted-id"))
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    assert!(
        response
            .headers()
            .contains_key(header::CONTENT_SECURITY_POLICY)
    );
    let body = response_json(response).await;
    assert!(body["code"].as_str().is_some_and(|code| code.len() == 10));
    assert!(
        body["shortUrl"]
            .as_str()
            .is_some_and(|url| url.starts_with(PUBLIC_BASE_URL))
    );
    assert!(
        body["expiresAt"]
            .as_str()
            .is_some_and(|date| date.ends_with('Z'))
    );
    close(&store).await;
}

#[tokio::test]
async fn request_timeout_limits_slow_challenge_verification() {
    let config = test_config_with(&[
        ("TURNSTILE_ENABLED", "true"),
        ("TURNSTILE_SITE_KEY", "test-site-key"),
        ("TURNSTILE_SECRET_KEY", "test-secret-key"),
        ("TEST_FORCE_CHALLENGE", "true"),
        ("REQUEST_TIMEOUT_MS", "10"),
    ]);
    let (app, store) = test_app_with_verifier(config, Arc::new(SlowChallengeVerifier));
    let response = call(
        &app,
        create_request(Body::from(
            r#"{"url":"https://example.com/slow","challengeToken":"test-token"}"#,
        )),
    )
    .await;

    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    close(&store).await;
}

#[tokio::test]
async fn rejects_invalid_json_bodies_origins_and_cross_origin_requests() {
    let (app, store) = test_app(test_config(), FakeTurnstile::new());
    let requests = [
        Request::builder()
            .method("POST")
            .uri("/api/links")
            .header(header::ORIGIN, PUBLIC_BASE_URL)
            .body(Body::from(r#"{"url":"https://example.com"}"#))
            .expect("non-JSON request is valid"),
        Request::builder()
            .method("POST")
            .uri("/api/links")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_BASE_URL)
            .body(Body::from("{"))
            .expect("malformed request is valid"),
        create_request(Body::from(r#"{"url":"https://example.com","extra":true}"#)),
        Request::builder()
            .method("POST")
            .uri("/api/links")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, "https://attacker.example")
            .body(Body::from(r#"{"url":"https://example.com"}"#))
            .expect("cross-origin request is valid"),
        create_request(Body::from(format!(
            r#"{{"url":"https://example.com","padding":"{}"}}"#,
            "x".repeat(16 * 1024)
        ))),
    ];

    for request in requests {
        assert_problem(
            call(&app, request).await,
            StatusCode::BAD_REQUEST,
            "invalid_request",
        )
        .await;
    }
    close(&store).await;
}

#[tokio::test]
async fn maps_url_alias_validation_and_alias_conflicts_without_echoing_input() {
    let (app, store) = test_app(test_config(), FakeTurnstile::new());
    assert_problem(
        call(
            &app,
            create_request(Body::from(r#"{"url":"ftp://example.com"}"#)),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "url_not_allowed",
    )
    .await;
    assert_problem(
        call(
            &app,
            create_request(Body::from(
                r#"{"url":"https://example.com","alias":"not valid"}"#,
            )),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "alias_invalid",
    )
    .await;

    store
        .claim("taken", "https://already.example", Duration::from_secs(60))
        .await
        .expect("fixture alias is claimed");
    let response = call(
        &app,
        create_request(Body::from(
            r#"{"url":"https://private.example/secret","alias":"taken","challengeToken":"secret-token"}"#,
        )),
    )
    .await;
    let body = assert_problem(response, StatusCode::CONFLICT, "alias_unavailable").await;
    let text = body.to_string();
    assert!(!text.contains("private.example"));
    assert!(!text.contains("secret-token"));
    close(&store).await;
}

#[tokio::test]
async fn reports_challenge_required_invalid_and_unavailable_with_expected_calls() {
    let config = test_config_with(&[
        ("TURNSTILE_ENABLED", "true"),
        ("TURNSTILE_SITE_KEY", "site-key"),
        ("TURNSTILE_SECRET_KEY", "secret-key"),
        ("TEST_FORCE_CHALLENGE", "true"),
    ]);

    let required_verifier = FakeTurnstile::new();
    let (required_app, required_store) = test_app(config.clone(), required_verifier.clone());
    let required = assert_problem(
        call(
            &required_app,
            create_request(Body::from(r#"{"url":"https://example.com"}"#)),
        )
        .await,
        StatusCode::FORBIDDEN,
        "challenge_required",
    )
    .await;
    assert_eq!(required["challenge"]["provider"], "turnstile");
    assert_eq!(
        required_verifier.call_count().expect("calls are readable"),
        0
    );
    close(&required_store).await;

    let invalid_verifier = FakeTurnstile::with_outcome(FakeTurnstileOutcome::Invalid);
    let (invalid_app, invalid_store) = test_app(config.clone(), invalid_verifier.clone());
    let invalid = assert_problem(
        call(
            &invalid_app,
            create_request(Body::from(
                r#"{"url":"https://example.com","challengeToken":"test-token"}"#,
            )),
        )
        .await,
        StatusCode::FORBIDDEN,
        "challenge_invalid",
    )
    .await;
    assert_eq!(invalid["challenge"]["siteKey"], "site-key");
    assert_eq!(
        invalid_verifier.call_count().expect("calls are readable"),
        1
    );
    close(&invalid_store).await;

    let unavailable_verifier = FakeTurnstile::with_outcome(FakeTurnstileOutcome::Unavailable);
    let (unavailable_app, unavailable_store) =
        test_app(config.clone(), unavailable_verifier.clone());
    assert_problem(
        call(
            &unavailable_app,
            create_request(Body::from(
                r#"{"url":"https://example.com","challengeToken":"test-token"}"#,
            )),
        )
        .await,
        StatusCode::SERVICE_UNAVAILABLE,
        "dependency_unavailable",
    )
    .await;
    assert_eq!(
        unavailable_verifier
            .call_count()
            .expect("calls are readable"),
        1
    );
    close(&unavailable_store).await;

    let successful_verifier = FakeTurnstile::new();
    let (successful_app, successful_store) = test_app(config, successful_verifier.clone());
    let successful = call(
        &successful_app,
        create_request(Body::from(
            r#"{"url":"https://example.com","challengeToken":"test-token"}"#,
        )),
    )
    .await;
    assert_eq!(successful.status(), StatusCode::CREATED);
    assert_eq!(
        successful_verifier
            .call_count()
            .expect("calls are readable"),
        1
    );
    close(&successful_store).await;
}

#[tokio::test]
async fn create_hard_limit_returns_retry_after() {
    let config = test_config_with(&[
        ("CREATE_DIRECT_LIMIT_10M", "1"),
        ("CREATE_HARD_LIMIT_10M", "2"),
        ("CREATE_HARD_LIMIT_1D", "3"),
    ]);
    let (app, store) = test_app(config, FakeTurnstile::new());

    for _ in 0..2 {
        let response = call(
            &app,
            create_request(Body::from(r#"{"url":"https://example.com"}"#)),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let limited = call(
        &app,
        create_request(Body::from(r#"{"url":"https://example.com"}"#)),
    )
    .await;
    assert_eq!(
        limited.headers().get(header::RETRY_AFTER),
        Some(&header::HeaderValue::from_static("120"))
    );
    assert_problem(limited, StatusCode::TOO_MANY_REQUESTS, "rate_limited").await;
    close(&store).await;
}

#[tokio::test]
async fn exposes_health_redirect_head_not_found_rate_limit_and_dependency_failure_contracts() {
    let config = test_config_with(&[("RESOLVE_LIMIT_10S", "2")]);
    let (app, store) = test_app(config, FakeTurnstile::new());
    store
        .claim(
            "abcdefghij",
            "https://example.com/destination",
            Duration::from_secs(60),
        )
        .await
        .expect("fixture link is claimed");

    let live = call(
        &app,
        Request::builder()
            .uri("/health/live")
            .body(Body::empty())
            .expect("live request is valid"),
    )
    .await;
    assert_eq!(live.status(), StatusCode::OK);
    assert_eq!(response_json(live).await["status"], "ok");

    let ready = call(
        &app,
        Request::builder()
            .uri("/health/ready")
            .body(Body::empty())
            .expect("ready request is valid"),
    )
    .await;
    assert_eq!(ready.status(), StatusCode::OK);

    let missing = call(
        &app,
        Request::builder()
            .uri("/not-a-valid-code!")
            .body(Body::empty())
            .expect("missing request is valid"),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static(
            "text/html; charset=utf-8"
        ))
    );

    let redirect = call(
        &app,
        Request::builder()
            .uri("/abcdefghij")
            .body(Body::empty())
            .expect("redirect request is valid"),
    )
    .await;
    assert_eq!(redirect.status(), StatusCode::FOUND);
    assert_eq!(
        redirect.headers().get(header::LOCATION),
        Some(&header::HeaderValue::from_static(
            "https://example.com/destination"
        ))
    );

    let limited = call(
        &app,
        Request::builder()
            .method("HEAD")
            .uri("/abcdefghij")
            .body(Body::empty())
            .expect("head request is valid"),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited.headers().get(header::RETRY_AFTER),
        Some(&header::HeaderValue::from_static("10"))
    );
    assert!(
        to_bytes(limited.into_body(), usize::MAX)
            .await
            .expect("head body is readable")
            .is_empty()
    );

    close(&store).await;

    let (unavailable_app, unavailable_store) = test_app(test_config(), FakeTurnstile::new());
    unavailable_store
        .set_failures(StoreFailures {
            lookup: true,
            ..StoreFailures::default()
        })
        .expect("lookup failure is configured");
    let unavailable = call(
        &unavailable_app,
        Request::builder()
            .uri("/abcdefghij")
            .body(Body::empty())
            .expect("unavailable request is valid"),
    )
    .await;
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    close(&unavailable_store).await;
}

#[tokio::test]
async fn readiness_reports_degraded_when_store_is_unavailable() {
    let (app, store) = test_app(test_config(), FakeTurnstile::new());
    store
        .set_failures(StoreFailures {
            ping: true,
            ..StoreFailures::default()
        })
        .expect("ping failure is configured");
    let response = call(
        &app,
        Request::builder()
            .uri("/health/ready")
            .body(Body::empty())
            .expect("ready request is valid"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response_json(response).await["status"], "degraded");
    close(&store).await;
}

#[tokio::test]
async fn static_fallback_preserves_problem_details_for_unknown_api_routes() {
    let config = test_config();
    let root = static_root();
    let memory_store = MemoryLinkStore::from_config(&config, Arc::new(OffsetDateTime::now_utc))
        .expect("test configuration enables the memory store");
    let store: Arc<dyn LinkStore> = Arc::new(memory_store.clone());
    let service = Arc::new(ShortLinkService::with_defaults(
        config.clone(),
        Arc::clone(&store),
        Arc::new(FakeTurnstile::new()),
    ));
    let app = build_app_with_static(config, store, service, root.clone());

    for path in ["/api", "/api/missing"] {
        assert_problem(
            call(
                &app,
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("API request is valid"),
            )
            .await,
            StatusCode::NOT_FOUND,
            "invalid_request",
        )
        .await;
    }

    for (path, expected_body) in [
        ("/robots.txt", b"User-agent: *\nDisallow:\n".as_slice()),
        ("/sitemap.xml", b"<urlset></urlset>".as_slice()),
        ("/assets/app.js", b"console.log('myurls')".as_slice()),
    ] {
        let asset_response = call(
            &app,
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("asset request is valid"),
        )
        .await;
        assert_eq!(asset_response.status(), StatusCode::OK, "{path} is served");
        assert!(
            asset_response
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY),
            "{path} has a content security policy"
        );
        assert_eq!(
            asset_response.headers().get(header::X_FRAME_OPTIONS),
            Some(&header::HeaderValue::from_static("DENY")),
            "{path} denies framing"
        );
        let expected_cache_control = if path.starts_with("/assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-store"
        };
        assert_eq!(
            asset_response.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static(expected_cache_control)),
            "{path} has the expected cache policy"
        );
        let asset_body = to_bytes(asset_response.into_body(), usize::MAX)
            .await
            .expect("asset response body is readable");
        assert_eq!(asset_body.as_ref(), expected_body, "{path} body matches");
    }

    close(&memory_store).await;
    fs::remove_dir_all(root).expect("temporary static root can be removed");
}

#[tokio::test]
async fn connect_info_peer_address_is_used_for_create_rate_limit_fingerprint() {
    let config = test_config();
    let (app, memory_store) = test_app(config.clone(), FakeTurnstile::new());
    let peer = SocketAddr::from(([203, 0, 113, 9], 43123));
    let service = app
        .into_make_service_with_connect_info::<SocketAddr>()
        .oneshot(peer)
        .await
        .expect("connection service is created");

    let response = service
        .oneshot(create_request(Body::from(
            r#"{"url":"https://example.com"}"#,
        )))
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
    close(&memory_store).await;
}
