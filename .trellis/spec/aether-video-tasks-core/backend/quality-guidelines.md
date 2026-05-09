# Quality Guidelines

`aether-video-tasks-core` is a state-machine crate. Quality here means preserving
provider-specific task semantics, avoiding hidden side effects, and keeping the
domain boundary independent from gateway state and database implementation.

## Keep The Public API Facade-Based

All modules are private and the stable API is re-exported from `lib.rs`. New
callers should import from `aether_video_tasks_core::{...}` rather than module
paths.

```rust
// crates/aether-video-tasks-core/src/lib.rs:35
pub use read_side::{read_data_backed_video_task_response, StoredVideoTaskReadSide};
pub use service::VideoTaskService;
pub use store::VideoTaskStore;
pub use store_backend::{FileVideoTaskStore, InMemoryVideoTaskStore};
pub use store_registry::VideoTaskRegistry;
```

DON'T make modules public for convenience:

```rust
// DON'T
pub mod openai;
pub mod gemini;
pub mod store_backend;
```

That would freeze implementation details as public contract.

## Preserve The Truth-Source Gate

`VideoTaskTruthSourceMode` is the top-level behavior switch. Rust-authored
read/projection paths must return `None`, `false`, or an empty batch unless the
service is in `RustAuthoritative` mode.

```rust
// crates/aether-video-tasks-core/src/service.rs:99
pub fn read_response(
    &self,
    route_family: Option<&str>,
    request_path: &str,
) -> Option<LocalVideoTaskReadResponse> {
    if self.truth_source_mode != VideoTaskTruthSourceMode::RustAuthoritative {
        return None;
    }
```

```rust
// crates/aether-video-tasks-core/src/service.rs:218
pub fn prepare_poll_refresh_batch(
    &self,
    limit: usize,
    trace_prefix: &str,
) -> Vec<LocalVideoTaskReadRefreshPlan> {
    if self.truth_source_mode != VideoTaskTruthSourceMode::RustAuthoritative || limit == 0 {
        return Vec::new();
    }
```

When adding a new read or follow-up path, decide whether it is safe in
`PythonSyncReport` mode. Most local projection paths are not.

## Use Domain Types Instead Of Loose JSON

Provider JSON is accepted only at the edge of mapping functions. Store and
service code should move `LocalVideoTaskSnapshot`, `LocalVideoTaskSeed`,
`LocalVideoTaskStatus`, and `LocalVideoTaskProjectionTarget` around.

```rust
// crates/aether-video-tasks-core/src/types.rs:36
pub struct LocalVideoTaskReadRefreshPlan {
    pub plan: ExecutionPlan,
    pub projection_target: LocalVideoTaskProjectionTarget,
}
```

```rust
// crates/aether-video-tasks-core/src/types.rs:48
pub enum LocalVideoTaskProjectionTarget {
    OpenAi { task_id: String },
    Gemini { short_id: String },
}
```

DON'T pass provider names and arbitrary ids as separate strings when an enum
variant already captures the valid combinations.

## Keep Provider Logic In Provider Files

OpenAI response/body/status logic belongs in `openai.rs`. Gemini operation and
metadata logic belongs in `gemini.rs`. The shared service should dispatch to
those implementations.

```rust
// crates/aether-video-tasks-core/src/service.rs:292
pub fn prepare_follow_up_sync_plan(
    &self,
    plan_kind: &str,
    request_path: &str,
    body_json: Option<&Value>,
    fallback_user_id: Option<&str>,
    fallback_api_key_id: Option<&str>,
    trace_id: &str,
) -> Option<LocalVideoTaskFollowUpPlan> {
    match plan_kind {
        "openai_video_remix_sync" => {
```

The provider files own the detailed plan assembly:

```rust
// crates/aether-video-tasks-core/src/gemini.rs:130
pub fn build_get_follow_up_plan(&self, trace_id: &str) -> Option<ExecutionPlan> {
    if !matches!(
        self.status,
        LocalVideoTaskStatus::Submitted
            | LocalVideoTaskStatus::Queued
            | LocalVideoTaskStatus::Processing
    ) {
        return None;
    }
```

## Use Deterministic Containers

The crate uses `BTreeMap` for registries and stored headers. Preserve that
unless there is a measured reason not to. Stable ordering keeps JSON snapshots
and tests predictable.

```rust
// crates/aether-video-tasks-core/src/store_registry.rs:11
pub struct VideoTaskRegistry {
    openai: BTreeMap<String, LocalVideoTaskSnapshot>,
    gemini: BTreeMap<String, LocalVideoTaskSnapshot>,
}
```

