# Persistence Guidelines

This crate does not own a SeaORM connection, Redis client, migration, or
transaction boundary. The generic `database-guidelines.md` file was removed because
it does not match the crate. Persistence here means:

- an in-memory `VideoTaskRegistry`
- an optional JSON file-backed store for local task snapshots
- conversion to and from `aether-data-contracts` video task records
- a read-side trait that lets application state provide stored tasks

Database queries and transactions belong in `aether-data` and gateway state.

## Store Trait Boundary

All local persistence flows go through `VideoTaskStore`. This keeps
`VideoTaskService` independent from the concrete backend.

```rust
// crates/aether-video-tasks-core/src/store.rs:8
pub trait VideoTaskStore: std::fmt::Debug + Send + Sync {
    fn insert(&self, snapshot: LocalVideoTaskSnapshot);
    fn read_openai(&self, task_id: &str) -> Option<LocalVideoTaskReadResponse>;
    fn read_gemini(&self, short_id: &str) -> Option<LocalVideoTaskReadResponse>;
    fn clone_openai(&self, task_id: &str) -> Option<OpenAiVideoTaskSeed>;
    fn clone_gemini(&self, short_id: &str) -> Option<GeminiVideoTaskSeed>;
```

Do not add direct database calls to `VideoTaskService`. Add a trait method only
if the operation is truly part of the local video task abstraction.

## Registry Shape

`VideoTaskRegistry` indexes OpenAI and Gemini snapshots separately. This avoids
provider-id ambiguity and keeps lookup functions explicit.

```rust
// crates/aether-video-tasks-core/src/store_registry.rs:11
pub struct VideoTaskRegistry {
    openai: BTreeMap<String, LocalVideoTaskSnapshot>,
    gemini: BTreeMap<String, LocalVideoTaskSnapshot>,
}
```

The registry is also the only place that should decide how to list active tasks:

```rust
// crates/aether-video-tasks-core/src/store_registry.rs:55
pub fn list_active_snapshots(&self, limit: usize) -> Vec<LocalVideoTaskSnapshot> {
    self.openai
        .values()
        .chain(self.gemini.values())
        .filter(|snapshot| snapshot.is_active_for_refresh())
        .take(limit)
        .cloned()
        .collect()
}
```

DON'T duplicate that active-status logic in a poller or provider module.

## In-Memory Store

`InMemoryVideoTaskStore` is the default backend. It is protected by a
`Mutex<VideoTaskRegistry>` and treats a poisoned lock as a miss or failed
projection.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:11
pub struct InMemoryVideoTaskStore {
    registry: Mutex<VideoTaskRegistry>,
}
```

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:49
fn list_active_snapshots(&self, limit: usize) -> Vec<LocalVideoTaskSnapshot> {
    let Ok(registry) = self.registry.lock() else {
        return Vec::new();
    };
    registry.list_active_snapshots(limit)
}
```

This is not a durable store. Use it for tests and ephemeral runtime state.

## File-Backed Store

