# Backend Development Guidelines

These guidelines cover `crates/aether-runtime-state`, the Aether domain crate
that provides memory and Redis-backed runtime state for coordination,
admission-control, rate-limit, KV, lock, and stream queue use cases.

## Evidence Used

- GitNexus repo context for `repo="Aether"` reports 3,140 files, 83,229 symbols,
  and 300 execution flows. The high-level clusters include `Runtime`,
  `Execution_runtime`, `Provider`, `Usage`, and `Backend`; `aether-runtime-state`
  sits in the runtime/domain state layer rather than the SQL repository layer.
- GitNexus `clusters` and `processes` resources were readable. Direct
  `query`/`cypher` tool calls returned a cancelled MCP response in this session,
  so the final examples are grounded in GitNexus resources, ABCoder AST output,
  and direct source reads.
- ABCoder was run against the isolated `aether-runtime-state` AST with
  `repo_name="aether-runtime-state"`. It reported packages for `error`,
  `memory`, `redis`, and Redis subpackages `client`, `kv`, `lock`,
  `namespace`, and `stream`.
- Direct source examples were taken from `crates/aether-runtime-state/src/*` and
  selected caller/baseline files such as `crates/aether-testkit/src/tunnel.rs`.

## Pre-Development Checklist

Before changing this crate:

- Read [Directory Structure](./directory-structure.md) to confirm whether the
  change belongs in the facade, memory backend, or a Redis runner module.
- Read [Database Guidelines](./database-guidelines.md) if the change touches
  Redis state, key naming, locks, streams, rate limits, or semaphores.
- Read [Error Handling](./error-handling.md) if the change adds a public method,
  validates config, parses Redis payloads, or wraps network operations.
- Read [Quality Guidelines](./quality-guidelines.md) before changing memory/Redis
  parity, visibility, tests, or atomic update behavior.
- Read [Logging Guidelines](./logging-guidelines.md) before adding any tracing
  event. This crate is intentionally quiet.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Facade, memory backend, Redis module split, placement rules | Complete |
| [Database Guidelines](./database-guidelines.md) | Redis state backend patterns, keyspace, KV, locks, streams, semaphores | Complete |
| [Error Handling](./error-handling.md) | `DataLayerError`, Redis conversion, validation, timeouts, semaphore errors | Complete |
| [Quality Guidelines](./quality-guidelines.md) | Type safety, visibility, parity, atomicity, tests, review checklist | Complete |
| [Logging Guidelines](./logging-guidelines.md) | Sparse `tracing::warn` usage and sensitive data restrictions | Complete |

## Crate Contract

`RuntimeState` is the public runtime-state facade:

```rust
// crates/aether-runtime-state/src/lib.rs:205
#[derive(Debug, Clone)]
pub struct RuntimeState {
    backend: Arc<RuntimeStateBackend>,
}
```

The backend enum is private. Callers use methods and traits such as
`ExpiringKvStore`, `RuntimeQueueStore`, `RuntimeState::lock_try_acquire`, and
`RuntimeState::semaphore` instead of branching on backend internals.

The Redis backend is composed from a shared client, keyspace, KV runner, lock
runner, and stream runner:

```rust
// crates/aether-runtime-state/src/lib.rs:216
struct RedisRuntimeBackend {
    client: RedisClient,
    keyspace: RedisKeyspace,
    kv: RedisKvRunner,
    lock: RedisLockRunner,
    stream: RedisStreamRunner,
    command_timeout_ms: Option<u64>,
}
```

## Quality Check

Before reporting work complete for this crate, verify:

- No template placeholders remain in this spec directory.
- Every guideline file includes source-backed examples with file paths.
- Redis key creation uses `RedisKeyspace`.
- Foreground Redis failures are returned as typed errors, not logged and hidden.
- Blocking Redis calls have configured timeout behavior.
- Memory and Redis behavior stay aligned or the difference is explicitly
  documented.
- New parser or validation behavior has a unit test that does not require a live
  Redis server.
- Hot-path operations do not add success logs or expose secrets in tracing fields.

## Language

All documentation in this directory is written in English.
