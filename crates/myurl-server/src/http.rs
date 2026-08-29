use std::{
    fmt,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, Extension, MatchedPath, Path, State,
        rejection::JsonRejection,
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::{
    config::{AppConfig, MAX_BODY_BYTES},
    error::{AppError, Challenge, ErrorCode, ResponseMetadata},
    ip::get_client_ip,
    ports::LinkStore,
    service::{
        CreateLinkContext, CreateLinkRequest as ServiceCreateLinkRequest, ResolveLinkContext,
        ResolveLinkRequest, ShortLinkService,
    },
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

    fn invalid_api_route(config: &AppConfig, request_id: RequestId) -> Self {
        let mut problem = Self::from_app_error(&AppError::invalid_request(), config, request_id);
        problem.status = StatusCode::NOT_FOUND.as_u16();
        problem
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

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    store: Arc<dyn LinkStore>,
    service: Arc<ShortLinkService>,
}

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; script-src 'self' https://challenges.cloudflare.com; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self' https://challenges.cloudflare.com; frame-src https://challenges.cloudflare.com";
const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=()";
const NOT_FOUND_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Link not found</title></head><body><p>This short link is unavailable or has expired.</p></body></html>";
const RATE_LIMITED_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Too many requests</title></head><body><p>Too many requests. Please try again later.</p></body></html>";
const UNAVAILABLE_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Service unavailable</title></head><body><p>The service is temporarily unable to complete this request.</p></body></html>";

/// Builds the HTTP router from already constructed service dependencies.
pub fn build_app(
    config: AppConfig,
    store: Arc<dyn LinkStore>,
    service: Arc<ShortLinkService>,
) -> Router {
    build_router(config, store, service).fallback(fallback)
}

/// Builds the production router with static files as the non-API fallback.
pub fn build_app_with_static(
    config: AppConfig,
    store: Arc<dyn LinkStore>,
    service: Arc<ShortLinkService>,
    web_root: PathBuf,
) -> Router {
    build_router(config, store, service).fallback_service(ServeDir::new(web_root))
}

fn build_router(
    config: AppConfig,
    store: Arc<dyn LinkStore>,
    service: Arc<ShortLinkService>,
) -> Router {
    let state = AppState {
        config: Arc::new(config),
        store,
        service,
    };

    Router::new()
        .route("/api/links", post(create_link))
        .route("/api", any(api_fallback))
        .route("/api/{*path}", any(api_fallback))
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/{code}", get(resolve_link).head(resolve_link_head))
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(request_id_middleware))
}

async fn create_link(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    request: Result<Json<CreateLinkRequest>, JsonRejection>,
) -> Response {
    if !is_json_content_type(&headers) || !has_valid_origin(&headers, &state.config) {
        return api_error(AppError::invalid_request(), &state.config, request_id);
    }
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return api_error(rejection.into(), &state.config, request_id),
    };
    let client_ip = client_ip(connect_info, &headers, &state.config);
    let service_request = ServiceCreateLinkRequest {
        url: request.url,
        alias: request.alias,
        challenge_token: request.challenge_token,
    };

    match state
        .service
        .create(&service_request, &CreateLinkContext { client_ip })
        .await
    {
        Ok(result) => (
            StatusCode::CREATED,
            Json(CreateLinkResponse {
                code: result.code,
                short_url: result.short_url,
                expires_at: result.expires_at,
            }),
        )
            .into_response(),
        Err(error) => api_error(error, &state.config, request_id),
    }
}

async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn readiness(State(state): State<AppState>) -> Response {
    match tokio::time::timeout(
        Duration::from_millis(state.config.redis_timeout_ms),
        state.store.ping(),
    )
    .await
    {
        Ok(Ok(())) => Json(HealthResponse { status: "ok" }).into_response(),
        Ok(Err(_)) | Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse { status: "degraded" }),
        )
            .into_response(),
    }
}

async fn resolve_link(
    State(state): State<AppState>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    resolve(state, connect_info, headers, code, false).await
}

async fn resolve_link_head(
    State(state): State<AppState>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    resolve(state, connect_info, headers, code, true).await
}

