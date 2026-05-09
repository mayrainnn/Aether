# Database Guidelines

This crate does not use SeaORM, SQLx, SQL migrations, tables, or transactions.
Its persistent or shared runtime backend is Redis. Treat this file as the Redis
state-backend guide for `aether-runtime-state`.

## Backend Selection

`RuntimeStateConfig` chooses memory or Redis. `Auto` resolves to Redis only when a
Redis config is present; otherwise it uses the in-memory backend.

```rust
// crates/aether-runtime-state/src/lib.rs:227
pub async fn from_config(mut config: RuntimeStateConfig) -> Result<Self, DataLayerError> {
    if matches!(config.backend, RuntimeStateBackendMode::Auto) {
        config.backend = if config.redis.is_some() {
            RuntimeStateBackendMode::Redis
        } else {
            RuntimeStateBackendMode::Memory
        };
    }
    config.validate()?;
    ...
}
```

Redis URLs are read from `AETHER_RUNTIME_REDIS_URL`,
`AETHER_GATEWAY_DATA_REDIS_URL`, or `REDIS_URL`. Key prefixes are read from
`AETHER_RUNTIME_REDIS_KEY_PREFIX` or `AETHER_GATEWAY_DATA_REDIS_KEY_PREFIX`
(`src/lib.rs:140` through `src/lib.rs:149`).

## Connection Handling

Use `RedisClientFactory` to validate config and create lazy clients. Do not open
clients directly from application code.

```rust
// crates/aether-runtime-state/src/redis/client.rs:37
impl RedisClientFactory {
    pub fn new(config: RedisClientConfig) -> Result<Self, DataLayerError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn connect_lazy(&self) -> Result<RedisClient, DataLayerError> {
        RedisClient::open(self.config.url.clone()).map_redis_err()
    }
}
```

`RuntimeState::redis` pings Redis before constructing the backend and then shares
one cloneable client across KV, lock, and stream runners:

```rust
// crates/aether-runtime-state/src/lib.rs:256
pub async fn redis(
    config: RedisClientConfig,
    command_timeout_ms: Option<u64>,
) -> Result<Self, DataLayerError> {
    let factory = RedisClientFactory::new(config)?;
    let client = factory.connect_lazy()?;
    let keyspace = factory.config().keyspace();
    ping_redis(&client, command_timeout_ms).await?;
    let kv = RedisKvRunner::new(client.clone(), keyspace.clone(), ...)?;
    let lock = RedisLockRunner::new(client.clone(), keyspace.clone(), ...)?;
    let stream = RedisStreamRunner::new(client.clone(), keyspace.clone(), ...)?;
    ...
}
```

Every command obtains a multiplexed async connection and maps Redis errors
through `RedisResultExt`.

## Keyspace Rules

All Redis keys must be composed with `RedisKeyspace`. The namespace normalizes
prefixes and adds sub-namespaces for locks and streams.

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:10
impl RedisKeyspace {
    pub fn new(prefix: Option<&str>) -> Self {
        let normalized = prefix.unwrap_or_default().trim().trim_matches(':');
        Self {
            namespace: CacheKeyNamespace::new(normalized),
        }
    }

    pub fn lock_key(&self, raw_key: &str) -> RedisLockKey {
        RedisLockKey(self.namespace.child("lock").key(raw_key))
    }

