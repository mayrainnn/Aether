# Repository Contracts

> Repository trait and data-contract rules for `aether-data-contracts`.

---

## Purpose

The central job of this crate is to define stable contracts between Aether
services and concrete data backends. It defines what a caller can ask for, what
shape storage returns, what write inputs are valid, and what errors can cross
the boundary. It does not define how SQL, Redis, transactions, pooling, or
retry logic work.

All repository traits use `async_trait` and require `Send + Sync`, making them
suitable for `Arc<dyn Trait>` usage in higher layers. Example:

```rust
#[async_trait]
pub trait MinimalCandidateSelectionReadRepository: Send + Sync {
    async fn list_for_exact_api_format(
        &self,
        api_format: &str,
    ) -> Result<Vec<StoredMinimalCandidateSelectionRow>, crate::DataLayerError>;
}
```

Source:
`crates/aether-data-contracts/src/repository/candidate_selection/types.rs:85`.

---

## Read/Write Split

Split read and write capabilities into separate traits whenever a domain has
both query and mutation behavior.

Examples:

- `BackgroundTaskReadRepository` and `BackgroundTaskWriteRepository` at
  `crates/aether-data-contracts/src/repository/background_tasks/types.rs:241`.
- `ProviderCatalogReadRepository` and `ProviderCatalogWriteRepository` at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:569`.
- `UsageReadRepository` and `UsageWriteRepository` at
  `crates/aether-data-contracts/src/repository/usage/types.rs:1343`.
- `VideoTaskReadRepository` and `VideoTaskWriteRepository` at
  `crates/aether-data-contracts/src/repository/video_tasks/types.rs:372`.

Compose the full domain trait with a blanket implementation:

```rust
pub trait VideoTaskRepository:
    VideoTaskReadRepository + VideoTaskWriteRepository + Send + Sync
{
}

impl<T> VideoTaskRepository for T where
    T: VideoTaskReadRepository + VideoTaskWriteRepository + Send + Sync
{
}
```

Source:
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:447`.

If a domain has only writes, do not invent a read trait. `SettlementRepository`
composes only `SettlementWriteRepository` at
`crates/aether-data-contracts/src/repository/settlement/types.rs:54`.

---

## Stored Models

`Stored*` types represent the normalized contract returned by storage. They
should contain typed fields, not raw database rows.

Example:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredProviderQuotaSnapshot {
    pub provider_id: String,
    pub billing_type: String,
    pub monthly_quota_usd: Option<f64>,
    pub monthly_used_usd: f64,
    pub is_active: bool,
}
```

Source:
`crates/aether-data-contracts/src/repository/quota/types.rs:3`.

Stored constructors should convert from backend-shaped primitives into safe
Rust types. `StoredVideoTask::new` accepts signed database fields such as
`progress_percent: i32`, `retry_count: i32`, and timestamp `i64` values, then
checks them with `try_from` at
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:86`.

Do not expose `sqlx::Row`, SeaORM entity models, Redis values, or raw JSON for
fields with known scalar meaning.

---

## Write Inputs

Write-input types capture caller intent before persistence. They should expose
validation when identity, status, or numeric invariants matter.

Example:

```rust
impl UpsertRequestCandidateRecord {
    pub fn validate(&self) -> Result<(), crate::DataLayerError> {
        if self.id.trim().is_empty() {
            return Err(crate::DataLayerError::InvalidInput(
                "request candidate upsert id cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}
```

Source:
`crates/aether-data-contracts/src/repository/candidates/types.rs:500`.

Use `Upsert*` when the method may create or update a row, as in
`UsageWriteRepository::upsert` at
`crates/aether-data-contracts/src/repository/usage/types.rs:1664`.

Use `Create*` and `Update*` when the operation is explicitly separated, as in
`CreateAdminGlobalModelRecord` and `UpdateAdminGlobalModelRecord` at
`crates/aether-data-contracts/src/repository/global_models/types.rs:593`.

---

## Query And Page DTOs

Use query structs when a method has more than one or two filter parameters or
when pagination/search flags need to stay compatible over time.

Examples:

- `BackgroundTaskListQuery` contains task key substring, kind, status, trigger,
  offset, and limit at
  `crates/aether-data-contracts/src/repository/background_tasks/types.rs:217`.
