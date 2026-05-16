# Quality Guidelines

> Code-quality rules for the `aether-usage-runtime` backend crate.

## Overview

This crate is a transformation-heavy service layer. Quality here means: deterministic seed building, explicit allowlists, narrow public surface, and test coverage around edge cases that would otherwise silently change billing or persistence.

The code already shows a strong pattern: public API at the facade level, private helpers for deep sanitization, and targeted tests next to the logic they protect.

## Required Patterns

Keep public exports in `lib.rs` and keep internal helpers private unless another crate truly needs them.

```rust
// crates/aether-usage-runtime/src/lib.rs:17
pub use body_capture::{
    apply_usage_body_capture_policy_to_event, apply_usage_body_capture_policy_to_record,
    UsageBodyCaptureEngine,
};
```

Use `pub(crate)` for helpers that are shared only within this crate.

```rust
// crates/aether-usage-runtime/src/body_capture.rs:393
pub(crate) fn sync_usage_body_ref_metadata(
    metadata: &mut Option<Value>,
    field: UsageBodyField,
    body_ref: Option<&str>,
) { ... }
```

Use explicit builder names for all conversion steps:

* `build_*_seed` for normalized inputs
* `build_*_event` for final usage events
* `build_*_record` for repository records
* `map_*` for provider usage normalization
* `sanitize_*` for allowlist/truncation logic

Example:

```rust
// crates/aether-usage-runtime/src/write.rs:1212
pub fn build_usage_event_data_seed(
    plan: &ExecutionPlan,
    report_context: Option<&Value>,
) -> UsageEventData
```

Prefer `Option` over sentinel values for missing fields. The crate uses `Option<String>`, `Option<Value>`, and `Option<u64>` consistently instead of empty strings or zero-as-missing.

Use `BTreeMap` for serialized field maps and metadata maps where stable ordering helps reproducible output or deterministic tests.

## Visibility Rules

`lib.rs` is the only place that should broaden visibility for downstream callers. Private modules stay private unless there is a cross-crate need.

`executor.rs` is intentionally private because the runtime only needs `spawn_on_usage_background_runtime`.

```rust
// crates/aether-usage-runtime/src/executor.rs:8
pub(crate) fn spawn_on_usage_background_runtime<F>(task: F) -> tokio::task::JoinHandle<F::Output>
```

Avoid making helper structs public just because a test uses them. In several files the test-only helper is `#[cfg(test)]` scoped instead.

```rust
// crates/aether-usage-runtime/src/queue.rs:108
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
struct UsageQueueRuntimeSettings { ... }
```

## Naming Conventions

Public types should describe the domain, not implementation details:

* `UsageRuntime`
* `UsageQueue`
* `UsageEvent`
* `UsageEventData`
* `UsageBodyCapturePolicy`
* `UsageSettlementWriter`
* `UsageDataEventRecorder`

Internal seeds should include the phase they belong to:

* `LifecycleUsageSeed`
* `TerminalUsageSeed`
* `SyncTerminalUsagePayloadSeed`
* `StreamTerminalUsagePayloadSeed`
* `RuntimeRequestCaptureSeed`

That naming keeps the write pipeline readable when a single path branches into sync terminal, stream terminal, or pending usage writes.

## Type Safety Patterns

Map all event payloads through typed structs before flattening to JSON or storage records.

```rust
// crates/aether-usage-runtime/src/event.rs:157
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UsageEventEnvelope {
    v: u8,
    #[serde(rename = "type")]
    event_type: UsageEventType,
    request_id: String,
    timestamp_ms: u64,
    data: UsageEventData,
}
```

Use explicit enums for lifecycle states and terminal states.

```rust
// crates/aether-usage-runtime/src/write.rs:24
enum UsageLifecycleState {
    Pending,
    Streaming,
}
```

Use allowlists when moving JSON metadata across boundaries. `request_metadata.rs` is the main pattern to copy.

```rust
// crates/aether-usage-runtime/src/request_metadata.rs:71
fn copy_allowed_metadata_fields(source: &Map<String, Value>, target: &mut Map<String, Value>) {
    copy_non_empty_string(source, target, "trace_id");
    copy_non_empty_string(source, target, "client_ip");
    copy_bool(source, target, "client_requested_stream");
    ...
}
```

## Testing Requirements

Every module with meaningful behavior should carry focused tests next to the implementation. This crate already does that in `config.rs`, `queue.rs`, `worker.rs`, `runtime.rs`, `write.rs`, `request_metadata.rs`, `body_capture.rs`, `report.rs`, `report_context.rs`, `settlement.rs`, and `usage_mapper.rs`.

Keep tests specific to the rule being protected. Good examples:

* `enabled_config_rejects_empty_stream_key`
* `usage_queue_applies_runtime_block_and_batch_settings`
* `usage_background_runtime_runs_on_dedicated_named_threads`
* `rejects_non_finite_costs_before_writing`
* `sanitizes_request_metadata_to_allowlist`
* `usage_event_round_trips_through_stream_fields`

These names explain the invariant, not just the function under test.

## Forbidden Patterns

DON'T bypass the facade and import deep private helpers from downstream crates. Import the public `aether_usage_runtime` exports instead.

DON'T add `unwrap()` in runtime or queue paths that can receive external data. The crate currently uses `?`, `ok()?`, or explicit fallback logic.

DON'T let raw provider JSON flow directly into storage. Normalize it through `UsageMapper`, `StandardizedUsage`, and the seed builders.

DON'T widen the metadata allowlist casually. `request_metadata.rs` exists specifically to keep the persisted metadata contract tight.

DON'T mix queue lifecycle and record-mapping logic in the same function. `UsageQueue` manages queue mechanics; `UsageRuntime` and `worker.rs` manage write behavior.

## Code Review Checklist

Check that the change preserves these existing expectations:

1. Public exports still go through `lib.rs`.
2. Config validation still fails loud for zero/empty queue settings.
3. Body and metadata capture still respect allowlists and size limits.
4. Queue poison messages still go to DLQ.
5. Terminal writes still fall back from queue to direct write when needed.
6. Tests cover the new edge case with a real payload example.

## Examples of Good Local Patterns

`UsageRuntime::record_sync_terminal` clones only the data it needs and logs a structured warning when billing enrichment fails.

`UsageQueueWorker::process_entries` keeps an `ack_ids` buffer so a single bad entry does not silently lose earlier successful entries.

`build_upsert_usage_record_from_event` copies data from `UsageEvent` into a record struct without inventing extra side effects.

`UsageMapper::map_from_response` tolerates provider shape differences by family, then normalizes the result.
