# Quality Guidelines

> Code standards for `crates/aether-scheduler-core`.

## Design Boundary

Keep this crate pure, deterministic, and synchronous. It should transform
already-loaded contract records into scheduler decisions, candidate records, and
ranking metadata.

The crate has only contract, format, wallet, regex, serde, JSON, and hashing
dependencies:

```toml
aether-ai-formats.workspace = true
aether-contracts.workspace = true
aether-data-contracts.workspace = true
aether-wallet.workspace = true
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
```

Source: `crates/aether-scheduler-core/Cargo.toml:10`.

Do not add dependencies for IO, async runtime ownership, database access,
logging, HTTP routing, or background execution without moving that behavior to a
caller crate first.

## Public Surface Discipline

Expose public APIs only through `src/lib.rs`. The root facade groups re-exports
by domain and keeps modules private:

```rust
pub use candidate::{
    auth_api_key_concurrency_limit_reached, candidate_is_selectable_with_runtime_state,
    candidate_runtime_skip_reason_with_state, candidate_supports_required_capability,
    collect_global_model_names_for_required_capability, enumerate_minimal_candidate_selection,
    enumerate_minimal_candidate_selection_with_model_directives,
    requested_capability_priority_for_candidate, CandidateRuntimeSelectabilityInput,
    EnumerateMinimalCandidateSelectionInput, SchedulerMinimalCandidateSelectionCandidate,
    SchedulerPriorityMode,
};
```

Source: `crates/aether-scheduler-core/src/lib.rs:20`.

Implementation helpers should stay private or `pub(super)`. Ranking comparators
are a good example:

```rust
pub(super) fn compare_rankable_candidates(
    left: &SchedulerRankableCandidate,
    right: &SchedulerRankableCandidate,
    context: SchedulerRankingContext,
) -> Ordering
```

Source: `crates/aether-scheduler-core/src/ranking/modes.rs:10`.

DON'T make submodules public as a shortcut:

```rust
// Wrong: leaks internal module layout as API.
pub mod ranking;
```

## Input Structs For Wide APIs

When a scheduler decision needs many inputs, use a small input struct with
borrowed fields instead of a long positional function signature.

Example:

```rust
pub struct EnumerateMinimalCandidateSelectionInput<'a> {
    pub rows: Vec<StoredMinimalCandidateSelectionRow>,
    pub normalized_api_format: &'a str,
    pub requested_model_name: &'a str,
    pub resolved_global_model_name: &'a str,
    pub require_streaming: bool,
    pub required_capabilities: Option<&'a serde_json::Value>,
    pub auth_constraints: Option<&'a crate::SchedulerAuthConstraints>,
}
```

Source: `crates/aether-scheduler-core/src/candidate/types.rs:31`.

`CandidateRuntimeSelectabilityInput` follows the same pattern for runtime
state:

```rust
pub struct CandidateRuntimeSelectabilityInput<'a> {
    pub candidate: &'a SchedulerMinimalCandidateSelectionCandidate,
    pub recent_candidates: &'a [StoredRequestCandidate],
    pub provider_concurrent_limits: &'a BTreeMap<String, usize>,
    pub provider_key_rpm_states: &'a BTreeMap<String, StoredProviderCatalogKey>,
    pub now_unix_secs: u64,
    pub provider_quota_blocks_requests: bool,
    pub account_quota_exhausted: bool,
    pub oauth_invalid: bool,
    pub rpm_reset_at: Option<u64>,
}
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:22`.

Prefer this pattern for future APIs that need more than four semantic inputs.

## Deterministic Ordering

Scheduler decisions must be stable. Use explicit sorting, stable tie-breakers,
and ordered collections when user-visible or test-visible output depends on
order.

Ranking ties end with candidate identity and original index:

```rust
left.capability_priority
    .cmp(&right.capability_priority)
    .then_with(|| compare_cross_format_demotion(left, right))
    .then_with(|| compare_demoted_format_preference(left, right))
    .then_with(|| compare_candidate_priority_slot(left, right, context.priority_mode))
    .then_with(|| compare_format_preference(left, right))
    .then_with(|| compare_candidate_identity_for_ranking(left, right))
    .then(left.original_index.cmp(&right.original_index))
```

Source: `crates/aether-scheduler-core/src/ranking/modes.rs:27`.

Global model names for capability prompts are deduplicated with `BTreeSet`:

```rust
let mut model_names = BTreeSet::new();
...
model_names.into_iter().collect()
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:117`.

Provider concurrency limits use `BTreeMap`, not `HashMap`, which keeps tests and
serialized debug output deterministic:

```rust
pub fn build_provider_concurrent_limit_map(
    providers: Vec<StoredProviderCatalogProvider>,
) -> BTreeMap<String, usize>
```

Source: `crates/aether-scheduler-core/src/provider.rs:30`.

