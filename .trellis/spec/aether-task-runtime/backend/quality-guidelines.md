# Quality Guidelines

> Code quality rules for `crates/aether-task-runtime`.

---

## Overview

This crate is intentionally small and type-centric. Quality here means keeping
the public primitives stable, generic, dependency-light, and cancellation-aware.
The source does not define a `Task` trait or a full executor abstraction today;
it defines metadata types plus `TaskSupervisor`.

The most important design rule is separation of ownership:

- `aether-task-runtime` owns shared task vocabulary and supervision mechanics.
- Application crates own concrete task registries, persistence, locks, and
  domain failures.
- Data crates own database representations.

---

## Required Public Type Shape

Serializable enums and configuration structs must preserve the current derive
style unless there is a specific compatibility reason to change it:

```rust
// crates/aether-task-runtime/src/lib.rs:8
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TaskKind {
    Scheduled,
    Daemon,
    OnDemand,
    FireAndForget,
}
```

`TaskStatus` follows the same convention:

```rust
// crates/aether-task-runtime/src/lib.rs:29
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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

The `Copy` derive matters because application registries store static
definitions and return them by value:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:254
pub(crate) fn task_definition(task_key: &str) -> Option<TaskDefinition> {
    task_definitions()
        .iter()
        .copied()
        .find(|definition| definition.key == task_key)
}
```

Do not remove `Copy` from `TaskDefinition`, `RetryPolicy`, `TaskKind`, or
`TaskStatus` unless all registry consumers are updated and the change is
deliberate.

---

## Stable String Forms

Use explicit `as_str` match arms for status and kind strings. Do not derive
string values implicitly from variant names.

```rust
// crates/aether-task-runtime/src/lib.rs:42
impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Retrying => "retrying",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }
}
```

This pattern makes persistence/API vocabulary explicit and reviewable. If a new
variant is added, add its string mapping in the same patch.

DON'T use `format!("{status:?}")` or lowercasing variant names to produce
stored values. That couples storage to Rust debug output.

---

## Visibility Rules

Value objects that are meant to be embedded in static registries expose fields:

```rust
// crates/aether-task-runtime/src/lib.rs:67
pub struct TaskDefinition {
    pub key: &'static str,
    pub kind: TaskKind,
    pub trigger: &'static str,
    pub singleton: bool,
    pub persist_history: bool,
    pub retry_policy: RetryPolicy,
}
```

Runtime state stays private and is exposed through accessors:

```rust
// crates/aether-task-runtime/src/lib.rs:97
pub struct TaskContext<TPayload = serde_json::Value> {
    run_id: String,
    task_key: String,
    payload: Option<TPayload>,
    cancellation_token: CancellationToken,
}
```

This is the local convention:

- Public fields are acceptable for immutable metadata records.
- Private fields are required for mutable or lifecycle-sensitive runtime state.
- Accessors return borrowed strings or cloned cancellation tokens, not direct
  mutable access.

Example accessor:

```rust
// crates/aether-task-runtime/src/lib.rs:120
pub fn run_id(&self) -> &str {
    &self.run_id
}
```

DON'T expose `TaskSupervisor.join_set` or `TaskContext.payload` as mutable
fields. Callers should not be able to corrupt supervision state or take payload
ownership unexpectedly.

---

## Async Bounds

Supervisor APIs require spawned work to be sendable, static, and output `()`:

```rust
// crates/aether-task-runtime/src/lib.rs:165
pub fn spawn_named<F>(&mut self, task_name: &'static str, future: F)
where
    F: Future<Output = ()> + Send + 'static,
```

Keep this bound unless a broader task-result model is intentionally designed.
Returning `()` forces domain errors to be handled by the task body or caller,
which is what the gateway currently does for persisted background runs.

`supervise_handle` has the same output rule:

```rust
// crates/aether-task-runtime/src/lib.rs:187
pub fn supervise_handle(&mut self, task_name: &'static str, mut handle: JoinHandle<()>) {
```

DON'T accept non-static task names or futures. The supervisor stores names in
spawned tasks and must not borrow stack data.

---

## Counter Safety

The supervisor uses saturating arithmetic for task count tracking:

```rust
// crates/aether-task-runtime/src/lib.rs:169
self.supervised_task_count = self.supervised_task_count.saturating_add(1);
```

Keep that pattern for any count derived from repeated task registration.
Background services are long-lived, and overflow should not panic the process.

The count is intentionally "tasks ever registered with this supervisor", not
"currently alive tasks":

```rust
// crates/aether-task-runtime/src/lib.rs:205
pub fn is_empty(&self) -> bool {
    self.supervised_task_count == 0
}
```

If a future change needs live counts, add a separate method with a precise name
instead of changing `task_count` semantics silently.

---

## Dependency Discipline

The current dependency list is small:

```toml
# crates/aether-task-runtime/Cargo.toml:9
[dependencies]
aether-runtime.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-util.workspace = true
tracing.workspace = true
```

No new dependency should be added for convenience. Prefer standard library,
existing `tokio`/`tokio-util`, or `aether-runtime` helpers.

DON'T add:

- `sea-orm` for persistence.
- `redis` for cancellation flags.
- `axum` for response mapping.
- `anyhow` or `thiserror` before a public fallible API exists.

---

## Testing Requirements

This crate currently has no local test module. Any behavioral change should add
focused tests in or near `crates/aether-task-runtime/src/lib.rs`, and should run:

```bash
cargo test -p aether-task-runtime
```

When changing `TaskSupervisor`, also run a downstream gateway test or compile
target that exercises `spawn_background_tasks`, because the gateway is the main
consumer:

```rust
// apps/aether-gateway/src/state/core.rs:1015
let mut supervise_worker =
    |task_key: &'static str, handle: Option<tokio::task::JoinHandle<()>>| {
        if let Some(handle) = handle {
            supervisor.supervise_handle(task_key, handle);
            record_boot(task_key);
        }
    };
```

Tests for future supervisor changes should cover:

- `shutdown` cancels and drains supervised tasks.
- `task_count` increments for both `spawn_named` and `supervise_handle`.
- cancellation tokens cloned from context and supervisor observe the same
  cancellation signal.
- panicking tasks do not panic `shutdown`.

---

## Code Review Checklist

Before accepting changes in this crate, verify:

- Public enum variants have explicit `as_str` mappings.
- Static task metadata remains `Copy` where application registries rely on it.
- `TaskDefinition` remains allocation-free for `key` and `trigger`.
- Runtime state fields remain private.
- Cancellation remains normal lifecycle, not a generic error.
- New async APIs preserve `Send + 'static` where work is spawned.
- No persistence or web framework dependency has entered this crate.
- Logging uses structured `tracing` fields and does not include payload bodies.
- Any semantic behavior change has at least a crate-level unit test.
