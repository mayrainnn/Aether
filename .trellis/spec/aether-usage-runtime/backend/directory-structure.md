# Directory Structure

> Backend layout rules for the `aether-usage-runtime` service crate.

## Overview

`crates/aether-usage-runtime` is a service-layer Rust crate for usage accounting. It is not an HTTP crate and it is not a repository crate. It owns usage event shapes, queue primitives, event-to-record conversion, body capture policy, and the async runtime used to write usage records without blocking request paths.

The crate is intentionally flat: each `src/*.rs` file owns one usage-runtime concern, and `src/lib.rs` re-exports the stable API surface. Keep new functionality in that flat module map unless it is clearly a private helper for one module.

Evidence:

```rust
// crates/aether-usage-runtime/src/lib.rs:1
mod body_capture;
pub mod config;
pub mod event;
mod executor;
pub mod queue;
pub mod record;
pub mod report;
pub mod report_context;
mod request_metadata;
pub mod runtime;
pub mod settlement;
pub mod standardized_usage;
pub mod usage_mapper;
pub mod worker;
pub mod write;
```

`body_capture`, `executor`, and `request_metadata` are private modules because callers should use the public policy, runtime, and event builders instead of assembling capture internals themselves.

## Directory Layout

```text
crates/aether-usage-runtime/
├── Cargo.toml
└── src/
    ├── lib.rs                  # public facade and re-exports
    ├── config.rs               # UsageRuntimeConfig validation and defaults
    ├── event.rs                # UsageEvent, UsageEventData, stream payload envelope
    ├── runtime.rs              # UsageRuntime orchestration and background writes
    ├── queue.rs                # RuntimeQueueStore-backed usage event queue
    ├── worker.rs               # queue consumer, record writer traits, DLQ handling
    ├── write.rs                # seed builders, usage extraction, sanitization, event building
    ├── record.rs               # UsageEvent -> UpsertUsageRecord mapping
    ├── settlement.rs           # settlement trigger boundary
    ├── body_capture.rs         # body capture level/limit enforcement
    ├── request_metadata.rs     # metadata allowlist and bounds enforcement
    ├── report.rs               # gateway sync/stream report DTOs and route inference
    ├── report_context.rs       # fills locally actionable report context
    ├── standardized_usage.rs   # re-export of aether_contracts::StandardizedUsage
    └── usage_mapper.rs         # provider usage JSON -> StandardizedUsage mapping
```

## Module Responsibilities

`runtime.rs` is the orchestration entry point. It defines `UsageRuntimeAccess`, `UsageBillingEventEnricher`, `UsageBodyCapturePolicy`, and `UsageRuntime`, then decides whether terminal usage goes to the queue or direct repository writes.

```rust
// crates/aether-usage-runtime/src/runtime.rs:54
#[async_trait]
pub trait UsageRuntimeAccess:
    UsageRecordWriter
    + UsageSettlementWriter
    + UsageBillingEventEnricher
    + crate::worker::ManualProxyNodeCounter
    + Send
    + Sync
{
    fn has_usage_writer(&self) -> bool;
    fn has_usage_worker_queue(&self) -> bool;
    fn usage_worker_queue(&self) -> Option<Arc<dyn RuntimeQueueStore>>;
}
```

`queue.rs` wraps `aether_runtime_state::RuntimeQueueStore`. It should stay generic over the runtime queue abstraction; do not add Redis-specific types or connection setup here.

```rust
// crates/aether-usage-runtime/src/queue.rs:11
#[derive(Clone)]
pub struct UsageQueue {
    runner: Arc<dyn RuntimeQueueStore>,
    config: UsageRuntimeConfig,
    stream: String,
    group: String,
    dlq_stream: String,
}
```

`worker.rs` owns the consumer loop and data-store traits used by the loop. Put worker-only concerns here, not in `runtime.rs`.

```rust
// crates/aether-usage-runtime/src/worker.rs:16
#[async_trait]
pub trait UsageEventRecorder: Send + Sync {
    async fn record_usage_event(&self, event: &UsageEvent) -> Result<(), DataLayerError>;
}

#[async_trait]
pub trait UsageRecordWriter: Send + Sync {
    async fn upsert_usage_record(
        &self,
        record: UpsertUsageRecord,
    ) -> Result<Option<StoredRequestUsageAudit>, DataLayerError>;
}
```

