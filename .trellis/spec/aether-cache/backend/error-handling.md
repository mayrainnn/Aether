# Error Handling

> Error and absence semantics for the `aether-cache` crate.

---

## Overview

`aether-cache` currently defines no custom error enum, imports no `thiserror`
or `anyhow`, and exposes no `Result` returning public API. This is intentional:
the crate models cache absence, expiration, removal, and poisoned-lock fallback
through neutral return values.

Evidence:

```toml
# crates/aether-cache/Cargo.toml:9
[dependencies]
```

```rust
// crates/aether-cache/src/ttl_map.rs:97
pub fn get_fresh(&self, key: &K, ttl: Duration) -> Option<V> {
```

Do not add a crate-wide error type unless callers truly need to distinguish
cache infrastructure failures from ordinary cache misses.

## Public Error Surface

The public surface uses these non-throwing return shapes:

| API | Failure or absence shape | Meaning |
|-----|--------------------------|---------|
| `insert` | `()` | Lock poison or capacity pruning issue is not surfaced |
| `remove` | `Option<V>` | `None` means absent or lock unavailable |
| `len` | `usize` | `0` means empty or lock unavailable |
| `is_empty` | `bool` | Derived from `len()` |
| `clear` | `()` | Lock poison is ignored |
| `get_fresh` | `Option<V>` | `None` means absent, expired, or lock unavailable |
| `contains_fresh` | `bool` | Derived from `get_fresh` |
| `snapshot_fresh` | `Vec<ExpiringMapFreshEntry<K, V>>` | Empty vector means no fresh entries or lock unavailable |

This is cache behavior, not durable state behavior. Callers must be able to
continue when a cache lookup misses.

## Lock Poison Handling

The crate deliberately handles `std::sync::Mutex::lock()` with `let Ok(...)`
instead of `unwrap()` or `expect()`.

```rust
// crates/aether-cache/src/ttl_map.rs:40
pub fn insert(&self, key: K, value: V, ttl: Duration, max_entries: usize) {
    let Ok(mut entries) = self.entries.lock() else {
        return;
    };
```

```rust
// crates/aether-cache/src/ttl_map.rs:66
pub fn remove(&self, key: &K) -> Option<V> {
    let Ok(mut entries) = self.entries.lock() else {
        return None;
    };
    entries.remove(key).map(|entry| entry.value)
}
```

```rust
// crates/aether-cache/src/ttl_map.rs:73
pub fn len(&self) -> usize {
    self.entries
        .lock()
        .map(|entries| entries.len())
        .unwrap_or(0)
}
```

If lock-poison behavior changes, update all public methods together and add a
test that makes the new behavior explicit. Do not leave a mixed policy where
some methods panic and others return neutral values.

## Expiration Is Absence

Expired entries are removed and surfaced as misses:

```rust
// crates/aether-cache/src/ttl_map.rs:102
let entry = entries.get(key).cloned()?;

// crates/aether-cache/src/ttl_map.rs:104
if entry.inserted_at.elapsed() > ttl {
    entries.remove(key);
    return None;
}
```

The unit test locks this behavior:

```rust
// crates/aether-cache/src/ttl_map.rs:150
#[test]
fn evicts_expired_entries_on_read() {
    let cache = ExpiringMap::new();
```

Do not turn expiration into an error. Callers such as auth-context and
dashboard-response caches already treat `None` as "compute or fetch again."

## Namespace Methods Are Infallible

`CacheKeyNamespace` methods return `Self`, `String`, or `&str`. They do not
validate prefix contents or return errors:

```rust
// crates/aether-cache/src/namespace.rs:23
pub fn key(&self, raw_key: &str) -> String {
    if self.prefix.is_empty() {
        return raw_key.to_string();
    }
    if raw_key.is_empty() {
        return self.prefix.clone();
    }
    format!("{}:{}", self.prefix, raw_key)
}
```

Normalization belongs to callers that know their domain. For example,
`RedisKeyspace` trims optional Redis prefixes before creating a namespace:

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:11
pub fn new(prefix: Option<&str>) -> Self {
    let normalized = prefix.unwrap_or_default().trim().trim_matches(':');
    Self {
        namespace: CacheKeyNamespace::new(normalized),
    }
}
```

Do not add Redis-specific validation to `CacheKeyNamespace`.

## Caller Error Boundaries

Higher layers convert their own durable-state errors to their own error types.
`aether-cache` does not participate in those conversions:

```rust
// apps/aether-gateway/src/rate_limit.rs:149
let raw = state.runtime_state.kv_get(scope_key).await.map_err(|err| {
    GatewayError::Internal(format!("frontdoor user rpm runtime read failed: {err}"))
})?;
```

The same caller separately uses `ExpiringMap` as a local cache for system
configuration:

```rust
// apps/aether-gateway/src/rate_limit.rs:229
let cache_key = SYSTEM_RPM_CONFIG_KEY.to_string();
if let Some(limit) = self
    .system_default_cache
    .get_fresh(&cache_key, SYSTEM_RPM_CONFIG_CACHE_TTL)
{
    return Ok(limit);
}
```

Keep this separation: durable-state failures surface through caller errors;
cache misses remain `Option`.

## DON'T Patterns

Do not panic on lock poison:

```rust
// DON'T: this breaks the crate's neutral fallback contract.
let mut entries = self.entries.lock().unwrap();
```

Do not add generic error variants for ordinary misses:

```rust
// DON'T: expiration is not an exceptional condition in this crate.
pub fn get_fresh(&self, key: &K, ttl: Duration) -> Result<V, CacheError>
```

Do not log from inside the primitive to explain a miss:

```rust
// DON'T: callers own observability and key sensitivity decisions.
warn!(?key, "cache miss");
```

## Review Checklist

When reviewing error-handling changes, check:

- All lock acquisition paths use one consistent policy.
- `Option` is still used for absent, expired, and removed values.
- Public API changes do not force all callers to handle a new error enum.
- Namespace validation remains caller-owned.
- Tests cover any changed semantics around stale entries, capacity, or lock
  fallback.
