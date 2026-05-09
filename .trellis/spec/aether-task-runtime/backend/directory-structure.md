# Directory Structure

> Actual module layout and ownership rules for `crates/aether-task-runtime`.

---

## Overview

`aether-task-runtime` is a tiny domain crate for shared async background-task
primitives. It is not an application, router, persistence layer, or worker
registry. The current crate has exactly one Rust source file and one public
module surface:

```text
crates/aether-task-runtime/
├── Cargo.toml
└── src/
    └── lib.rs
```

The ABCoder UniAST artifact for `repo_name="aether-task-runtime"` reports one
Rust module named `aether-task-runtime`, one package, and one source file:
`src/lib.rs`. It also reports six public types and fifteen public methods:
`TaskKind`, `TaskStatus`, `RetryPolicy`, `TaskDefinition`, `TaskContext`, and
`TaskSupervisor`.

GitNexus repo resources place the surrounding background-worker code in the
broader Aether runtime area, but the code that consumes this crate lives mostly
in `apps/aether-gateway/src/task_runtime/mod.rs` and
`apps/aether-gateway/src/state/core.rs`. Keep those as integration examples,
not as part of this crate's ownership boundary.

---

## Cargo Boundary

The crate depends only on runtime and async primitives:

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

Do not add SeaORM, Redis, axum, gateway, or data-repository dependencies here.
Persistence belongs in `aether-data`, `aether-data-contracts`, or an application
adapter. This crate should remain usable by any service that needs task metadata
and cancellation-aware supervision.

---

## Source File Organization

`src/lib.rs` is organized from pure metadata to runtime supervision:

1. Imports.
2. Serializable task taxonomy: `TaskKind` and `TaskStatus`.
3. Serializable task configuration: `RetryPolicy` and `TaskDefinition`.
4. Per-run execution context: `TaskContext<TPayload>`.
5. Cancellation-aware task owner: `TaskSupervisor`.
6. `Default` implementation for `TaskSupervisor`.

That order is intentional: higher-level application code should be able to read
the public vocabulary before reading the async supervision mechanics.

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

Task taxonomy is defined before task definitions because `TaskDefinition` embeds
`TaskKind` and is commonly used by application registries.

---

## Public Surface

The crate exports all public items directly from `lib.rs`; there is no nested
module path such as `aether_task_runtime::supervisor::TaskSupervisor`.

Use the direct crate path in consumers:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:9
pub(crate) use aether_task_runtime::TaskSupervisor;
use aether_task_runtime::{RetryPolicy, TaskDefinition, TaskKind};
```

That gateway module re-exports `TaskSupervisor` as `pub(crate)` so gateway code
can use its own `crate::task_runtime::TaskSupervisor` alias without leaking this
crate's implementation details into every caller.

Do not create a new module just to group one enum or one helper. Add a module
only when a new area has enough independent behavior to justify a separate file,
for example a future in-memory executor or a future retry scheduler. Even then,
keep `lib.rs` as the public facade.

---

## Type Responsibilities

`TaskKind` and `TaskStatus` are serializable vocabulary enums. Their `as_str`
methods define stable database/API string forms for consumers:

```rust
// crates/aether-task-runtime/src/lib.rs:18
impl TaskKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Daemon => "daemon",
            Self::OnDemand => "on_demand",
            Self::FireAndForget => "fire_and_forget",
        }
    }
}
```

`RetryPolicy` is intentionally small:

```rust
// crates/aether-task-runtime/src/lib.rs:56
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
}
```

It records metadata; it does not implement retry loops. Application code reads
it when creating persisted runs, as the gateway does here:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:475
let max_attempts = task_definition(TASK_KEY_PROVIDER_DELETE)
    .map(|item| item.retry_policy.max_attempts)
    .unwrap_or(1);
```

`TaskDefinition` is a static description of a task:

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

`TaskContext<TPayload>` is per-run state and cancellation access:

```rust
// crates/aether-task-runtime/src/lib.rs:97
pub struct TaskContext<TPayload = serde_json::Value> {
    run_id: String,
    task_key: String,
    payload: Option<TPayload>,
    cancellation_token: CancellationToken,
}
```

`TaskSupervisor` owns spawned background task handles and a shared cancellation
token:

```rust
// crates/aether-task-runtime/src/lib.rs:145
pub struct TaskSupervisor {
    cancellation_token: CancellationToken,
    join_set: JoinSet<()>,
    supervised_task_count: usize,
}
```

---

## Consumer Layout Pattern

Application crates build their registries outside this crate. The gateway keeps
all concrete task keys and definitions in its own module:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:47
const TASK_DEFINITIONS: &[TaskDefinition] = &[
    TaskDefinition::new(
        TASK_KEY_PROVIDER_DELETE,
        TaskKind::OnDemand,
        "manual",
        false,
        true,
        RETRY_ONCE,
    ),
];
```

The gateway then uses `TaskSupervisor` from application state startup:

```rust
// apps/aether-gateway/src/state/core.rs:999
pub fn spawn_background_tasks(&self) -> crate::task_runtime::TaskSupervisor {
    let mut supervisor = crate::task_runtime::TaskSupervisor::new();
```

This is the expected split: `aether-task-runtime` supplies primitives; the
gateway decides what tasks exist, how they persist, and what data stores they
use.

---

## Naming Conventions

Use `Task*` names for shared primitives exported by this crate. The current
public names are concise and domain-specific:

- `TaskKind` for the high-level scheduling category.
- `TaskStatus` for lifecycle vocabulary.
- `RetryPolicy` for retry metadata.
- `TaskDefinition` for static task declarations.
- `TaskContext` for per-run data and cancellation.
- `TaskSupervisor` for owning and cancelling spawned workers.

String forms returned from `as_str` are lower snake case and must stay stable
once consumed by storage or APIs. Rust variants remain PascalCase.

Task names passed to `TaskSupervisor::spawn_named` are `&'static str`, not
allocated strings, because the name is held by a spawned task:

```rust
// crates/aether-task-runtime/src/lib.rs:165
pub fn spawn_named<F>(&mut self, task_name: &'static str, future: F)
where
    F: Future<Output = ()> + Send + 'static,
```

---

## When New Files Apply

Keep this crate single-file until a new responsibility is truly independent.
Good reasons to split later:

- A retry executor with its own tests and state machine.
- A task registry abstraction that is reused by more than one application.
- A context payload codec that needs separate serde policy.

Bad reasons to split:

- Moving each enum into its own file.
- Adding a `database.rs` file for app-owned persistence.
- Creating a `utils.rs` bucket for one helper.

The current structure is intentionally compact and should stay that way until
the public surface grows enough that `src/lib.rs` becomes hard to scan.
