use std::{collections::BTreeMap, env, str::FromStr};

use ipnet::IpNet;
use thiserror::Error;

use crate::ip::parse_cidr;
use url::Url;

pub const LINK_TTL_SECONDS: u64 = 7_776_000;
pub const MAX_URL_BYTES: usize = 4_096;
pub const MAX_BODY_BYTES: usize = 16 * 1024;
pub const AUTO_CODE_LENGTH: usize = 10;
pub const MAX_CODE_ATTEMPTS: usize = 5;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeEnvironment {
    Development,
    Test,
    Production,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnstileMode {
    Cloudflare,
    Test,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TurnstileConfig {
    pub enabled: bool,
    pub mode: TurnstileMode,
    pub site_key: String,
    pub secret_key: String,
    pub hostname: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Limits {
    pub direct_10m: u64,
    pub hard_10m: u64,
    pub hard_1d: u64,
    pub resolve_10s: u64,
    pub challenge_score: u64,
    pub block_score: u64,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AppConfig {
    pub node_env: NodeEnvironment,
    pub port: u16,
    pub public_base_url: String,
    pub public_base_origin: String,
    pub redis_url: String,
    pub ip_hash_secret: Vec<u8>,
    pub trust_proxy_cidrs: Vec<IpNet>,
    pub turnstile: TurnstileConfig,
    pub limits: Limits,
    pub redis_timeout_ms: u64,
    pub turnstile_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub test_force_challenge: bool,
    pub test_store: Option<String>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    MissingRequired(&'static str),
    #[error("Invalid NODE_ENV")]
    InvalidNodeEnvironment,
    #[error("Invalid numeric configuration: {0}")]
    InvalidNumeric(&'static str),
    #[error("Invalid boolean configuration: {0}")]
    InvalidBoolean(&'static str),
    #[error("Invalid PUBLIC_BASE_URL")]
    InvalidPublicBaseUrl,
    #[error("PUBLIC_BASE_URL must use HTTPS in production")]
    PublicBaseUrlMustUseHttps,
    #[error("Invalid REDIS_URL")]
    InvalidRedisUrl,
    #[error("Invalid REDIS_PASSWORD")]
    InvalidRedisPassword,
    #[error("IP_HASH_SECRET must contain at least 32 bytes")]
    ShortIpHashSecret,
    #[error("IP_HASH_SECRET must not use an example value")]
    ExampleIpHashSecret,
    #[error("Invalid TRUST_PROXY_CIDRS")]
    InvalidTrustProxyCidrs,
    #[error("Invalid limit relationship")]
    InvalidLimitRelationship,
    #[error("Invalid TURNSTILE_MODE")]
    InvalidTurnstileMode,
    #[error("Turnstile keys are required when enabled")]
    MissingTurnstileKeys,
    #[error("Production Turnstile configuration is incomplete")]
    IncompleteProductionTurnstile,
    #[error("Test Turnstile mode is only available in test environment")]
    TestTurnstileModeOutsideTest,
    #[error("TEST_FORCE_CHALLENGE is only available in test environment")]
    TestForceChallengeOutsideTest,
    #[error("TEST_STORE is only available as memory in test environment")]
    InvalidTestStore,
}

impl AppConfig {
    pub fn from_env<I, K, V>(environment: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let values = environment
            .into_iter()
            .map(|(name, value)| (name.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect::<BTreeMap<_, _>>();

        Self::from_values(&values)
    }

    pub fn from_process_env() -> Result<Self, ConfigError> {
        Self::from_env(env::vars())
    }

    fn from_values(values: &BTreeMap<String, String>) -> Result<Self, ConfigError> {
        let node_env = parse_node_environment(required(values, "NODE_ENV")?)?;
        let public_base = parse_public_base_url(required(values, "PUBLIC_BASE_URL")?, node_env)?;
        let ip_hash_secret = required(values, "IP_HASH_SECRET")?;
        validate_ip_hash_secret(ip_hash_secret, node_env)?;

        let direct_10m = parse_integer(values, "CREATE_DIRECT_LIMIT_10M", 5, 1, MAX_SAFE_INTEGER)?;
        let hard_10m = parse_integer(values, "CREATE_HARD_LIMIT_10M", 20, 1, MAX_SAFE_INTEGER)?;
        let hard_1d = parse_integer(values, "CREATE_HARD_LIMIT_1D", 100, 1, MAX_SAFE_INTEGER)?;
        let resolve_10s = parse_integer(values, "RESOLVE_LIMIT_10S", 600, 1, 1_000_000)?;
        let challenge_score =
            parse_integer(values, "RISK_CHALLENGE_SCORE", 3, 0, MAX_SAFE_INTEGER)?;
        let block_score = parse_integer(values, "RISK_BLOCK_SCORE", 8, 0, MAX_SAFE_INTEGER)?;
        if hard_10m <= direct_10m || hard_1d <= hard_10m || block_score <= challenge_score {
            return Err(ConfigError::InvalidLimitRelationship);
        }

        let turnstile_enabled = parse_boolean(values, "TURNSTILE_ENABLED", true)?;
        let turnstile_mode = parse_turnstile_mode(
            values
                .get("TURNSTILE_MODE")
                .map_or("cloudflare", String::as_str),
        )?;
        let site_key = values
            .get("TURNSTILE_SITE_KEY")
            .cloned()
            .unwrap_or_default();
        let secret_key = values
            .get("TURNSTILE_SECRET_KEY")
            .cloned()
            .unwrap_or_default();
        let hostname = values
            .get("TURNSTILE_HOSTNAME")
            .cloned()
            .unwrap_or_default();

        if turnstile_enabled && (site_key.is_empty() || secret_key.is_empty()) {
            return Err(ConfigError::MissingTurnstileKeys);
        }
        if node_env == NodeEnvironment::Production
            && (!turnstile_enabled
                || turnstile_mode != TurnstileMode::Cloudflare
                || hostname.is_empty())
        {
            return Err(ConfigError::IncompleteProductionTurnstile);
        }
        if turnstile_mode == TurnstileMode::Test && node_env != NodeEnvironment::Test {
            return Err(ConfigError::TestTurnstileModeOutsideTest);
        }

        let test_force_challenge = parse_boolean(values, "TEST_FORCE_CHALLENGE", false)?;
        if test_force_challenge && node_env != NodeEnvironment::Test {
            return Err(ConfigError::TestForceChallengeOutsideTest);
        }
        let test_store = values.get("TEST_STORE").cloned();
        if test_store.as_deref().is_some_and(|store| store != "memory")
            || (test_store.is_some() && node_env != NodeEnvironment::Test)
        {
            return Err(ConfigError::InvalidTestStore);
        }

        let port = parse_integer(values, "APP_PORT", 3_000, 1, 65_535)? as u16;
        let redis_url = validate_redis_url(
            values
                .get("REDIS_URL")
                .map_or("redis://redis:6379/0", String::as_str),
            values.get("REDIS_PASSWORD").map_or("", String::as_str),
        )?;

        Ok(Self {
            node_env,
            port,
            public_base_url: public_base.clone(),
            public_base_origin: public_base,
            redis_url,
            ip_hash_secret: ip_hash_secret.as_bytes().to_vec(),
            trust_proxy_cidrs: parse_trust_proxy_cidrs(
                values.get("TRUST_PROXY_CIDRS").map(String::as_str),
                node_env,
            )?,
            turnstile: TurnstileConfig {
                enabled: turnstile_enabled,
                mode: turnstile_mode,
                site_key,
                secret_key,
                hostname,
            },
            limits: Limits {
                direct_10m,
                hard_10m,
                hard_1d,
                resolve_10s,
                challenge_score,
                block_score,
            },
            redis_timeout_ms: parse_integer(values, "REDIS_TIMEOUT_MS", 750, 1, MAX_SAFE_INTEGER)?,
            turnstile_timeout_ms: parse_integer(
                values,
                "TURNSTILE_TIMEOUT_MS",
                2_500,
                1,
                MAX_SAFE_INTEGER,
            )?,
            request_timeout_ms: parse_integer(
                values,
                "REQUEST_TIMEOUT_MS",
                10_000,
                1,
                MAX_SAFE_INTEGER,
            )?,
            shutdown_timeout_ms: parse_integer(
                values,
                "SHUTDOWN_TIMEOUT_MS",
                10_000,
                1,
                MAX_SAFE_INTEGER,
            )?,
            test_force_challenge,
            test_store,
        })
    }
}

fn required<'a>(
    values: &'a BTreeMap<String, String>,
    name: &'static str,
) -> Result<&'a str, ConfigError> {
    values
        .get(name)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or(ConfigError::MissingRequired(name))
}

fn parse_node_environment(raw: &str) -> Result<NodeEnvironment, ConfigError> {
    match raw {
        "development" => Ok(NodeEnvironment::Development),
        "test" => Ok(NodeEnvironment::Test),
        "production" => Ok(NodeEnvironment::Production),
        _ => Err(ConfigError::InvalidNodeEnvironment),
    }
}

fn parse_integer(
    values: &BTreeMap<String, String>,
    name: &'static str,
    fallback: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, ConfigError> {
    let raw = values
        .get(name)
        .map_or_else(|| fallback.to_string(), Clone::clone);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ConfigError::InvalidNumeric(name));
    }
    let value = raw
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidNumeric(name))?;
    if value < minimum || value > maximum {
        return Err(ConfigError::InvalidNumeric(name));
    }
    Ok(value)
}

fn parse_boolean(
    values: &BTreeMap<String, String>,
    name: &'static str,
    fallback: bool,
) -> Result<bool, ConfigError> {
    match values
        .get(name)
        .map_or(if fallback { "true" } else { "false" }, String::as_str)
    {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::InvalidBoolean(name)),
    }
}

fn parse_public_base_url(raw: &str, node_env: NodeEnvironment) -> Result<String, ConfigError> {
    let parsed = Url::parse(raw).map_err(|_| ConfigError::InvalidPublicBaseUrl)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed
            .password()
            .is_some_and(|password| !password.is_empty())
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ConfigError::InvalidPublicBaseUrl);
    }
    if node_env == NodeEnvironment::Production && parsed.scheme() != "https" {
        return Err(ConfigError::PublicBaseUrlMustUseHttps);
    }

    Ok(parsed.origin().ascii_serialization())
}

fn validate_ip_hash_secret(raw: &str, node_env: NodeEnvironment) -> Result<(), ConfigError> {
    if raw.len() < 32 {
        return Err(ConfigError::ShortIpHashSecret);
    }
    if node_env == NodeEnvironment::Production && raw.contains("replace-with") {
        return Err(ConfigError::ExampleIpHashSecret);
    }
    Ok(())
}

fn validate_redis_url(raw: &str, password: &str) -> Result<String, ConfigError> {
    let mut parsed = Url::parse(raw).map_err(|_| ConfigError::InvalidRedisUrl)?;
    if !matches!(parsed.scheme(), "redis" | "rediss")
        || parsed.host_str().is_none()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !valid_redis_database(parsed.path())
    {
        return Err(ConfigError::InvalidRedisUrl);
    }

    if !password.is_empty() {
        if let Some(encoded_password) = parsed.password().filter(|password| !password.is_empty()) {
            let decoded_password = decode_percent_encoded_password(encoded_password)?;
            if decoded_password != password {
                return Err(ConfigError::InvalidRedisPassword);
            }
        }
        parsed
            .set_password(Some(password))
            .map_err(|_| ConfigError::InvalidRedisPassword)?;
    }

    Ok(parsed.into())
}

fn valid_redis_database(path: &str) -> bool {
    if path.is_empty() || path == "/" {
        return true;
    }

    let database = path.strip_prefix('/').and_then(parse_javascript_number);
    database.is_some_and(|database| (0.0..=15.0).contains(&database) && database.fract() == 0.0)
}

fn parse_javascript_number(raw: &str) -> Option<f64> {
    let hexadecimal = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"));
    if let Some(value) = hexadecimal {
        return u64::from_str_radix(value, 16)
            .ok()
            .map(|value| value as f64);
    }
    let binary = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B"));
    if let Some(value) = binary {
        return u64::from_str_radix(value, 2).ok().map(|value| value as f64);
    }
    let octal = raw.strip_prefix("0o").or_else(|| raw.strip_prefix("0O"));
    if let Some(value) = octal {
        return u64::from_str_radix(value, 8).ok().map(|value| value as f64);
    }
    f64::from_str(raw).ok()
}

fn decode_percent_encoded_password(encoded: &str) -> Result<String, ConfigError> {
    let bytes = encoded.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ConfigError::InvalidRedisPassword);
            }
            index += 3;
        } else {
            index += 1;
        }
    }

    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            decoded.push((hex_value(bytes[index + 1]) << 4) | hex_value(bytes[index + 2]));
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| ConfigError::InvalidRedisPassword)
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => unreachable!("validated hexadecimal byte"),
    }
}

