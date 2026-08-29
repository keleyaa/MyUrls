#![cfg(feature = "test-support")]

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use myurl_server::{
    AppConfig, AppError, ChallengeError, ChallengeVerifier, CreateLinkContext, CreateLinkRequest,
    CreateResult, ErrorCode, LinkStore, ResolveLinkContext, ResolveLinkRequest, ShortCodeGenerator,
    ShortLinkService,
    config::{AUTO_CODE_LENGTH, LINK_TTL_SECONDS, Limits},
    domain::short_code::{ShortCodeError, is_valid_code},
    ip::fingerprint_ip,
    testing::{
        DeterministicClock, FakeTurnstile, FakeTurnstileOutcome, MemoryLinkStore,
        SequenceShortCodeGenerator, StoreFailures,
    },
};
use time::{Duration as TimeDuration, macros::datetime};

const CLIENT_IP: &str = "198.51.100.4";

#[derive(Clone)]
struct AdvancingChallengeVerifier {
    clock: DeterministicClock,
    duration: TimeDuration,
}

impl AdvancingChallengeVerifier {
    fn new(clock: DeterministicClock, duration: TimeDuration) -> Self {
        Self { clock, duration }
    }
}

#[async_trait]
impl ChallengeVerifier for AdvancingChallengeVerifier {
    async fn verify(&self, token: &str) -> Result<bool, ChallengeError> {
        self.clock
            .advance(self.duration)
            .map_err(ChallengeError::from_source)?;
        Ok(token == "test-token")
    }
}

