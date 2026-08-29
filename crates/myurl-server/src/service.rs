use std::{num::NonZeroU64, sync::Arc, time::Duration};

use time::{OffsetDateTime, UtcOffset, macros::format_description};

use crate::{
    config::{AUTO_CODE_LENGTH, AppConfig, MAX_CODE_ATTEMPTS},
    domain::{
        alias::{is_reserved_code, normalize_alias},
        risk::{RiskDecision, RiskInput, evaluate_risk},
        short_code::{ShortCodeError, generate_short_code, is_valid_code},
        time::{expiry_at, utc_date},
        url_policy::normalize_target_url,
    },
    error::{AppError, DomainError},
    ip::fingerprint_ip,
    ports::{ChallengeVerifier, CreateResult, LinkStore},
};

const RESOLVE_RETRY_AFTER_SECONDS: NonZeroU64 = NonZeroU64::new(10).expect("10 is nonzero");

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;
pub type ShortCodeGenerator = Arc<dyn Fn() -> Result<String, ShortCodeError> + Send + Sync>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateLinkRequest {
    pub url: String,
    pub alias: Option<String>,
    pub challenge_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateLinkContext {
    pub client_ip: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveLinkRequest {
    pub code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveLinkContext {
    pub client_ip: String,
}

pub struct ShortLinkService {
    config: AppConfig,
    store: Arc<dyn LinkStore>,
    challenge_verifier: Arc<dyn ChallengeVerifier>,
    clock: Clock,
    short_code_generator: ShortCodeGenerator,
}

impl ShortLinkService {
    pub fn new(
        config: AppConfig,
        store: Arc<dyn LinkStore>,
        challenge_verifier: Arc<dyn ChallengeVerifier>,
        clock: Clock,
        short_code_generator: ShortCodeGenerator,
    ) -> Self {
        Self {
            config,
            store,
            challenge_verifier,
            clock,
            short_code_generator,
        }
    }

    /// Builds a service with the production UTC clock and secure code generator.
    pub fn with_defaults(
        config: AppConfig,
        store: Arc<dyn LinkStore>,
        challenge_verifier: Arc<dyn ChallengeVerifier>,
    ) -> Self {
        Self::new(
            config,
            store,
            challenge_verifier,
            Arc::new(OffsetDateTime::now_utc),
            Arc::new(generate_short_code),
        )
    }

    pub async fn create(
        &self,
        request: &CreateLinkRequest,
        context: &CreateLinkContext,
    ) -> Result<CreateResult, AppError> {
        let fingerprint = fingerprint_ip(&self.config.ip_hash_secret, &context.client_ip);
        let date = utc_date((self.clock)()).map_err(AppError::runtime)?;
        let counts = self
            .store
            .increment_create_counters(&fingerprint, &date)
            .await?;
        let risk_score = self.store.risk_score(&fingerprint).await?;

        match evaluate_risk(RiskInput {
            ten_minute_count: counts.ten_minute_count,
            daily_count: counts.daily_count,
            risk_score,
            limits: &self.config.limits,
            challenge_enabled: self.config.turnstile.enabled,
            force_challenge: self.config.test_force_challenge,
        }) {
            RiskDecision::Block => return Err(AppError::rate_limited()),
            RiskDecision::Challenge => self.verify_challenge(request, &fingerprint).await?,
            RiskDecision::Allow => {}
        }

        let target_url = self
            .normalize_target_url(&request.url, &fingerprint)
            .await?;
        let alias = match request.alias.as_deref() {
            Some(alias) => Some(self.validate_alias(alias, &fingerprint).await?),
            None => None,
        };
        let expiry = expiry_at((self.clock)()).map_err(AppError::runtime)?;
        let code = match alias {
            Some(alias) => {
                self.claim_alias(&alias, &target_url, &fingerprint, expiry)
                    .await?
            }
            None => self.claim_generated_code(&target_url, expiry).await?,
        };

        Ok(CreateResult {
            short_url: format!("{}/{}", self.config.public_base_origin, code),
            code,
            expires_at: format_expiry(expiry)?,
        })
    }

    pub async fn resolve(
        &self,
        request: &ResolveLinkRequest,
        context: &ResolveLinkContext,
    ) -> Result<Option<String>, AppError> {
        let fingerprint = fingerprint_ip(&self.config.ip_hash_secret, &context.client_ip);
        let count = self.store.increment_resolve_counter(&fingerprint).await?;
        if count > self.config.limits.resolve_10s {
            return Err(AppError::rate_limited_for(RESOLVE_RETRY_AFTER_SECONDS));
        }

        if !is_valid_code(&request.code) {
            return Ok(None);
        }

        Ok(self.store.lookup(&request.code).await?)
    }

    async fn verify_challenge(
        &self,
        request: &CreateLinkRequest,
        fingerprint: &str,
    ) -> Result<(), AppError> {
        let Some(token) = request
            .challenge_token
            .as_deref()
            .filter(|token| !token.is_empty())
        else {
            return Err(self.challenge_required_error());
        };

        if !self.challenge_verifier.verify(token).await? {
            self.record_risk(fingerprint, 3).await?;
            return Err(self.challenge_invalid_error());
        }

        Ok(())
    }

    async fn normalize_target_url(
        &self,
        input: &str,
        fingerprint: &str,
    ) -> Result<String, AppError> {
        match normalize_target_url(input) {
            Ok(target_url) => Ok(target_url),
            Err(DomainError::UrlNotAllowed) => {
                self.record_risk(fingerprint, 1).await?;
                Err(AppError::url_not_allowed())
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn validate_alias(&self, alias: &str, fingerprint: &str) -> Result<String, AppError> {
        let alias = match normalize_alias(alias) {
            Ok(alias) => alias,
            Err(DomainError::AliasInvalid) => {
                self.record_risk(fingerprint, 1).await?;
                return Err(AppError::alias_invalid());
            }
            Err(error) => return Err(error.into()),
        };

        if is_reserved_code(&alias) {
            self.record_risk(fingerprint, 1).await?;
            return Err(AppError::alias_unavailable());
        }

        Ok(alias)
    }

    async fn claim_alias(
        &self,
        alias: &str,
        target_url: &str,
        fingerprint: &str,
        expiry: OffsetDateTime,
    ) -> Result<String, AppError> {
        if self.claim(alias, target_url, expiry).await? {
            return Ok(alias.to_owned());
        }

        self.record_risk(fingerprint, 1).await?;
        Err(AppError::alias_unavailable())
    }

    async fn claim_generated_code(
        &self,
        target_url: &str,
        expiry: OffsetDateTime,
    ) -> Result<String, AppError> {
        for _ in 0..MAX_CODE_ATTEMPTS {
            let candidate = (self.short_code_generator)().map_err(AppError::runtime)?;
            if candidate.len() != AUTO_CODE_LENGTH
                || !is_valid_code(&candidate)
                || is_reserved_code(&candidate)
            {
                continue;
            }
            if self.claim(&candidate, target_url, expiry).await? {
                return Ok(candidate);
            }
        }

        Err(AppError::code_generation_exhausted())
    }

    async fn claim(
        &self,
        code: &str,
        target_url: &str,
        expiry: OffsetDateTime,
    ) -> Result<bool, AppError> {
        let ttl = Duration::try_from(expiry - (self.clock)()).map_err(AppError::runtime)?;
        Ok(self.store.claim(code, target_url, ttl).await?)
    }

    async fn record_risk(&self, fingerprint: &str, points: u64) -> Result<(), AppError> {
        self.store.add_risk_score(fingerprint, points).await?;
        Ok(())
    }

    fn challenge_required_error(&self) -> AppError {
        AppError::challenge_required(self.config.turnstile.site_key.clone())
            .unwrap_or_else(AppError::runtime)
    }

    fn challenge_invalid_error(&self) -> AppError {
        AppError::challenge_invalid(self.config.turnstile.site_key.clone())
            .unwrap_or_else(AppError::runtime)
    }
}

fn format_expiry(expiry: OffsetDateTime) -> Result<String, AppError> {
    expiry
        .to_offset(UtcOffset::UTC)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .map_err(AppError::runtime)
}