    pub fn stream_name(&self, raw_name: &str) -> RedisStreamName {
        RedisStreamName(self.namespace.child("stream").key(raw_name))
    }
}
```

DO NOT concatenate prefixes manually. The test in `src/redis/namespace.rs:35`
documents the expected forms: `aether:auth:user`, `aether:lock:poller`, and
`aether:stream:audit`.

## KV Patterns

Use `RedisKvRunner` for normal string KV operations. `setex` applies the default
TTL when no TTL is passed and validates zero defaults at runner construction.

```rust
// crates/aether-runtime-state/src/redis/kv.rs:74
pub async fn setex(
    &self,
    key: &str,
    value: &str,
    ttl_seconds: Option<u64>,
) -> Result<String, DataLayerError> {
    let resolved_ttl = ttl_seconds.unwrap_or(self.config.default_ttl_seconds);
    let namespaced_key = self.keyspace.key(key);
    self.run_with_timeout("redis kv setex", async {
        ...
        redis::cmd("SETEX")
            .arg(&namespaced_key)
            .arg(resolved_ttl)
            .arg(value)
            .query_async(&mut connection)
            .await
            .map_redis_err()
    })
    .await
}
```

Use facade methods such as `RuntimeState::kv_get_many` and
`RuntimeState::kv_delete_many` for bulk operations so empty input short-circuits
and namespacing stays consistent (`src/lib.rs:398` and `src/lib.rs:449`).

## Rate Limits

Rate-limit check and consume is atomic in Redis through one Lua script. It checks
user and key counters before incrementing either counter:

```rust
// crates/aether-runtime-state/src/lib.rs:30
const RATE_LIMIT_CHECK_AND_CONSUME_SCRIPT: &str = r#"
local user_key = KEYS[1]
local key_key = KEYS[2]
...
return {1, 0, 0, remaining}
"#;
```

Do not implement rate limiting with separate `GET`, `INCR`, and `EXPIRE` calls.
The memory backend mirrors the same semantics with a single mutex-protected
counter map in `src/memory.rs:192`.

## Locks

Use `RedisLockRunner` or `RuntimeState::lock_try_acquire` for locks. Acquired
locks include an owner and token; release and renew must compare tokens in Redis
before mutating the key.

```rust
// crates/aether-runtime-state/src/redis/lock.rs:121
pub async fn release(&self, lease: &RedisLockLease) -> Result<bool, DataLayerError> {
    validate_lease(lease)?;
    self.run_with_timeout("redis lock release", async {
        ...
        let deleted = redis::Script::new(
            "if redis.call('get', KEYS[1]) == ARGV[1] then \
                 return redis.call('del', KEYS[1]) \
             else \
                 return 0 \
             end",
        )
        .key(&lease.key.0)
        .arg(&lease.token)
        .invoke_async::<i32>(&mut connection)
        .await
        .map_redis_err()?;
        Ok(deleted > 0)
    })
    .await
}
```

DO NOT delete a lock by key alone. Token comparison is the ownership guard.

## Streams and Queues

Use `RedisStreamRunner` for raw Redis streams and `RuntimeQueueStore` for
runtime-neutral queue use. Consumer groups are created idempotently: BUSYGROUP is
treated as success.

```rust
// crates/aether-runtime-state/src/redis/stream.rs:147
pub async fn ensure_consumer_group(
    &self,
    stream: &RedisStreamName,
    group: &RedisConsumerGroup,
    start_id: &str,
) -> Result<(), DataLayerError> {
    ...
    match result {
        Ok(_) => Ok(()),
        Err(err) if err.code() == Some("BUSYGROUP") => Ok(()),
        Err(err) => Err(redis_error(err)),
    }
}
```

When using blocking reads through `RuntimeQueueStore::read_group`, the facade
expands command timeout beyond the requested block duration:

```rust
// crates/aether-runtime-state/src/lib.rs:1775
fn redis_stream_command_timeout_for_block(
    command_timeout_ms: Option<u64>,
    read_block_ms: Option<u64>,
) -> Option<u64> {
    match (command_timeout_ms, read_block_ms) {
        (Some(timeout_ms), Some(block_ms)) => {
            Some(timeout_ms.max(block_ms.saturating_add(DEFAULT_COMMAND_TIMEOUT_MS)))
        }
        ...
    }
}
```

## Semaphores

Distributed semaphores use Redis sorted sets. Each holder is a token member scored
by expiry time; acquire removes expired holders, checks `ZCARD`, and adds the new
token atomically.

```rust
// crates/aether-runtime-state/src/lib.rs:1495
async fn redis_try_acquire(
    &self,
    redis: &RedisRuntimeBackend,
    token: &str,
) -> Result<usize, RuntimeSemaphoreError> {
    let now_ms = unix_time_ms();
    let expires_at_ms = now_ms.saturating_add(self.config.lease_ttl_ms);
    let key = redis.keyspace.key(&self.key);
    ...
}
```

Keep semaphore key naming under `admission:{gate}` (`src/lib.rs:1383`) and let
`RedisKeyspace` add the configured prefix.

## No SQL Migrations

There are no migrations for this crate. Runtime Redis state is ephemeral and
namespaced by configuration. If a future change requires durable schema evolution,
it belongs in `aether-data` or another data crate, not in runtime-state.

## DO NOT Patterns

DO NOT use SeaORM, SQLx, or SQL migrations in this crate.

DO NOT manually build Redis keys with `format!("{prefix}:{key}")`.

DO NOT run unbounded Redis commands. Every command path should use the configured
timeout helper.

DO NOT store long-lived business records in runtime-state. Store only runtime
coordination data: counters, queues, leases, scores, and expiring KV.
