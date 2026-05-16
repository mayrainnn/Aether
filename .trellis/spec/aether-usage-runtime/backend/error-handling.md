# Error Handling

> Error-handling rules for the `aether-usage-runtime` backend crate.

## Overview

This crate does not define a crate-specific error enum. It uses `aether_data_contracts::DataLayerError` as the public error carrier and converts lower-level failures into `DataLayerError::InvalidConfiguration`, `InvalidInput`, or `UnexpectedValue` depending on the failure class.

The main rule is to fail loud on structural mistakes and to degrade gracefully on runtime side effects. Validation and serialization failures bubble up immediately. Background writes and worker-loop failures are usually logged with `warn!` and then the loop continues or falls back to direct writes.

Evidence:

```rust
// crates/aether-usage-runtime/src/config.rs:39
pub fn validate(&self) -> Result<(), DataLayerError> { /* ... */ }

// crates/aether-usage-runtime/src/runtime.rs:486
fn join_error_to_data_layer(err: tokio::task::JoinError) -> DataLayerError {
    DataLayerError::UnexpectedValue(format!("usage builder task join failed: {err}"))
}
```

## Error Types

`DataLayerError::InvalidConfiguration` is used for config validation and precondition failures.

```rust
// crates/aether-usage-runtime/src/config.rs:44
if self.stream_key.trim().is_empty() {
    return Err(DataLayerError::InvalidConfiguration(
        "usage runtime stream_key cannot be empty".to_string(),
    ));
}
```

`DataLayerError::InvalidInput` is used when caller-provided runtime values are structurally valid but semantically impossible, such as a non-finite settlement cost.

```rust
// crates/aether-usage-runtime/src/settlement.rs:55
fn finite_cost(value: f64) -> Result<f64, DataLayerError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(InvalidInput(
            "wallet settlement cost must be finite".to_string(),
        ))
    }
}
```

`DataLayerError::UnexpectedValue` is the default wrapper for serialization, deserialization, or task-join failures.

```rust
// crates/aether-usage-runtime/src/event.rs:189
let payload = serde_json::to_string(&payload).map_err(|err| {
    DataLayerError::UnexpectedValue(format!(
        "failed to serialize usage event payload: {err}"
    ))
})?;
```

## Propagation Patterns

Use `?` for value-validation and contract checks when the caller should stop immediately.

```rust
// crates/aether-usage-runtime/src/queue.rs:25
config.validate()?;

// crates/aether-usage-runtime/src/runtime.rs:95
config.validate()?;
```

Use `map_err` when converting library failures into `DataLayerError` with context.

```rust
// crates/aether-usage-runtime/src/queue.rs:100
serde_json::to_string(&json!({...}))
    .map_err(|err| DataLayerError::UnexpectedValue(err.to_string()))?;
```

Use `Option` for intentionally skipped work. `UsageRuntime::spawn_worker` returns `None` when the runtime is disabled or the queue is unavailable; that is not an error.

```rust
// crates/aether-usage-runtime/src/runtime.rs:110
pub fn spawn_worker<T>(&self, data: Arc<T>) -> Option<tokio::task::JoinHandle<()>>
```

Use `warn!` when the failure should be observed but should not crash the request path or the worker loop.

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

## Queue and Worker Errors

`UsageQueue::new` validates config before building a queue wrapper. That means queue construction failures are configuration problems, not runtime I/O problems.

`UsageQueue::ack_and_delete` first acks then deletes. If ack fails, delete is not attempted; if delete fails, the whole operation returns that error.

`UsageQueue::push_dead_letter` serializes an inline JSON payload before writing to the DLQ stream. A JSON encoding failure becomes `UnexpectedValue`, not a silent drop.

```rust
// crates/aether-usage-runtime/src/queue.rs:88
pub async fn push_dead_letter(
    &self,
    entry: &RuntimeQueueEntry,
    error: &str,
) -> Result<String, DataLayerError> {
    let fields = BTreeMap::from([(
        "payload".to_string(),
        serde_json::to_string(&json!({
            "entry_id": entry.id,
            "fields": entry.fields,
            "error": error,
        }))
        .map_err(|err| DataLayerError::UnexpectedValue(err.to_string()))?,
    )]);
    ...
}
```

`UsageQueueWorker::process_entry` turns malformed stream payloads into DLQ entries and then reports the entry as handled so the worker does not spin forever on poison messages.

```rust
// crates/aether-usage-runtime/src/worker.rs:192
let event = match UsageEvent::from_stream_fields(&entry.fields) {
    Ok(event) => event,
    Err(err) => {
        self.queue.push_dead_letter(entry, &err.to_string()).await?;
        return Ok(true);
    }
};
```

## Runtime Error Strategy

`UsageRuntime::record_pending`, `record_stream_started`, `record_sync_terminal`, `record_stream_terminal`, and `submit_terminal_event` all short-circuit when disabled. That is a control decision, not an error condition.

The runtime prefers fallback writes over hard failure when possible:

1. Try queue enqueue.
2. If queue creation or enqueue fails, log `warn!`.
3. Fall back to direct record writing.
4. Keep the request path alive.

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

`apply_body_capture_policy_from_data` and `apply_body_capture_policy_to_record_from_data` also log and fall back to default policy if the policy lookup fails.

## API / Serialization Errors

`UsageEvent::from_stream_fields` rejects missing payloads, invalid JSON, and version mismatches with `UnexpectedValue`.

```rust
// crates/aether-usage-runtime/src/event.rs:197
let payload = fields.get("payload").ok_or_else(|| {
    DataLayerError::UnexpectedValue(
        "usage event stream entry missing payload field".to_string(),
    )
})?;
```

`resolve_error_message` and `decode_body_for_storage` are intentionally permissive. If body decoding fails, they return `None` or a fallback `Value::String`; they do not fail the whole event build.

## Common Mistakes

DON'T introduce a new crate-local error enum unless a real caller needs typed matching beyond `DataLayerError`.

DON'T panic on bad payloads in queue consumers. Poison entries should go to the DLQ and then be acknowledged.

DON'T propagate background task join errors as raw `JoinError`. Convert them to `DataLayerError::UnexpectedValue` with context.

DON'T convert disabled runtime paths into errors. A disabled usage runtime is a normal configuration state.

DON'T swallow serialization failures that would corrupt persistent data. Event and queue payload builders should return `Err`.
