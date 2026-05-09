# Backend Development Guidelines

> Entry point for `aether-usage-runtime` backend development guidance.

## Scope

These guidelines apply to `crates/aether-usage-runtime/`, the service-layer Rust crate that owns usage tracking runtime behavior for Aether.

The crate is responsible for:

* usage event DTOs and stream serialization
* queue-backed and direct usage recording
* body capture policy and capture metadata
* request metadata allowlisting and truncation
* usage event to repository-record mapping
* terminal usage seed builders for sync and stream execution
* settlement trigger decisions after usage records are stored
* provider usage JSON mapping into `StandardizedUsage`

The crate is not responsible for:

* HTTP routing
* concrete SeaORM/sqlx queries
* database migrations
* concrete Redis client setup
* admin API response formatting
* frontend usage dashboards

## Pre-Development Checklist

Before editing `aether-usage-runtime`, read the guide that matches your change:

1. For module placement, exports, or new files, read [Directory Structure](./directory-structure.md).
2. For new `Result` paths, queue failures, or fallback behavior, read [Error Handling](./error-handling.md).
3. For metadata, body capture, mapping, visibility, or tests, read [Quality Guidelines](./quality-guidelines.md).
4. For `tracing::warn!` changes, read [Logging Guidelines](./logging-guidelines.md).
5. For queue, record writes, settlement, or persistence boundaries, read [Database Guidelines](./database-guidelines.md).

## Guidelines Index

| Guide | Description | Current status |
|-------|-------------|----------------|
| [Directory Structure](./directory-structure.md) | Flat module map, public facade rules, where new code belongs | Filled from current source |
| [Error Handling](./error-handling.md) | `DataLayerError` usage, queue/DLQ behavior, runtime fallbacks | Filled from current source |
| [Quality Guidelines](./quality-guidelines.md) | Naming, visibility, type safety, tests, forbidden patterns | Filled from current source |
| [Logging Guidelines](./logging-guidelines.md) | `tracing::warn!` conventions, fields, levels, sensitive data rules | Filled from current source |
| [Database Guidelines](./database-guidelines.md) | Queue and repository contracts, direct vs queued writes, migrations boundary | Filled from current source |

## Architectural Facts

The public API is centralized in `src/lib.rs`; do not bypass it from other crates.

```rust
// crates/aether-usage-runtime/src/lib.rs:38
pub use runtime::{
    UsageBillingEventEnricher, UsageBodyCapturePolicy, UsageRequestRecordLevel, UsageRuntime,
    UsageRuntimeAccess, DEFAULT_USAGE_REQUEST_BODY_CAPTURE_LIMIT_BYTES,
    DEFAULT_USAGE_RESPONSE_BODY_CAPTURE_LIMIT_BYTES,
};
```

The runtime is dependency-injected through `UsageRuntimeAccess` and related writer traits, not through concrete database clients.

```rust
// crates/aether-usage-runtime/src/runtime.rs:54
pub trait UsageRuntimeAccess:
    UsageRecordWriter
    + UsageSettlementWriter
    + UsageBillingEventEnricher
    + crate::worker::ManualProxyNodeCounter
    + Send
    + Sync
```

The queue path is optional and has a direct-write fallback.

```rust
// crates/aether-usage-runtime/src/runtime.rs:355
if let Some(runner) = data.usage_worker_queue() {
    match UsageQueue::new(runner, self.config.clone()) {
        Ok(queue) => match queue.enqueue(&event).await {
            Ok(_) => return,
            Err(err) => warn!(...),
        },
        Err(err) => warn!(...),
    }
}
```

Usage metadata is allowlisted and bounded before storage.

```rust
// crates/aether-usage-runtime/src/request_metadata.rs:51
pub(crate) fn sanitize_usage_request_metadata(value: Option<Value>) -> Option<Value>
```

## Code Intelligence Evidence

GitNexus was used with `repo="Aether"` to inspect the indexed repository context and Usage cluster. The index reported `Aether` with 83,229 symbols and 300 execution flows. The Usage cluster included `crates/aether-usage-runtime/src/body_capture.rs` and related usage/admin/gateway symbols.

ABCoder was configured for `repo_name="aether-usage-runtime"`. The target AST file identified one Rust module set named `aether-usage-runtime` and packages including `body_capture`, `config`, `event`, `executor`, `queue`, `record`, `report`, `report_context`, `request_metadata`, `runtime`, `settlement`, `usage_mapper`, `worker`, and `write`.

## Quality Check

Before claiming a change is done:

1. Confirm new public APIs are re-exported deliberately in `lib.rs`.
2. Confirm no concrete database/Redis client leaked into this crate.
3. Confirm `DataLayerError` conversions preserve context.
4. Confirm raw bodies and sensitive headers are not logged.
5. Confirm metadata changes go through the allowlist.
6. Confirm terminal event changes update both `write.rs` and `record.rs`.
7. Confirm queue changes preserve DLQ and ack-after-process behavior.
8. Run the narrow Rust tests for this crate or the relevant gateway usage tests when source code changes are made.

## Language

All documentation in this directory is written in English. Keep future updates specific to real `aether-usage-runtime` code; do not add generic Rust advice without a local example and file path.
