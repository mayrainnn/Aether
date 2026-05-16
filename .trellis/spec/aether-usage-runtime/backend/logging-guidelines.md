# Logging Guidelines

> Logging and tracing rules for the `aether-usage-runtime` backend crate.

## Overview

This crate uses `tracing::warn!` for recoverable failures and operational anomalies. It does not currently emit `info!`, `debug!`, or custom spans in the core runtime path. That is intentional: usage recording is a hot path, and the crate should stay quiet unless something needs operator attention.

The main logging style is structured and machine-readable. Logs consistently include an `event_name`, a `log_type`, a request or worker identifier, and the error or fallback mode that explains the failure.

Evidence:

```rust
// crates/aether-usage-runtime/src/runtime.rs:137
warn!(
    event_name = "usage_pending_record_failed",
    log_type = "event",
    request_id = %request_id,
    error = %err,
    "usage runtime failed to record sync pending usage"
);
```

## Log Levels

Use `warn!` when:

* a background usage write fails but the request should continue
* queue reads, claims, or worker maintenance fail and should retry later
* body-capture policy lookup fails and the runtime falls back to default policy
* billing enrichment fails and the runtime still records the usage event

Example worker warnings:

```rust
// crates/aether-usage-runtime/src/worker.rs:89
warn!(
    event_name = "usage_worker_consumer_group_failed",
    log_type = "ops",
    worker_consumer = %self.consumer,
    worker_group = %self.config.consumer_group,
    error = %err,
    "usage worker failed to ensure consumer group"
);
```

There is no evidence in this crate of `error!` escalation or `info!` lifecycle logging. Keep that quiet default unless a new operational requirement appears.

## Structured Logging

Use stable field names so downstream log search can group similar failures:

* `event_name` for semantic event classification
* `log_type` to distinguish `event` vs `ops`
* `request_id` for per-request correlation
* `worker_consumer` and `worker_group` for queue maintenance
* `fallback` when the code intentionally switches behavior
* `node_id` for manual proxy counter updates
* `error` for the error string or debug-formatted error

Example of a fallback log:

```rust
// crates/aether-usage-runtime/src/runtime.rs:360
warn!(
    event_name = "usage_terminal_enqueue_failed",
    log_type = "event",
    request_id = %event.request_id,
    fallback = "direct_write",
    error = %err,
    "usage runtime failed to enqueue terminal usage event; falling back to direct write"
)
```

Example of a policy fallback log:

```rust
// crates/aether-usage-runtime/src/runtime.rs:497
warn!(
    event_name = "usage_body_capture_policy_read_failed",
    log_type = "event",
    request_id = %event.request_id,
    fallback = "default",
    error = %err,
    "usage runtime failed to read body capture policy; keeping default capture"
);
```

## What To Log

Log only the failures that matter to operations or correctness:

* queue initialization failures
* queue read and reclaim failures
* DLQ push failures
* direct upsert failures
* settlement failures
* billing enrichment failures
* body-capture policy lookup failures
* manual proxy node counter failures

These are the observable boundaries of the crate. The code does not log every successful write or every constructed seed.

## What NOT To Log

Do not log raw request bodies, provider responses, or tokenized secrets. This crate already sanitizes metadata and masks sensitive headers before persistence; logs should follow the same rule.

Do not log full `request_metadata` blobs or queue entry payloads. If you need context, log the `request_id`, `event_name`, `worker_consumer`, or a compact `node_id`.

Do not add debug noise around every seed builder. `build_*` functions should stay pure and silent.

Do not add span nesting unless the crate starts doing long-lived multi-step work that cannot be explained by the current warning fields. At present, the structured warnings are enough.

## Common Patterns

`UsageRuntime` logs failure and then keeps the request path alive:

* record build failure -> warn
* record persistence failure -> warn
* queue enqueue failure -> warn and direct write

`UsageQueueWorker` logs operational failures and sleeps or retries instead of panicking:

* ensure consumer group failure -> warn and stop
* read failure -> warn and back off
* reclaim failure -> warn
* process failure -> warn and continue loop

`ManualProxyNodeCounter` failures are intentionally best-effort:

```rust
// crates/aether-usage-runtime/src/worker.rs:250
warn!(
    event_name = "manual_proxy_node_increment_failed",
    log_type = "ops",
    node_id = %node_id,
    error = ?err,
    "failed to increment manual proxy node request count"
);
```

## Review Checklist

Before approving a logging change, check that:

1. The log level matches the failure severity.
2. The structured fields still include a request, worker, or node identifier.
3. Sensitive payload data is not included.
4. The message explains the fallback or failure path.
5. The new log does not duplicate an existing warning without adding new value.
