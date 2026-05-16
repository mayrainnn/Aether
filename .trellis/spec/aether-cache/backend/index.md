# Backend Development Guidelines

> Entry point for backend work in the `aether-cache` crate.

---

## Package Summary

`aether-cache` is the Aether foundation crate for small, synchronous,
TTL-based in-memory cache primitives. It is a leaf utility crate: no internal
Aether dependencies, no async runtime dependency, no database dependency, and
no logging dependency.

Evidence:

```toml
# crates/aether-cache/Cargo.toml:1
[package]
name = "aether-cache"
description = "Shared in-memory cache primitives for Aether Rust services"

# crates/aether-cache/Cargo.toml:9
[dependencies]
```

The public API is intentionally narrow and exported only through `src/lib.rs`:

```rust
// crates/aether-cache/src/lib.rs:1
mod namespace;
mod ttl_map;

// crates/aether-cache/src/lib.rs:4
pub use namespace::CacheKeyNamespace;
pub use ttl_map::{ExpiringMap, ExpiringMapFreshEntry};
```

ABCoder parsed one module, three packages, and three source files for
`repo_name="aether-cache"`:

```text
crates/aether-cache/
|-- Cargo.toml
`-- src/
    |-- lib.rs
    |-- namespace.rs
    `-- ttl_map.rs
```

GitNexus repo context confirmed the broader `Aether` index is available
(`repo="Aether"`) with 3,140 files, 83,229 symbols, and 300 execution flows.
This crate stays in the foundation layer and is consumed by higher layers such
as `apps/aether-gateway` and `crates/aether-runtime-state`.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Crate layout, facade exports, module ownership, and expansion rules | Filled |
| [Error Handling](./error-handling.md) | Infallible API shape, `Option` semantics, lock-poison fallback behavior, and caller boundaries | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Generic bounds, visibility, TTL/capacity semantics, dependency policy, and tests | Filled |
| [Logging Guidelines](./logging-guidelines.md) | No-logging stance for cache primitives and caller-side observability rules | Filled |

`database-guidelines.md` was removed because this crate has no database, ORM,
Redis, migration, transaction, query, or connection-handling code. Redis naming
helpers in `aether-runtime-state` consume `CacheKeyNamespace`; the Redis
connection and command behavior does not belong in this crate's spec.

## Pre-Development Checklist

Before editing `crates/aether-cache/`, verify:

- The change is a reusable cache or key-namespace primitive, not a caller
  policy.
- The crate remains dependency-free unless a maintainer explicitly approves a
  new dependency.
- The public surface still goes through `src/lib.rs`.
- `ExpiringMap` remains synchronous and usable from sync wrappers.
- Expiration behavior is still driven by caller-provided `Duration` values.
- Capacity behavior is still caller-provided via `max_entries`.
- Lock-poison handling remains explicit and tested if changed.
- Namespace behavior preserves empty-prefix and empty-suffix semantics.
- Callers that rely on `Option` cache misses do not need new error handling.

## Public Contract

`ExpiringMap<K, V>` owns an internal `std::sync::Mutex<HashMap<K,
TimedEntry<V>>>` and exposes methods that accept `&self`, so wrappers can store
it inside structs without requiring mutable access:

```rust
// crates/aether-cache/src/ttl_map.rs:12
#[derive(Debug)]
pub struct ExpiringMap<K, V> {
    entries: Mutex<HashMap<K, TimedEntry<V>>>,
}
```

The read APIs clone values instead of returning borrowed references. This keeps
the mutex guard internal and prevents callers from holding the lock across
their own work:

```rust
// crates/aether-cache/src/ttl_map.rs:97
pub fn get_fresh(&self, key: &K, ttl: Duration) -> Option<V>
where
    V: Clone
```

`CacheKeyNamespace` is a small owned-prefix helper for colon-separated keys:

```rust
// crates/aether-cache/src/namespace.rs:1
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKeyNamespace {
    prefix: String,
}
```

## Known Consumers

Use callers to understand intended usage, but keep caller policy out of this
crate.

```rust
// apps/aether-gateway/src/cache/auth_context.rs:8
pub(crate) struct AuthContextCache {
    entries: ExpiringMap<String, GatewayControlAuthContext>,
}

// apps/aether-gateway/src/cache/auth_context.rs:13
pub(crate) fn get_fresh(
    &self,
    cache_key: &str,
    ttl: Duration,
) -> Option<GatewayControlAuthContext> {
    self.entries.get_fresh(&cache_key.to_string(), ttl)
}
```

```rust
// crates/aether-runtime-state/src/redis/namespace.rs:11
pub fn new(prefix: Option<&str>) -> Self {
    let normalized = prefix.unwrap_or_default().trim().trim_matches(':');
    Self {
        namespace: CacheKeyNamespace::new(normalized),
    }
}
```

## Quality Gate

Minimum verification for this crate:

```bash
cargo test -p aether-cache
```

When changing public method signatures or semantics, also compile or test the
known direct consumers:

- `apps/aether-gateway/src/cache/*`
- `apps/aether-gateway/src/rate_limit.rs`
- `crates/aether-runtime-state/src/redis/namespace.rs`

Run a template-residue scan before completion. The result should be empty for
placeholder phrases and HTML comment markers.

## Review Focus

Reviewers should focus on:

- Expiration pruning on insert, read, and snapshot.
- Capacity eviction choosing the oldest inserted entry.
- Whether new APIs leak mutex guards or borrowed references.
- Whether `V: Clone` is only required on read/snapshot APIs.
- Whether namespace formatting still avoids double colons for empty parts.
- Whether new behavior is locked by unit tests in the owning module.
- Whether callers remain responsible for logging, metrics, and fallback policy.

## Non-Goals

This spec intentionally does not cover:

- Database schema, SeaORM repositories, migrations, or transactions.
- Redis command execution or connection pooling.
- HTTP caching, cache-control headers, or response middleware.
- Distributed cache invalidation.
- Async task scheduling.
- Provider selection, billing, usage aggregation, or admin API behavior.