`FileVideoTaskStore` loads the registry at startup and persists the whole
registry on mutation. Missing and empty files are valid empty registries.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:77
impl FileVideoTaskStore {
    pub fn new(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let registry = Self::load_registry(&path)?;
        Ok(Self {
            path,
            registry: Mutex::new(registry),
        })
    }
```

The write path serializes pretty JSON to a temporary file and then renames it
over the target path.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:99
fn persist_registry(&self, registry: &VideoTaskRegistry) -> std::io::Result<()> {
    if let Some(parent) = self.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(registry)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let temp_path = self.path.with_extension("tmp");
    std::fs::write(&temp_path, bytes)?;
    std::fs::rename(temp_path, &self.path)?;
    Ok(())
}
```

DON'T write directly to the final file. Keep the temp-file-then-rename pattern
so interrupted writes are less likely to leave a partial JSON file.

## Projection Writes

Provider poll responses update existing snapshots through projection methods.
The projection returns `false` if the task is missing, the lock cannot be
acquired, or the file store cannot persist the changed registry.

```rust
// crates/aether-video-tasks-core/src/store_backend.rs:164
fn project_openai(&self, task_id: &str, provider_body: &Map<String, Value>) -> bool {
    self.mutate_registry(|registry| registry.project_openai(task_id, provider_body))
}
```

```rust
// crates/aether-video-tasks-core/src/store_registry.rs:85
pub fn project_openai(&mut self, task_id: &str, provider_body: &Map<String, Value>) -> bool {
    let Some(LocalVideoTaskSnapshot::OpenAi(seed)) = self.openai.get_mut(task_id) else {
        return false;
    };
    seed.apply_provider_body(provider_body);
    true
}
```

Do not write provider JSON directly into the registry. Always let the provider
seed map the body into the local status model.

## Data-Contract Conversion

The crate converts local snapshots into `UpsertVideoTask` records, but it does
not execute the upsert. The gateway owns the actual data write.

```rust
// crates/aether-video-tasks-core/src/snapshot.rs:10
impl LocalVideoTaskSnapshot {
    pub fn to_upsert_record(&self) -> UpsertVideoTask {
        match self {
            Self::OpenAi(seed) => seed.to_upsert_record(),
            Self::Gemini(seed) => seed.to_upsert_record(),
        }
    }
```

OpenAI and Gemini seeds store a serialized snapshot in request metadata so a
later hydration can recover the exact local state:

```rust
// crates/aether-video-tasks-core/src/openai.rs:623
request_metadata: Some(json!({
    "rust_owner": "async_task",
    "rust_local_snapshot": LocalVideoTaskSnapshot::OpenAi(self.clone()),
})),
```

```rust
// crates/aether-video-tasks-core/src/gemini.rs:345
request_metadata: Some(json!({
    "rust_owner": "async_task",
    "rust_local_snapshot": LocalVideoTaskSnapshot::Gemini(self.clone()),
})),
```

Hydration first tries to recover that embedded snapshot:

```rust
// crates/aether-video-tasks-core/src/snapshot.rs:18
pub fn from_stored_task(task: &StoredVideoTask) -> Option<Self> {
    task.request_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("rust_local_snapshot"))
        .cloned()
        .and_then(|value| serde_json::from_value::<LocalVideoTaskSnapshot>(value).ok())
}
```

When that metadata is unavailable, callers can reconstruct from stored task
fields plus a `LocalVideoTaskTransport` using `from_stored_task_with_transport`.

## Read Side Integration

The only database-facing abstraction in this crate is a reader trait. The
gateway implements it against its data state.

```rust
// crates/aether-video-tasks-core/src/read_side.rs:10
#[async_trait]
pub trait StoredVideoTaskReadSide: Send + Sync {
    async fn find_stored_video_task(
        &self,
        key: VideoTaskLookupKey<'_>,
    ) -> Result<Option<StoredVideoTask>, DataLayerError>;
}
```

```rust
// apps/aether-gateway/src/data/state/integrations.rs:101
impl StoredVideoTaskReadSide for GatewayDataState {
    async fn find_stored_video_task(
        &self,
        key: VideoTaskLookupKey<'_>,
    ) -> Result<Option<StoredVideoTask>, DataLayerError> {
        GatewayDataState::find_video_task(self, key).await
    }
}
```

Keep new database filters and batch queries out of this crate unless they can be
expressed as data-contract traits without importing the data implementation.

## Testing Persistence

Gateway tests already exercise the file store persistence contract:

```rust
// apps/aether-gateway/src/video_tasks/tests/plans.rs:366
#[test]
fn file_video_task_store_persists_snapshots_across_service_rebuilds() {
    let store_path =
        std::env::temp_dir().join(format!("aether-video-task-store-{}.json", Uuid::new_v4()));
```

When changing file persistence, add or update a round-trip test that creates a
store, records a snapshot, drops the service, reopens the service, and reads the
same task back.

## DON'T Patterns

DON'T add SeaORM entities, migrations, or connection pools in this crate.

DON'T treat an absent file as a startup error. `load_registry` intentionally
returns an empty registry for missing and empty files.

DON'T bypass `VideoTaskRegistry` by making its maps public. The separate OpenAI
and Gemini maps are implementation details.

DON'T store raw authorization headers in logs or in database-facing fields
outside `LocalVideoTaskTransport`; headers are only for rebuilding follow-up
execution plans.
