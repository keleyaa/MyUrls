use std::{env, fmt, sync::Arc, time::Duration};

use myurl_server::{AppError, ErrorCode, LinkStore, RedisLinkStore};
use redis::aio::MultiplexedConnection;
use tokio::{task::JoinSet, time::sleep};
use uuid::Uuid;

const REDIS_TIMEOUT_MS: u64 = 1_500;
const LINK_TTL_SECONDS: u64 = 7_776_000;
const CREATE_TEN_MINUTE_TTL_SECONDS: i64 = 600;
const CREATE_DAILY_TTL_SECONDS: i64 = 172_800;
const RESOLVE_TTL_SECONDS: i64 = 10;
const RISK_TTL_SECONDS: i64 = 600;

#[derive(Debug)]
struct IntegrationFailure;

impl fmt::Display for IntegrationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Redis integration check failed")
    }
}

impl std::error::Error for IntegrationFailure {}

type TestResult<T> = Result<T, IntegrationFailure>;

#[tokio::test]
#[ignore = "requires the integration runner and a real Redis server"]
async fn redis_link_store_with_real_redis() -> TestResult<()> {
    if env::var("MYURL_REDIS_INTEGRATION").as_deref() != Ok("1") {
        return Err(IntegrationFailure);
    }

    let namespace = format!("it-{}", Uuid::new_v4().simple());
    let redis_url = env::var("REDIS_URL").map_err(|_| IntegrationFailure)?;
    let client = redis::Client::open(redis_url.as_str()).map_err(|_| IntegrationFailure)?;
    let mut inspector = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| IntegrationFailure)?;

    let before_cleanup_result = cleanup_namespace(&mut inspector, &namespace).await;
    let suite_result = if before_cleanup_result.is_ok() {
        run_suite(&redis_url, &mut inspector, &namespace).await
    } else {
        Err(IntegrationFailure)
    };
    let after_cleanup_result = cleanup_namespace(&mut inspector, &namespace).await;
    let close_result = quit(&mut inspector).await;

    if before_cleanup_result.is_err() || after_cleanup_result.is_err() {
        eprintln!("Redis integration cleanup failed");
    }

    suite_result?;
    before_cleanup_result?;
    after_cleanup_result?;
    close_result
}

async fn run_suite(
    redis_url: &str,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let store = Arc::new(
        RedisLinkStore::connect(redis_url, REDIS_TIMEOUT_MS)
            .await
            .map_err(|_| IntegrationFailure)?,
    );
    check(store.ping().await.is_ok())?;
    claim_uses_nx_and_keeps_link_ttl(&store, inspector, namespace).await?;
    create_counters_are_atomic_and_keep_ttls(&store, inspector, namespace).await?;
    resolve_counter_keeps_ttl(&store, inspector, namespace).await?;
    risk_scores_are_added_and_keep_ttl(&store, inspector, namespace).await?;
    concurrent_claims_have_one_winner(&store, namespace).await?;
    expiry_is_observed(&store, inspector, namespace).await?;
    a_new_store_reads_existing_data(&store, redis_url, namespace).await?;

    check(store.close().await.is_ok())?;
    let closed_code = format!("{namespace}-closed-connection");
    let closed_error = store.lookup(&closed_code).await.err();
    check(matches!(
        closed_error.map(AppError::from).map(|error| error.code()),
        Some(ErrorCode::DependencyUnavailable)
    ))
}

async fn claim_uses_nx_and_keeps_link_ttl(
    store: &Arc<RedisLinkStore>,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let code = format!("{namespace}-claim");
    let first_target = "https://example.test/first";
    let second_target = "https://example.test/second";

    check(
        store
            .claim(&code, first_target, Duration::from_secs(LINK_TTL_SECONDS))
            .await
            .map_err(|_| IntegrationFailure)?,
    )?;
    check(
        !store
            .claim(&code, second_target, Duration::from_secs(LINK_TTL_SECONDS))
            .await
            .map_err(|_| IntegrationFailure)?,
    )?;
    check(
        store
            .lookup(&code)
            .await
            .map_err(|_| IntegrationFailure)?
            .as_deref()
            == Some(first_target),
    )?;

    check(ttl(inspector, &format!("myurl:link:{code}")).await? >= (LINK_TTL_SECONDS - 5) as i64)
}

async fn create_counters_are_atomic_and_keep_ttls(
    store: &Arc<RedisLinkStore>,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let fingerprint = format!("{namespace}-create");
    let utc_date = "2026-08-26";
    let mut tasks = JoinSet::new();

    for _ in 0..20 {
        let store = Arc::clone(store);
        let fingerprint = fingerprint.clone();
        tasks.spawn(async move {
            store
                .increment_create_counters(&fingerprint, utc_date)
                .await
        });
    }

    let mut ten_minute_counts = Vec::new();
    let mut daily_counts = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let counts = result
            .map_err(|_| IntegrationFailure)?
            .map_err(|_| IntegrationFailure)?;
        ten_minute_counts.push(counts.ten_minute_count);
        daily_counts.push(counts.daily_count);
    }
    ten_minute_counts.sort_unstable();
    daily_counts.sort_unstable();
    let expected = (1..=20).collect::<Vec<_>>();
    check(ten_minute_counts == expected)?;
    check(daily_counts == expected)?;

    check(
        ttl(inspector, &format!("myurl:rate:create:10m:{fingerprint}")).await?
            >= CREATE_TEN_MINUTE_TTL_SECONDS - 5,
    )?;
    check(
        ttl(
            inspector,
            &format!("myurl:rate:create:1d:{utc_date}:{fingerprint}"),
        )
        .await?
            >= CREATE_DAILY_TTL_SECONDS - 5,
    )
}

