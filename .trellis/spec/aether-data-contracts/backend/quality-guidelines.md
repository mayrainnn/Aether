# Quality Guidelines

> Code quality standards for the `aether-data-contracts` crate.

---

## Overview

This crate is a contract crate. Quality means stable public types, explicit
validation, deterministic data shapes, and no runtime side effects. Keep the
crate small enough that higher layers can depend on it without pulling in
database drivers, HTTP frameworks, async runtimes, or logging stacks.

The dependency set at `crates/aether-data-contracts/Cargo.toml:9` is the quality
baseline: `aether-ai-formats`, `async-trait`, `chrono`, `serde`, `serde_json`,
and `thiserror`. New dependencies should be treated as architectural changes,
not convenience additions.

---

## Naming Conventions

Use names that explain how the type crosses the storage boundary:

- `Stored*` for read models produced by repositories.
- `Upsert*`, `Create*`, and `Update*` for caller-provided write inputs.
- `*Query`, `*Filter`, `*Page`, `*Summary`, `*Count`, and `*Bucket` for read
  request/result DTOs.
- `*ReadRepository` and `*WriteRepository` for split traits.
- `*Repository` for the blanket composed trait.

Example from background tasks:

```rust
pub struct StoredBackgroundTaskRun { /* stored read model */ }
pub struct UpsertBackgroundTaskRun { /* write input */ }
pub struct BackgroundTaskListQuery { /* read query */ }
pub trait BackgroundTaskReadRepository: Send + Sync { /* reads */ }
pub trait BackgroundTaskWriteRepository: Send + Sync { /* writes */ }
```

Source:
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:81`.

Do not introduce vague type names like `Data`, `Item`, `Params`, `Model`, or
`RepositoryImpl` in this crate. Concrete implementation names belong in
`aether-data`.

---

## Public Surface And Visibility

Keep domain `types.rs` private behind domain facades. Each domain `mod.rs`
should contain only `mod types;` and `pub use types::{...};`. The provider
catalog facade shows the expected shape at
`crates/aether-data-contracts/src/repository/provider_catalog/mod.rs:1`.

Private helpers should stay private. `api_format_matches` is private at
`crates/aether-data-contracts/src/repository/candidate_selection/types.rs:81`
because callers should use domain methods such as
`StoredMinimalCandidateSelectionRow::key_supports_api_format`, not helper
implementation details.

Use `pub` only for contract types and trait methods that consumers must import.
The crate currently does not use `pub(crate)` because there are no cross-module
internal helpers that need crate-wide visibility.

---

## Type Safety Patterns

Prefer enums over raw strings when the value set is known. Examples:

- `BackgroundTaskKind` and `BackgroundTaskStatus` in
  `crates/aether-data-contracts/src/repository/background_tasks/types.rs:9`.
- `RequestCandidateStatus` in
  `crates/aether-data-contracts/src/repository/candidates/types.rs:9`.
- `VideoTaskStatus` in
  `crates/aether-data-contracts/src/repository/video_tasks/types.rs:7`.
- `UsageBodyCaptureState`, `UsageBodyCaptureStorage`, and `UsageBodyField` in
  `crates/aether-data-contracts/src/repository/usage/types.rs:1140`.

When a database stores the enum as text, provide explicit conversion helpers.
`BackgroundTaskStatus::as_database` and `BackgroundTaskStatus::from_database`
live at `crates/aether-data-contracts/src/repository/background_tasks/types.rs:52`.

When a value is JSON-extensible by design, use `serde_json::Value` but isolate
the JSON field to the exact contract point. `StoredProviderCatalogKey` keeps
fields such as `capabilities`, `rate_multipliers`, `fingerprint`, and
`status_snapshot` as `Option<serde_json::Value>` at
`crates/aether-data-contracts/src/repository/provider_catalog/types.rs:242`.
Do not replace typed scalar fields with arbitrary JSON.

Use deterministic containers when serialized or compared summaries should be
stable. `BackgroundTaskSummary` uses `BTreeMap<String, u64>` for `by_status`
and `by_kind` at
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:233`.

---

## Validation Rules

Constructors and `validate` methods are part of the contract. They should fail
loudly before invalid values leave this crate.

Examples:

- `StoredPublicGlobalModel::new` checks non-empty `id` and `name`, then applies
  embedding billing rules at
  `crates/aether-data-contracts/src/repository/global_models/types.rs:158`.
