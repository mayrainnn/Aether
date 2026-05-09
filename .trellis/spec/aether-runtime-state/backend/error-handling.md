# Error Handling

`aether-runtime-state` uses the shared `aether_data_contracts::DataLayerError`
for most fallible runtime-state operations and defines one local error enum for
distributed semaphore admission failures. Preserve that split: storage and Redis
problems use `DataLayerError`; semaphore admission state uses
`RuntimeSemaphoreError`.

## Shared DataLayerError Surface

The crate re-exports `DataLayerError` from `src/error.rs`:

```rust
// crates/aether-runtime-state/src/error.rs:1
pub use aether_data_contracts::DataLayerError;

pub(crate) fn redis_error(error: impl std::fmt::Display) -> DataLayerError {
    DataLayerError::redis(error)
}
```

Public Redis, KV, queue, lock, and rate-limit methods should return
`Result<_, DataLayerError>` unless they are specifically semaphore admission
APIs. This keeps callers aligned with the data layer contract used across Aether.

## Redis Error Conversion

Redis operations should use the crate-local `RedisResultExt` helper:

```rust
// crates/aether-runtime-state/src/error.rs:7
pub(crate) trait RedisResultExt<T> {
    fn map_redis_err(self) -> Result<T, DataLayerError>;
}

impl<T> RedisResultExt<T> for Result<T, redis::RedisError> {
    fn map_redis_err(self) -> Result<T, DataLayerError> {
        self.map_err(redis_error)
    }
}
```

Use `.map_redis_err()?` after `redis::cmd(...).query_async(...)` and Redis
connection creation. Do not hand-format Redis errors in individual methods unless
the code is translating a semantic Redis payload problem into `UnexpectedValue`.

## Configuration Validation

Validate configuration before creating long-lived runtime objects. Invalid
configuration is returned as `DataLayerError::InvalidConfiguration`.

```rust
// crates/aether-runtime-state/src/lib.rs:167
pub fn validate(&self) -> Result<(), DataLayerError> {
    if matches!(self.backend, RuntimeStateBackendMode::Redis) && self.redis.is_none() {
        return Err(DataLayerError::InvalidConfiguration(
            "AETHER_RUNTIME_BACKEND=redis requires AETHER_RUNTIME_REDIS_URL, AETHER_GATEWAY_DATA_REDIS_URL, or REDIS_URL".to_string(),
        ));
    }
    if let Some(redis) = &self.redis {
        redis.validate()?;
    }
    if self.memory.max_kv_entries == 0 {
        return Err(DataLayerError::InvalidConfiguration(
            "runtime memory max_kv_entries must be positive".to_string(),
        ));
    }
    Ok(())
}
```

Runner configs follow the same pattern:
`RedisKvRunnerConfig::validate` rejects zero TTLs in `src/redis/kv.rs:24`,
`RedisLockRunnerConfig::validate` rejects zero TTLs in `src/redis/lock.rs:36`,
and `RedisStreamRunnerConfig::validate` checks blocking-read timeouts in
`src/redis/stream.rs:84`.

## Input Validation Before Network Calls

Validate caller-provided keys, owners, groups, stream names, fields, and TTLs
before opening a Redis connection. This crate intentionally fails fast for local
input bugs.

```rust
// crates/aether-runtime-state/src/redis/lock.rs:84
pub async fn try_acquire(
    &self,
    key: &RedisLockKey,
    owner: &str,
    ttl_ms: Option<u64>,
) -> Result<Option<RedisLockLease>, DataLayerError> {
    validate_owner(owner)?;
    validate_key(key)?;
    let ttl_ms = self.resolve_ttl_ms(ttl_ms)?;
    let token = format!("{owner}:{}", Uuid::new_v4());
    ...
}
```

`src/redis/stream.rs:420` through `src/redis/stream.rs:453` has the equivalent
helpers for streams, groups, consumers, and positions.

## Unexpected Redis Payloads

Use `DataLayerError::UnexpectedValue` when Redis returns a payload that is valid
Redis but not the shape this crate expects.

```rust
// crates/aether-runtime-state/src/redis/stream.rs:456
fn parse_reclaim_result(value: RedisValue) -> Result<RedisStreamReclaimResult, DataLayerError> {
    let RedisValue::Array(parts) = value else {
        return Err(DataLayerError::UnexpectedValue(
            "redis xautoclaim returned non-array payload".to_string(),
        ));
    };
    ...
}
```

Do not silently drop malformed Redis payloads in reclaim parsing. The parser
already accepts both array and map field layouts where Redis versions differ, but
shape mismatches should remain visible to callers.

## Timeout Errors

All Redis operations that can block or hit the network must be wrapped in a
timeout helper. Timeouts become `DataLayerError::TimedOut` with an operation name:

```rust
// crates/aether-runtime-state/src/lib.rs:1723
async fn run_redis_with_timeout<T, F>(
    timeout_ms: Option<u64>,
    operation: &'static str,
    future: F,
) -> Result<T, DataLayerError>
where
    F: Future<Output = Result<T, DataLayerError>>,
{
    if let Some(timeout_ms) = timeout_ms {
        tokio::time::timeout(Duration::from_millis(timeout_ms), future)
            .await
            .map_err(|_| {
                DataLayerError::TimedOut(format!("{operation} exceeded {timeout_ms}ms timeout"))
            })?
    } else {
        future.await
    }
}
```

Use descriptive operation strings such as `"runtime kv mget"` or
`"redis stream reclaim"` so upstream logs and error messages identify the
failing command family.

## Semaphore Error Model

Distributed semaphore APIs return `RuntimeSemaphoreError`, not `DataLayerError`,
because saturation is a valid admission-control result and not a storage failure.

```rust
// crates/aether-runtime-state/src/lib.rs:1267
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeSemaphoreError {
    #[error("runtime semaphore {gate} is saturated at {limit}")]
    Saturated { gate: &'static str, limit: usize },
    #[error("runtime semaphore {gate} is unavailable: {message}")]
    Unavailable { gate: &'static str, limit: usize, message: String },
    #[error("{0}")]
    InvalidConfiguration(String),
}
```

Use `Saturated` when a healthy gate is full, `Unavailable` when Redis or the
lease state cannot be trusted, and `InvalidConfiguration` for zero or impossible
lease settings.

## DO NOT Patterns

DO NOT return `anyhow::Error` from this crate's public API. The source currently
has no `anyhow` dependency, and callers expect typed `DataLayerError` or
`RuntimeSemaphoreError`.

DO NOT log and swallow Redis release or renew failures except in permit `Drop`,
where the async cleanup cannot return an error to the caller.

DO NOT convert Redis parser failures to empty vectors. Empty vectors are valid
results for no work; malformed payloads must use `UnexpectedValue`.
