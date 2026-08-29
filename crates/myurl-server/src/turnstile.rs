use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    config::{AppConfig, NodeEnvironment},
    error::ChallengeError,
    ports::ChallengeVerifier,
};

const SITEVERIFY_ENDPOINT: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
const CREATE_LINK_ACTION: &str = "create_link";

/// Verifies Turnstile challenge tokens against Cloudflare's fixed siteverify endpoint.
pub struct CloudflareTurnstileVerifier {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    secret_key: String,
    hostname: String,
    node_env: NodeEnvironment,
    timeout: Duration,
}

#[derive(Deserialize)]
struct TurnstileResponse {
    success: bool,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    action: Option<String>,
}

impl CloudflareTurnstileVerifier {
    pub fn new(config: &AppConfig) -> Self {
        Self::from_config(
            config,
            reqwest::Url::parse(SITEVERIFY_ENDPOINT)
                .expect("fixed Turnstile endpoint must be valid"),
        )
    }

    fn from_config(config: &AppConfig, endpoint: reqwest::Url) -> Self {
        Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Turnstile HTTP client configuration must be valid"),
            endpoint,
            secret_key: config.turnstile.secret_key.clone(),
            hostname: config.turnstile.hostname.clone(),
            node_env: config.node_env,
            timeout: Duration::from_millis(config.turnstile_timeout_ms),
        }
    }

    #[cfg(test)]
    fn with_test_endpoint(config: &AppConfig, endpoint: reqwest::Url) -> Self {
        Self::from_config(config, endpoint)
    }
}

#[async_trait]
impl ChallengeVerifier for CloudflareTurnstileVerifier {
    async fn verify(&self, token: &str) -> Result<bool, ChallengeError> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .timeout(self.timeout)
            .form(&[("secret", self.secret_key.as_str()), ("response", token)])
            .send()
            .await
            .map_err(|_| ChallengeError::unavailable())?;
        if !response.status().is_success() {
            return Err(ChallengeError::unavailable());
        }

        let response = response
            .json::<TurnstileResponse>()
            .await
            .map_err(|_| ChallengeError::unavailable())?;
        if !response.success {
            return Ok(false);
        }
        if self.node_env == NodeEnvironment::Production
            && (response.hostname.as_deref() != Some(self.hostname.as_str())
                || response.action.as_deref() != Some(CREATE_LINK_ACTION))
        {
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::{
        AppError, ChallengeVerifier, ErrorCode,
        config::{AppConfig, Limits, NodeEnvironment, TurnstileConfig, TurnstileMode},
    };

    use super::{CREATE_LINK_ACTION, CloudflareTurnstileVerifier};

    const SECRET_KEY: &str = "turnstile-secret-key";
    const HOSTNAME: &str = "myurl.example";

    fn config(node_env: NodeEnvironment, timeout: Duration) -> AppConfig {
        AppConfig {
            node_env,
            port: 3_000,
            public_base_url: "https://myurl.example".to_owned(),
            public_base_origin: "https://myurl.example".to_owned(),
            redis_url: "redis://redis:6379/0".to_owned(),
            ip_hash_secret: b"test-secret-that-is-at-least-32-bytes-long".to_vec(),
            trust_proxy_cidrs: Vec::new(),
            turnstile: TurnstileConfig {
                enabled: true,
                mode: TurnstileMode::Cloudflare,
                site_key: "site-key".to_owned(),
                secret_key: SECRET_KEY.to_owned(),
                hostname: HOSTNAME.to_owned(),
            },
            limits: Limits {
                direct_10m: 5,
                hard_10m: 20,
                hard_1d: 100,
                resolve_10s: 600,
                challenge_score: 3,
                block_score: 8,
            },
            redis_timeout_ms: 750,
            turnstile_timeout_ms: u64::try_from(timeout.as_millis())
                .expect("test timeout must fit into milliseconds"),
            request_timeout_ms: 10_000,
            shutdown_timeout_ms: 10_000,
            test_force_challenge: false,
            test_store: None,
        }
    }

    fn verifier(
        server: &MockServer,
        node_env: NodeEnvironment,
        timeout: Duration,
    ) -> CloudflareTurnstileVerifier {
        CloudflareTurnstileVerifier::with_test_endpoint(
            &config(node_env, timeout),
            server
                .uri()
                .parse()
                .expect("Wiremock URI must be a valid URL"),
        )
    }

    fn assert_unavailable(result: Result<bool, crate::ChallengeError>) {
        let error = AppError::from(result.expect_err("expected Turnstile to be unavailable"));
        assert_eq!(error.code(), ErrorCode::DependencyUnavailable);
        assert_eq!(error.status_code(), 503);
    }

    #[tokio::test]
    async fn sends_a_form_request_and_accepts_a_successful_non_production_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Development,
            Duration::from_millis(100),
        );