- `UpsertBackgroundTaskRun::validate` rejects empty run identity and
  `progress_percent > 100` at
  `crates/aether-data-contracts/src/repository/background_tasks/types.rs:127`.
- `StoredRequestCandidate::new` converts signed database columns into unsigned
  Rust fields with `try_from` at
  `crates/aether-data-contracts/src/repository/candidates/types.rs:76`.
- `UpsertUsageRecord::validate` rejects empty identity fields and non-finite
  cost values at `crates/aether-data-contracts/src/repository/usage/types.rs:1598`.

Keep validation close to the type that owns the invariant. Do not rely on
callers or repository implementations to remember cross-field rules such as
"embedding global models need request/input pricing".

---

## Builder And Conversion Patterns

Large stored records should use a minimal constructor for required identity
fields, then builder-style methods for optional field groups. The provider
catalog models are the reference pattern:

```rust
pub fn with_transport_fields(
    mut self,
    base_url: String,
    header_rules: Option<serde_json::Value>,
) -> Result<Self, crate::DataLayerError> {
    if base_url.trim().is_empty() {
        return Err(crate::DataLayerError::UnexpectedValue(
            "provider_endpoints.base_url is empty".to_string(),
        ));
    }
    self.base_url = base_url;
    Ok(self)
}
```

Source:
`crates/aether-data-contracts/src/repository/provider_catalog/types.rs:197`.

Use `into_stored` when a write input has the same shape as the stored read
model. `UpsertBackgroundTaskRun::into_stored` starts at
`crates/aether-data-contracts/src/repository/background_tasks/types.rs:146`,
and `UpsertVideoTask::into_stored` starts at
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:254`.

Use `From<Stored*> for Upsert*` when an update flow legitimately edits a
previously stored row. The video task contract implements this at
`crates/aether-data-contracts/src/repository/video_tasks/types.rs:298`.

---

## Serde Compatibility

Only stored/read DTOs and wire-facing helper DTOs should derive
`serde::Serialize` and `serde::Deserialize`. Write inputs derive serde when
they are intentionally passed across crate/process boundaries; otherwise keep
them plain Rust structs.

For externally visible enum strings, use serde attributes instead of manual
string fields. `RequestCandidateStatus` uses `#[serde(rename_all = "snake_case")]`
at `crates/aether-data-contracts/src/repository/candidates/types.rs:9`.

For optional fields added after older payloads existed, use
`#[serde(default, skip_serializing_if = "Option::is_none")]` as shown by
`StoredProviderModelMapping.endpoint_ids` at
`crates/aether-data-contracts/src/repository/candidate_selection/types.rs:8`
and by usage body fields at
`crates/aether-data-contracts/src/repository/usage/types.rs:61`.

---

## Testing Requirements

Unit tests should live in the owning `types.rs` next to the invariant they
protect. Current examples:

- Global model tests for embedding billing validation start at
  `crates/aether-data-contracts/src/repository/global_models/types.rs:872`.
- Provider catalog key defaults and rate-limit builder tests start at
  `crates/aether-data-contracts/src/repository/provider_catalog/types.rs:467`.
- Settlement validation tests start at
  `crates/aether-data-contracts/src/repository/settlement/types.rs:66`.
- Usage tests cover invalid usage rows, typed metadata precedence, body-ref
  parsing, capture-source behavior, and curl body-source selection starting at
  `crates/aether-data-contracts/src/repository/usage/types.rs:1760`.
- Video task tests cover status parsing and numeric conversion errors starting
  at `crates/aether-data-contracts/src/repository/video_tasks/types.rs:469`.

When adding a new validation rule, add a focused unit test in the same
`types.rs`. Do not rely only on storage implementation tests in `aether-data`;
the contract crate should prove its own invariants.

---

## Forbidden Patterns

Do not add runtime implementations:

```rust
// DON'T: repository contracts must not own pools or execute SQL.
pub struct PostgresUsageReadRepository {
    pool: sqlx::PgPool,
}
```

Do not silently coerce corrupt data:

```rust
// DON'T: negative database counters must not wrap or become zero.
let total_tokens = value as u64;
```

Use checked conversions like `parse_u64` at
`crates/aether-data-contracts/src/repository/usage/types.rs:1725`.

Do not expose helper modules directly:

```rust
// DON'T: external API should stay on the domain facade.
pub mod types;
```

Keep `mod types; pub use types::{...};` as the public boundary.
