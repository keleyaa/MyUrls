use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use async_trait::async_trait;
use thiserror::Error;
use time::{Duration as TimeDuration, OffsetDateTime};

use crate::{
    config::{AppConfig, NodeEnvironment},
    domain::short_code::ShortCodeError,
    error::{ChallengeError, StoreError},
    ports::{ChallengeVerifier, CreateCounts, LinkStore},
};

pub const CREATE_TEN_MINUTE_TTL_SECONDS: u64 = 600;
pub const CREATE_DAILY_TTL_SECONDS: u64 = 172_800;
pub const RESOLVE_TTL_SECONDS: u64 = 10;
pub const RISK_TTL_SECONDS: u64 = 600;

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;
pub type ShortCodeGenerator = Arc<dyn Fn() -> Result<String, ShortCodeError> + Send + Sync>;

/// A mutable, deterministic time source for service and adapter tests.
#[derive(Clone)]
pub struct DeterministicClock {
    now: Arc<Mutex<OffsetDateTime>>,
}

#[derive(Debug, Error, Eq, PartialEq)]
#[error("deterministic clock state is unavailable")]
pub struct DeterministicClockError;

impl DeterministicClock {
    pub fn new(now: OffsetDateTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(now)),
        }
    }

    pub fn now(&self) -> Result<OffsetDateTime, DeterministicClockError> {
        self.lock().map(|now| *now)
    }

    pub fn set(&self, now: OffsetDateTime) -> Result<(), DeterministicClockError> {
        *self.lock()? = now;
        Ok(())
    }

    pub fn advance(&self, duration: TimeDuration) -> Result<(), DeterministicClockError> {
        let mut now = self.lock()?;
        *now = now.checked_add(duration).ok_or(DeterministicClockError)?;
        Ok(())
    }

    pub fn as_clock(&self) -> Clock {
        let clock = self.clone();
        Arc::new(move || {
            clock
                .now()
                .expect("deterministic clock must not be poisoned during a test")
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, OffsetDateTime>, DeterministicClockError> {
        self.now.lock().map_err(|_| DeterministicClockError)
    }
}

/// A deterministic sequence for exercising generated-code collisions in service tests.
#[derive(Clone)]
pub struct SequenceShortCodeGenerator {
    codes: Arc<Mutex<VecDeque<String>>>,
}

impl SequenceShortCodeGenerator {
    pub fn new<I, S>(codes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            codes: Arc::new(Mutex::new(codes.into_iter().map(Into::into).collect())),
        }
    }

    pub fn next_code(&self) -> Result<String, ShortCodeError> {
        self.codes
            .lock()
            .map_err(|_| ShortCodeError::RandomnessUnavailable)?
            .pop_front()
            .ok_or(ShortCodeError::RandomByteSourceExhausted)
    }

    pub fn as_generator(&self) -> ShortCodeGenerator {
        let generator = self.clone();
        Arc::new(move || generator.next_code())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreFailures {
    pub claim: bool,
    pub lookup: bool,
    pub increment_resolve_counter: bool,
    pub increment_create_counters: bool,
    pub risk_score: bool,
    pub add_risk_score: bool,
    pub ping: bool,
    pub close: bool,
}

impl StoreFailures {
    pub const fn all() -> Self {
        Self {
            claim: true,
            lookup: true,
            increment_resolve_counter: true,
            increment_create_counters: true,
            risk_score: true,
            add_risk_score: true,
            ping: true,
            close: true,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
#[error("memory store requires NODE_ENV=test and TEST_STORE=memory")]
pub struct MemoryStoreConfigError;

pub fn memory_store_is_configured(config: &AppConfig) -> bool {
    config.node_env == NodeEnvironment::Test && config.test_store.as_deref() == Some("memory")
}

/// An expiring in-memory LinkStore for tests. It never logs or exposes stored target URLs.
#[derive(Clone)]
pub struct MemoryLinkStore {
    clock: Clock,
    state: Arc<Mutex<MemoryStoreState>>,
}

#[derive(Default)]
struct MemoryStoreState {
    links: BTreeMap<String, ExpiringValue<String>>,
    counters: BTreeMap<CounterKey, ExpiringValue<u64>>,
    risks: BTreeMap<String, ExpiringValue<u64>>,
    failures: StoreFailures,
}

struct ExpiringValue<T> {
    value: T,
    expires_at: OffsetDateTime,
    logical_ttl_seconds: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CounterKey {
    Resolve(String),
    CreateTenMinute(String),
    CreateDaily {
        fingerprint: String,
        utc_date: String,
    },
}

impl MemoryLinkStore {
    pub fn from_config(config: &AppConfig, clock: Clock) -> Result<Self, MemoryStoreConfigError> {
        if !memory_store_is_configured(config) {
            return Err(MemoryStoreConfigError);
        }

        Ok(Self::new(clock))
    }

    pub fn set_failures(&self, failures: StoreFailures) -> Result<(), StoreError> {
        self.lock_state()?.failures = failures;
        Ok(())
    }

    /// Configures a starting risk score without involving the LinkStore command path.
    pub fn set_risk_score(
        &self,
        fingerprint: impl Into<String>,
        score: u64,
    ) -> Result<(), StoreError> {
        let now = (self.clock)();
        let expires_at = expiry_after_seconds(now, RISK_TTL_SECONDS)?;
        self.lock_state()?.risks.insert(
            fingerprint.into(),
            ExpiringValue {
                value: score,
                expires_at,
                logical_ttl_seconds: RISK_TTL_SECONDS,
            },
        );
        Ok(())
    }

    pub fn link_ttl_seconds(&self, code: &str) -> Result<Option<u64>, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        Ok(logical_ttl_seconds(&mut state.links, &code.to_owned(), now))
    }

    pub fn create_counter_ttl_seconds(
        &self,
        fingerprint: &str,
        utc_date: &str,
    ) -> Result<Option<(u64, u64)>, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        let ten_minute = logical_ttl_seconds(
            &mut state.counters,
            &CounterKey::CreateTenMinute(fingerprint.to_owned()),
            now,
        );
        let daily = logical_ttl_seconds(
            &mut state.counters,
            &CounterKey::CreateDaily {
                fingerprint: fingerprint.to_owned(),
                utc_date: utc_date.to_owned(),
            },
            now,
        );

        Ok(ten_minute.zip(daily))
    }

    pub fn resolve_counter_ttl_seconds(
        &self,
        fingerprint: &str,
    ) -> Result<Option<u64>, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        Ok(logical_ttl_seconds(
            &mut state.counters,
            &CounterKey::Resolve(fingerprint.to_owned()),
            now,
        ))
    }

    pub fn risk_ttl_seconds(&self, fingerprint: &str) -> Result<Option<u64>, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        Ok(logical_ttl_seconds(
            &mut state.risks,
            &fingerprint.to_owned(),
            now,
        ))
    }

    fn new(clock: Clock) -> Self {
        Self {
            clock,
            state: Arc::new(Mutex::new(MemoryStoreState::default())),
        }
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, MemoryStoreState>, StoreError> {
        self.state.lock().map_err(|_| StoreError::unavailable())
    }
}

#[async_trait]
impl LinkStore for MemoryLinkStore {
    async fn claim(&self, code: &str, target_url: &str, ttl: Duration) -> Result<bool, StoreError> {
        let now = (self.clock)();
        let expires_at = expiry_after_duration(now, ttl)?;
        let code = code.to_owned();
        let mut state = self.lock_state()?;
        if state.failures.claim {
            return Err(StoreError::unavailable());
        }
        if read_expiring(&mut state.links, &code, now).is_some() {
            return Ok(false);
        }

        state.links.insert(
            code,
            ExpiringValue {
                value: target_url.to_owned(),
                expires_at,
                logical_ttl_seconds: ttl.as_secs(),
            },
        );
        Ok(true)
    }

    async fn lookup(&self, code: &str) -> Result<Option<String>, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        if state.failures.lookup {
            return Err(StoreError::unavailable());
        }

        Ok(read_expiring(&mut state.links, &code.to_owned(), now))
    }

    async fn increment_resolve_counter(&self, fingerprint: &str) -> Result<u64, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        if state.failures.increment_resolve_counter {
            return Err(StoreError::unavailable());
        }

        increment_expiring(
            &mut state.counters,
            CounterKey::Resolve(fingerprint.to_owned()),
            RESOLVE_TTL_SECONDS,
            now,
        )
    }

    async fn increment_create_counters(
        &self,
        fingerprint: &str,
        utc_date: &str,
    ) -> Result<CreateCounts, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        if state.failures.increment_create_counters {
            return Err(StoreError::unavailable());
        }

        let ten_minute_count = increment_expiring(
            &mut state.counters,
            CounterKey::CreateTenMinute(fingerprint.to_owned()),
            CREATE_TEN_MINUTE_TTL_SECONDS,
            now,
        )?;
        let daily_count = increment_expiring(
            &mut state.counters,
            CounterKey::CreateDaily {
                fingerprint: fingerprint.to_owned(),
                utc_date: utc_date.to_owned(),
            },
            CREATE_DAILY_TTL_SECONDS,
            now,
        )?;
        Ok(CreateCounts {
            ten_minute_count,
            daily_count,
        })
    }

    async fn risk_score(&self, fingerprint: &str) -> Result<u64, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        if state.failures.risk_score {
            return Err(StoreError::unavailable());
        }

        Ok(read_expiring(&mut state.risks, &fingerprint.to_owned(), now).unwrap_or(0))
    }

    async fn add_risk_score(&self, fingerprint: &str, points: u64) -> Result<u64, StoreError> {
        let now = (self.clock)();
        let mut state = self.lock_state()?;
        if state.failures.add_risk_score {
            return Err(StoreError::unavailable());
        }

        add_to_expiring(
            &mut state.risks,
            fingerprint.to_owned(),
            points,
            RISK_TTL_SECONDS,
            now,
        )
    }

    async fn ping(&self) -> Result<(), StoreError> {
        let state = self.lock_state()?;
        if state.failures.ping {
            return Err(StoreError::unavailable());
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), StoreError> {
        let state = self.lock_state()?;
        if state.failures.close {
            return Err(StoreError::unavailable());
        }
        Ok(())
    }
}

fn read_expiring<K, T>(
    values: &mut BTreeMap<K, ExpiringValue<T>>,
    key: &K,
    now: OffsetDateTime,
) -> Option<T>
where
    K: Ord,
    T: Clone,
{
    if values.get(key).is_some_and(|entry| entry.expires_at <= now) {
        values.remove(key);
        return None;
    }

    values.get(key).map(|entry| entry.value.clone())
}

fn logical_ttl_seconds<K, T>(
    values: &mut BTreeMap<K, ExpiringValue<T>>,
    key: &K,
    now: OffsetDateTime,
) -> Option<u64>
where
    K: Ord,
{
    if values.get(key).is_some_and(|entry| entry.expires_at <= now) {
        values.remove(key);
        return None;
    }

    values.get(key).map(|entry| entry.logical_ttl_seconds)
}

fn increment_expiring<K>(
    values: &mut BTreeMap<K, ExpiringValue<u64>>,
    key: K,
    ttl_seconds: u64,
    now: OffsetDateTime,
) -> Result<u64, StoreError>
where
    K: Ord,
{
    add_to_expiring(values, key, 1, ttl_seconds, now)
}

fn add_to_expiring<K>(
    values: &mut BTreeMap<K, ExpiringValue<u64>>,
    key: K,
    points: u64,
    ttl_seconds: u64,
    now: OffsetDateTime,
) -> Result<u64, StoreError>
where
    K: Ord,
{
    if let Some(entry) = values.get_mut(&key)
        && entry.expires_at > now
    {
        entry.value = entry
            .value
            .checked_add(points)
            .ok_or_else(StoreError::unavailable)?;
        return Ok(entry.value);
    }

    values.remove(&key);
    values.insert(
        key,
        ExpiringValue {
            value: points,
            expires_at: expiry_after_seconds(now, ttl_seconds)?,
            logical_ttl_seconds: ttl_seconds,
        },
    );
    Ok(points)
}

fn expiry_after_seconds(now: OffsetDateTime, seconds: u64) -> Result<OffsetDateTime, StoreError> {
    let seconds = i64::try_from(seconds).map_err(|_| StoreError::unavailable())?;
    now.checked_add(TimeDuration::seconds(seconds))
        .ok_or_else(StoreError::unavailable)
}

fn expiry_after_duration(now: OffsetDateTime, ttl: Duration) -> Result<OffsetDateTime, StoreError> {
    let seconds = i64::try_from(ttl.as_secs()).map_err(|_| StoreError::unavailable())?;
    let with_seconds = now
        .checked_add(TimeDuration::seconds(seconds))
        .ok_or_else(StoreError::unavailable)?;
    with_seconds
        .checked_add(TimeDuration::nanoseconds(i64::from(ttl.subsec_nanos())))
        .ok_or_else(StoreError::unavailable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FakeTurnstileOutcome {
    Invalid,
    Unavailable,
}

#[derive(Clone)]
pub struct FakeTurnstile {
    state: Arc<Mutex<FakeTurnstileState>>,
}

struct FakeTurnstileState {
    outcome: Option<FakeTurnstileOutcome>,
    calls: u64,
}

impl Default for FakeTurnstile {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeTurnstile {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeTurnstileState {
                outcome: None,
                calls: 0,
            })),
        }
    }

    pub fn with_outcome(outcome: FakeTurnstileOutcome) -> Self {
        let verifier = Self::new();
        verifier
            .set_outcome(Some(outcome))
            .expect("new fake Turnstile state must not be poisoned");
        verifier
    }

    pub fn set_outcome(&self, outcome: Option<FakeTurnstileOutcome>) -> Result<(), ChallengeError> {
        self.lock_state()?.outcome = outcome;
        Ok(())
    }

    pub fn call_count(&self) -> Result<u64, ChallengeError> {
        Ok(self.lock_state()?.calls)
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, FakeTurnstileState>, ChallengeError> {
        self.state.lock().map_err(|_| ChallengeError::unavailable())
    }
}

#[async_trait]
impl ChallengeVerifier for FakeTurnstile {
    async fn verify(&self, token: &str) -> Result<bool, ChallengeError> {
        let mut state = self.lock_state()?;
        state.calls = state
            .calls
            .checked_add(1)
            .ok_or_else(ChallengeError::unavailable)?;

        match state.outcome {
            Some(FakeTurnstileOutcome::Invalid) => Ok(false),
            Some(FakeTurnstileOutcome::Unavailable) => Err(ChallengeError::unavailable()),
            None => Ok(token == "test-token"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use time::{Duration as TimeDuration, macros::datetime};

    use crate::{
        AppConfig, AppError, ErrorCode, LinkStore, config::NodeEnvironment,
        ports::ChallengeVerifier,
    };

    use super::{
        CREATE_DAILY_TTL_SECONDS, CREATE_TEN_MINUTE_TTL_SECONDS, DeterministicClock, FakeTurnstile,
        FakeTurnstileOutcome, MemoryLinkStore, RESOLVE_TTL_SECONDS, RISK_TTL_SECONDS,
        SequenceShortCodeGenerator, StoreFailures, memory_store_is_configured,
    };

    fn test_clock() -> DeterministicClock {
        DeterministicClock::new(datetime!(2026-08-26 4:00 UTC))
    }

    fn test_store() -> MemoryLinkStore {
        let clock = test_clock();
        MemoryLinkStore::new(clock.as_clock())
    }

    fn base_env() -> BTreeMap<String, String> {
        [
            ("NODE_ENV", "test"),
            ("PUBLIC_BASE_URL", "https://myurl.example"),
            (
                "IP_HASH_SECRET",
                "test-secret-that-is-at-least-32-bytes-long",
            ),
            ("TURNSTILE_ENABLED", "false"),
            ("TURNSTILE_MODE", "test"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
    }

    fn test_config(test_store: Option<&str>) -> AppConfig {
        let mut environment = base_env();
        if let Some(test_store) = test_store {
            environment.insert("TEST_STORE".to_owned(), test_store.to_owned());
        }
        AppConfig::from_env(environment).unwrap()
    }

    fn assert_dependency_unavailable<T>(result: Result<T, crate::StoreError>) {
        let error = match result {
            Ok(_) => panic!("expected a store failure"),
            Err(error) => AppError::from(error),
        };
        assert_eq!(error.code(), ErrorCode::DependencyUnavailable);
        assert_eq!(error.status_code(), 503);
    }

    #[tokio::test]
    async fn claim_has_nx_semantics_without_overwriting_an_existing_link() {
        let store = test_store();
        assert!(
            store
                .claim(
                    "short-code",
                    "https://first.example/",
                    Duration::from_secs(60)
                )
                .await
                .unwrap()
        );
        assert!(
            !store
                .claim(
                    "short-code",
                    "https://second.example/",
                    Duration::from_secs(60)
                )
                .await
                .unwrap()
        );
        assert_eq!(
            store.lookup("short-code").await.unwrap(),
            Some("https://first.example/".to_owned())
        );
    }

    #[tokio::test]
    async fn lookup_removes_expired_links_before_returning_a_miss() {
        let store = test_store();
        assert!(
            store
                .claim("expired", "https://example.test/", Duration::ZERO)
                .await
                .unwrap()
        );

        assert_eq!(store.lookup("expired").await.unwrap(), None);
        assert_eq!(store.link_ttl_seconds("expired").unwrap(), None);
    }

    #[tokio::test]
    async fn counters_and_risk_scores_keep_their_first_write_ttls() {
        let clock = test_clock();
        let store = MemoryLinkStore::new(clock.as_clock());
        let fingerprint = "fingerprint";
        let utc_date = "2026-08-26";

        assert_eq!(
            store
                .increment_create_counters(fingerprint, utc_date)
                .await
                .unwrap()
                .ten_minute_count,
            1
        );
        assert_eq!(
            store
                .increment_create_counters(fingerprint, utc_date)
                .await
                .unwrap()
                .daily_count,
            2
        );
        assert_eq!(
            store.increment_resolve_counter(fingerprint).await.unwrap(),
            1
        );
        assert_eq!(store.add_risk_score(fingerprint, 2).await.unwrap(), 2);

        assert_eq!(
            store
                .create_counter_ttl_seconds(fingerprint, utc_date)
                .unwrap(),
            Some((CREATE_TEN_MINUTE_TTL_SECONDS, CREATE_DAILY_TTL_SECONDS))
        );
        assert_eq!(
            store.resolve_counter_ttl_seconds(fingerprint).unwrap(),
            Some(RESOLVE_TTL_SECONDS)
        );
        assert_eq!(
            store.risk_ttl_seconds(fingerprint).unwrap(),
            Some(RISK_TTL_SECONDS)
        );

        clock.advance(TimeDuration::seconds(9)).unwrap();
        assert_eq!(
            store.increment_resolve_counter(fingerprint).await.unwrap(),
            2
        );
        assert_eq!(store.add_risk_score(fingerprint, 3).await.unwrap(), 5);
        assert_eq!(
            store.resolve_counter_ttl_seconds(fingerprint).unwrap(),
            Some(RESOLVE_TTL_SECONDS)
        );
        assert_eq!(
            store.risk_ttl_seconds(fingerprint).unwrap(),
            Some(RISK_TTL_SECONDS)
        );

        clock.advance(TimeDuration::seconds(2)).unwrap();
        assert_eq!(
            store.increment_resolve_counter(fingerprint).await.unwrap(),
            1
        );
        assert_eq!(store.risk_score(fingerprint).await.unwrap(), 5);

        clock.advance(TimeDuration::seconds(589)).unwrap();
        assert_eq!(store.risk_score(fingerprint).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn configured_risk_scores_are_read_and_incremented() {
        let store = test_store();
        store.set_risk_score("fingerprint", 7).unwrap();

        assert_eq!(store.risk_score("fingerprint").await.unwrap(), 7);
        assert_eq!(store.add_risk_score("fingerprint", 3).await.unwrap(), 10);
        assert_eq!(store.risk_score("fingerprint").await.unwrap(), 10);
    }

    #[tokio::test]
    async fn each_configured_store_failure_maps_to_dependency_unavailable() {
        let store = test_store();
        store.set_failures(StoreFailures::all()).unwrap();

        assert_dependency_unavailable(
            store
                .claim("code", "https://example.test/", Duration::from_secs(60))
                .await,
        );
        assert_dependency_unavailable(store.lookup("code").await);
        assert_dependency_unavailable(store.increment_resolve_counter("fingerprint").await);
        assert_dependency_unavailable(
            store
                .increment_create_counters("fingerprint", "2026-08-26")
                .await,
        );
        assert_dependency_unavailable(store.risk_score("fingerprint").await);
        assert_dependency_unavailable(store.add_risk_score("fingerprint", 1).await);
        assert_dependency_unavailable(store.ping().await);
        assert_dependency_unavailable(store.close().await);
    }

    #[tokio::test]
    async fn fake_turnstile_records_calls_and_controls_its_outcomes() {
        let verifier = FakeTurnstile::new();
        assert!(verifier.verify("test-token").await.unwrap());
        assert!(!verifier.verify("invalid-token").await.unwrap());

        verifier
            .set_outcome(Some(FakeTurnstileOutcome::Invalid))
            .unwrap();
        assert!(!verifier.verify("test-token").await.unwrap());

        verifier
            .set_outcome(Some(FakeTurnstileOutcome::Unavailable))
            .unwrap();
        assert!(verifier.verify("test-token").await.is_err());
        assert_eq!(verifier.call_count().unwrap(), 4);
    }

    #[test]
    fn deterministic_helpers_supply_fixed_time_and_code_sequences() {
        let clock = test_clock();
        let now = clock.as_clock();
        assert_eq!(now(), datetime!(2026-08-26 4:00 UTC));

        let generator = SequenceShortCodeGenerator::new(["taken12345", "fresh12345"]);
        let next_code = generator.as_generator();
        assert_eq!(next_code().unwrap(), "taken12345");
        assert_eq!(next_code().unwrap(), "fresh12345");
        assert!(next_code().is_err());
    }

    #[test]
    fn memory_store_construction_is_gated_by_test_config() {
        let enabled_config = test_config(Some("memory"));
        assert!(memory_store_is_configured(&enabled_config));
        assert!(MemoryLinkStore::from_config(&enabled_config, test_clock().as_clock()).is_ok());

        let missing_store_config = test_config(None);
        assert!(!memory_store_is_configured(&missing_store_config));
        assert!(
            MemoryLinkStore::from_config(&missing_store_config, test_clock().as_clock()).is_err()
        );

        let mut development_environment = base_env();
        development_environment.insert("NODE_ENV".to_owned(), "development".to_owned());
        development_environment.insert("TURNSTILE_MODE".to_owned(), "cloudflare".to_owned());
        let development_config = AppConfig::from_env(development_environment).unwrap();
        assert_eq!(development_config.node_env, NodeEnvironment::Development);
        assert!(!memory_store_is_configured(&development_config));
        assert!(
            MemoryLinkStore::from_config(&development_config, test_clock().as_clock()).is_err()
        );
    }
}
