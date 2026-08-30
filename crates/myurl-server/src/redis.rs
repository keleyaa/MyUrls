use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use async_trait::async_trait;
use redis::{AsyncConnectionConfig, FromRedisValue, aio::MultiplexedConnection};
use tokio::{sync::Mutex, time::timeout};

use crate::{
    error::StoreError,
    ports::{CreateCounts, LinkStore},
};

const CREATE_COUNTERS_SCRIPT: &str = r#"
local short_count = redis.call('INCR', KEYS[1])
if short_count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
local daily_count = redis.call('INCR', KEYS[2])
if daily_count == 1 then redis.call('EXPIRE', KEYS[2], ARGV[2]) end
return { short_count, daily_count }
"#;

const SINGLE_COUNTER_SCRIPT: &str = r#"
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return count
"#;

const RISK_SCRIPT: &str = r#"
local existed = redis.call('EXISTS', KEYS[1])
local score = redis.call('INCRBY', KEYS[1], ARGV[1])
if existed == 0 then redis.call('EXPIRE', KEYS[1], ARGV[2]) end
return score
"#;

const CREATE_TEN_MINUTE_TTL_SECONDS: u64 = 600;
const CREATE_DAILY_TTL_SECONDS: u64 = 172_800;
const RESOLVE_TTL_SECONDS: u64 = 10;
const RISK_TTL_SECONDS: u64 = 600;

struct ConnectionSlot {
    connection: Option<MultiplexedConnection>,
    generation: u64,
}

/// A reconnecting Redis adapter used by the production link store.
pub struct RedisLinkStore {
    client: redis::Client,
    connection: Mutex<ConnectionSlot>,
    closed: AtomicBool,
    timeout: Duration,
}

impl RedisLinkStore {
    pub async fn connect(redis_url: &str, timeout_ms: u64) -> Result<Self, StoreError> {
        let operation_timeout = Duration::from_millis(timeout_ms);
        let client = redis::Client::open(redis_url).map_err(|_| StoreError::unavailable())?;
        let connection = Self::open_connection(&client, operation_timeout).await?;

        Ok(Self {
            client,
            connection: Mutex::new(ConnectionSlot {
                connection: Some(connection),
                generation: 0,
            }),
            closed: AtomicBool::new(false),
            timeout: operation_timeout,
        })
    }

    async fn open_connection(
        client: &redis::Client,
        operation_timeout: Duration,
    ) -> Result<MultiplexedConnection, StoreError> {
        let connection_config = AsyncConnectionConfig::new()
            .set_connection_timeout(operation_timeout)
            .set_response_timeout(operation_timeout);
        timeout(
            operation_timeout,
            client.get_multiplexed_async_connection_with_config(&connection_config),
        )
        .await
        .map_err(|_| StoreError::unavailable())?
        .map_err(|_| StoreError::unavailable())
    }

    async fn connection(&self) -> Result<(u64, MultiplexedConnection), StoreError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(StoreError::unavailable());
        }

        let mut slot = self.connection.lock().await;
        if let Some(connection) = slot.connection.clone() {
            return Ok((slot.generation, connection));
        }

        let new_connection = Self::open_connection(&self.client, self.timeout).await?;
        if self.closed.load(Ordering::Acquire) {
            return Err(StoreError::unavailable());
        }
        slot.generation = slot.generation.wrapping_add(1);
        slot.connection = Some(new_connection.clone());
        Ok((slot.generation, new_connection))
    }

    async fn invalidate_connection(&self, generation: u64) {
        let mut slot = self.connection.lock().await;
        if slot.generation == generation {
            slot.connection.take();
        }
    }

    async fn execute<T>(&self, command: redis::Cmd) -> Result<T, StoreError>
    where
        T: FromRedisValue,
    {
        let (generation, mut connection) = self.connection().await?;
        let operation = async {
            command
                .query_async(&mut connection)
                .await
                .map_err(|_| StoreError::unavailable())
        };
        let result = match timeout(self.timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(StoreError::unavailable()),
        };

        if result.is_err() && !self.closed.load(Ordering::Acquire) {
            self.invalidate_connection(generation).await;
        }
        result
    }
}

#[async_trait]
impl LinkStore for RedisLinkStore {
    async fn claim(&self, code: &str, target_url: &str, ttl: Duration) -> Result<bool, StoreError> {
        let mut command = redis::cmd("SET");
        command
            .arg(link_key(code))
            .arg(target_url)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs());
        let result: Option<String> = self.execute(command).await?;