fn test_config() -> AppConfig {
    let environment = [
        ("NODE_ENV", "test"),
        ("PUBLIC_BASE_URL", "https://myurl.example"),
        (
            "IP_HASH_SECRET",
            "test-secret-that-is-at-least-32-bytes-long",
        ),
        ("TURNSTILE_ENABLED", "true"),
        ("TURNSTILE_MODE", "test"),
        ("TURNSTILE_SITE_KEY", "site-key"),
        ("TURNSTILE_SECRET_KEY", "turnstile-secret-key"),
        ("TURNSTILE_HOSTNAME", "myurl.example"),
        ("TEST_STORE", "memory"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect::<BTreeMap<_, _>>();

    AppConfig::from_env(environment).unwrap()
}

fn test_service<I, S>(
    config: AppConfig,
    codes: I,
) -> (ShortLinkService, MemoryLinkStore, FakeTurnstile)
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let clock = DeterministicClock::new(datetime!(2026-08-26 4:00 UTC));
    let store = MemoryLinkStore::from_config(&config, clock.as_clock()).unwrap();
    let verifier = FakeTurnstile::new();
    let generator = SequenceShortCodeGenerator::new(codes);
    let service = ShortLinkService::new(
        config,
        Arc::new(store.clone()),
        Arc::new(verifier.clone()),
        clock.as_clock(),
        generator.as_generator(),
    );

    (service, store, verifier)
}

fn create_request(url: &str) -> CreateLinkRequest {
    CreateLinkRequest {
        url: url.to_owned(),
        alias: None,
        challenge_token: None,
    }
}

fn create_context() -> CreateLinkContext {
    CreateLinkContext {
        client_ip: CLIENT_IP.to_owned(),
    }
}

fn resolve_context() -> ResolveLinkContext {
    ResolveLinkContext {
        client_ip: CLIENT_IP.to_owned(),
    }
}

fn error_code<T>(result: Result<T, AppError>) -> ErrorCode {
    match result {
        Ok(_) => panic!("expected application error"),
        Err(error) => error.code(),
    }
}

fn fingerprint(config: &AppConfig, client_ip: &str) -> String {
    fingerprint_ip(&config.ip_hash_secret, client_ip)
}

#[tokio::test]
async fn creates_an_automatic_link_with_a_fixed_expiry_and_stable_origin() {
    let config = test_config();
    let (service, store, _) = test_service(config, ["Abcd123456"]);

    let result = service
        .create(
            &create_request("HTTPS://Example.COM/docs?q=1"),
            &create_context(),
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        CreateResult {
            code: "Abcd123456".to_owned(),
            short_url: "https://myurl.example/Abcd123456".to_owned(),
            expires_at: "2026-11-24T04:00:00.000Z".to_owned(),
        }
    );
    assert_eq!(
        store.lookup("Abcd123456").await.unwrap(),
        Some("https://example.com/docs?q=1".to_owned())
    );
    assert_eq!(
        store.link_ttl_seconds("Abcd123456").unwrap(),
        Some(LINK_TTL_SECONDS)
    );
}

#[tokio::test]
async fn creates_a_link_with_expiry_matching_the_ttl_claimed_after_a_time_consuming_challenge() {
    let mut config = test_config();
    config.test_force_challenge = true;
    let clock = DeterministicClock::new(datetime!(2026-08-26 4:00 UTC));
    let store = MemoryLinkStore::from_config(&config, clock.as_clock()).unwrap();
    let service = ShortLinkService::new(
        config,
        Arc::new(store.clone()),
        Arc::new(AdvancingChallengeVerifier::new(
            clock.clone(),
            TimeDuration::seconds(5),
        )),
        clock.as_clock(),
        SequenceShortCodeGenerator::new(["Abcd123456"]).as_generator(),
    );
    let request = CreateLinkRequest {
        url: "https://example.com".to_owned(),
        alias: None,
        challenge_token: Some("test-token".to_owned()),
    };

    let result = service.create(&request, &create_context()).await.unwrap();

    assert_eq!(result.expires_at, "2026-11-24T04:00:05.000Z");
    assert_eq!(
        store.link_ttl_seconds("Abcd123456").unwrap(),
        Some(LINK_TTL_SECONDS)
    );
    clock
        .advance(TimeDuration::seconds(
            i64::try_from(LINK_TTL_SECONDS).expect("link TTL must fit in i64"),
        ))
        .unwrap();
    assert_eq!(store.lookup("Abcd123456").await.unwrap(), None);
}

#[tokio::test]
async fn uses_the_production_default_clock_and_secure_generator() {
    let config = test_config();
    let clock = DeterministicClock::new(datetime!(2026-08-26 4:00 UTC));
    let store = MemoryLinkStore::from_config(&config, clock.as_clock()).unwrap();
    let service =
        ShortLinkService::with_defaults(config, Arc::new(store), Arc::new(FakeTurnstile::new()));

    let result = service
        .create(
            &create_request("https://example.com/defaults"),
            &create_context(),
        )
        .await
        .unwrap();

    assert_eq!(result.code.len(), AUTO_CODE_LENGTH);
    assert!(is_valid_code(&result.code));
}

#[tokio::test]
async fn normalizes_and_claims_a_custom_alias() {
    let (service, store, _) = test_service(test_config(), std::iter::empty::<String>());
    let request = CreateLinkRequest {
        url: "https://example.com".to_owned(),
        alias: Some("  Launch_42 ".to_owned()),
        challenge_token: None,
    };

    let result = service.create(&request, &create_context()).await.unwrap();

    assert_eq!(result.code, "launch_42");
    assert_eq!(
        store.lookup("launch_42").await.unwrap(),
        Some("https://example.com/".to_owned())
    );
}

#[tokio::test]
async fn records_risk_for_rejected_urls_and_aliases_without_overwriting_links() {
    let mut config = test_config();
    config.turnstile.enabled = false;
    let expected_fingerprint = fingerprint(&config, CLIENT_IP);
    let (service, store, _) = test_service(config, std::iter::empty::<String>());
    store
        .claim(
            "launch",
            "https://already.example/",
            Duration::from_secs(LINK_TTL_SECONDS),
        )
        .await
        .unwrap();

    assert_eq!(
        error_code(
            service
                .create(&create_request("http://localhost/admin"), &create_context(),)
                .await
        ),
        ErrorCode::UrlNotAllowed
    );

    let invalid_alias = CreateLinkRequest {
        url: "https://example.com".to_owned(),
        alias: Some("bad".to_owned()),
        challenge_token: None,
    };
    assert_eq!(
        error_code(service.create(&invalid_alias, &create_context()).await),
        ErrorCode::AliasInvalid
    );

    let reserved_alias = CreateLinkRequest {
        url: "https://example.com".to_owned(),
        alias: Some("HEALTH".to_owned()),
        challenge_token: None,
    };
    assert_eq!(
        error_code(service.create(&reserved_alias, &create_context()).await),
        ErrorCode::AliasUnavailable
    );

    let taken_alias = CreateLinkRequest {
        url: "https://example.com".to_owned(),
        alias: Some("launch".to_owned()),
        challenge_token: None,
    };
    assert_eq!(
        error_code(service.create(&taken_alias, &create_context()).await),
        ErrorCode::AliasUnavailable
    );
    assert_eq!(store.risk_score(&expected_fingerprint).await.unwrap(), 4);
    assert_eq!(
        store.lookup("launch").await.unwrap(),
        Some("https://already.example/".to_owned())
    );
}

#[tokio::test]
async fn does_not_record_risk_for_an_oversized_invalid_request_url() {
    let config = test_config();
    let expected_fingerprint = fingerprint(&config, CLIENT_IP);
    let (service, store, _) = test_service(config, std::iter::empty::<String>());
    let oversized = format!("https://example.com/{}", "a".repeat(4_077));

    assert_eq!(
        error_code(
            service
                .create(&create_request(&oversized), &create_context())
                .await
        ),
        ErrorCode::InvalidRequest
    );
    assert_eq!(store.risk_score(&expected_fingerprint).await.unwrap(), 0);
}

#[tokio::test]
async fn requires_and_accepts_a_valid_challenge_after_the_direct_threshold() {
    let mut config = test_config();
    config.limits = Limits {
        direct_10m: 1,
        hard_10m: 20,
        hard_1d: 100,
        resolve_10s: 600,
        challenge_score: 3,
        block_score: 8,
    };
    let (service, _, verifier) = test_service(config, ["Code011234", "Code021234"]);

    service
        .create(
            &create_request("https://example.com/first"),
            &create_context(),
        )
        .await
        .unwrap();
    assert_eq!(
        error_code(
            service
                .create(
                    &create_request("https://example.com/second"),
                    &create_context()
                )
                .await
        ),
        ErrorCode::ChallengeRequired
    );

    let valid_request = CreateLinkRequest {
        url: "https://example.com/third".to_owned(),
        alias: None,
        challenge_token: Some("test-token".to_owned()),
    };
    let result = service
        .create(&valid_request, &create_context())
        .await
        .unwrap();

    assert_eq!(result.code, "Code021234");
    assert_eq!(verifier.call_count().unwrap(), 1);
}

#[tokio::test]
async fn rejects_invalid_challenges_and_records_three_risk_points() {
    let mut config = test_config();
    config.limits.direct_10m = 1;
    let expected_fingerprint = fingerprint(&config, CLIENT_IP);
    let (service, store, _) = test_service(config, ["Code011234"]);

    service
        .create(
            &create_request("https://example.com/first"),
            &create_context(),
        )
        .await
        .unwrap();
    let invalid_request = CreateLinkRequest {
        url: "https://example.com/second".to_owned(),
        alias: None,
        challenge_token: Some("invalid-token".to_owned()),
    };

    assert_eq!(
        error_code(service.create(&invalid_request, &create_context()).await),
        ErrorCode::ChallengeInvalid
    );
    assert_eq!(store.risk_score(&expected_fingerprint).await.unwrap(), 3);
}

#[tokio::test]
async fn maps_unavailable_challenge_verification_to_a_dependency_error() {
    let mut config = test_config();
    config.limits.direct_10m = 1;
    let clock = DeterministicClock::new(datetime!(2026-08-26 4:00 UTC));
    let store = MemoryLinkStore::from_config(&config, clock.as_clock()).unwrap();
    let verifier = FakeTurnstile::with_outcome(FakeTurnstileOutcome::Unavailable);
    let generator = SequenceShortCodeGenerator::new(["Code011234"]);
    let service = ShortLinkService::new(
        config,
        Arc::new(store),
        Arc::new(verifier),
        clock.as_clock(),
        generator.as_generator(),
    );

    service
        .create(
            &create_request("https://example.com/first"),
            &create_context(),
        )
        .await
        .unwrap();
    let request = CreateLinkRequest {
        url: "https://example.com/second".to_owned(),
        alias: None,
        challenge_token: Some("test-token".to_owned()),
    };

    assert_eq!(
        error_code(service.create(&request, &create_context()).await),
        ErrorCode::DependencyUnavailable
    );
}

#[tokio::test]
async fn blocks_hard_limits_and_existing_risk_scores() {
    let mut config = test_config();
    config.limits = Limits {
        direct_10m: 1,
        hard_10m: 2,
        hard_1d: 3,
        resolve_10s: 600,
        challenge_score: 3,
        block_score: 8,
    };
    let (service, _, _) = test_service(config.clone(), ["Code011234"]);

    service
        .create(
            &create_request("https://example.com/first"),
            &create_context(),
        )
        .await
        .unwrap();
    assert_eq!(
        error_code(
            service
                .create(
                    &create_request("https://example.com/second"),
                    &create_context()
                )
                .await
        ),
        ErrorCode::ChallengeRequired
    );
    assert_eq!(
        error_code(
            service
                .create(
                    &create_request("https://example.com/third"),
                    &create_context()
                )
                .await
        ),
        ErrorCode::RateLimited
    );

    let risk_client_ip = "203.0.113.10";
    let risk_fingerprint = fingerprint(&config, risk_client_ip);
    let (risk_service, risk_store, _) = test_service(config, ["Code567890"]);
    risk_store.set_risk_score(risk_fingerprint, 8).unwrap();
    let risk_context = CreateLinkContext {
        client_ip: risk_client_ip.to_owned(),
    };
    assert_eq!(
        error_code(
            risk_service
                .create(&create_request("https://example.com"), &risk_context)
                .await
        ),
        ErrorCode::RateLimited
    );
}

#[tokio::test]
async fn retries_generated_code_collisions_and_preserves_the_existing_target() {
    let config = test_config();
    let (service, store, _) = test_service(config, ["taken12345", "taken12345", "fresh12345"]);
    store
        .claim(
            "taken12345",
            "https://old.example/",
            Duration::from_secs(LINK_TTL_SECONDS),
        )
        .await
        .unwrap();

    let result = service
        .create(&create_request("https://example.com"), &create_context())
        .await
        .unwrap();

    assert_eq!(result.code, "fresh12345");
    assert_eq!(
        store.lookup("taken12345").await.unwrap(),
        Some("https://old.example/".to_owned())
    );
}

#[tokio::test]
async fn exhausts_generated_code_attempts_without_overwriting_the_existing_target() {
    let config = test_config();
    let (service, store, _) = test_service(
        config,
        [
            "taken12345",
            "taken12345",
            "taken12345",
            "taken12345",
            "taken12345",
        ],
    );
    store
        .claim(
            "taken12345",
            "https://old.example/",
            Duration::from_secs(LINK_TTL_SECONDS),
        )
        .await
        .unwrap();

    assert_eq!(
        error_code(
            service
                .create(&create_request("https://example.com"), &create_context())
                .await
        ),
        ErrorCode::CodeGenerationExhausted
    );
    assert_eq!(
        store.lookup("taken12345").await.unwrap(),
        Some("https://old.example/".to_owned())
    );
}

#[tokio::test]
async fn skips_reserved_and_malformed_generated_codes() {
    let (service, _, _) = test_service(test_config(), ["api", "bad.code!!", "fresh12345"]);

    let result = service
        .create(&create_request("https://example.com"), &create_context())
        .await
        .unwrap();

    assert_eq!(result.code, "fresh12345");
}

#[tokio::test]
async fn returns_a_client_safe_error_when_code_generation_fails() {
    let config = test_config();
    let clock = DeterministicClock::new(datetime!(2026-08-26 4:00 UTC));
    let store = MemoryLinkStore::from_config(&config, clock.as_clock()).unwrap();
    let generator: ShortCodeGenerator = Arc::new(|| Err(ShortCodeError::RandomnessUnavailable));
    let service = ShortLinkService::new(
        config,
        Arc::new(store),
        Arc::new(FakeTurnstile::new()),
        clock.as_clock(),
        generator,
    );

    assert_eq!(
        error_code(
            service
                .create(&create_request("https://example.com"), &create_context())
                .await
        ),
        ErrorCode::DependencyUnavailable
    );
}

#[tokio::test]
async fn maps_risk_recording_failures_to_dependency_unavailable() {
    let config = test_config();
    let (service, store, _) = test_service(config, std::iter::empty::<String>());
    store
        .set_failures(StoreFailures {
            add_risk_score: true,
            ..StoreFailures::default()
        })
        .unwrap();

    assert_eq!(
        error_code(
            service
                .create(&create_request("https://localhost"), &create_context())
                .await
        ),
        ErrorCode::DependencyUnavailable
    );
}

#[tokio::test]
async fn maps_create_store_failures_to_dependency_unavailable() {
    for failures in [
        StoreFailures {
            increment_create_counters: true,
            ..StoreFailures::default()
        },
        StoreFailures {
            risk_score: true,
            ..StoreFailures::default()
        },
        StoreFailures {
            claim: true,
            ..StoreFailures::default()
        },
    ] {
        let (service, store, _) = test_service(test_config(), ["Code011234"]);
        store.set_failures(failures).unwrap();

        assert_eq!(
            error_code(
                service
                    .create(&create_request("https://example.com"), &create_context())
                    .await
            ),
            ErrorCode::DependencyUnavailable
        );
    }
}

#[tokio::test]
async fn resolves_existing_and_missing_codes_and_rejects_invalid_codes() {
    let (service, store, _) = test_service(test_config(), std::iter::empty::<String>());
    store
        .claim(
            "valid1",
            "https://example.com/",
            Duration::from_secs(LINK_TTL_SECONDS),
        )
        .await
        .unwrap();

    assert_eq!(
        service
            .resolve(
                &ResolveLinkRequest {
                    code: "valid1".to_owned(),
                },
                &resolve_context(),
            )
            .await
            .unwrap(),
        Some("https://example.com/".to_owned())
    );
    assert_eq!(
        service
            .resolve(
                &ResolveLinkRequest {
                    code: "missing".to_owned(),
                },
                &resolve_context(),
            )
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        service
            .resolve(
                &ResolveLinkRequest {
                    code: "bad".to_owned(),
                },
                &resolve_context(),
            )
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn rate_limits_resolution_before_code_validation() {
    let mut config = test_config();
    config.limits.resolve_10s = 1;
    let (service, store, _) = test_service(config, std::iter::empty::<String>());
    store
        .claim(
            "valid1",
            "https://example.com/",
            Duration::from_secs(LINK_TTL_SECONDS),
        )
        .await
        .unwrap();

    assert_eq!(
        service
            .resolve(
                &ResolveLinkRequest {
                    code: "bad".to_owned(),
                },
                &resolve_context(),
            )
            .await
            .unwrap(),
        None
    );
    let error = service
        .resolve(
            &ResolveLinkRequest {
                code: "valid1".to_owned(),
            },
            &resolve_context(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RateLimited);
    assert_eq!(error.retry_after_seconds(), Some(10));
}

#[tokio::test]
async fn maps_resolve_counter_and_lookup_failures_to_dependency_unavailable() {
    let (counter_service, counter_store, _) =
        test_service(test_config(), std::iter::empty::<String>());
    counter_store
        .set_failures(StoreFailures {
            increment_resolve_counter: true,
            ..StoreFailures::default()
        })
        .unwrap();
    assert_eq!(
        error_code(
            counter_service
                .resolve(
                    &ResolveLinkRequest {
                        code: "valid1".to_owned(),
                    },
                    &resolve_context(),
                )
                .await
        ),
        ErrorCode::DependencyUnavailable
    );

    let (lookup_service, lookup_store, _) =
        test_service(test_config(), std::iter::empty::<String>());
    lookup_store
        .set_failures(StoreFailures {
            lookup: true,
            ..StoreFailures::default()
        })
        .unwrap();
    assert_eq!(
        error_code(
            lookup_service
                .resolve(
                    &ResolveLinkRequest {
                        code: "valid1".to_owned(),
                    },
                    &resolve_context(),
                )
                .await
        ),
        ErrorCode::DependencyUnavailable
    );
}
