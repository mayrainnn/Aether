# Directory Structure

`crates/aether-video-tasks-core/` is a domain crate for video task lifecycle
handling. It is not a transport crate and it is not a database crate. Its job
is to normalize provider-specific video task state, build follow-up plans, and
project read responses from either stored tasks or local snapshots.

## Actual Layout

```text
crates/aether-video-tasks-core/
├── Cargo.toml
└── src/
    ├── body.rs
    ├── follow_up.rs
    ├── gemini.rs
    ├── lib.rs
    ├── openai.rs
    ├── path.rs
    ├── read_side.rs
    ├── service.rs
    ├── snapshot.rs
    ├── store.rs
    ├── store_backend.rs
    ├── store_registry.rs
    ├── sync.rs
    ├── transport.rs
    ├── transport_domain.rs
    ├── types.rs
    └── util.rs
```

ABCoder resolved exactly those source files for the `aether-video-tasks-core`
AST. GitNexus resources show the repo as a large multi-layer workspace, but
this crate stays small and focused inside the domain layer.

## Module Roles

`src/lib.rs` is the public facade. It keeps modules private and re-exports only
the stable surface:

```rust
// crates/aether-video-tasks-core/src/lib.rs:1
mod body;
mod follow_up;
mod gemini;
mod openai;
mod path;
mod read_side;
mod service;
mod snapshot;
mod store;
mod store_backend;
mod store_registry;
mod sync;
mod transport;
mod transport_domain;
mod types;
mod util;
```

```rust
// crates/aether-video-tasks-core/src/lib.rs:35
pub use read_side::{read_data_backed_video_task_response, StoredVideoTaskReadSide};
pub use service::VideoTaskService;
pub use store::VideoTaskStore;
pub use store_backend::{FileVideoTaskStore, InMemoryVideoTaskStore};
pub use store_registry::VideoTaskRegistry;
```

`src/types.rs` holds the core domain data model: status enums, snapshot
containers, transport metadata, persistence metadata, and projection targets.
These types are the shape that everything else moves around.

```rust
// crates/aether-video-tasks-core/src/types.rs:60
pub enum LocalVideoTaskStatus {
    Submitted,
    Queued,
    Processing,
    Completed,
    Failed,
    Cancelled,
    Expired,
    Deleted,
}
```

`src/service.rs` is the orchestration layer. It decides whether the crate is in
`PythonSyncReport` or `RustAuthoritative` mode, chooses the store backend, and
delegates to provider-specific helpers.

```rust
// crates/aether-video-tasks-core/src/service.rs:24
impl VideoTaskService {
    pub fn new(mode: VideoTaskTruthSourceMode) -> Self {
        Self::with_store(mode, Arc::new(InMemoryVideoTaskStore::default()))
    }
```

`src/store.rs` defines the store trait. `src/store_backend.rs` implements it for
an in-memory registry and a JSON file-backed registry.

```rust
// crates/aether-video-tasks-core/src/store.rs:8
pub trait VideoTaskStore: std::fmt::Debug + Send + Sync {
    fn insert(&self, snapshot: LocalVideoTaskSnapshot);
    fn read_openai(&self, task_id: &str) -> Option<LocalVideoTaskReadResponse>;
```

`src/store_registry.rs` contains the actual `BTreeMap` indexing logic. That is
where snapshots are inserted, cloned, listed, mutated, and projected.

`src/snapshot.rs` converts `StoredVideoTask` rows into local snapshots and then
projects those snapshots back into client responses.

`src/openai.rs` and `src/gemini.rs` own provider-specific behavior. They are
the only places that should know how OpenAI and Gemini payloads differ.

`src/path.rs` owns route parsing and report-kind lookup. Keep string matching
here instead of scattering `strip_prefix`/`strip_suffix` logic throughout the
crate.

`src/body.rs` owns tiny JSON/context readers. `src/util.rs` stays intentionally
minimal.

`src/read_side.rs` bridges the domain crate to the data layer through the
`StoredVideoTaskReadSide` trait. `src/transport_domain.rs` turns execution plans
into transport and persistence metadata.

`src/sync.rs` builds finalize plans, success plans, and local read responses for
sync flows. `src/follow_up.rs` builds the report context used by those plans.
`src/transport.rs` provides content-variant parsing and provider status mapping.

## Organization Pattern

The crate follows a split between:

1. Core data types in `types.rs`
2. Pure transformation helpers in `body.rs`, `path.rs`, `transport.rs`, and
   `transport_domain.rs`
3. Provider-specific adapters in `openai.rs` and `gemini.rs`
4. Orchestration in `service.rs`
5. Persistence adapters in `store.rs`, `store_backend.rs`, and
   `store_registry.rs`

That pattern is deliberate. The service layer does not know how JSON is stored.
The store layer does not know how a Gemini operation name becomes a download
URL. The provider modules do not know whether the caller uses memory or file
backing.

## Consumer Boundary

The gateway wraps this crate rather than replacing it:

```rust
// apps/aether-gateway/src/video_tasks/service.rs:9
pub(crate) struct VideoTaskService(aether_video_tasks_core::VideoTaskService);
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

That tells you the intended boundary: this crate stays reusable, while the
gateway adapts it to app state and data access.

## Naming Conventions

Use `LocalVideoTask*` for domain-local payloads, `OpenAiVideoTaskSeed` and
`GeminiVideoTaskSeed` for provider-specific seeds, and `VideoTask*` for the
service/store abstractions. File names are lower snake case and each file owns a
single concept.

Report kinds and signatures are snake_case strings, such as
`openai_video_remix_sync_finalize` and `gemini:video`. Keep those strings in the
matching resolver modules so the string vocabulary stays centralized.

## Adding New Files

Add a new file only when the current split cannot stay focused. Good examples
would be another provider module or a new pure conversion helper if the
transformation logic becomes too dense.

Do not add `handlers.rs`, `state.rs`, or `database.rs` here. Those names imply a
different layer.

## DON'T Patterns

DON'T expose the internal modules just to shorten imports:

```rust
// DON'T: crates/aether-video-tasks-core/src/lib.rs
pub mod service;
pub mod store_backend;
```

Keep the facade private and re-export the stable API instead.

DON'T collapse provider-specific behavior into `service.rs`. The service should
coordinate, not interpret every payload shape.

DON'T move JSON file persistence into the gateway. This crate already owns the
store boundary and the snapshot conversion path.