async fn resolve(
    state: AppState,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    code: String,
    head_only: bool,
) -> Response {
    let client_ip = client_ip(connect_info, &headers, &state.config);
    let request = ResolveLinkRequest { code };
    match state
        .service
        .resolve(&request, &ResolveLinkContext { client_ip })
        .await
    {
        Ok(Some(target_url)) => redirect_response(&target_url),
        Ok(None) => browser_error(StatusCode::NOT_FOUND, NOT_FOUND_PAGE, head_only, None),
        Err(error) if error.code() == ErrorCode::RateLimited => browser_error(
            StatusCode::TOO_MANY_REQUESTS,
            RATE_LIMITED_PAGE,
            head_only,
            error.retry_after_seconds(),
        ),
        Err(_) => browser_error(
            StatusCode::SERVICE_UNAVAILABLE,
            UNAVAILABLE_PAGE,
            head_only,
            None,
        ),
    }
}

async fn api_fallback(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    ProblemDetails::invalid_api_route(&state.config, request_id).into_response()
}

async fn fallback(method: Method) -> Response {
    browser_error(
        StatusCode::NOT_FOUND,
        NOT_FOUND_PAGE,
        method == Method::HEAD,
        None,
    )
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

fn api_error(error: AppError, config: &AppConfig, request_id: RequestId) -> Response {
    ProblemDetails::from_app_error(&error, config, request_id).into_response()
}

fn client_ip(
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    config: &AppConfig,
) -> String {
    let remote_address = connect_info.map(|Extension(peer)| peer.0.ip().to_string());
    get_client_ip(
        remote_address.as_deref(),
        header_value(headers, "x-forwarded-for"),
        header_value(headers, "forwarded"),
        &config.trust_proxy_cidrs,
    )
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    header_value(headers, header::CONTENT_TYPE.as_str()).is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn has_valid_origin(headers: &HeaderMap, config: &AppConfig) -> bool {
    headers
        .get(header::ORIGIN)
        .is_none_or(|origin| origin.to_str().ok() == Some(config.public_base_origin.as_str()))
}

fn redirect_response(target_url: &str) -> Response {
    let Ok(location) = HeaderValue::from_str(target_url) else {
        return browser_error(
            StatusCode::SERVICE_UNAVAILABLE,
            UNAVAILABLE_PAGE,
            false,
            None,
        );
    };
    let mut response = StatusCode::FOUND.into_response();
    response.headers_mut().insert(header::LOCATION, location);
    response
}

fn browser_error(
    status: StatusCode,
    page: &'static str,
    head_only: bool,
    retry_after_seconds: Option<u64>,
) -> Response {
    let body = if head_only { "" } else { page };
    let mut response = (status, Html(body)).into_response();
    if let Some(seconds) = retry_after_seconds {
        let value = HeaderValue::from_str(&seconds.to_string())
            .expect("retry-after seconds are a valid HTTP header value");
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

async fn request_id_middleware(mut request: axum::extract::Request, next: Next) -> Response {
    let request_id = request_id_from_headers(request.headers());
    let route = request.extensions().get::<MatchedPath>().map_or_else(
        || "unmatched".to_owned(),
        |matched_path| matched_path.as_str().to_owned(),
    );
    request.extensions_mut().insert(request_id.clone());
    let started_at = Instant::now();
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(request_id.as_str())
            .expect("validated request IDs are valid HTTP header values"),
    );
    tracing::info!(
        request_id = %request_id,
        route = %route,
        status = response.status().as_u16(),
        outcome = outcome_class(response.status()),
        dependency = dependency_class(response.status()),
        duration_ms = started_at.elapsed().as_secs_f64() * 1_000.0,
        "request complete"
    );
    response
}

fn outcome_class(status: StatusCode) -> &'static str {
    match status {
        status if status.is_success() => "success",
        status if status.is_redirection() => "redirect",
        status if status.is_client_error() => "rejected",
        _ => "failed",
    }
}

fn dependency_class(status: StatusCode) -> &'static str {
    if status == StatusCode::SERVICE_UNAVAILABLE {
        "unavailable"
    } else {
        "none"
    }
}

async fn security_headers_middleware(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static(PERMISSIONS_POLICY),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
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
            ("TEST_STORE", "memory"),
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

    #[cfg(feature = "test-support")]
    #[test]
    fn build_app_constructs_the_axum_routes() {
        use std::sync::Arc;

        use crate::{
            LinkStore, ShortLinkService,
            testing::{FakeTurnstile, MemoryLinkStore},
        };

        let config = test_config(PUBLIC_BASE_ORIGIN);
        let store: Arc<dyn LinkStore> = Arc::new(
            MemoryLinkStore::from_config(&config, Arc::new(time::OffsetDateTime::now_utc))
                .expect("test configuration enables the memory store"),
        );
        let service = Arc::new(ShortLinkService::with_defaults(
            config.clone(),
            store.clone(),
            Arc::new(FakeTurnstile::new()),
        ));

        let _app = super::build_app(config, store, service);
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
