# Directory Structure

> Backend organization rules for the `aether-data-contracts` crate.

---

## Scope

`aether-data-contracts` is the shared data-layer contract crate under
`crates/aether-data-contracts/`. It owns repository traits, query DTOs,
stored-domain read models, write inputs, and conversion/validation helpers
used by concrete storage crates such as `aether-data`.

This crate is not a database implementation. Keep SQL, connection pools,
transactions, migrations, Redis clients, and SeaORM/sqlx code out of this
crate. The evidence is visible in `crates/aether-data-contracts/Cargo.toml:9`:
the only dependencies are `aether-ai-formats`, `async-trait`, `chrono`,
`serde`, `serde_json`, and `thiserror`; there is no `sqlx`, `sea-orm`,
`redis`, `tokio`, or `tracing` dependency.

GitNexus indexes Aether as a large multi-crate Rust repository with 83,229
symbols and 300 execution flows. In that graph, this crate sits at the data
contract boundary: higher layers depend on its public traits and models, while
the concrete read/write behavior lives in implementation crates.

---

## Actual Layout

```text
crates/aether-data-contracts/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    └── repository/
        ├── mod.rs
        ├── background_tasks/
        │   ├── mod.rs
        │   └── types.rs
        ├── billing/
        │   ├── mod.rs
        │   └── types.rs
        ├── candidate_selection/
        │   ├── mod.rs
        │   └── types.rs
        ├── candidates/
        │   ├── mod.rs
        │   └── types.rs
        ├── global_models/
        │   ├── mod.rs
        │   └── types.rs
        ├── provider_catalog/
        │   ├── mod.rs
        │   └── types.rs
        ├── quota/
        │   ├── mod.rs
        │   └── types.rs
        ├── settlement/
        │   ├── mod.rs
        │   └── types.rs
        ├── usage/
        │   ├── mod.rs
        │   └── types.rs
        └── video_tasks/
            ├── mod.rs
            └── types.rs
```

The crate root is intentionally tiny. `src/lib.rs` declares private `error`,
public `repository`, and re-exports the shared error type at
`crates/aether-data-contracts/src/lib.rs:1`.

```rust
mod error;
pub mod repository;

pub use error::DataLayerError;
```

`src/repository/mod.rs` is the domain namespace index. It declares ten public
submodules at `crates/aether-data-contracts/src/repository/mod.rs:1`:
`background_tasks`, `billing`, `candidate_selection`, `candidates`,
`global_models`, `provider_catalog`, `quota`, `settlement`, `usage`, and
`video_tasks`.

---

## Module Pattern

Every repository domain follows the same two-file shape:

```rust
mod types;

pub use types::{
    BackgroundTaskKind, BackgroundTaskListQuery, BackgroundTaskReadRepository,
    BackgroundTaskRepository, BackgroundTaskStatus, BackgroundTaskSummary,
    BackgroundTaskWriteRepository, StoredBackgroundTaskEvent, StoredBackgroundTaskRun,
    StoredBackgroundTaskRunPage, UpsertBackgroundTaskEvent, UpsertBackgroundTaskRun,
};
```

Source: `crates/aether-data-contracts/src/repository/background_tasks/mod.rs:1`.

Use `mod.rs` only as a re-export facade. Put domain structs, enums, traits,
helpers, and tests in the sibling `types.rs`. This keeps external imports
stable, for example `aether_data_contracts::repository::usage::UsageReadRepository`,
while allowing helper functions inside `types.rs` to remain private.

Do not put trait methods, conversion helpers, or tests in the domain `mod.rs`.
If a new domain needs more code than re-exports, create internal private
helpers in `types.rs` first. Split into more files only after the domain has a
real internal boundary, not because the file is long.

---

## Domain Responsibilities

