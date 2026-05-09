# Error Handling

> Error contracts and propagation rules for `aether-data-contracts`.

---

## Overview

This crate uses one public error type: `DataLayerError`. All fallible
constructors, conversion helpers, validation methods, and repository trait
methods return `Result<_, crate::DataLayerError>`. The crate does not use
`anyhow`, does not expose HTTP errors, and does not map errors into axum
responses.

`DataLayerError` is defined with `thiserror` at
`crates/aether-data-contracts/src/error.rs:1`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DataLayerError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unexpected database value: {0}")]
    UnexpectedValue(String),
}
```

The full enum also includes `Postgres`, `Redis`, `Sql`, and `TimedOut` variants
at `crates/aether-data-contracts/src/error.rs:9`. Those variants let concrete
storage crates preserve backend context without importing backend-specific
error types into this crate.

---

## Error Variant Semantics

Use `InvalidConfiguration` for setup problems supplied by the application or
environment. This crate currently defines the variant but does not construct it
directly; concrete data-layer implementations should use it when a connection
or repository cannot be configured.

Use `InvalidInput` when caller-provided write inputs are malformed before they
reach a database. Examples include empty upsert IDs and non-finite costs.
`UpsertUsageRecord::validate` returns `InvalidInput` for empty `request_id`,
`provider_name`, `model`, `status`, or `billing_status` at
`crates/aether-data-contracts/src/repository/usage/types.rs:1598`.

Use `UnexpectedValue` when a database or stored representation cannot be
converted into the contract's typed model. Examples include unsupported enum
strings, negative numeric database values that should become unsigned Rust
types, missing identity values in stored rows, and non-finite stored prices.

Use `Postgres`, `Redis`, and `Sql` to wrap backend errors at storage boundaries.
The helper constructors convert any displayable error into owned strings:

```rust
impl DataLayerError {
    pub fn postgres(error: impl std::fmt::Display) -> Self {
        Self::Postgres(error.to_string())
    }
}
```

Source: `crates/aether-data-contracts/src/error.rs:25`.

---

## Result Signatures

Repository traits must return `crate::DataLayerError` directly. Do not use
`Box<dyn Error>`, `anyhow::Error`, or backend-specific error types in public
contracts.

```rust
#[async_trait]
pub trait BackgroundTaskReadRepository: Send + Sync {
    async fn find_run(
        &self,
        run_id: &str,
    ) -> Result<Option<StoredBackgroundTaskRun>, crate::DataLayerError>;
}
```

Source:
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:241`.

Optional lookups use `Result<Option<T>, DataLayerError>`; list operations use
`Result<Vec<T>, DataLayerError>` or a typed page/summary result. This keeps
"not found" separate from "storage failed".

Mutation methods also return typed outcomes where unsupported behavior is part
of the contract. `AdminBillingMutationOutcome<T>` includes `Unavailable` at
`crates/aether-data-contracts/src/repository/billing/types.rs:145`, and default
admin billing methods return `Ok(AdminBillingMutationOutcome::Unavailable)` at
`crates/aether-data-contracts/src/repository/billing/types.rs:183`.

---

## Database Value Conversion

Stored-row constructors accept database-shaped primitive values when the
storage layer may produce signed integers or strings. They convert into the
safe public model and fail loudly when a value is invalid.

```rust
let progress_percent = u16::try_from(progress_percent).map_err(|_| {
    crate::DataLayerError::UnexpectedValue(format!(
        "invalid progress_percent: {progress_percent}"
    ))
})?;
```

Source:
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:127`.

Use this pattern for every signed-to-unsigned conversion. Do not cast with
`as`, except where the input has already been semantically constrained. For
example, `StoredProviderQuotaSnapshot::new` currently maps optional quota reset
timestamps with `map(|value| value as u64)` at
`crates/aether-data-contracts/src/repository/quota/types.rs:42`; new code
should prefer checked conversion helpers like `coerce_optional_unix_secs` in
`video_tasks` at `crates/aether-data-contracts/src/repository/video_tasks/types.rs:457`.

String-backed enums must parse through explicit methods and reject unknown
values:

```rust
pub fn from_database(value: &str) -> Result<Self, crate::DataLayerError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "processing" => Ok(Self::Processing),
        other => Err(crate::DataLayerError::UnexpectedValue(format!(
            "unsupported video_tasks.status: {other}"
        ))),
    }
}
```

Source:
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:19`.

---

## Validation Methods

Write-input structs validate caller-provided identity fields and finite numeric
values before concrete repositories persist them.

```rust
pub fn validate(&self) -> Result<(), crate::DataLayerError> {
    if self.request_id.trim().is_empty() {
        return Err(crate::DataLayerError::InvalidInput(
            "settlement request_id cannot be empty".to_string(),
        ));
    }
    Ok(())
}
```

Source:
`crates/aether-data-contracts/src/repository/settlement/types.rs:18`.

Use `InvalidInput` for caller write models such as `UsageSettlementInput` and
`UpsertUsageRecord`. Use `UnexpectedValue` for stored models built from
database rows such as `StoredRequestUsageAudit::new` at
`crates/aether-data-contracts/src/repository/usage/types.rs:116`.

Validation must reject non-finite floats. Examples:

- `StoredProviderQuotaSnapshot::new` rejects non-finite quota values at
  `crates/aether-data-contracts/src/repository/quota/types.rs:32`.
- `UpsertUsageRecord::validate` rejects non-finite `total_cost_usd`,
  `cache_creation_cost_usd`, `cache_read_cost_usd`, `output_price_per_1m`, and
  `actual_total_cost_usd` at
  `crates/aether-data-contracts/src/repository/usage/types.rs:1625`.
- `validate_optional_price` rejects negative or non-finite model prices at
  `crates/aether-data-contracts/src/repository/global_models/types.rs:14`.

---

## Error Surfacing To Callers

This crate should only create rich domain/storage errors. It should not log
errors, attach HTTP status codes, translate into `axum::response::IntoResponse`,
or decide retry policy. Callers in higher layers choose whether an
`InvalidInput` becomes a `400`, whether `TimedOut` becomes a retry, or whether
`Postgres` is logged as an internal error.

Keep error messages stable and field-qualified. Existing messages include
database/table context such as `"usage.request_id is empty"` at
`crates/aether-data-contracts/src/repository/usage/types.rs:156`,
`"models.provider_model_name is empty"` at
`crates/aether-data-contracts/src/repository/global_models/types.rs:478`, and
`"unsupported request_candidates.status: {other}"` at
`crates/aether-data-contracts/src/repository/candidates/types.rs:23`.

---

## Common Mistakes

Do not silently default unknown database enum values:

```rust
// DON'T: this hides corrupt data and changes behavior across callers.
let status = VideoTaskStatus::Completed;
```

Use `from_database` and propagate the error with `?`.

Do not collapse missing rows and backend failures:

```rust
// DON'T: callers cannot distinguish not found from a broken repository.
async fn find(&self, id: &str) -> Option<StoredVideoTask>;
```

Use `Result<Option<T>, DataLayerError>` as shown by `VideoTaskReadRepository`
at `crates/aether-data-contracts/src/repository/video_tasks/types.rs:372`.

Do not introduce `anyhow::Result` in public traits. It would erase the stable
data-layer error vocabulary and force every caller to downcast.

