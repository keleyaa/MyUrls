use std::fmt;

use axum::{
    Json,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::AppConfig,
    error::{AppError, Challenge, ErrorCode, ResponseMetadata},
};

const PROBLEM_JSON_CONTENT_TYPE: &str = "application/problem+json";
const REQUEST_ID_HEADER: &str = "x-request-id";

/// JSON body accepted by the create-link endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLinkRequest {
    pub url: String,
    pub alias: Option<String>,
    pub challenge_token: Option<String>,
}

/// JSON body returned after creating a short link.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLinkResponse {
    pub code: String,
    pub short_url: String,
    pub expires_at: String,
}

/// A request ID that is safe to include in a response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RequestId(String);

impl RequestId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Client-safe RFC 9457-style error response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    problem_type: String,
    title: &'static str,
    status: u16,
    code: ErrorCode,
    request_id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    challenge: Option<Challenge>,
}

impl ProblemDetails {
    pub fn from_app_error(error: &AppError, config: &AppConfig, request_id: RequestId) -> Self {
        Self::from_response_metadata(error.response_metadata(), config, request_id)
    }

    fn from_response_metadata(
        metadata: ResponseMetadata,
        config: &AppConfig,
        request_id: RequestId,
    ) -> Self {
        Self {
            problem_type: format!(
                "{}/problems/{}",
                config.public_base_origin,
                metadata.code.as_str()
            ),
            title: problem_title(metadata.code),
            status: metadata.status_code,
            code: metadata.code,
            request_id,
            retry_after_seconds: metadata.retry_after_seconds.map(|seconds| seconds.get()),
            challenge: metadata.challenge,
        }
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let retry_after_seconds = self.retry_after_seconds;
        let status = StatusCode::from_u16(self.status)
            .expect("AppError response metadata always contains a valid HTTP status");
        let mut response = (status, Json(self)).into_response();
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(PROBLEM_JSON_CONTENT_TYPE),
        );
        if let Some(retry_after_seconds) = retry_after_seconds {
            let value = HeaderValue::from_str(&retry_after_seconds.to_string())
                .expect("retry-after seconds are a valid HTTP header value");
            headers.insert(header::RETRY_AFTER, value);
        }
        response
    }
}

/// Returns a request ID that is safe to echo in a response.
pub fn request_id_from_headers(headers: &HeaderMap) -> RequestId {
    if let Some(value) = headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_valid_request_id(value))
    {
        return RequestId(value.to_owned());
    }

    RequestId(Uuid::new_v4().to_string())
}

