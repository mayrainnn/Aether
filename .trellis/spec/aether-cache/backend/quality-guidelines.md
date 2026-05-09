# Quality Guidelines

> Code quality standards for the `aether-cache` crate.

---

## Overview

`aether-cache` should stay small, dependency-free, synchronous, and generic.
Its quality bar is not complex architecture; it is predictable semantics that
higher layers can rely on without pulling policy, async runtime, logging, or
database concerns into a foundation crate.

The current public surface:

```rust
// crates/aether-cache/src/lib.rs:4
pub use namespace::CacheKeyNamespace;
pub use ttl_map::{ExpiringMap, ExpiringMapFreshEntry};
```

The current implementation is entirely `std`:

```rust
// crates/aether-cache/src/ttl_map.rs:1
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};
```

## Dependency Policy

Do not add dependencies for behavior already handled by `std`. The crate
currently has no dependency entries:

```toml
# crates/aether-cache/Cargo.toml:9
[dependencies]
```

Allowed without discussion:

- Standard library collections, hashes, synchronization, and time types.
- Module-local tests using `std::thread::sleep` for tiny TTL windows.

Requires explicit design review:

- `tokio` or any async runtime dependency.
- `tracing`, `metrics`, or any observability dependency.
- `serde` serialization for cache internals.
- `dashmap`, `moka`, Redis clients, SeaORM, or database crates.

## Visibility Rules

Expose only the reusable API from `lib.rs`. Keep implementation modules
private:

```rust
// crates/aether-cache/src/lib.rs:1
mod namespace;
mod ttl_map;
```

Keep storage internals private:

```rust
// crates/aether-cache/src/ttl_map.rs:6
struct TimedEntry<V> {
    value: V,
    inserted_at: Instant,
}

// crates/aether-cache/src/ttl_map.rs:13
pub struct ExpiringMap<K, V> {
    entries: Mutex<HashMap<K, TimedEntry<V>>>,
}
```

Expose snapshot data only through the intentionally public DTO:

```rust
// crates/aether-cache/src/ttl_map.rs:17
pub struct ExpiringMapFreshEntry<K, V> {
    pub key: K,
    pub value: V,
    pub age: Duration,
}
```

Do not make `TimedEntry` public. Its `inserted_at` field is an implementation
detail; callers should only see `age` in a freshness-filtered snapshot.

## Generic Bound Rules

Keep trait bounds as narrow as the method needs. Write methods in separate impl
blocks when the value type only needs `Clone` for reads:

```rust
// crates/aether-cache/src/ttl_map.rs:32
impl<K, V> ExpiringMap<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn insert(&self, key: K, value: V, ttl: Duration, max_entries: usize) {
```

```rust
// crates/aether-cache/src/ttl_map.rs:92
impl<K, V> ExpiringMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn get_fresh(&self, key: &K, ttl: Duration) -> Option<V> {
```

Do not put `V: Clone` on the whole type or on write-only operations. This crate
must support storing non-clone values when callers only insert, remove, clear,
or inspect length.

## Locking Rules

Use `std::sync::Mutex` because the primitive is synchronous and short-lived.
Do not add async locks to this crate:

```rust
// crates/aether-cache/src/ttl_map.rs:14
entries: Mutex<HashMap<K, TimedEntry<V>>>,
```

Never return references into the map. Clone while the lock is held, then return
owned values:

```rust
// crates/aether-cache/src/ttl_map.rs:102
let entry = entries.get(key).cloned()?;

// crates/aether-cache/src/ttl_map.rs:109
Some(entry.value)
```

This avoids leaking mutex guard lifetimes into callers and avoids holding the
lock while higher-layer code performs I/O or logging.

## TTL And Capacity Semantics

Prune expired entries before inserting and snapshotting:

```rust
// crates/aether-cache/src/ttl_map.rs:45
prune_expired(&mut entries, ttl);
```

Evict oldest entries when the capacity cap is hit:

```rust
// crates/aether-cache/src/ttl_map.rs:46
while max_entries > 0 && entries.len() >= max_entries {
    let Some(oldest_key) = entries
        .iter()
        .min_by_key(|(_, entry)| entry.inserted_at)
        .map(|(key, _)| key.clone())
    else {
        break;
    };
    entries.remove(&oldest_key);
}
```

`max_entries == 0` currently disables capacity eviction because the loop is
guarded by `max_entries > 0`. Preserve this behavior unless a task explicitly
changes the contract and updates callers.

`ttl.is_zero()` clears existing entries in the prune helper:

```rust
// crates/aether-cache/src/ttl_map.rs:137
if ttl.is_zero() {
    entries.clear();
    return;
}
```

If a task changes zero-TTL behavior, add a focused unit test because the current
tests cover expiration and capacity but not zero-TTL insertion.

## Namespace Quality Rules

`CacheKeyNamespace` must keep empty-prefix and empty-suffix behavior stable:

```rust
// crates/aether-cache/src/namespace.rs:13
pub fn child(&self, suffix: &str) -> Self {
    if self.prefix.is_empty() {
        return Self::new(suffix);
    }
    if suffix.is_empty() {
        return self.clone();
    }
    Self::new(format!("{}:{}", self.prefix, suffix))
}
```

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

Do not trim or validate in `CacheKeyNamespace::new`. The current Redis consumer
normalizes the optional prefix before constructing the namespace:

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:12
let normalized = prefix.unwrap_or_default().trim().trim_matches(':');
```

## Testing Requirements

Minimum command:

```bash
cargo test -p aether-cache
```

Current tests prove:

- `CacheKeyNamespace` composes root and child keys.
- `ExpiringMap` evicts expired entries on read.
- `ExpiringMap` evicts the oldest entry when capacity is hit.

Examples:

```rust
// crates/aether-cache/src/namespace.rs:42
#[test]
fn composes_scoped_keys() {
    let root = CacheKeyNamespace::new("aether");
    let child = root.child("auth");
```

```rust
// crates/aether-cache/src/ttl_map.rs:169
#[test]
fn evicts_oldest_entry_when_capacity_is_hit() {
    let cache = ExpiringMap::new();
```

Add tests for every semantic change. In particular, add tests for:

- `remove` returning owned values.
- `snapshot_fresh` ages and pruning.
- `clear` behavior.
- zero TTL behavior if touched.
- `max_entries == 0` if touched.

## DON'T Patterns

Do not introduce caller policy into the primitive:

```rust
// DON'T: gateway auth context belongs in apps/aether-gateway.
pub struct AuthContextCache {
    entries: ExpiringMap<String, GatewayControlAuthContext>,
}
```

Do not expose internals:

```rust
// DON'T: callers must not depend on inserted_at or mutex storage.
pub struct TimedEntry<V> {
    pub inserted_at: Instant,
    pub value: V,
}
```

Do not make cache reads borrow from the map:

```rust
// DON'T: this would expose lock lifetime and invite guard leaks.
pub fn get_fresh(&self, key: &K, ttl: Duration) -> Option<&V>
```

Do not replace explicit freshness names with ambiguous names:

```rust
// DON'T: this hides whether TTL was checked.
pub fn get(&self, key: &K) -> Option<V>
```

## Code Review Checklist

Reviewers should verify:

- New code preserves the leaf-crate boundary.
- Dependencies remain empty or are explicitly justified.
- Public exports are intentional and re-exported from `lib.rs`.
- Lock handling does not panic.
- Trait bounds are method-specific.
- New code does not require async.
- TTL and capacity behavior has tests.
- Namespace formatting remains colon-separated and empty-part safe.
