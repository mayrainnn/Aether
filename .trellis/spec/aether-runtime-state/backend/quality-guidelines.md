# Quality Guidelines

This crate is a runtime coordination boundary. Code quality depends on preserving
memory/Redis semantic parity, typed public contracts, bounded async operations,
and deterministic behavior for tests and local deployments.

## Type Safety

Prefer small domain types over untagged strings when a value crosses an API
boundary. Redis stream and lock wrappers make call sites harder to mix up:

```rust
// crates/aether-runtime-state/src/redis/stream.rs:13
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RedisStreamName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RedisConsumerGroup(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RedisConsumerName(pub String);
```

Runtime-facing types remove Redis prefixes from consumers:

```rust
// crates/aether-runtime-state/src/lib.rs:978
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLockLease {
    pub key: String,
    pub owner: String,
    pub token: String,
    pub ttl_ms: u64,
}
```

When adding APIs, expose Redis wrapper types only on Redis-specific runners.
Expose runtime-neutral types from `RuntimeState`.

## Visibility

Use `pub` only for stable crate API, `pub(crate)` for intra-crate helpers, and
private items for backend details.

```rust
// crates/aether-runtime-state/src/memory.rs:22
#[derive(Debug, Clone)]
pub(crate) struct MemoryKvEntry {
    pub(crate) value: String,
    pub(crate) inserted_at: Instant,
    pub(crate) expires_at: Option<Instant>,
}
```

`RuntimeStateBackend` and `RedisRuntimeBackend` are private by design
(`src/lib.rs:210` and `src/lib.rs:216`). Do not expose them to make one call site
easier; add a facade method instead.

## Memory and Redis Parity

Every high-level `RuntimeState` operation should implement both backends unless
there is an explicitly documented difference. The facade method should be the
single semantic contract:

```rust
// crates/aether-runtime-state/src/lib.rs:391
pub async fn kv_get(&self, key: &str) -> Result<Option<String>, DataLayerError> {
    match self.backend.as_ref() {
        RuntimeStateBackend::Memory(memory) => Ok(memory.kv_get(key).await),
        RuntimeStateBackend::Redis(redis) => redis.kv.get(key).await,
    }
}
```

If Redis has a richer feature than memory, document the difference and keep the
memory fallback conservative. Existing examples are memory queue reclaim and
queue delete, which are no-ops in `src/memory.rs:397` and `src/memory.rs:405`.

## Time and Expiration

Clamp externally supplied TTLs where a zero value would create surprising Redis
behavior, and reject zero values in configuration validation.

```rust
// crates/aether-runtime-state/src/lib.rs:560
pub async fn check_and_consume_rate_limit(
    &self,
    input: RateLimitInput<'_>,
) -> Result<RateLimitCheck, DataLayerError> {
    ...
    Duration::from_secs(input.ttl_seconds.max(1)),
    ...
}
```

Use `Instant` for in-memory expiration and `SystemTime` or Unix milliseconds only
when Redis scores or external time formats require epoch values.

## Atomicity

Use Redis Lua scripts for check-and-update operations that must be atomic across
multiple commands. Examples:

- Rate-limit check and consume uses `RATE_LIMIT_CHECK_AND_CONSUME_SCRIPT` in
  `src/lib.rs:30`.
- Lock release and renew compare token values before deleting or extending
  leases in `src/redis/lock.rs:129` and `src/redis/lock.rs:160`.
- Distributed semaphore acquire, renew, release, and snapshot use sorted-set
  scripts in `src/lib.rs:1510`, `src/lib.rs:1569`, `src/lib.rs:1609`, and
  `src/lib.rs:1640`.

DO NOT split these into separate `GET` plus `DEL` or `ZCARD` plus `ZADD` command
sequences; concurrent workers would observe inconsistent state.

## Async Concurrency

The in-memory backend uses `tokio::sync::Mutex` for maps and atomic counters for
sequence or metric fields. Keep lock scopes short and avoid awaiting while
holding a map entry that does not need the await.

```rust
// crates/aether-runtime-state/src/memory.rs:99
pub(crate) fn kv_set_nowait(&self, key: &str, value: String, ttl: Option<Duration>) -> bool {
    let Ok(mut kv) = self.kv.try_lock() else {
        return false;
    };
    ...
}
```

`*_local_nowait` methods must stay best-effort and memory-only. For Redis they
return `false` (`src/lib.rs:316` and `src/lib.rs:323`) because a network call
cannot be made synchronously without lying about completion.

## Tests

Prefer focused unit tests inside the module that owns the behavior. Existing
coverage includes:

- Memory KV expiration, one-shot take, rate-limit rejection, and semaphore permit
  drop in `src/lib.rs:1792` through `src/lib.rs:1862`.
- Redis config and keyspace construction tests in `src/redis/client.rs:52` and
  `src/redis/namespace.rs:31`.
- Lock input validation before network calls in `src/redis/lock.rs:293`.
- Stream config, invalid-input, and reclaim parser tests in
  `src/redis/stream.rs:599` through `src/redis/stream.rs:747`.

New Redis parser behavior should be unit-tested without requiring a running Redis
server. Network baseline tests belong in testkit or bin baselines, not in this
crate's ordinary unit tests.

## Review Checklist

- Does the public method keep memory and Redis semantics aligned?
- Are all user inputs validated before network work?
- Does every Redis network operation have a timeout path?
- Are namespaced keys composed through `RedisKeyspace`?
- Are atomic multi-step Redis updates implemented as Lua scripts?
- Are errors typed as `DataLayerError` or `RuntimeSemaphoreError`?
- Is a focused unit test added for parser, validation, or memory parity logic?

## DO NOT Patterns

DO NOT add new dependencies for convenience. This crate already has the pieces it
uses: `redis`, `tokio`, `async-trait`, `serde`, `serde_json`, `thiserror`,
`tracing`, `uuid`, `url`, and Aether internal crates.

DO NOT bypass the facade by exposing `redis::Client` as the main runtime-state
API. Redis runners expose client references for low-level baselines, but service
code should depend on `RuntimeState`, `ExpiringKvStore`, or `RuntimeQueueStore`.

DO NOT use `unwrap()` in runtime paths. The current runtime code uses fallible
conversion with defaults such as `try_from(...).unwrap_or(0)` only after Redis
has returned numeric values that need saturation.