        assert!(verifier.verify("test-token").await.unwrap());

        let requests = server
            .received_requests()
            .await
            .expect("Wiremock must retain received requests");
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.url.path(), "/");
        assert_eq!(
            request
                .headers
                .get("content-type")
                .expect("request must include content type"),
            "application/x-www-form-urlencoded"
        );
        let form: BTreeMap<_, _> = url::form_urlencoded::parse(&request.body)
            .into_owned()
            .collect();
        assert_eq!(form.get("secret").map(String::as_str), Some(SECRET_KEY));
        assert_eq!(form.get("response").map(String::as_str), Some("test-token"));
    }

    #[tokio::test]
    async fn returns_false_when_the_provider_rejects_the_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": false
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Development,
            Duration::from_millis(100),
        );

        assert!(!verifier.verify("invalid-token").await.unwrap());
    }

    #[tokio::test]
    async fn accepts_a_successful_production_response_with_matching_claims() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "hostname": HOSTNAME,
                "action": CREATE_LINK_ACTION
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Production,
            Duration::from_millis(100),
        );

        assert!(verifier.verify("test-token").await.unwrap());
    }

    #[tokio::test]
    async fn returns_false_when_the_production_hostname_or_action_claim_is_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Production,
            Duration::from_millis(100),
        );

        assert!(!verifier.verify("test-token").await.unwrap());
    }

    #[tokio::test]
    async fn returns_false_when_the_production_hostname_does_not_match() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "hostname": "other.example",
                "action": "create_link"
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Production,
            Duration::from_millis(100),
        );

        assert!(!verifier.verify("test-token").await.unwrap());
    }

    #[tokio::test]
    async fn returns_false_when_the_production_action_does_not_match() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "hostname": HOSTNAME,
                "action": "other_action"
            })))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Production,
            Duration::from_millis(100),
        );

        assert!(!verifier.verify("test-token").await.unwrap());
    }

    #[tokio::test]
    async fn maps_non_success_statuses_to_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Development,
            Duration::from_millis(100),
        );

        assert_unavailable(verifier.verify("test-token").await);
    }

    #[tokio::test]
    async fn does_not_follow_redirects_or_send_the_form_to_the_redirect_target() {
        let redirect_server = MockServer::start().await;
        let redirect_target = MockServer::start().await;
        let redirect_location = redirect_target.uri();
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(307).insert_header("Location", redirect_location.as_str()),
            )
            .mount(&redirect_server)
            .await;
        let verifier = verifier(
            &redirect_server,
            NodeEnvironment::Development,
            Duration::from_millis(100),
        );

        assert_unavailable(verifier.verify("test-token").await);

        let target_requests = redirect_target
            .received_requests()
            .await
            .expect("Wiremock must retain received requests");
        assert!(target_requests.is_empty());
    }

    #[tokio::test]
    async fn maps_timeouts_to_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({"success": true})),
            )
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Development,
            Duration::from_millis(10),
        );

        assert_unavailable(verifier.verify("test-token").await);
    }

    #[tokio::test]
    async fn maps_malformed_json_to_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        let verifier = verifier(
            &server,
            NodeEnvironment::Development,
            Duration::from_millis(100),
        );

        assert_unavailable(verifier.verify("test-token").await);
    }
}
