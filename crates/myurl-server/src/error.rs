use std::{error::Error as StdError, fmt, num::NonZeroU64};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

const DEFAULT_RETRY_AFTER_SECONDS: NonZeroU64 = NonZeroU64::new(120).unwrap();

type InternalError = Box<dyn StdError + Send + Sync + 'static>;

/// Stable error identifiers returned by the HTTP API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    ChallengeRequired,
    ChallengeInvalid,
    AliasUnavailable,
    UrlNotAllowed,
    AliasInvalid,
    RateLimited,
    DependencyUnavailable,
    CodeGenerationExhausted,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ChallengeRequired => "challenge_required",
            Self::ChallengeInvalid => "challenge_invalid",
            Self::AliasUnavailable => "alias_unavailable",
            Self::UrlNotAllowed => "url_not_allowed",
            Self::AliasInvalid => "alias_invalid",
            Self::RateLimited => "rate_limited",
            Self::DependencyUnavailable => "dependency_unavailable",
            Self::CodeGenerationExhausted => "code_generation_exhausted",
        }
    }

    pub const fn status_code(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::ChallengeRequired | Self::ChallengeInvalid => 403,
            Self::AliasUnavailable => 409,
            Self::UrlNotAllowed | Self::AliasInvalid => 422,
            Self::RateLimited => 429,
            Self::DependencyUnavailable | Self::CodeGenerationExhausted => 503,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The challenge provider currently supported by the service.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeProvider {
    Turnstile,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ChallengeValidationError {
    #[error("challenge site key must not be empty")]
    EmptySiteKey,
}

/// A public challenge description. Its field names match the shared JSON contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Challenge {
    provider: ChallengeProvider,
    #[serde(rename = "siteKey")]
    site_key: String,
}

impl Challenge {
    pub fn turnstile(site_key: impl Into<String>) -> Result<Self, ChallengeValidationError> {
        Self::try_turnstile(site_key)
    }

    pub fn try_turnstile(site_key: impl Into<String>) -> Result<Self, ChallengeValidationError> {
        let site_key = site_key.into();
        if site_key.is_empty() {
            return Err(ChallengeValidationError::EmptySiteKey);
        }

        Ok(Self {
            provider: ChallengeProvider::Turnstile,
            site_key,
        })
    }

    pub const fn provider(&self) -> ChallengeProvider {
        self.provider
    }

    pub fn site_key(&self) -> &str {
        &self.site_key
    }
}

impl<'de> Deserialize<'de> for Challenge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ChallengePayload {
            provider: ChallengeProvider,
            #[serde(rename = "siteKey")]
            site_key: String,
        }

        let ChallengePayload { provider, site_key } = ChallengePayload::deserialize(deserializer)?;
        match provider {
            ChallengeProvider::Turnstile => {
                Self::try_turnstile(site_key).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Errors raised by domain validation and decision logic.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("request is invalid")]
    InvalidRequest,
    #[error("target URL is not allowed")]
    UrlNotAllowed,
    #[error("alias is invalid")]
    AliasInvalid,
    #[error("alias is unavailable")]
    AliasUnavailable,
    #[error("a challenge is required")]
    ChallengeRequired(Challenge),
    #[error("the challenge is invalid")]
    ChallengeInvalid(Challenge),
    #[error("request is rate limited")]
    RateLimited { retry_after_seconds: NonZeroU64 },
    #[error("short code generation attempts were exhausted")]
    CodeGenerationExhausted,
}

impl DomainError {
    pub fn challenge_required(
        site_key: impl Into<String>,
    ) -> Result<Self, ChallengeValidationError> {
        Challenge::try_turnstile(site_key).map(Self::ChallengeRequired)
    }

    pub fn challenge_invalid(
        site_key: impl Into<String>,
    ) -> Result<Self, ChallengeValidationError> {
        Challenge::try_turnstile(site_key).map(Self::ChallengeInvalid)
    }

    pub const fn rate_limited() -> Self {
        Self::RateLimited {
            retry_after_seconds: DEFAULT_RETRY_AFTER_SECONDS,
        }
    }

    pub const fn rate_limited_for(retry_after_seconds: NonZeroU64) -> Self {
        Self::RateLimited {
            retry_after_seconds,
        }
    }
}

/// Internal errors from a link-store adapter.
#[derive(Debug, Error)]
#[error("store dependency unavailable")]
pub struct StoreError {
    #[source]
    source: Option<InternalError>,
}

impl StoreError {
    pub const fn unavailable() -> Self {
        Self { source: None }
    }

    pub fn from_source<E>(source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            source: Some(Box::new(source)),
        }
    }
}