async fn resolve_counter_keeps_ttl(
    store: &Arc<RedisLinkStore>,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let fingerprint = format!("{namespace}-resolve");

    check(
        store
            .increment_resolve_counter(&fingerprint)
            .await
            .map_err(|_| IntegrationFailure)?
            == 1,
    )?;
    check(
        store
            .increment_resolve_counter(&fingerprint)
            .await
            .map_err(|_| IntegrationFailure)?
            == 2,
    )?;
    check(
        ttl(inspector, &format!("myurl:rate:resolve:10s:{fingerprint}")).await?
            >= RESOLVE_TTL_SECONDS - 2,
    )
}

async fn risk_scores_are_added_and_keep_ttl(
    store: &Arc<RedisLinkStore>,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let fingerprint = format!("{namespace}-risk");

    check(
        store
            .add_risk_score(&fingerprint, 3)
            .await
            .map_err(|_| IntegrationFailure)?
            == 3,
    )?;
    check(
        store
            .add_risk_score(&fingerprint, 1)
            .await
            .map_err(|_| IntegrationFailure)?
            == 4,
    )?;
    check(
        store
            .risk_score(&fingerprint)
            .await
            .map_err(|_| IntegrationFailure)?
            == 4,
    )?;
    check(
        ttl(inspector, &format!("myurl:risk:create:10m:{fingerprint}")).await?
            >= RISK_TTL_SECONDS - 5,
    )
}

async fn concurrent_claims_have_one_winner(
    store: &Arc<RedisLinkStore>,
    namespace: &str,
) -> TestResult<()> {
    let code = format!("{namespace}-concurrent");
    let mut tasks = JoinSet::new();

    for index in 0..20 {
        let store = Arc::clone(store);
        let code = code.clone();
        tasks.spawn(async move {
            store
                .claim(
                    &code,
                    &format!("https://example.test/{index}"),
                    Duration::from_secs(LINK_TTL_SECONDS),
                )
                .await
        });
    }

    let mut winners = 0;
    while let Some(result) = tasks.join_next().await {
        if result
            .map_err(|_| IntegrationFailure)?
            .map_err(|_| IntegrationFailure)?
        {
            winners += 1;
        }
    }

    check(winners == 1)
}

async fn expiry_is_observed(
    store: &Arc<RedisLinkStore>,
    inspector: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let code = format!("{namespace}-expires");
    let key = format!("myurl:link:{code}");
    let mut command = redis::cmd("SET");
    command
        .arg(&key)
        .arg("https://example.test/expired")
        .arg("EX")
        .arg(1_u64);
    let result: String = query(inspector, &command).await?;
    check(result == "OK")?;

    sleep(Duration::from_millis(1_100)).await;
    check(
        store
            .lookup(&code)
            .await
            .map_err(|_| IntegrationFailure)?
            .is_none(),
    )
}

async fn a_new_store_reads_existing_data(
    store: &Arc<RedisLinkStore>,
    redis_url: &str,
    namespace: &str,
) -> TestResult<()> {
    let code = format!("{namespace}-reconnect");
    let target = "https://example.test/reconnect";
    check(
        store
            .claim(&code, target, Duration::from_secs(LINK_TTL_SECONDS))
            .await
            .map_err(|_| IntegrationFailure)?,
    )?;

    let replacement = RedisLinkStore::connect(redis_url, REDIS_TIMEOUT_MS)
        .await
        .map_err(|_| IntegrationFailure)?;
    check(
        replacement
            .lookup(&code)
            .await
            .map_err(|_| IntegrationFailure)?
            .as_deref()
            == Some(target),
    )?;
    check(replacement.close().await.is_ok())
}

async fn ttl(connection: &mut MultiplexedConnection, key: &str) -> TestResult<i64> {
    let mut command = redis::cmd("TTL");
    command.arg(key);
    query(connection, &command).await
}

async fn cleanup_namespace(
    connection: &mut MultiplexedConnection,
    namespace: &str,
) -> TestResult<()> {
    let pattern = format!("myurl:*:{namespace}-*");
    let mut cursor = 0_u64;

    loop {
        let mut scan = redis::cmd("SCAN");
        scan.arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100);
        let (next_cursor, keys): (u64, Vec<String>) = query(connection, &scan).await?;

        if !keys.is_empty() {
            let mut unlink = redis::cmd("UNLINK");
            unlink.arg(keys);
            let _: i64 = query(connection, &unlink).await?;
        }

        if next_cursor == 0 {
            return Ok(());
        }
        cursor = next_cursor;
    }
}

async fn quit(connection: &mut MultiplexedConnection) -> TestResult<()> {
    let command = redis::cmd("QUIT");
    let result: String = query(connection, &command).await?;
    check(result == "OK")
}

async fn query<T>(connection: &mut MultiplexedConnection, command: &redis::Cmd) -> TestResult<T>
where
    T: redis::FromRedisValue,
{
    command
        .query_async(connection)
        .await
        .map_err(|_| IntegrationFailure)
}

fn check(condition: bool) -> TestResult<()> {
    condition.then_some(()).ok_or(IntegrationFailure)
}