pub fn is_valid_request_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=80).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn problem_title(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidRequest => "Invalid request",
        ErrorCode::ChallengeRequired => "Challenge required",
        ErrorCode::ChallengeInvalid => "Challenge invalid",
        ErrorCode::AliasUnavailable => "Alias unavailable",
        ErrorCode::UrlNotAllowed => "URL not allowed",
        ErrorCode::AliasInvalid => "Alias invalid",
        ErrorCode::RateLimited => "Rate limited",
        ErrorCode::DependencyUnavailable => "Dependency unavailable",
        ErrorCode::CodeGenerationExhausted => "Code generation exhausted",
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        extract::{FromRequest, Json},
        http::{HeaderMap, HeaderValue, Request, header},
        response::IntoResponse,
    };
    use serde_json::{Value, json};

    use super::{
        AppConfig, AppError, CreateLinkRequest, CreateLinkResponse, ProblemDetails, RequestId,
        is_valid_request_id, request_id_from_headers,
    };

    const PUBLIC_BASE_ORIGIN: &str = "https://myurl.example";

    fn test_config(public_base_origin: &str) -> AppConfig {
        AppConfig::from_env([
            ("NODE_ENV", "test"),
            ("PUBLIC_BASE_URL", public_base_origin),
            (
                "IP_HASH_SECRET",
                "test-secret-that-is-at-least-32-bytes-long",
            ),
            ("TURNSTILE_ENABLED", "false"),
        ])
        .unwrap()
    }

    fn validated_request_id(value: &str) -> RequestId {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", value.parse().unwrap());
        request_id_from_headers(&headers)
    }

    #[test]
    fn create_link_dtos_use_camel_case_and_reject_unknown_request_fields() {
        let request: CreateLinkRequest = serde_json::from_value(json!({
            "url": "https://example.com/docs",
            "alias": "docs",
            "challengeToken": "challenge-token"
        }))
        .unwrap();
        assert_eq!(request.url, "https://example.com/docs");
        assert_eq!(request.alias.as_deref(), Some("docs"));
        assert_eq!(request.challenge_token.as_deref(), Some("challenge-token"));

        let unknown_field = serde_json::from_value::<CreateLinkRequest>(json!({
            "url": "https://example.com/docs",
            "unexpected": true
        }));
        assert!(unknown_field.is_err());

        let response = CreateLinkResponse {
            code: "docs".to_owned(),
            short_url: "https://myurl.example/docs".to_owned(),
            expires_at: "2026-11-27T12:00:00.000Z".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({
                "code": "docs",
                "shortUrl": "https://myurl.example/docs",
                "expiresAt": "2026-11-27T12:00:00.000Z"
            })
        );
    }

    #[tokio::test]
    async fn problem_details_use_safe_fields_content_type_retry_and_challenge() {
        let config = test_config(PUBLIC_BASE_ORIGIN);
        let request_id = validated_request_id("req_abc123");
        let rate_limited = ProblemDetails::from_app_error(
            &AppError::rate_limited_for(std::num::NonZeroU64::new(45).unwrap()),
            &config,
            request_id.clone(),
        )
        .into_response();
        assert_eq!(rate_limited.status(), 429);
        assert_eq!(
            rate_limited.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
        assert_eq!(
            rate_limited.headers().get(header::RETRY_AFTER).unwrap(),
            "45"
        );
        assert_eq!(
            response_json(rate_limited).await,
            json!({
                "type": "https://myurl.example/problems/rate_limited",
                "title": "Rate limited",
                "status": 429,
                "code": "rate_limited",
                "requestId": "req_abc123",
                "retryAfterSeconds": 45
            })
        );

        let challenge = ProblemDetails::from_app_error(
            &AppError::challenge_required("public-site-key").unwrap(),
            &config,
            request_id,
        )
        .into_response();
        assert_eq!(challenge.status(), 403);
        assert!(challenge.headers().get(header::RETRY_AFTER).is_none());
        assert_eq!(
            response_json(challenge).await,
            json!({
                "type": "https://myurl.example/problems/challenge_required",
                "title": "Challenge required",
                "status": 403,
                "code": "challenge_required",
                "requestId": "req_abc123",
                "challenge": {
                    "provider": "turnstile",
                    "siteKey": "public-site-key"
                }
            })
        );
    }

    #[tokio::test]
    async fn problem_details_redact_internal_error_text() {
        let config = test_config(PUBLIC_BASE_ORIGIN);
        let secret = "redis://:secret@private.example/15 https://target.example token=private";
        let problem = ProblemDetails::from_app_error(
            &AppError::runtime(std::io::Error::other(secret)),
            &config,
            validated_request_id("req_abc123"),
        )
        .into_response();
        let response = response_json(problem).await;
        let serialized = serde_json::to_string(&response).unwrap();

        assert_eq!(
            response["type"],
            "https://myurl.example/problems/dependency_unavailable"
        );
        assert_eq!(response["title"], "Dependency unavailable");
        assert_eq!(response["status"], 503);
        assert_eq!(response["code"], "dependency_unavailable");
        assert_eq!(response["requestId"], "req_abc123");
        assert!(response.get("retryAfterSeconds").is_none());
        assert!(response.get("challenge").is_none());
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains("target.example"));
        assert!(!serialized.contains("token=private"));
    }

    #[test]
    fn request_id_helper_accepts_only_safe_ascii_values() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req_abc.123:trace-1".parse().unwrap());
        assert_eq!(
            request_id_from_headers(&headers).as_str(),
            "req_abc.123:trace-1"
        );
        assert!(is_valid_request_id("A"));
        assert!(is_valid_request_id(&"a".repeat(80)));

        for invalid in [
            "",
            "_request",
            "request space",
            "request/slash",
            &"a".repeat(81),
        ] {
            assert!(!is_valid_request_id(invalid));
        }

        headers.insert(
            "x-request-id",
            HeaderValue::from_bytes(b"\xffrequest").unwrap(),
        );
        let replacement = request_id_from_headers(&headers);
        assert!(is_valid_request_id(replacement.as_str()));
        assert_eq!(replacement.as_str().len(), 36);

        let generated = request_id_from_headers(&HeaderMap::new());
        assert!(is_valid_request_id(generated.as_str()));
        assert_eq!(generated.as_str().len(), 36);
    }

    #[tokio::test]
    async fn problem_details_do_not_reflect_untrusted_request_id_and_use_configured_origin() {
        let raw_request_id = "attacker/request-id";
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", raw_request_id.parse().unwrap());

        let response = ProblemDetails::from_app_error(
            &AppError::invalid_request(),
            &test_config("https://trusted.myurl.example"),
            request_id_from_headers(&headers),
        )
        .into_response();
        let body = response_json(response).await;
        let serialized = serde_json::to_string(&body).unwrap();
        let response_request_id = body["requestId"].as_str().unwrap();

        assert_eq!(
            body["type"],
            "https://trusted.myurl.example/problems/invalid_request"
        );
        assert_ne!(response_request_id, raw_request_id);
        assert!(!serialized.contains(raw_request_id));
        assert!(is_valid_request_id(response_request_id));
    }

    #[tokio::test]
    async fn json_rejections_map_to_generic_invalid_request() {
        let request = Request::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"url":"https://example.com","unexpected":true}"#,
            ))
            .unwrap();
        let rejection = Json::<CreateLinkRequest>::from_request(request, &())
            .await
            .unwrap_err();
        assert!(rejection.to_string().contains("unexpected"));

        let error: AppError = rejection.into();
        assert_eq!(error.code().as_str(), "invalid_request");
        assert_eq!(error.status_code(), 400);
        assert_eq!(error.to_string(), "invalid request");
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }
}