`background_tasks` defines scheduler/daemon run and event contracts. It owns
`BackgroundTaskKind`, `BackgroundTaskStatus`, `StoredBackgroundTaskRun`,
`UpsertBackgroundTaskRun`, query/page/summary DTOs, and read/write repository
traits. The read trait starts at
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:241`.

`billing` defines billing model context lookups and admin billing rule/collector
mutations. It uses `AdminBillingMutationOutcome<T>` for optional mutation
support, including the `Unavailable` state at
`crates/aether-data-contracts/src/repository/billing/types.rs:145`.

`candidate_selection` defines minimal rows used by routing candidate selection.
It depends only on `aether-ai-formats` for API-format alias matching through
`api_format_matches` at
`crates/aether-data-contracts/src/repository/candidate_selection/types.rs:81`.

`candidates` defines per-request candidate traces, decision traces, health
status counts, timeline buckets, and request-candidate repository traits. It is
allowed to import provider catalog stored models to enrich decision traces, as
shown at `crates/aether-data-contracts/src/repository/candidates/types.rs:5`.

`global_models` defines public catalog model read contracts and admin global
model/provider model mutation contracts. It also owns model-specific validation
such as embedding pricing requirements at
`crates/aether-data-contracts/src/repository/global_models/types.rs:26`.

`provider_catalog` defines provider, endpoint, and key stored models plus
catalog read/write traits. Large stored records use small constructors and
builder-style `with_*` methods, for example `StoredProviderCatalogKey::new` at
`crates/aether-data-contracts/src/repository/provider_catalog/types.rs:299`.

`quota` defines provider quota snapshots and reset/read contracts. Its stored
model validates finite quota values at
`crates/aether-data-contracts/src/repository/quota/types.rs:32`.

`settlement` defines usage settlement write input/result contracts and only a
write repository. `UsageSettlementInput::validate` starts at
`crates/aether-data-contracts/src/repository/settlement/types.rs:18`.

`usage` is the largest contract family. It owns usage audit read models,
summary query DTOs, body capture metadata, cleanup windows, and usage
read/write repositories. Its comments intentionally document canonical storage
ownership at `crates/aether-data-contracts/src/repository/usage/types.rs:5`.

`video_tasks` defines video task states, lookup keys, filters, counters, and
read/write contracts for task polling and claiming. The `VideoTaskLookupKey`
enum starts at `crates/aether-data-contracts/src/repository/video_tasks/types.rs:342`.

---

## File Naming Rules

Use snake_case domain directories that match existing table or business
contract names:

- `background_tasks`
- `candidate_selection`
- `global_models`
- `provider_catalog`
- `video_tasks`

Inside each domain, keep the file names `mod.rs` and `types.rs`. The current
crate deliberately does not use `models.rs`, `repository.rs`, `traits.rs`, or
`dto.rs`; adding those names would make imports less predictable.

New public types should use the existing prefixes:

- `Stored*` for read models returned by storage implementations.
- `Upsert*`, `Create*`, or `Update*` for write inputs.
- `*Query`, `*Filter`, `*Page`, `*Summary`, `*Count`, and `*Bucket` for read
  parameter/result DTOs.
- `*ReadRepository`, `*WriteRepository`, and `*Repository` for trait families.

---

## Adding A New Domain

When adding a new repository contract family:

1. Add `pub mod <domain>;` in `src/repository/mod.rs`.
2. Create `src/repository/<domain>/mod.rs` with only `mod types;` and `pub use`.
3. Put all public records, queries, enums, traits, helper functions, and unit
   tests in `src/repository/<domain>/types.rs`.
4. Return `Result<_, crate::DataLayerError>` from fallible constructors and
   trait methods.
5. Keep concrete storage dependencies in `aether-data`, not here.

Example trait shape:

```rust
#[async_trait]
pub trait VideoTaskReadRepository: Send + Sync {
    async fn find(
        &self,
        key: VideoTaskLookupKey<'_>,
    ) -> Result<Option<StoredVideoTask>, crate::DataLayerError>;
}
```

Source: `crates/aether-data-contracts/src/repository/video_tasks/types.rs:372`.

---

## Directory Anti-Patterns

Do not add concrete database implementation files here:

```rust
// DON'T: this crate must not own sqlx pools or SQL statements.
pub struct SqlxUsageRepository {
    pool: sqlx::PgPool,
}
```

That belongs in `crates/aether-data/`, which already imports these contracts
and builds concrete repositories around them.

Do not bypass the facade modules:

```rust
// DON'T: external callers should not depend on internal private layout.
use aether_data_contracts::repository::usage::types::UsageReadRepository;
```

Use the re-exported namespace instead:

```rust
use aether_data_contracts::repository::usage::UsageReadRepository;
```