```rust
// crates/aether-video-tasks-core/src/types.rs:92
pub struct LocalVideoTaskTransport {
    pub upstream_base_url: String,
    pub provider_name: Option<String>,
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: String,
    pub headers: BTreeMap<String, String>,
```

## Route And Report Strings Stay Centralized

Route parsing is in `path.rs`; report-kind conversions are split between
`path.rs`, `sync.rs`, and provider modules. When adding a report kind, update
all affected resolver functions and tests in one change.

```rust
// crates/aether-video-tasks-core/src/path.rs:170
pub fn resolve_local_video_registry_mutation(
    truth_source_mode: VideoTaskTruthSourceMode,
    request_path: &str,
    report_kind: &str,
) -> Option<LocalVideoTaskRegistryMutation> {
```

```rust
// crates/aether-video-tasks-core/src/sync.rs:323
pub fn resolve_local_sync_success_background_report_kind(
    report_kind: &str,
) -> Option<&'static str> {
    match report_kind {
        "openai_video_delete_sync_finalize" => Some("openai_video_delete_sync_success"),
```

DON'T add route extraction with broad substring checks:

```rust
// DON'T
if request_path.contains("operations") {
    // ambiguous across Gemini read and cancel routes
}
```

Use the existing extractor functions that reject suffixes and nested segments.

## Header Handling Rules

When constructing follow-up `GET` and `DELETE` plans, strip entity headers that
belong to the original create/remix request. When constructing a body-bearing
follow-up plan, set a content type and strip `content-length`.

```rust
// crates/aether-video-tasks-core/src/openai.rs:375
let mut headers = self.transport.headers.clone();
headers.remove("content-type");
headers.remove("content-length");
```

```rust
// crates/aether-video-tasks-core/src/openai.rs:512
let mut headers = self.transport.headers.clone();
headers.remove("content-length");
let content_type = self
    .transport
    .content_type
    .clone()
    .unwrap_or_else(|| "application/json".to_string());
```

DON'T forward a stale `content-length` header after changing method, body, or
streaming behavior.

## Testing Requirements

Unit tests live beside the pure logic they protect. Add tests in the same file
for parsing, status mapping, report-kind resolution, and response shape changes.

```rust
// crates/aether-video-tasks-core/src/path.rs:287
#[test]
fn resolves_video_task_read_lookup_key_for_supported_read_paths() {
    assert_eq!(
        resolve_video_task_read_lookup_key(Some("openai"), "/v1/videos/task_123"),
        Some(VideoTaskLookupKey::Id("task_123"))
    );
```

```rust
// crates/aether-video-tasks-core/src/sync.rs:431
#[test]
fn builds_internal_finalize_video_plan_for_supported_video_signatures() {
    let openai_plan = build_internal_finalize_video_plan(
        "trace-openai-video",
        "openai:video",
```

Cross-crate behavior is covered by gateway tests. Keep those tests when changing
the public facade or persistence contract:

```rust
// apps/aether-gateway/src/video_tasks/tests/plans.rs:366
#[test]
fn file_video_task_store_persists_snapshots_across_service_rebuilds() {
    let store_path =
        std::env::temp_dir().join(format!("aether-video-task-store-{}.json", Uuid::new_v4()));
```

## Code Review Checklist

Check these items for every change:

- Does the change keep source mode behavior explicit?
- Are provider-specific payload decisions still in `openai.rs` or `gemini.rs`?
- Are new report kinds handled in `path.rs`, `sync.rs`, provider code, and
  tests?
- Are new statuses mapped in client response builders, database status
  conversion, and provider projection code?
- Does any new follow-up plan preserve proxy, transport profile, timeout, and
  key metadata from `LocalVideoTaskTransport`?
- Does any body-bearing plan remove stale `content-length`?
- Does the change avoid adding direct SeaORM, Redis, axum, or gateway state
  dependencies?

## Forbidden Patterns

DON'T mutate the registry maps directly from store backends. All mutation goes
through `VideoTaskRegistry` methods.

DON'T use panics for invalid provider payloads. Return `None`, `false`, or an
immediate response.

DON'T invent new public wrapper types in the gateway when the core facade
already exports the needed type.

DON'T add dependencies to `Cargo.toml` unless the new dependency is required by
domain logic and cannot be implemented with existing `serde_json`, `uuid`,
`url`, `async-trait`, or Aether contract crates.