`write.rs` is the heavy transformation module. It builds lifecycle seeds, terminal events, token extraction, error classification, body storage values, and capture sanitization. New conversion rules for provider response bodies normally belong here, close to `map_usage_from_response`, `extract_token_counts_from_value`, and `resolve_error_message`.

`record.rs` is deliberately narrower: it maps a completed `UsageEvent` into an `UpsertUsageRecord`. Do not duplicate event-building logic in `record.rs`.

`request_metadata.rs` is a private allowlist and bounds module. Add metadata keys here only when the value must survive into persisted usage metadata. Routing fields such as `candidate_id` are mapped onto first-class record columns in `record.rs` and should not remain as arbitrary metadata.

## Public Facade Rules

`lib.rs` is the only public facade. Export types there when downstream crates are expected to use them. Keep helper functions private or `pub(crate)` unless a gateway/admin/billing caller already needs them.

```rust
// crates/aether-usage-runtime/src/lib.rs:38
pub use runtime::{
    UsageBillingEventEnricher, UsageBodyCapturePolicy, UsageRequestRecordLevel, UsageRuntime,
    UsageRuntimeAccess, DEFAULT_USAGE_REQUEST_BODY_CAPTURE_LIMIT_BYTES,
    DEFAULT_USAGE_RESPONSE_BODY_CAPTURE_LIMIT_BYTES,
};
```

Gateway integration consumes the facade rather than importing private modules:

```rust
// apps/aether-gateway/src/execution_runtime/sync/execution.rs:78
let context_seed = build_terminal_usage_context_seed(plan, report_context);
let payload_seed = build_sync_terminal_usage_payload_seed(payload);
state
    .usage_runtime
    .record_sync_terminal(state.data.as_ref(), context_seed, payload_seed);
```

## Naming Conventions

Use `Usage*` prefixes for public crate-owned types: `UsageRuntime`, `UsageQueue`, `UsageEvent`, `UsageEventData`, `UsageBodyCapturePolicy`, `UsageSettlementWriter`.

Use `build_*_seed` for pure seed construction, `build_*_event` for final `UsageEvent` construction, and `record_*` for side-effecting runtime entry points.

Use private seed structs in `write.rs` when values are internal to construction, and expose only stable seeds that callers need to pass across runtime boundaries:

```rust
// crates/aether-usage-runtime/src/write.rs:67
pub struct LifecycleUsageSeed { /* public request lifecycle seed */ }

// crates/aether-usage-runtime/src/write.rs:30
struct UsageRoutingSeed { /* private routing merge helper */ }
```

## Where New Code Belongs

Add a new queue operation to `queue.rs` only if it is a generic `RuntimeQueueStore` operation used by usage recording.

Add a new worker behavior to `worker.rs` if it changes consumer lifecycle, dead-letter handling, acknowledgements, or post-write side effects.

Add a new persisted usage field in this order: event data in `event.rs`, construction in `write.rs`, record mapping in `record.rs`, metadata allowlisting in `request_metadata.rs` only if the field is intentionally not a first-class column.

Add new report DTO fields in `report.rs`; translate them into terminal payload seeds in `write.rs`.

Add new route/actionability inference in `report_context.rs` only when the report needs gateway state to become locally actionable.

## Examples To Follow

`runtime.rs` is the pattern for resilient async orchestration: validate config on construction, short-circuit when disabled, spawn blocking work for CPU/JSON builders, log failures with structured fields, and keep request paths fire-and-forget.

`worker.rs` is the pattern for queue consumers: ensure group once, reclaim stale entries on an interval, process batches, send malformed payloads to DLQ, and acknowledge only successfully handled entries.

`request_metadata.rs` is the pattern for privacy-sensitive enrichment: allowlist fields, sanitize paths and query strings, truncate large values, and drop everything else.

## DON'T Patterns

DON'T add HTTP handlers, axum extractors, or route-specific response building to this crate. Gateway callers should call the facade from `apps/aether-gateway`.

DON'T add SeaORM/sqlx models or migration code here. This crate depends on `aether-data-contracts` and `aether-runtime-state` contracts, not concrete database repositories.

DON'T make private helper modules public just to simplify one caller. Add a narrow facade export in `lib.rs` only after there is a real cross-crate need.

DON'T bypass `UsageRuntimeAccess` with ad hoc repository arguments. `GatewayDataState` implements the full integration contract in `apps/aether-gateway/src/data/state/integrations.rs:240`, so runtime code can stay dependency-injected.
