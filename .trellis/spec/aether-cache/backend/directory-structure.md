# Directory Structure

> Backend organization rules for the `aether-cache` crate.

---

## Scope

`aether-cache` lives at `crates/aether-cache/` and is a foundation-layer Rust
crate. It provides reusable in-memory cache primitives and key namespace
helpers for higher layers. It must not import application, gateway, data,
runtime-state, provider, billing, or admin crates.

Evidence:

```toml
# crates/aether-cache/Cargo.toml:1
[package]
name = "aether-cache"

# crates/aether-cache/Cargo.toml:9
[dependencies]
```

The empty dependency table is part of the current design. Add dependencies only
when the cache primitive itself cannot reasonably stay in `std`.

## Actual Layout

```text
crates/aether-cache/
|-- Cargo.toml
`-- src/
    |-- lib.rs
    |-- namespace.rs
    `-- ttl_map.rs
```

ABCoder `get_repo_structure(repo_name="aether-cache")` reported these package
paths:

```text
aether-cache                -> src/lib.rs
aether-cache::namespace     -> src/namespace.rs
aether-cache::ttl_map       -> src/ttl_map.rs
```

Keep new modules similarly small and single-purpose. A new module should
represent a reusable primitive, not a caller-specific policy cache.

## Facade Module

`src/lib.rs` is only the public facade. It declares private implementation
modules and re-exports the stable API:

```rust
// crates/aether-cache/src/lib.rs:1
mod namespace;
mod ttl_map;

// crates/aether-cache/src/lib.rs:4
pub use namespace::CacheKeyNamespace;
pub use ttl_map::{ExpiringMap, ExpiringMapFreshEntry};
```

Do not make callers import `aether_cache::ttl_map::ExpiringMap` or
`aether_cache::namespace::CacheKeyNamespace`. The modules are private so the
crate can reorganize internals without changing higher-layer imports.

## `ttl_map.rs`

`src/ttl_map.rs` owns the generic TTL map implementation. ABCoder
`get_file_structure` found these exported nodes:

- `ExpiringMap<K, V>` at `src/ttl_map.rs:12`
- `ExpiringMapFreshEntry<K, V>` at `src/ttl_map.rs:17`
- `ExpiringMap.insert` at `src/ttl_map.rs:40`
- `ExpiringMap.remove` at `src/ttl_map.rs:66`
- `ExpiringMap.len` at `src/ttl_map.rs:73`
- `ExpiringMap.is_empty` at `src/ttl_map.rs:80`
- `ExpiringMap.clear` at `src/ttl_map.rs:84`
- `ExpiringMap.get_fresh` at `src/ttl_map.rs:97`
- `ExpiringMap.contains_fresh` at `src/ttl_map.rs:112`
- `ExpiringMap.snapshot_fresh` at `src/ttl_map.rs:116`

The private storage type stays in this module:

```rust
// crates/aether-cache/src/ttl_map.rs:6
#[derive(Debug, Clone)]
struct TimedEntry<V> {
    value: V,
    inserted_at: Instant,
}
```

Keep entry metadata private. Callers receive either values, booleans, removed
values, lengths, or `ExpiringMapFreshEntry` snapshots.

## `namespace.rs`

`src/namespace.rs` owns colon-separated cache namespace composition. It uses an
owned `String` prefix and deliberately exposes only methods, not the field:

```rust
// crates/aether-cache/src/namespace.rs:1
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKeyNamespace {
    prefix: String,
}
```

ABCoder `get_file_structure` found four methods:

- `CacheKeyNamespace::new` at `src/namespace.rs:7`
- `CacheKeyNamespace.child` at `src/namespace.rs:13`
- `CacheKeyNamespace.key` at `src/namespace.rs:23`
- `CacheKeyNamespace.prefix` at `src/namespace.rs:33`

Keep new namespace behavior in this file. Redis-specific types, lock keys, and
stream names stay in `aether-runtime-state`, as shown by the current consumer:

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:22
pub fn lock_key(&self, raw_key: &str) -> RedisLockKey {
    RedisLockKey(self.namespace.child("lock").key(raw_key))
}
```

## Module Expansion Rules

Add a new source file only when the new concept is reusable across callers and
does not fit `ttl_map.rs` or `namespace.rs`.

Good candidates:

- A different generic in-memory primitive with no caller policy.
- A shared key composition helper that is not Redis-specific.
- A test-only support module if repeated tests become hard to read.

Bad candidates:

- `gateway_cache.rs` for gateway-only auth context behavior.
- `redis_cache.rs` with connection or command logic.
- `database_cache.rs` with SeaORM models or migrations.
- `metrics.rs` that introduces a global observability dependency.

## Naming Conventions

Use names that describe the primitive, not the caller:

```rust
// crates/aether-cache/src/ttl_map.rs:13
pub struct ExpiringMap<K, V> {
    entries: Mutex<HashMap<K, TimedEntry<V>>>,
}
```

Use `Fresh` in APIs that enforce TTL freshness:

```rust
// crates/aether-cache/src/ttl_map.rs:112
pub fn contains_fresh(&self, key: &K, ttl: Duration) -> bool {
    self.get_fresh(key, ttl).is_some()
}
```

Do not add ambiguous methods such as `get`, `contains`, or `snapshot` unless
they either do not care about TTL or the name explicitly says how freshness is
checked.

## Tests Stay Beside The Module

Unit tests live in the module that owns the behavior:

```rust
// crates/aether-cache/src/namespace.rs:42
#[test]
fn composes_scoped_keys() {
    let root = CacheKeyNamespace::new("aether");
    let child = root.child("auth");
```

```rust
// crates/aether-cache/src/ttl_map.rs:150
#[test]
fn evicts_expired_entries_on_read() {
    let cache = ExpiringMap::new();
```

When adding behavior, add the test in the same file unless it is an integration
contract with another crate.
