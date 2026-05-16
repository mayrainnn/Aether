# Logging Guidelines

> Tracing and observability conventions for `aether-task-runtime`.

---

## Overview

This crate uses `tracing`, not `log`, `println!`, or application-specific audit
records. Its own logging surface is intentionally tiny: `src/lib.rs` imports
only `tracing::warn`, and logs only unexpected supervised task join failures.

```rust
// crates/aether-task-runtime/src/lib.rs:6
use tracing::warn;
```

The crate delegates normal task spawn logging to `aether-runtime::task::spawn_named`.
That helper emits a debug event before awaiting the future:

```rust
// crates/aether-runtime/src/task.rs:3
pub fn spawn_named<F>(task_name: &'static str, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move {
        tracing::debug!(task = task_name, "spawned runtime task");
        future.await
    })
}
```

Do not duplicate that debug spawn log in this crate. `TaskSupervisor` should
only add supervision-specific failure context.

---

## Log Levels

Current level usage:

- `debug!` is used by `aether-runtime::task::spawn_named` for normal task spawn.
- `warn!` is used by `aether-task-runtime` when a supervised task returns a
  `JoinError`.
- `info!` is not used in this crate today.
- `error!` is not used in this crate today.

The supervisor's warning path is:

```rust
// crates/aether-task-runtime/src/lib.rs:178
result = &mut handle => {
    if let Err(error) = result {
        warn!(task = task_name, error = ?error, "supervised task failed");
    }
}
```

Use `warn!` for task join failures because the supervisor can continue, but the
worker failed unexpectedly. Reserve `error!` for a future condition where the
runtime itself cannot preserve its contract.

Cancellation does not log:

```rust
// crates/aether-task-runtime/src/lib.rs:174
_ = cancellation_token.cancelled() => {
    handle.abort();
    let _ = handle.await;
}
```

Do not add warnings for this path. Shutdown-induced aborts are expected.

---

## Structured Fields

Use structured fields before the message. The current schema is:

- `task = task_name`
- `error = ?error`
- message: `"supervised task failed"`

```rust
// crates/aether-task-runtime/src/lib.rs:180
warn!(task = task_name, error = ?error, "supervised task failed");
```

`task_name` is a `&'static str`, so it is safe to attach directly. `JoinError`
uses `?error` debug formatting because `JoinError` carries panic/cancellation
details rather than a stable user-facing display string.

If future code logs task metadata, prefer low-cardinality fields:

- `task` for static task name.
- `kind` for `TaskKind::as_str()`.
- `status` for `TaskStatus::as_str()`.

Avoid adding high-cardinality or sensitive fields in this crate. Run IDs and
payload-specific identifiers belong in application logs where retention and
privacy policy are known.

---

## What To Log In This Crate

Log only events that the supervisor can observe directly:

1. A supervised task's `JoinHandle` fails.
2. A future runtime-level invariant failure in supervision code.

The same join-failure warning is used for caller-provided handles:

```rust
// crates/aether-task-runtime/src/lib.rs:196
result = &mut handle => {
    if let Err(error) = result {
        warn!(task = task_name, error = ?error, "supervised task failed");
    }
}
```

Do not log task business progress here. The gateway logs and persists those
events because it owns concrete task semantics:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:381
if let Err(error) = app.upsert_background_task_event(event).await {
    warn!(error = ?error, run_id = %run_id, "failed to upsert background task event");
}
```

That application log includes `run_id` because the gateway knows the persistence
contract and operational context. `aether-task-runtime` does not.

---

## What Not To Log

Do not log payload contents from `TaskContext<TPayload>`. The payload defaults to
`serde_json::Value`, which can contain prompts, tokens, customer data, provider
configuration, or other sensitive material:

```rust
// crates/aether-task-runtime/src/lib.rs:98
pub struct TaskContext<TPayload = serde_json::Value> {
    run_id: String,
    task_key: String,
    payload: Option<TPayload>,
    cancellation_token: CancellationToken,
}
```

Do not log the full `TaskContext` with `?context` despite its `Debug` derive.
The derive is useful for tests and controlled diagnostics, not for automatic
runtime logging.

DON'T:

```rust
// Bad pattern for this crate: may expose payload data.
warn!(?context, "task context failed");
```

Prefer:

```rust
// Good pattern: static task label plus structured error.
warn!(task = task_name, error = ?error, "supervised task failed");
```

---

## Span Policy

This crate currently creates no spans. Do not add broad spans around every
supervised task unless there is a specific trace-correlation requirement. The
inner task body may already create spans in the application layer, and
`spawn_named` already emits the static task label at spawn time.

If spans are added later, use task-level fields derived from static metadata:

- `task`
- `kind`
- `trigger`

Do not include serialized payloads, API keys, provider credentials, or prompt
text in span fields.

---

## Review Checklist

For logging changes, verify:

- No `println!`, `dbg!`, or `eprintln!` is introduced.
- `tracing` macros use structured fields.
- cancellation remains quiet during expected shutdown.
- payload bodies are never logged.
- task names are static and low cardinality.
- warnings remain actionable: they should indicate a failed supervised task, not
  normal lifecycle.
- application-specific persistence failures stay in application crates.
