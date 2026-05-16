# Error Handling

> Error, cancellation, and failure-reporting conventions for `aether-task-runtime`.

---

## Overview

This crate currently defines no custom error type. It also exposes no public
`Result`-returning API. That is deliberate: `aether-task-runtime` is a small
runtime primitive crate, not the owner of task execution business errors or
database errors.

The public API handles only two failure-like events:

1. A supervised `JoinHandle<()>` returns `Err(JoinError)`.
2. A caller asks the supervisor or context to cancel work via
   `CancellationToken`.

Join failures are logged and swallowed by the supervisor. Cancellation is a
normal shutdown path and is not logged as an error by this crate.

---

## No Crate Error Enum

Do not add `TaskRuntimeError` just because a new function might fail. First
check whether the failure belongs to the caller's domain. The current source has
only infallible constructors, accessors, and lifecycle methods:

```rust
// crates/aether-task-runtime/src/lib.rs:105
impl<TPayload> TaskContext<TPayload> {
    pub fn new(
        run_id: impl Into<String>,
        task_key: impl Into<String>,
        payload: Option<TPayload>,
        cancellation_token: CancellationToken,
    ) -> Self {
```

`TaskContext::new` accepts already-validated identifiers and payload values. It
does not parse, persist, or validate external input, so it returns `Self`.

`TaskSupervisor::new` is similarly infallible:

```rust
// crates/aether-task-runtime/src/lib.rs:152
impl TaskSupervisor {
    pub fn new() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            join_set: JoinSet::new(),
            supervised_task_count: 0,
        }
    }
}
```

If a future change introduces parsing, database access, or external I/O, keep
that logic outside this crate unless multiple applications genuinely need the
same error boundary.

---

## Supervised Join Failures

`TaskSupervisor` treats a task panic or cancellation-induced join failure as an
observability event. The supervisor logs the `JoinError` with structured fields
and keeps draining its own wrapper task:

```rust
// crates/aether-task-runtime/src/lib.rs:171
self.join_set.spawn(async move {
    let mut handle = spawn_named(task_name, future);
    tokio::select! {
        _ = cancellation_token.cancelled() => {
            handle.abort();
            let _ = handle.await;
        }
        result = &mut handle => {
            if let Err(error) = result {
                warn!(task = task_name, error = ?error, "supervised task failed");
            }
        }
    }
});
```

The same pattern is used when the caller already has a `JoinHandle<()>`:

```rust
// crates/aether-task-runtime/src/lib.rs:187
pub fn supervise_handle(&mut self, task_name: &'static str, mut handle: JoinHandle<()>) {
    self.supervised_task_count = self.supervised_task_count.saturating_add(1);
    let cancellation_token = self.cancellation_token.clone();
```

Both paths return `()` because they are registration methods. The caller cannot
observe task completion through the supervisor API; it can only cancel and await
supervisor shutdown.

---

## Cancellation Is Not An Error

Cancellation is a first-class runtime signal. `TaskContext` exposes both polling
and awaitable access:

```rust
// crates/aether-task-runtime/src/lib.rs:132
pub fn cancellation_token(&self) -> CancellationToken {
    self.cancellation_token.clone()
}

// crates/aether-task-runtime/src/lib.rs:136
pub fn is_cancelled(&self) -> bool {
    self.cancellation_token.is_cancelled()
}

// crates/aether-task-runtime/src/lib.rs:140
pub async fn cancelled(&self) {
    self.cancellation_token.cancelled().await;
}
```

`TaskSupervisor::shutdown` also treats cancellation as expected lifecycle:

```rust
// crates/aether-task-runtime/src/lib.rs:217
pub async fn shutdown(mut self) {
    self.cancel();
    while self.join_set.join_next().await.is_some() {}
}
```

Do not log cancellation as `error!` or return a failure solely because the
shared token was cancelled. Callers should decide whether cancellation maps to
`Cancelled`, `Skipped`, or another domain status.

---

## Caller-Owned Business Errors

Supervised futures must return `()`. If the worker body can fail, handle the
domain error inside the future before registering it:

```rust
// crates/aether-task-runtime/src/lib.rs:165
pub fn spawn_named<F>(&mut self, task_name: &'static str, future: F)
where
    F: Future<Output = ()> + Send + 'static,
```

The gateway follows this rule by mapping task-specific failures to persisted
status and events in its own module:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:602
Err(err) => {
    warn!(
        "gateway admin provider delete task failed for provider {}: {:?}",
        provider_id, err
    );
    app.put_provider_delete_task(crate::LocalProviderDeleteTaskState {
        task_id: run_id.clone(),
        provider_id: provider_id.clone(),
        status: "failed".to_string(),
```

That code belongs in the application because it knows provider IDs, local task
state, persistence semantics, and user-facing failure text.

---

## API Error Responses

This crate has no axum handlers and no HTTP response types. Do not add API
error response structs here. HTTP-facing mapping belongs in the gateway or an
application crate that owns request/response semantics.

The nearest public status vocabulary is `TaskStatus`, but it is not an HTTP
error format:

```rust
// crates/aether-task-runtime/src/lib.rs:32
pub enum TaskStatus {
    Queued,
    Running,
    Retrying,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
}
```

Use this enum for shared lifecycle language. Let the caller map it to database
rows, JSON fields, metrics, or API responses.

---

## DON'T Patterns

DON'T make supervised futures return `Result` and expect `TaskSupervisor` to
propagate it. The current bound is `Future<Output = ()>`.

DON'T convert cancellation into a generic error. Cancellation is represented by
`CancellationToken` and explicit lifecycle status.

DON'T add `anyhow` or `thiserror` to this crate until a real public fallible
operation exists. `Cargo.toml` currently has neither dependency.

DON'T use `unwrap` or `expect` inside supervisor shutdown paths. The current
implementation intentionally ignores the aborted handle result:

```rust
// crates/aether-task-runtime/src/lib.rs:174
_ = cancellation_token.cancelled() => {
    handle.abort();
    let _ = handle.await;
}
```

The ignored result prevents shutdown from panicking when a task was deliberately
aborted.