## JSON Handling

Treat `serde_json::Value` as untrusted stored or report data. Parse by shape,
trim strings, ignore missing optional fields, and fail only when the data
contract requires a valid shape.

Request-candidate string fields are trimmed and empty strings disappear:

```rust
object
    .get(key)
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned)
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:620`.

Capability checks accept object, string, and numeric truthy values, but reject
other JSON shapes:

```rust
match value {
    serde_json::Value::Bool(value) => *value,
    serde_json::Value::String(value) => value.eq_ignore_ascii_case("true"),
    serde_json::Value::Number(value) => value.as_i64().is_some_and(|value| value > 0),
    _ => false,
}
```

Source: `crates/aether-scheduler-core/src/candidate/capability.rs:24`.

DON'T use lossy JSON coercion for scheduler policy fields. For example,
`priority_slot` is parsed as `i32` only:

```rust
value
    .as_object()
    .and_then(|object| object.get(key))
    .and_then(Value::as_i64)
    .and_then(|value| i32::try_from(value).ok())
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:642`.

## Type Safety And Derives

Use enums for scheduler modes and status buckets. Public modes derive serde when
they cross config or report boundaries:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SchedulerRankingMode {
    FixedOrder,
    #[default]
    CacheAffinity,
    LoadBalance,
}
```

Source: `crates/aether-scheduler-core/src/ranking/types.rs:5`.

Health buckets are ordered so comparators can prefer healthier keys:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderKeyHealthBucket {
    Low,
    Degraded,
    Healthy,
}
```

Source: `crates/aether-scheduler-core/src/health.rs:26`.

Keep new enums small and domain-specific. Avoid stringly typed public modes
unless they are persisted as report metadata.

## Numeric Safety

Use saturating arithmetic, fallible integer conversion, and clamping around
runtime counters and scores.

Examples:

```rust
now_unix_secs.saturating_sub(observed_at_unix_secs) <= ACTIVE_REQUEST_WINDOW_SECS
```

Source: `crates/aether-scheduler-core/src/health.rs:512`.

```rust
provider.concurrent_limit
    .and_then(|limit| usize::try_from(limit).ok())
    .filter(|limit| *limit > 0)
```

Source: `crates/aether-scheduler-core/src/provider.rs:36`.

```rust
Some(score.clamp(0.0, 1.0))
```

Source: `crates/aether-scheduler-core/src/health.rs:243`.

DON'T cast signed database values directly into `usize`.

## Testing Requirements

Keep tests module-local with `#[cfg(test)] mod tests`. The crate currently has
unit tests in these modules:

- `src/affinity.rs:141`
- `src/auth.rs:99`
- `src/candidate/mod.rs:23`
- `src/health.rs:632`
- `src/model.rs:388`
- `src/provider.rs:46`
- `src/ranking/mod.rs:102`
- `src/request_candidate.rs:835`

Tests should lock policy, not just smoke-test output. Examples:

- cooldown requires two recent failures and is cleared by a recent success:
  `src/health.rs:696` and `src/health.rs:712`;
- cache affinity reports `RANKING_REASON_CACHED_AFFINITY`:
  `src/ranking/mod.rs:225`;
- model directive suffixes require explicit enablement:
  `src/auth.rs:224` and `src/model.rs:414`;
- report context preserves ranking and proxy metadata:
  `src/request_candidate.rs:937` and `src/request_candidate.rs:1116`.

When adding a new scheduling branch, add at least one positive test and one
negative or fallback test in the owning module.

## Forbidden Patterns

DON'T add async or IO:

```rust
// Wrong for scheduler-core.
pub async fn choose_candidate(state: AppState) -> Result<Response, Error> { ... }
```

DON'T duplicate policy order between a bool function and a reason function.
`candidate_is_selectable_with_runtime_state` already delegates to
`candidate_runtime_skip_reason_with_state`.

DON'T log secrets or raw session keys. `affinity.rs` hashes client session keys
before composing v2 cache keys:

```rust
let session_hash = hash_session_key(session_key);
```

Source: `crates/aether-scheduler-core/src/affinity.rs:89`.

DON'T make malformed stored JSON look like missing optional data. Preserve the
strict `DataLayerError::UnexpectedValue` behavior in model priority parsing.

## Review Checklist

Before merging a scheduler-core change, check:

- The API belongs in a pure domain crate and needs no DB, HTTP, cache, tracing,
  or task runtime.
- The function is exported through `lib.rs` only if a caller outside the module
  needs it.
- Candidate ordering stays deterministic after new tie-breakers.
- JSON inputs distinguish missing optional values from malformed persisted
  values.
- New skip reasons are stable machine strings and are tested.
- Health/RPM math uses clamping, saturating arithmetic, and fallible conversion.
- Module-local tests cover both the new accepted path and rejection/fallback
  behavior.