        match result.as_deref() {
            Some("OK") => Ok(true),
            None => Ok(false),
            _ => Err(StoreError::unavailable()),
        }
    }

    async fn lookup(&self, code: &str) -> Result<Option<String>, StoreError> {
        let mut command = redis::cmd("GET");
        command.arg(link_key(code));
        self.execute(command).await
    }

    async fn increment_resolve_counter(&self, fingerprint: &str) -> Result<u64, StoreError> {
        let result: i64 = self
            .execute(eval_command(
                SINGLE_COUNTER_SCRIPT,
                &[resolve_counter_key(fingerprint)],
                &[RESOLVE_TTL_SECONDS],
            ))
            .await?;
        positive_count(result)
    }

    async fn increment_create_counters(
        &self,
        fingerprint: &str,
        utc_date: &str,
    ) -> Result<CreateCounts, StoreError> {
        let results: Vec<i64> = self
            .execute(eval_command(
                CREATE_COUNTERS_SCRIPT,
                &[
                    create_ten_minute_counter_key(fingerprint),
                    create_daily_counter_key(utc_date, fingerprint),
                ],
                &[CREATE_TEN_MINUTE_TTL_SECONDS, CREATE_DAILY_TTL_SECONDS],
            ))
            .await?;
        let [ten_minute_count, daily_count]: [i64; 2] =
            results.try_into().map_err(|_| StoreError::unavailable())?;

        Ok(CreateCounts {
            ten_minute_count: positive_count(ten_minute_count)?,
            daily_count: positive_count(daily_count)?,
        })
    }

    async fn risk_score(&self, fingerprint: &str) -> Result<u64, StoreError> {
        let mut command = redis::cmd("GET");
        command.arg(risk_key(fingerprint));
        let value: Option<String> = self.execute(command).await?;

        value.map_or(Ok(0), |value| {
            value.parse().map_err(|_| StoreError::unavailable())
        })
    }

    async fn add_risk_score(&self, fingerprint: &str, points: u64) -> Result<u64, StoreError> {
        let result: i64 = self
            .execute(eval_command(
                RISK_SCRIPT,
                &[risk_key(fingerprint)],
                &[points, RISK_TTL_SECONDS],
            ))
            .await?;
        positive_count(result)
    }

    async fn ping(&self) -> Result<(), StoreError> {
        let command = redis::cmd("PING");
        let result: String = self.execute(command).await?;
        if result == "PONG" {
            Ok(())
        } else {
            Err(StoreError::unavailable())
        }
    }

    async fn close(&self) -> Result<(), StoreError> {
        self.closed.store(true, Ordering::Release);
        let connection = match timeout(self.timeout, async {
            let mut slot = self.connection.lock().await;
            slot.connection.take()
        })
        .await
        {
            Ok(connection) => connection,
            Err(_) => return Err(StoreError::unavailable()),
        };
        let Some(mut connection) = connection else {
            return Ok(());
        };

        let quit = timeout(self.timeout, async {
            let result: String = redis::cmd("QUIT")
                .query_async(&mut connection)
                .await
                .map_err(|_| StoreError::unavailable())?;
            if result == "OK" {
                Ok(())
            } else {
                Err(StoreError::unavailable())
            }
        })
        .await;

        // The retained handle is detached before QUIT, so a timeout cannot block shutdown.
        drop(connection);

        match quit {
            Ok(result) => result,
            Err(_) => Err(StoreError::unavailable()),
        }
    }
}

fn eval_command(script: &str, keys: &[String], arguments: &[u64]) -> redis::Cmd {
    let mut command = redis::cmd("EVAL");
    command.arg(script).arg(keys.len());
    for key in keys {
        command.arg(key);
    }
    for argument in arguments {
        command.arg(argument);
    }
    command
}

fn positive_count(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(StoreError::unavailable)
}

fn link_key(code: &str) -> String {
    format!("myurl:link:{code}")
}

fn create_ten_minute_counter_key(fingerprint: &str) -> String {
    format!("myurl:rate:create:10m:{fingerprint}")
}

fn create_daily_counter_key(utc_date: &str, fingerprint: &str) -> String {
    format!("myurl:rate:create:1d:{utc_date}:{fingerprint}")
}

fn resolve_counter_key(fingerprint: &str) -> String {
    format!("myurl:rate:resolve:10s:{fingerprint}")
}

fn risk_key(fingerprint: &str) -> String {
    format!("myurl:risk:create:10m:{fingerprint}")
}

#[cfg(test)]
mod tests {
    use super::positive_count;

    #[test]
    fn positive_count_rejects_negative_values() {
        assert!(positive_count(-1).is_err());
    }

    #[test]
    fn positive_count_rejects_zero() {
        assert!(positive_count(0).is_err());
    }

    #[test]
    fn positive_count_accepts_positive_values() {
        assert_eq!(positive_count(1).unwrap(), 1);
    }
}
