# Logging Guidelines

> Observability rules for the `aether-cache` crate.

---

## Overview

`aether-cache` currently performs no logging and has no logging dependency.
This is intentional for a foundation cache primitive: callers know whether a
miss, expiration, or fallback is worth logging, and callers know whether a key
or value contains sensitive data.

Evidence:

```toml
# crates/aether-cache/Cargo.toml:9
[dependencies]
```

```rust
// crates/aether-cache/src/ttl_map.rs:1
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};
```

There is no `tracing`, `log`, `metrics`, or `opentelemetry` import in
`crates/aether-cache/`.

## Default Rule: Do Not Log Inside The Primitive

The cache API intentionally returns neutral values for misses, stale entries,
and lock-poison fallback:

```rust
// crates/aether-cache/src/ttl_map.rs:97
pub fn get_fresh(&self, key: &K, ttl: Duration) -> Option<V> {
    let Ok(mut entries) = self.entries.lock() else {
        return None;
    };
```

A `None` result does not have enough context to decide a log level. It may mean:

- The entry was never inserted.
- The entry expired and was pruned.
- The mutex was poisoned.
- The caller intentionally allows a cold cache path.

Keep that ambiguity out of foundation-layer logs.

## Caller-Side Logging Boundary

Higher layers log durable-state failures and fallback decisions where they have
domain context. Example from the gateway rate limiter:

```rust
// apps/aether-gateway/src/rate_limit.rs:186
match self.check_and_consume_runtime(state, &plan).await {
    Ok(outcome) => return Ok(outcome),
    Err(err) => {
        warn!(
            error = ?err,
            user_rpm_key = %plan.user_rpm_key,
            key_rpm_key = %plan.key_rpm_key,
            "frontdoor user rpm redis check failed"
        );
```

That warning belongs in `aether-gateway` because the gateway knows the runtime
backend, failure mode, configured fallback policy, and operational impact.
`ExpiringMap` only knows that an in-memory lookup missed or an insert happened.

## Log Levels If Logging Is Ever Introduced

Do not introduce logging for ordinary cache operations. If a future task
explicitly adds observability, use these rules:

- `trace`: only for opt-in local debugging of non-sensitive aggregate state.
- `debug`: rare, bounded diagnostic logs during tests or startup diagnostics.
- `info`: not appropriate for per-key cache hits, misses, inserts, removals,
  or snapshot reads.
- `warn`: only for unexpected infrastructure degradation if the crate exposes
  a way to distinguish it without leaking key/value data.
- `error`: not appropriate inside this crate today because the API does not
  surface durable failures.

Any such change requires an explicit dependency decision and tests or caller
verification. Do not sneak in `tracing` as a convenience dependency.

## Structured Fields

The current crate has no structured fields. If a future design adds caller
provided instrumentation hooks, do not include raw keys or values by default.
Prefer aggregate fields:

```text
cache_name      // caller-provided logical cache name
operation       // insert | remove | snapshot | prune
entry_count     // count only, not key list
expired_count   // count only
capacity_limit  // numeric limit
```

Avoid fields like:

```text
raw_key
value
prefix
namespace
auth_context
token
api_key
redis_key
```

`CacheKeyNamespace` is used to form Redis locks and stream names in
`aether-runtime-state`; those names can include operational identifiers:

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:22
pub fn lock_key(&self, raw_key: &str) -> RedisLockKey {
    RedisLockKey(self.namespace.child("lock").key(raw_key))
}
```

Do not log such raw keys from this crate.

## What To Log In Callers

Caller wrappers may log:

- Durable backend failures before falling back to local cache behavior.
- Explicit configuration fallback decisions.
- Startup diagnostics that report cache settings without keys or values.
- Aggregate counters emitted by a caller-owned metrics layer.

Example caller-owned cache wrapper:

```rust
// apps/aether-gateway/src/cache/scheduler_affinity.rs:42
pub(crate) fn fresh_entries(&self, ttl: Duration) -> Vec<SchedulerAffinitySnapshotEntry> {
    self.entries
        .snapshot_fresh(ttl)
        .into_iter()
        .map(
            |ExpiringMapFreshEntry { key, value, age }| SchedulerAffinitySnapshotEntry {
```

If gateway code wants to log scheduler-affinity snapshot sizes, it should do so
around `fresh_entries`, after deciding whether the cache key is safe to expose.

## What Not To Log

Never log from `aether-cache`:

- Raw cache keys.
- Values stored in `ExpiringMap`.
- Namespace prefixes.
- Redis lock or stream names.
- Auth contexts, API keys, tokens, wallet identifiers, model routing keys, or
  billing identifiers.
- Per-request hit/miss noise.
- Every expiration prune.

The primitive cannot classify this data. Treat all keys and values as
potentially sensitive.

## DON'T Patterns

Do not add per-operation logging:

```rust
// DON'T: key/value safety is caller-specific.
debug!(?key, "cache hit");
```

Do not warn on ordinary misses:

```rust
// DON'T: a miss is often the expected cold-cache path.
warn!(?key, "cache miss");
```

Do not log lock poison with raw state:

```rust
// DON'T: this adds a logging dependency and may expose key/value debug output.
error!(entries = ?self.entries, "cache lock poisoned");
```

## Review Checklist

When reviewing observability changes, check:

- `Cargo.toml` still has no logging dependency unless the task explicitly
  approved one.
- New code does not log keys, values, prefixes, or namespace strings.
- Cache hit/miss/expiration remains quiet by default.
- Durable failure logs remain in the caller layer.
- Any caller logs use structured fields and avoid high-cardinality values.