/// Internal errors from a challenge-verifier adapter.
#[derive(Debug, Error)]
#[error("challenge dependency unavailable")]
pub struct ChallengeError {
    #[source]
    source: Option<InternalError>,
}

impl ChallengeError {
    pub const fn unavailable() -> Self {
        Self { source: None }
    }

    pub fn from_source<E>(source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            source: Some(Box::new(source)),
        }
    }
}

/// Runtime errors whose original source is retained for diagnostics.
#[derive(Debug, Error)]
#[error("application runtime failure")]
pub struct RuntimeError {
    #[source]
    source: InternalError,
}

impl RuntimeError {
    pub fn from_source<E>(source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            source: Box::new(source),
        }
    }
}

/// Client-safe fields needed to build an error response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseMetadata {
    pub code: ErrorCode,
    pub status_code: u16,
    pub retry_after_seconds: Option<NonZeroU64>,
    pub challenge: Option<Challenge>,
}

/// Application errors grouped by their ownership boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid request")]
    InvalidRequest,
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("dependency unavailable")]
    Store(#[from] StoreError),
    #[error("dependency unavailable")]
    Challenge(#[from] ChallengeError),
    #[error("application runtime failure")]
    Runtime(#[from] RuntimeError),
}

impl AppError {
    pub const fn invalid_request() -> Self {
        Self::InvalidRequest
    }

    pub fn challenge_required(
        site_key: impl Into<String>,
    ) -> Result<Self, ChallengeValidationError> {
        DomainError::challenge_required(site_key).map(Self::Domain)
    }

    pub fn challenge_invalid(
        site_key: impl Into<String>,
    ) -> Result<Self, ChallengeValidationError> {
        DomainError::challenge_invalid(site_key).map(Self::Domain)
    }

    pub const fn alias_unavailable() -> Self {
        Self::Domain(DomainError::AliasUnavailable)
    }

    pub const fn url_not_allowed() -> Self {
        Self::Domain(DomainError::UrlNotAllowed)
    }

    pub const fn alias_invalid() -> Self {
        Self::Domain(DomainError::AliasInvalid)
    }

    pub const fn rate_limited() -> Self {
        Self::Domain(DomainError::rate_limited())
    }

    pub const fn rate_limited_for(retry_after_seconds: NonZeroU64) -> Self {
        Self::Domain(DomainError::rate_limited_for(retry_after_seconds))
    }

    pub const fn code_generation_exhausted() -> Self {
        Self::Domain(DomainError::CodeGenerationExhausted)
    }

    pub fn runtime<E>(source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Runtime(RuntimeError::from_source(source))
    }

    pub fn response_metadata(&self) -> ResponseMetadata {
        match self {
            Self::InvalidRequest => ResponseMetadata::for_code(ErrorCode::InvalidRequest),
            Self::Domain(error) => error.response_metadata(),
            Self::Store(_) | Self::Challenge(_) | Self::Runtime(_) => {
                ResponseMetadata::for_code(ErrorCode::DependencyUnavailable)
            }
        }
    }

    pub fn code(&self) -> ErrorCode {
        self.response_metadata().code
    }

    pub fn status_code(&self) -> u16 {
        self.response_metadata().status_code
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.response_metadata()
            .retry_after_seconds
            .map(NonZeroU64::get)
    }

    pub fn challenge(&self) -> Option<&Challenge> {
        match self {
            Self::Domain(DomainError::ChallengeRequired(challenge))
            | Self::Domain(DomainError::ChallengeInvalid(challenge)) => Some(challenge),
            Self::InvalidRequest
            | Self::Store(_)
            | Self::Challenge(_)
            | Self::Runtime(_)
            | Self::Domain(
                DomainError::InvalidRequest
                | DomainError::UrlNotAllowed
                | DomainError::AliasInvalid
                | DomainError::AliasUnavailable
                | DomainError::RateLimited { .. }
                | DomainError::CodeGenerationExhausted,
            ) => None,
        }
    }
}

impl DomainError {
    fn response_metadata(&self) -> ResponseMetadata {
        match self {
            Self::InvalidRequest => ResponseMetadata::for_code(ErrorCode::InvalidRequest),
            Self::UrlNotAllowed => ResponseMetadata::for_code(ErrorCode::UrlNotAllowed),
            Self::AliasInvalid => ResponseMetadata::for_code(ErrorCode::AliasInvalid),
            Self::AliasUnavailable => ResponseMetadata::for_code(ErrorCode::AliasUnavailable),
            Self::ChallengeRequired(challenge) => {
                ResponseMetadata::with_challenge(ErrorCode::ChallengeRequired, challenge.clone())
            }
            Self::ChallengeInvalid(challenge) => {
                ResponseMetadata::with_challenge(ErrorCode::ChallengeInvalid, challenge.clone())
            }
            Self::RateLimited {
                retry_after_seconds,
            } => ResponseMetadata {
                code: ErrorCode::RateLimited,
                status_code: ErrorCode::RateLimited.status_code(),
                retry_after_seconds: Some(*retry_after_seconds),
                challenge: None,
            },
            Self::CodeGenerationExhausted => {
                ResponseMetadata::for_code(ErrorCode::CodeGenerationExhausted)
            }
        }
    }
}

impl ResponseMetadata {
    fn for_code(code: ErrorCode) -> Self {
        Self {
            code,
            status_code: code.status_code(),
            retry_after_seconds: None,
            challenge: None,
        }
    }

    fn with_challenge(code: ErrorCode, challenge: Challenge) -> Self {
        Self {
            code,
            status_code: code.status_code(),
            retry_after_seconds: None,
            challenge: Some(challenge),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AppError, Challenge, ChallengeError, DomainError, ErrorCode, ResponseMetadata, StoreError,
    };
    use std::{error::Error as StdError, num::NonZeroU64};

    #[test]
    fn error_codes_have_exact_contract_strings_and_statuses() {
        let cases = [
            (ErrorCode::InvalidRequest, "invalid_request", 400),
            (ErrorCode::ChallengeRequired, "challenge_required", 403),
            (ErrorCode::ChallengeInvalid, "challenge_invalid", 403),
            (ErrorCode::AliasUnavailable, "alias_unavailable", 409),
            (ErrorCode::UrlNotAllowed, "url_not_allowed", 422),
            (ErrorCode::AliasInvalid, "alias_invalid", 422),
            (ErrorCode::RateLimited, "rate_limited", 429),
            (
                ErrorCode::DependencyUnavailable,
                "dependency_unavailable",
                503,
            ),
            (
                ErrorCode::CodeGenerationExhausted,
                "code_generation_exhausted",
                503,
            ),
        ];

        for (code, expected_name, expected_status) in cases {
            assert_eq!(code.as_str(), expected_name);
            assert_eq!(code.status_code(), expected_status);
            assert_eq!(
                serde_json::to_string(&code).unwrap(),
                format!("\"{expected_name}\"")
            );
        }
    }

    #[test]
    fn rate_limited_metadata_has_default_and_explicit_retry_after() {
        let default = AppError::rate_limited().response_metadata();
        assert_eq!(default.code, ErrorCode::RateLimited);
        assert_eq!(default.status_code, 429);
        assert_eq!(default.retry_after_seconds, NonZeroU64::new(120));

        let explicit = AppError::rate_limited_for(NonZeroU64::new(45).unwrap()).response_metadata();
        assert_eq!(explicit.retry_after_seconds, NonZeroU64::new(45));
        assert!(NonZeroU64::new(0).is_none());
    }

    #[test]
    fn challenge_serializes_with_contract_field_names() {
        let challenge = Challenge::turnstile("site-key").unwrap();
        assert_eq!(
            serde_json::to_value(challenge).unwrap(),
            json!({"provider": "turnstile", "siteKey": "site-key"})
        );

        let required = AppError::challenge_required("site-key")
            .unwrap()
            .response_metadata();
        assert_eq!(required.code, ErrorCode::ChallengeRequired);
        assert_eq!(required.status_code, 403);
        assert_eq!(
            required.challenge,
            Some(Challenge::turnstile("site-key").unwrap())
        );
    }

    #[test]
    fn challenge_rejects_unknown_provider_and_empty_site_key() {
        let unknown_provider = serde_json::from_value::<Challenge>(json!({
            "provider": "other",
            "siteKey": "site-key"
        }));
        assert!(unknown_provider.is_err());

        let empty_site_key = serde_json::from_value::<Challenge>(json!({
            "provider": "turnstile",
            "siteKey": ""
        }));
        assert!(empty_site_key.is_err());
        assert!(Challenge::turnstile("").is_err());
    }

    #[test]
    fn internal_sources_are_chained_but_redacted_from_client_metadata() {
        let source_text = "redis password and private adapter details";
        let errors = [
            AppError::from(StoreError::from_source(std::io::Error::other(source_text))),
            AppError::from(ChallengeError::from_source(std::io::Error::other(
                source_text,
            ))),
            AppError::runtime(std::io::Error::other(source_text)),
        ];

        for error in errors {
            let metadata = error.response_metadata();
            assert_eq!(metadata.code, ErrorCode::DependencyUnavailable);
            assert_eq!(metadata.status_code, 503);
            assert_eq!(metadata.retry_after_seconds, None);
            assert_eq!(metadata.challenge, None);
            assert!(!format!("{metadata:?}").contains(source_text));
            assert!(!error.to_string().contains(source_text));
            assert!(source_chain_contains(&error, source_text));
        }
    }

    fn source_chain_contains(error: &(dyn StdError + 'static), expected: &str) -> bool {
        let mut source = error.source();
        while let Some(error) = source {
            if error.to_string().contains(expected) {
                return true;
            }
            source = error.source();
        }

        false
    }

    #[test]
    fn public_domain_cases_map_to_their_contract_codes() {
        let cases = [
            (AppError::invalid_request(), ErrorCode::InvalidRequest, 400),
            (
                AppError::from(DomainError::InvalidRequest),
                ErrorCode::InvalidRequest,
                400,
            ),
            (
                AppError::alias_unavailable(),
                ErrorCode::AliasUnavailable,
                409,
            ),
            (AppError::url_not_allowed(), ErrorCode::UrlNotAllowed, 422),
            (AppError::alias_invalid(), ErrorCode::AliasInvalid, 422),
            (
                AppError::code_generation_exhausted(),
                ErrorCode::CodeGenerationExhausted,
                503,
            ),
        ];

        for (error, code, status) in cases {
            let metadata: ResponseMetadata = error.response_metadata();
            assert_eq!(metadata.code, code);
            assert_eq!(metadata.status_code, status);
        }

        let invalid = AppError::from(DomainError::ChallengeInvalid(
            Challenge::turnstile("key").unwrap(),
        ));
        assert_eq!(invalid.code(), ErrorCode::ChallengeInvalid);
        assert_eq!(invalid.status_code(), 403);
    }
}
