# Backend Development Guidelines

These guidelines describe the actual `crates/aether-video-tasks-core/` backend
domain crate. The crate models local video task lifecycle behavior for OpenAI
and Gemini video generation flows. It builds follow-up `ExecutionPlan`s,
projects provider responses into local task state, reads stored tasks through a
trait boundary, and optionally persists local snapshots to a JSON file.

The original generic database guide was removed. This crate does not own
SeaORM, Redis, SQL migrations, or transactions. Use
`persistence-guidelines.md` for the local registry, file store, and
data-contract conversion rules.

## Final Guide Set

| Guide | Purpose | Source anchors |
|-------|---------|----------------|
| [Directory Structure](./directory-structure.md) | Module layout, responsibility split, facade exports, and where new code belongs. | `src/lib.rs`, `src/service.rs`, `src/store_backend.rs` |
| [Error Handling](./error-handling.md) | `Option`, `bool`, `std::io::Result`, and `DataLayerError` propagation contracts. | `src/read_side.rs`, `src/store_backend.rs`, `src/path.rs` |
| [Logging Guidelines](./logging-guidelines.md) | Why this crate has no tracing macros and how callers should log video task integration events. | `src/service.rs`, `src/types.rs`, gateway tracing examples |
| [Persistence Guidelines](./persistence-guidelines.md) | In-memory/file-backed registry behavior, snapshot conversion, and read-side integration. | `src/store.rs`, `src/store_registry.rs`, `src/snapshot.rs` |
| [Quality Guidelines](./quality-guidelines.md) | State-machine invariants, provider split, report-kind updates, tests, and forbidden patterns. | `src/types.rs`, `src/openai.rs`, `src/gemini.rs`, `src/sync.rs` |

## Crate Boundary

`aether-video-tasks-core` belongs to the domain layer. It depends on:

```toml
# crates/aether-video-tasks-core/Cargo.toml:8
[dependencies]
aether-contracts.workspace = true
aether-data-contracts.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
url.workspace = true
uuid.workspace = true
```

That dependency list is part of the design. The crate should not import axum,
SeaORM, Redis, gateway state, scheduler state, or provider transport
implementations.

## Core Public Surface

The public facade exports the service, store traits/backends, read-side bridge,
provider response mappers, route helpers, sync helpers, and domain types.

```rust
// crates/aether-video-tasks-core/src/lib.rs:35
pub use read_side::{read_data_backed_video_task_response, StoredVideoTaskReadSide};
pub use service::VideoTaskService;
pub use store::VideoTaskStore;
pub use store_backend::{FileVideoTaskStore, InMemoryVideoTaskStore};
pub use store_registry::VideoTaskRegistry;
```

New callers should use the facade and avoid internal module paths.

## Important Runtime Modes

The most important design choice is the truth source mode:

```rust
// crates/aether-video-tasks-core/src/types.rs:16
pub enum VideoTaskTruthSourceMode {
    #[default]
    PythonSyncReport,
    RustAuthoritative,
}
```

In `PythonSyncReport` mode, Rust builds inline sync results but does not serve
local authoritative reads. In `RustAuthoritative` mode, local snapshots can
serve reads, generate poll-refresh batches, project provider responses, and
persist local task state.

## Provider Split

OpenAI and Gemini have different IDs, status payloads, content routes, and
cancel/read semantics. Keep provider-specific logic in the provider files:

```rust
// crates/aether-video-tasks-core/src/openai.rs:130
pub fn build_content_stream_action(
    &self,
    query_string: Option<&str>,
    trace_id: &str,
) -> Option<LocalVideoTaskContentAction> {
```

```rust
// crates/aether-video-tasks-core/src/gemini.rs:130
pub fn build_get_follow_up_plan(&self, trace_id: &str) -> Option<ExecutionPlan> {
```

The shared service should choose the right provider seed and delegate.

## Persistence Summary

Local storage is trait-based. The default store is in-memory; an optional file
store persists the whole registry as JSON.

```rust
// crates/aether-video-tasks-core/src/service.rs:25
pub fn new(mode: VideoTaskTruthSourceMode) -> Self {
    Self::with_store(mode, Arc::new(InMemoryVideoTaskStore::default()))
}
```

```rust
// crates/aether-video-tasks-core/src/service.rs:29
pub fn with_file_store(
    mode: VideoTaskTruthSourceMode,
    path: impl Into<PathBuf>,
) -> std::io::Result<Self> {
```

Database-backed reads happen through `StoredVideoTaskReadSide`, implemented by
the gateway data state. This crate only depends on `aether-data-contracts`.

## Testing Summary

Most tests are module-level unit tests beside pure logic. Gateway tests protect
cross-crate behavior, especially facade imports and file-store round trips.

Use existing tests as shape references:

- `crates/aether-video-tasks-core/src/path.rs` tests route extractors and lookup
  key resolution.
- `crates/aether-video-tasks-core/src/sync.rs` tests finalize response and
  plan-building behavior.
- `crates/aether-video-tasks-core/src/transport.rs` tests variant parsing and
  Gemini video URL extraction.
- `apps/aether-gateway/src/video_tasks/tests/plans.rs` tests poll-refresh plans
  and file-backed store persistence.

## When Editing This Crate

Use this checklist before changing code that consumes these specs:

- Check whether the change is provider-specific or shared.
- Keep unsupported paths as `None` rather than internal errors.
- Preserve the `RustAuthoritative` gate for local reads and projections.
- Keep local snapshot updates in `VideoTaskRegistry`.
- Strip stale entity headers when building body-less follow-up requests.
- Update the related unit tests in the same file.
- Update gateway integration tests if facade exports or persistence behavior
  change.

## Removed Guide

`database-guidelines.md` is intentionally absent from the final file set. The
crate interacts with persisted video tasks through contract types and traits,
not by owning queries or transactions. If future work adds direct database
access here, that should be treated as an architectural change and documented by
adding a new guide with concrete source examples.
