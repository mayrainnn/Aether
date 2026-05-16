# Backend Development Guidelines

> Project-specific backend guidance for `crates/aether-task-runtime`.

---

## Scope

These specs cover the Rust crate at:

```text
crates/aether-task-runtime/
```

The crate is a domain-layer primitive crate for Aether background-task metadata,
cancellation, and supervision. It is not the gateway's concrete task registry,
not the database implementation for background task runs, and not an HTTP API
layer.

The current public surface is in one file:

```rust
// crates/aether-task-runtime/src/lib.rs:11
pub enum TaskKind {
    Scheduled,
    Daemon,
    OnDemand,
    FireAndForget,
}
```

Use these guidelines when changing `TaskKind`, `TaskStatus`, `RetryPolicy`,
`TaskDefinition`, `TaskContext`, or `TaskSupervisor`.

---

## Evidence Sources

These guidelines were filled from the actual codebase, not from the template
text.

GitNexus evidence:

- `gitnexus://repo/Aether/context` reports the Aether index as available with
  3,140 files, 83,229 symbols, and 300 execution flows.
- `gitnexus://repo/Aether/clusters` includes the broader runtime cluster used
  for background worker context.
- `gitnexus://repo/Aether/cluster/Runtime` shows gateway maintenance/runtime
  worker symbols that consume task runtime concepts.

ABCoder evidence:

- The current Codex tool surface did not expose callable ABCoder MCP functions,
  but the local ABCoder UniAST artifact exists at
  `/Users/mayrain/abcoder-asts/aether-task-runtime-ast.json`.
- That artifact reports one Rust module named `aether-task-runtime`, one
  package, one file (`src/lib.rs`), six public types, and fifteen public
  methods.
- The AST lists imports from `aether_runtime::task::spawn_named`,
  `tokio::task::{JoinHandle, JoinSet}`, `tokio_util::sync::CancellationToken`,
  and `tracing::warn`.

Direct source evidence:

- `crates/aether-task-runtime/src/lib.rs`
- `crates/aether-task-runtime/Cargo.toml`
- Gateway consumers in `apps/aether-gateway/src/task_runtime/mod.rs`
- Gateway startup integration in `apps/aether-gateway/src/state/core.rs`

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Single-file crate layout, public facade, consumer boundary, and when new files are justified. | Current |
| [Error Handling](./error-handling.md) | No crate error enum, join failure logging, cancellation semantics, and caller-owned business errors. | Current |
| [Quality Guidelines](./quality-guidelines.md) | Public type shape, derive policy, visibility rules, async bounds, dependency discipline, and test expectations. | Current |
| [Logging Guidelines](./logging-guidelines.md) | `tracing` usage, warn-level join failures, structured fields, and payload redaction rules. | Current |

`database-guidelines.md` was deleted because this crate has no database code.
It imports no SeaORM, SQLx, Redis, or repository traits. Persistence is performed
by application/data crates, for example the gateway's background task module:

```rust
// apps/aether-gateway/src/task_runtime/mod.rs:316
pub(crate) async fn upsert_run_with_logging(
    app: &AppState,
    run: UpsertBackgroundTaskRun,
) -> Option<StoredBackgroundTaskRun> {
```

That pattern should stay outside `aether-task-runtime`.

---

## Crate Snapshot

Current dependency boundary:

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

Current type list from source and ABCoder:

- `TaskKind`
- `TaskStatus`
- `RetryPolicy`
- `TaskDefinition`
- `TaskContext<TPayload = serde_json::Value>`
- `TaskSupervisor`

Current supervisor methods:

- `new`
- `cancellation_token`
- `spawn_named`
- `supervise_handle`
- `is_empty`
- `task_count`
- `cancel`
- `shutdown`

Current context methods:

- `new`
- `run_id`
- `task_key`
- `payload`
- `cancellation_token`
- `is_cancelled`
- `cancelled`

---

## Non-Goals

Do not use this crate for:

- concrete gateway task keys;
- database row models;
- background task SQL queries;
- Redis cancellation flags;
- HTTP response mapping;
- provider-specific task state;
- billing, quota, or usage task business logic.

The gateway owns concrete task definitions:

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

The crate owns the reusable primitives used by that registry.

---

## Change Policy

Before modifying this crate:

1. Check whether the change belongs in the gateway or data layer instead.
2. Preserve stable string forms returned by `TaskKind::as_str` and
   `TaskStatus::as_str`.
3. Keep `TaskDefinition` static and cheap to copy.
4. Keep `TaskSupervisor` cancellation-aware.
5. Avoid new dependencies unless the public primitive layer truly needs them.
6. Add tests for semantic changes, especially supervisor shutdown behavior.

Example supervisor shutdown contract:

```rust
// crates/aether-task-runtime/src/lib.rs:217
pub async fn shutdown(mut self) {
    self.cancel();
    while self.join_set.join_next().await.is_some() {}
}
```

If this behavior changes, update both the code tests and these specs.

---

## Verification Expectation

For documentation-only spec changes, verify:

- no template placeholders remain;
- no HTML comments remain;
- every listed file exists;
- deleted files are removed from this index;
- examples cite real file paths and line numbers.

For code changes in the crate, also run:

```bash
cargo test -p aether-task-runtime
```

For changes affecting `TaskSupervisor`, include at least one downstream
verification path through the gateway startup integration:

```rust
// apps/aether-gateway/src/state/core.rs:999
pub fn spawn_background_tasks(&self) -> crate::task_runtime::TaskSupervisor {
    let mut supervisor = crate::task_runtime::TaskSupervisor::new();
```
