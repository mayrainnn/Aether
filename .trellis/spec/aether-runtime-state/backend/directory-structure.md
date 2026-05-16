# Directory Structure

Runtime state code in `aether-runtime-state` is organized as a small domain crate
with one facade module and narrowly scoped backend modules. Keep this crate
focused on ephemeral runtime coordination: expiring key-value state, rate-limit
counters, Redis-backed locks, stream queues, and distributed semaphores.

## Source Layout

Current source files:

```text
crates/aether-runtime-state/
|-- Cargo.toml
`-- src/
    |-- error.rs
    |-- lib.rs
    |-- memory.rs
    `-- redis/
        |-- client.rs
        |-- kv.rs
        |-- lock.rs
        |-- mod.rs
        |-- namespace.rs
        `-- stream.rs
```

ABCoder's `aether-runtime-state` AST reports one module named
`aether-runtime-state` with packages for `error`, `memory`, `redis`, and the four
Redis subpackages `client`, `kv`, `lock`, `namespace`, and `stream`.

## Public Facade

`src/lib.rs` is the public facade and the memory/Redis dispatcher. It declares
private modules and re-exports the public runtime-state API:

```rust
// crates/aether-runtime-state/src/lib.rs:1
mod error;
mod memory;
pub mod redis;

pub use crate::redis::{
    RedisClient, RedisClientConfig, RedisClientFactory, RedisConsumerGroup,
    RedisConsumerName, RedisKeyspace, RedisKvRunner, RedisKvRunnerConfig,
    RedisLockLease, RedisLockRunner, RedisLockRunnerConfig, RedisStreamEntry,
    RedisStreamName, RedisStreamReclaimConfig, RedisStreamRunner,
    RedisStreamRunnerConfig,
};
pub use error::DataLayerError;
pub use memory::MemoryRuntimeStateConfig;
```

Add new public runtime concepts to `lib.rs` only when they are part of the crate's
stable API. Keep backend helpers private or `pub(crate)` until another crate
actually needs them.

## Backend Dispatch Pattern

The central type is `RuntimeState`, a cheap clone around `Arc<RuntimeStateBackend>`.
The backend enum is private, so callers cannot depend on memory or Redis internals:

```rust
// crates/aether-runtime-state/src/lib.rs:205
#[derive(Debug, Clone)]
pub struct RuntimeState {
    backend: Arc<RuntimeStateBackend>,
}

#[derive(Debug)]
enum RuntimeStateBackend {
    Memory(MemoryRuntimeBackend),
    Redis(RedisRuntimeBackend),
}
```

New runtime operations should follow the existing shape: expose one method on
`RuntimeState`, match on `RuntimeStateBackend`, keep memory behavior in
`memory.rs`, and keep Redis command details in `redis/*` or a small helper in
`lib.rs` when the operation spans runners.

## Memory Backend

`src/memory.rs` is not a test-only mock. It is the in-process backend used when
`RuntimeStateBackendMode::Auto` has no Redis configuration and when tests need a
deterministic backend.

```rust
// crates/aether-runtime-state/src/memory.rs:35
pub(crate) struct MemoryRuntimeBackend {
    config: MemoryRuntimeStateConfig,
    kv: Mutex<HashMap<String, MemoryKvEntry>>,
    counters: Mutex<HashMap<String, MemoryCounterEntry>>,
    sets: Mutex<HashMap<String, BTreeSet<String>>>,
    scores: Mutex<HashMap<String, BTreeMap<String, f64>>>,
    queues: Mutex<HashMap<String, VecDeque<RuntimeQueueEntry>>>,
    queue_seq: AtomicU64,
    locks: Mutex<HashMap<String, MemoryLockEntry>>,
    semaphores: Mutex<HashMap<String, BTreeMap<String, u64>>>,
}
```

Keep memory-only helpers `pub(crate)`. Do not expose `MemoryRuntimeBackend`; the
public constructor is `RuntimeState::memory(...)`.

## Redis Modules

`src/redis/mod.rs` is a thin re-export surface:

```rust
// crates/aether-runtime-state/src/redis/mod.rs:1
mod client;
mod kv;
mod lock;
mod namespace;
mod stream;

pub use client::{RedisClient, RedisClientConfig, RedisClientFactory};
pub use kv::{RedisKvRunner, RedisKvRunnerConfig};
pub use lock::{RedisLockKey, RedisLockLease, RedisLockRunner, RedisLockRunnerConfig};
pub use namespace::RedisKeyspace;
```

Redis files are split by Redis data-structure responsibility:

- `client.rs`: validates `RedisClientConfig`, constructs lazy `redis::Client`.
- `namespace.rs`: composes raw keys, lock keys, and stream names through
  `aether_cache::CacheKeyNamespace`.
- `kv.rs`: string key-value commands and default TTL handling.
- `lock.rs`: single-key leases with token-checked Lua release and renew.
- `stream.rs`: consumer groups, stream append/read/ack/delete/reclaim parsing.

## Naming Rules

Use domain-first names for facade types: `RuntimeState`, `RuntimeLockLease`,
`RuntimeQueueEntry`, `RuntimeSemaphore`. Use Redis-prefixed names only for raw
Redis runner surfaces: `RedisStreamRunner`, `RedisLockRunner`, `RedisKeyspace`.

Configuration types end in `Config`, result snapshots end in `Snapshot`, and
small enum helpers expose `as_str()` when the string is used outside the type:

```rust
// crates/aether-runtime-state/src/lib.rs:190
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStateBackendKind {
    Memory,
    Redis,
}
```

## Placement Rules

- Put new facade methods and traits in `src/lib.rs` beside related operations.
- Put Redis command wrappers in the narrowest `redis/*` module.
- Put shared Redis command constructors in `src/redis/mod.rs` only if several
  modules need them.
- Put in-memory semantic equivalents in `src/memory.rs`, not inside tests.
- Put error conversion helpers in `src/error.rs`; do not duplicate
  `map_err(DataLayerError::redis)` at each call site.

## DO NOT Patterns

DO NOT add a second public backend enum or expose `RuntimeStateBackend`. It would
make callers branch around the facade and break memory/Redis parity.

DO NOT put SQL, HTTP handlers, provider selection, or billing logic in this crate.
GitNexus classifies Aether as a multi-layer gateway; this crate belongs to the
runtime/domain state layer, not the application or repository layers.

DO NOT add a new top-level file for one Redis command. Extend the relevant runner
module unless the new concern introduces a distinct Redis data structure.
