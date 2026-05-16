# Error Handling

This crate does not define a custom error enum. Error handling is intentionally
split by boundary:

- `Option<T>` means the route, report kind, provider body, or local state does
  not support a video task operation.
- `bool` means a registry projection or mutation did not apply.
- `std::io::Result<T>` is used only while opening or loading the file-backed
  local store.
- `Result<T, DataLayerError>` is used only by the read-side trait that calls
  into the external data layer.

That keeps the domain crate quiet and lets the gateway decide how to turn data
layer failures into API errors.

## Primary Error Surfaces

`VideoTaskService::with_file_store` exposes initialization errors because the
caller must know when the local registry file cannot be loaded.

```rust
// crates/aether-video-tasks-core/src/service.rs:29
pub fn with_file_store(
    mode: VideoTaskTruthSourceMode,
    path: impl Into<PathBuf>,
) -> std::io::Result<Self> {
    Ok(Self::with_store(
        mode,
        Arc::new(FileVideoTaskStore::new(path)?),
    ))
}
```

`read_side.rs` exposes data-layer failures because the storage backend lives
outside this crate.

```rust
// crates/aether-video-tasks-core/src/read_side.rs:18
pub async fn read_data_backed_video_task_response(
    state: &impl StoredVideoTaskReadSide,
    route_family: Option<&str>,
    request_path: &str,
) -> Result<Option<LocalVideoTaskReadResponse>, DataLayerError> {
```

The gateway maps that error into its own application error at the app boundary:

```rust
// apps/aether-gateway/src/state/video.rs:15
self.data
    .read_video_task_response(route_family, request_path)
    .await
    .map_err(|err| GatewayError::Internal(err.to_string()))
```

## Option Is The Unsupported-Path Contract

Use `None` for unsupported report kinds, invalid route shapes, missing context,
or provider payloads that cannot produce a local task. Do not turn these into
internal errors unless the caller boundary needs to do so.

```rust
// crates/aether-video-tasks-core/src/service.rs:88
pub fn apply_finalize_mutation(&self, request_path: &str, report_kind: &str) {
    let Some(mutation) = resolve_local_video_registry_mutation(
        self.truth_source_mode,
        request_path,
        report_kind,
    ) else {
        return;
    };
    self.store.apply_mutation(mutation);
}
```

The path helpers rely on `Option` and `?` to reject malformed routes without
panicking:

```rust
// crates/aether-video-tasks-core/src/path.rs:10
pub fn extract_openai_task_id_from_path(path: &str) -> Option<&str> {
    let suffix = path.strip_prefix("/v1/videos/")?;
    if suffix.is_empty()
        || suffix.contains('/')
        || suffix.ends_with(":cancel")
        || suffix.ends_with(":delete")
    {
        return None;
    }
    Some(suffix)
}
```

## Result Is For Real External Failures

The file store maps JSON corruption into `std::io::ErrorKind::InvalidData`.
Missing or empty files are not errors; they mean an empty registry.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:87
fn load_registry(path: &Path) -> std::io::Result<VideoTaskRegistry> {
    if !path.exists() {
        return Ok(VideoTaskRegistry::default());
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(VideoTaskRegistry::default());
    }
    serde_json::from_slice(&bytes)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}
```

File writes are currently best-effort through the `VideoTaskStore` trait. The
trait has `insert` and `apply_mutation` methods that cannot return errors, so
the file-backed implementation reports durable mutation success only through
methods that already return `bool`, such as projections.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:111
fn mutate_registry(&self, mutator: impl FnOnce(&mut VideoTaskRegistry) -> bool) -> bool {
    let Ok(mut registry) = self.registry.lock() else {
        return false;
    };
    if !mutator(&mut registry) {
        return false;
    }
    self.persist_registry(&registry).is_ok()
}
```

If new callers require write failure visibility for `insert`, change the trait
contract instead of adding side-channel logging or panics.

## API Error Responses Are Data

This crate does not throw API errors. It returns `LocalVideoTaskReadResponse`
with the status code and JSON body that the gateway can send.

```rust
// crates/aether-video-tasks-core/src/snapshot.rs:100
pub fn read_response(&self) -> LocalVideoTaskReadResponse {
    match self {
        Self::OpenAi(seed) => match seed.status {
            LocalVideoTaskStatus::Cancelled => LocalVideoTaskReadResponse {
                status_code: 404,
                body_json: json!({"detail": "Video task was cancelled"}),
            },
```

OpenAI content streaming uses the same pattern: non-ready content is an
immediate response, not an exception.

```rust
// crates/aether-video-tasks-core/src/openai.rs:135
match self.status {
    LocalVideoTaskStatus::Submitted
    | LocalVideoTaskStatus::Queued
    | LocalVideoTaskStatus::Processing => {
        return Some(LocalVideoTaskContentAction::Immediate {
            status_code: 202,
            body_json: json!({
                "detail": format!(
                    "Video is still processing (status: {})",
                    map_openai_task_status(self.status)
                )
            }),
        });
    }
```

## Conversion Patterns

Use `let Some(...) = ... else { return ...; }` when a branch should stop
cleanly. Use `?` for short `Option` chains. Use `map_err` only where an external
error type needs to cross a boundary.

```rust
// crates/aether-video-tasks-core/src/read_side.rs:38
let Some(task) = state.find_stored_video_task(lookup).await? else {
    return Ok(None);
};
```

```rust
// crates/aether-video-tasks-core/src/transport_domain.rs:80
pub fn from_stored_task(task: &StoredVideoTask) -> Option<Self> {
    let client_api_format = non_empty_owned(task.client_api_format.as_ref())
        .or_else(|| non_empty_owned(task.provider_api_format.as_ref()))?;
```

## DON'T Patterns

DON'T add `anyhow` or `thiserror` to this crate for unsupported routes. The
current API already uses `None` to mean "not a video task shape".

DON'T unwrap provider JSON fields:

```rust
// DON'T
let status = provider_body["status"].as_str().unwrap();
```

Follow the existing defensive extraction style:

```rust
// crates/aether-video-tasks-core/src/openai.rs:88
let raw_status = provider_body
    .get("status")
    .and_then(Value::as_str)
    .map(str::trim)
    .unwrap_or_default();
```

DON'T collapse data-layer failures into `Ok(None)` in `read_side.rs`. A missing
task is `Ok(None)`, but a failed repository call must propagate as
`DataLayerError`.

DON'T log and swallow initialization failures from `with_file_store`. Return
them to the caller so startup configuration can fail visibly.
