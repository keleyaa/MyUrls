use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::{ChallengeError, StoreError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCounts {
    pub ten_minute_count: u64,
    pub daily_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateResult {
    pub code: String,
    #[serde(rename = "shortUrl")]
    pub short_url: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
}

pub use crate::error::Challenge;

#[async_trait]
pub trait LinkStore: Send + Sync {
    async fn claim(&self, code: &str, target_url: &str, ttl: Duration) -> Result<bool, StoreError>;

    async fn lookup(&self, code: &str) -> Result<Option<String>, StoreError>;

    async fn increment_resolve_counter(&self, fingerprint: &str) -> Result<u64, StoreError>;

    async fn increment_create_counters(
        &self,
        fingerprint: &str,
        utc_date: &str,
    ) -> Result<CreateCounts, StoreError>;

    async fn risk_score(&self, fingerprint: &str) -> Result<u64, StoreError>;

    async fn add_risk_score(&self, fingerprint: &str, points: u64) -> Result<u64, StoreError>;

    async fn ping(&self) -> Result<(), StoreError>;

    async fn close(&self) -> Result<(), StoreError>;
}

#[async_trait]
pub trait ChallengeVerifier: Send + Sync {
    async fn verify(&self, token: &str) -> Result<bool, ChallengeError>;
}