- `ProviderCatalogKeyListQuery` contains provider ID, search, active filter,
  offset, limit, and order at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:521`.
- `VideoTaskQueryFilter` contains user, status, model substring, and client API
  format filters at
  `crates/aether-data-contracts/src/repository/video_tasks/types.rs:352`.

Use page wrappers when the total count is part of the contract:

```rust
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct StoredBackgroundTaskRunPage {
    pub items: Vec<StoredBackgroundTaskRun>,
    pub total: usize,
}
```

Source:
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:227`.

Do not add loose method signatures with many primitive parameters if the query
is likely to grow. A query DTO is easier to extend without breaking all
implementors.

---

## Default Trait Methods

Default trait methods are allowed only for capability evolution. They should
return neutral behavior that preserves compatibility for implementors that do
not support the new feature yet.

Examples:

- `BillingReadRepository::find_model_context_by_model_id` returns `Ok(None)` by
  default at `crates/aether-data-contracts/src/repository/billing/types.rs:162`.
- Admin billing mutation methods return
  `Ok(AdminBillingMutationOutcome::Unavailable)` by default at
  `crates/aether-data-contracts/src/repository/billing/types.rs:183`.
- `UsageWriteRepository::cleanup_stale_pending_requests` and `cleanup_usage`
  return default summaries at
  `crates/aether-data-contracts/src/repository/usage/types.rs:1675`.

Do not use a default method to hide required behavior. If every implementation
must support the operation, leave the method abstract so compilation forces each
backend to implement it.

---

## Cross-Domain Contracts

Cross-domain imports are allowed only when the contract itself needs another
contract type. The clearest example is decision trace enrichment:

```rust
use crate::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
```

Source:
`crates/aether-data-contracts/src/repository/candidates/types.rs:5`.

`build_decision_trace` then accepts request candidates plus provider, endpoint,
and key records and returns a fully enriched `DecisionTrace` at
`crates/aether-data-contracts/src/repository/candidates/types.rs:335`.

Avoid introducing cross-domain imports for convenience helpers. If a helper
does not need another domain's public type in its signature, keep it local.

---

## Body And Metadata Contracts

Usage body capture is a typed contract, not an unstructured logging format.
`UsageBodyField` maps body fields to stable capture keys and storage field
names at `crates/aether-data-contracts/src/repository/usage/types.rs:1276`.

Body references use a stable URI-like format:

```rust
pub fn usage_body_ref(request_id: &str, field: UsageBodyField) -> String {
    format!("usage://request/{request_id}/{}", field.as_storage_field())
}
```

Source:
`crates/aether-data-contracts/src/repository/usage/types.rs:1324`.

`parse_usage_body_ref` must reject malformed refs and empty request IDs rather
than guessing. It returns `Option<(String, UsageBodyField)>` at
`crates/aether-data-contracts/src/repository/usage/types.rs:1328`.

---

## Implementation Boundaries

Concrete repositories should implement these traits outside this crate. The
implementation crates may own pools, statements, transactions, retry logic,
logging spans, and metrics. This crate owns only:

- public trait signatures,
- stored/read DTOs,
- write DTOs,
- query/filter/page/summary DTOs,
- enum/string conversion helpers,
- validation helpers,
- contract-level unit tests.

Do not add a dependency from `aether-data-contracts` back to `aether-data`.
That would invert the intended boundary and make all consumers pull in concrete
database code.

---

## Contract Anti-Patterns

Do not expose backend-specific types:

```rust
// DON'T: storage-specific errors leak implementation choices.
async fn find(&self, id: &str) -> Result<Option<StoredVideoTask>, sqlx::Error>;
```

Use `crate::DataLayerError`.

Do not collapse read and write traits just because one implementation has both:

```rust
// DON'T: read-only callers should not require write capability.
pub trait UsageRepository {
    async fn find_by_id(...);
    async fn upsert(...);
}
```

Follow the split used by `UsageReadRepository` and `UsageWriteRepository`.

Do not make query DTOs depend on UI or HTTP vocabulary. Keep names storage and
domain oriented, like `UsageDashboardSummaryQuery` or `VideoTaskQueryFilter`,
not `RequestParams` or `AdminPagePayload`.