fn parse_trust_proxy_cidrs(
    raw: Option<&str>,
    node_env: NodeEnvironment,
) -> Result<Vec<IpNet>, ConfigError> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };

    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let cidr = parse_cidr(part).ok_or(ConfigError::InvalidTrustProxyCidrs)?;
            if node_env == NodeEnvironment::Production && cidr.prefix_len() == 0 {
                return Err(ConfigError::InvalidTrustProxyCidrs);
            }
            Ok(cidr)
        })
        .collect()
}

fn parse_turnstile_mode(raw: &str) -> Result<TurnstileMode, ConfigError> {
    match raw {
        "cloudflare" => Ok(TurnstileMode::Cloudflare),
        "test" => Ok(TurnstileMode::Test),
        _ => Err(ConfigError::InvalidTurnstileMode),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ipnet::IpNet;

    use crate::ip::get_client_ip;

    use super::{AppConfig, ConfigError, Limits, NodeEnvironment, TurnstileMode};

    const IP_HASH_SECRET: &str = "test-secret-that-is-at-least-32-bytes-long";

    fn base_env() -> BTreeMap<String, String> {
        [
            ("NODE_ENV", "test"),
            ("PUBLIC_BASE_URL", "https://myurl.example"),
            ("IP_HASH_SECRET", IP_HASH_SECRET),
            ("REDIS_URL", "redis://:password@redis:6379/15"),
            ("TURNSTILE_ENABLED", "true"),
            ("TURNSTILE_MODE", "test"),
            ("TURNSTILE_SITE_KEY", "site-key"),
            ("TURNSTILE_SECRET_KEY", "turnstile-secret-key"),
            ("TURNSTILE_HOSTNAME", "myurl.example"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
    }

    fn set(env: &mut BTreeMap<String, String>, name: &str, value: &str) {
        env.insert(name.to_owned(), value.to_owned());
    }

    fn config_error(env: &BTreeMap<String, String>) -> ConfigError {
        match AppConfig::from_env(env) {
            Ok(_) => panic!("expected configuration parsing to fail"),
            Err(error) => error,
        }
    }

    fn assert_error(env: &BTreeMap<String, String>, expected: ConfigError) {
        assert_eq!(config_error(env), expected);
    }

    fn assert_redacted_error(env: &BTreeMap<String, String>, secrets: &[&str]) {
        let rendered = config_error(env).to_string();
        for secret in secrets {
            assert!(
                !rendered.contains(secret),
                "configuration error leaked a secret: {rendered}"
            );
        }
    }

    fn production_env() -> BTreeMap<String, String> {
        let mut env = base_env();
        set(&mut env, "NODE_ENV", "production");
        set(&mut env, "TURNSTILE_MODE", "cloudflare");
        env
    }

    #[test]
    fn parses_defaults_and_structured_proxy_cidrs() {
        let mut env = base_env();
        env.remove("REDIS_URL");
        set(&mut env, "TRUST_PROXY_CIDRS", "10.0.0.0/8, 2001:db8::/32");

        let config = AppConfig::from_env(&env).unwrap();

        assert_eq!(config.node_env, NodeEnvironment::Test);
        assert_eq!(config.port, 3_000);
        assert_eq!(config.public_base_url, "https://myurl.example");
        assert_eq!(config.public_base_origin, "https://myurl.example");
        assert_eq!(config.redis_url, "redis://redis:6379/0");
        assert_eq!(
            config.trust_proxy_cidrs,
            vec![
                "10.0.0.0/8".parse::<IpNet>().unwrap(),
                "2001:db8::/32".parse::<IpNet>().unwrap(),
            ]
        );
        assert_eq!(
            config.limits,
            Limits {
                direct_10m: 5,
                hard_10m: 20,
                hard_1d: 100,
                resolve_10s: 600,
                challenge_score: 3,
                block_score: 8,
            }
        );
        assert_eq!(config.redis_timeout_ms, 750);
        assert_eq!(config.turnstile_timeout_ms, 2_500);
        assert_eq!(config.request_timeout_ms, 10_000);
        assert_eq!(config.shutdown_timeout_ms, 10_000);
        assert!(!config.test_force_challenge);
        assert_eq!(config.test_store, None);
    }

    #[test]
    fn merges_redis_password_and_redacts_redis_credentials_from_errors() {
        let mut env = base_env();
        set(&mut env, "REDIS_URL", "redis://redis:6379/15");
        set(&mut env, "REDIS_PASSWORD", "p@ss/word");

        let config = AppConfig::from_env(&env).unwrap();
        assert_eq!(config.redis_url, "redis://:p%40ss%2Fword@redis:6379/15");

        set(
            &mut env,
            "REDIS_URL",
            "redis://:p%40ss%2Fword@redis:6379/15",
        );
        let config = AppConfig::from_env(&env).unwrap();
        assert_eq!(config.redis_url, "redis://:p%40ss%2Fword@redis:6379/15");

        set(
            &mut env,
            "REDIS_URL",
            "redis://:existing-password@redis:6379/15",
        );
        set(&mut env, "REDIS_PASSWORD", "different-password");
        assert_error(&env, ConfigError::InvalidRedisPassword);
        assert_redacted_error(&env, &["existing-password", "different-password"]);
    }

    #[test]
    fn rejects_missing_or_invalid_node_environment() {
        let mut env = base_env();
        env.remove("NODE_ENV");
        assert_error(&env, ConfigError::MissingRequired("NODE_ENV"));

        set(&mut env, "NODE_ENV", "staging");
        assert_error(&env, ConfigError::InvalidNodeEnvironment);
    }

    #[test]
    fn rejects_missing_public_base_and_invalid_origins() {
        let mut env = base_env();
        env.remove("PUBLIC_BASE_URL");
        assert_error(&env, ConfigError::MissingRequired("PUBLIC_BASE_URL"));

        for value in [
            "https://myurl.example/path",
            "https://user:password@myurl.example",
            "https://myurl.example?query=value",
            "https://myurl.example#fragment",
            "ftp://myurl.example",
        ] {
            set(&mut env, "PUBLIC_BASE_URL", value);
            assert_error(&env, ConfigError::InvalidPublicBaseUrl);
        }
    }

    #[test]
    fn rejects_public_base_url_with_bare_query_or_fragment_delimiter() {
        let mut env = base_env();
        for value in ["https://myurl.example?", "https://myurl.example#"] {
            set(&mut env, "PUBLIC_BASE_URL", value);
            assert_error(&env, ConfigError::InvalidPublicBaseUrl);
        }
    }

    #[test]
    fn rejects_short_and_example_ip_hash_secrets_without_leaking_them() {
        let mut env = base_env();
        set(&mut env, "IP_HASH_SECRET", "too-short-secret");
        assert_error(&env, ConfigError::ShortIpHashSecret);
        assert_redacted_error(&env, &["too-short-secret"]);

        let mut env = production_env();
        let example_secret = "replace-with-a-secret-that-is-at-least-32-bytes";
        set(&mut env, "IP_HASH_SECRET", example_secret);
        assert_error(&env, ConfigError::ExampleIpHashSecret);
        assert_redacted_error(&env, &[example_secret]);
    }

    #[test]
    fn rejects_invalid_redis_schemes_and_databases() {
        let mut env = base_env();
        for value in [
            "http://redis:6379",
            "redis://redis:6379/16",
            "redis://redis:6379/not-a-db",
        ] {
            set(&mut env, "REDIS_URL", value);
            assert_error(&env, ConfigError::InvalidRedisUrl);
        }
    }

    #[test]
    fn rejects_redis_url_with_bare_query_or_fragment_delimiter() {
        let mut env = base_env();
        for value in ["redis://redis:6379/0?", "redis://redis:6379/0#"] {
            set(&mut env, "REDIS_URL", value);
            assert_error(&env, ConfigError::InvalidRedisUrl);
        }
    }

    #[test]
    fn normalizes_mapped_ipv6_proxy_cidrs_for_client_ip_trust() {
        let mut env = base_env();
        set(&mut env, "TRUST_PROXY_CIDRS", "::ffff:10.0.0.0/104");

        let config = AppConfig::from_env(&env).unwrap();

        assert_eq!(
            config.trust_proxy_cidrs,
            vec!["10.0.0.0/8".parse::<IpNet>().unwrap()]
        );
        assert_eq!(
            get_client_ip(
                Some("10.0.0.8"),
                Some("198.51.100.4"),
                None,
                &config.trust_proxy_cidrs,
            ),
            "198.51.100.4"
        );
    }

    #[test]
    fn rejects_invalid_proxy_cidrs_and_production_unbounded_trust() {
        let mut env = base_env();
        set(&mut env, "TRUST_PROXY_CIDRS", "not-a-cidr");
        assert_error(&env, ConfigError::InvalidTrustProxyCidrs);

        let mut env = production_env();
        for value in ["0.0.0.0/0", "::/0"] {
            set(&mut env, "TRUST_PROXY_CIDRS", value);
            assert_error(&env, ConfigError::InvalidTrustProxyCidrs);
        }
    }

    #[test]
    fn rejects_invalid_port_and_timeout_values() {
        let mut env = base_env();
        for value in ["0", "65536", "not-a-number"] {
            set(&mut env, "APP_PORT", value);
            assert_error(&env, ConfigError::InvalidNumeric("APP_PORT"));
        }

        for (name, value) in [
            ("REDIS_TIMEOUT_MS", "0"),
            ("TURNSTILE_TIMEOUT_MS", "invalid"),
            ("REQUEST_TIMEOUT_MS", "0"),
            ("SHUTDOWN_TIMEOUT_MS", "invalid"),
        ] {
            env.remove("APP_PORT");
            set(&mut env, name, value);
            assert_error(&env, ConfigError::InvalidNumeric(name));
            env.remove(name);
        }
    }

    #[test]
    fn rejects_invalid_booleans_and_turnstile_modes() {
        let mut env = base_env();
        set(&mut env, "TURNSTILE_ENABLED", "yes");
        assert_error(&env, ConfigError::InvalidBoolean("TURNSTILE_ENABLED"));

        set(&mut env, "TURNSTILE_ENABLED", "true");
        set(&mut env, "TURNSTILE_MODE", "captcha");
        assert_error(&env, ConfigError::InvalidTurnstileMode);

        set(&mut env, "TURNSTILE_MODE", "test");
        set(&mut env, "TEST_FORCE_CHALLENGE", "yes");
        assert_error(&env, ConfigError::InvalidBoolean("TEST_FORCE_CHALLENGE"));
    }

    #[test]
    fn rejects_invalid_limit_relationships() {
        let mut env = base_env();
        for (name, value) in [
            ("CREATE_HARD_LIMIT_10M", "5"),
            ("CREATE_HARD_LIMIT_1D", "20"),
            ("RISK_BLOCK_SCORE", "3"),
        ] {
            set(&mut env, name, value);
            assert_error(&env, ConfigError::InvalidLimitRelationship);
            env.remove(name);
        }
    }

    #[test]
    fn requires_https_and_complete_cloudflare_turnstile_in_production() {
        let mut env = production_env();
        set(&mut env, "PUBLIC_BASE_URL", "http://myurl.example");
        assert_error(&env, ConfigError::PublicBaseUrlMustUseHttps);

        set(&mut env, "PUBLIC_BASE_URL", "https://myurl.example");
        set(&mut env, "TURNSTILE_MODE", "test");
        assert_error(&env, ConfigError::IncompleteProductionTurnstile);

        set(&mut env, "TURNSTILE_MODE", "cloudflare");
        set(&mut env, "TURNSTILE_ENABLED", "false");
        assert_error(&env, ConfigError::IncompleteProductionTurnstile);

        set(&mut env, "TURNSTILE_ENABLED", "true");
        env.remove("TURNSTILE_HOSTNAME");
        assert_error(&env, ConfigError::IncompleteProductionTurnstile);
    }

    #[test]
    fn rejects_test_only_turnstile_mode_and_flags_outside_test() {
        let mut env = base_env();
        set(&mut env, "NODE_ENV", "development");
        assert_error(&env, ConfigError::TestTurnstileModeOutsideTest);

        set(&mut env, "TURNSTILE_MODE", "cloudflare");
        set(&mut env, "TEST_FORCE_CHALLENGE", "true");
        assert_error(&env, ConfigError::TestForceChallengeOutsideTest);

        set(&mut env, "TEST_FORCE_CHALLENGE", "false");
        set(&mut env, "TEST_STORE", "memory");
        assert_error(&env, ConfigError::InvalidTestStore);
    }

    #[test]
    fn allows_disabled_turnstile_and_memory_store_only_in_test() {
        let mut env = base_env();
        set(&mut env, "TURNSTILE_ENABLED", "false");
        set(&mut env, "TEST_STORE", "memory");
        let config = AppConfig::from_env(&env).unwrap();
        assert!(!config.turnstile.enabled);
        assert_eq!(config.turnstile.mode, TurnstileMode::Test);
        assert_eq!(config.test_store.as_deref(), Some("memory"));

        set(&mut env, "TEST_STORE", "filesystem");
        assert_error(&env, ConfigError::InvalidTestStore);
    }
}
