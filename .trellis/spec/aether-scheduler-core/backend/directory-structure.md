# Directory Structure

> Module organization rules for `crates/aether-scheduler-core`.

## Scope

`aether-scheduler-core` is a pure domain crate for provider scheduling. Its
`Cargo.toml` description says it is "Pure scheduler health and quota logic
extracted from aether-gateway".

Source: `crates/aether-scheduler-core/Cargo.toml:7`.

The crate has no Axum router, no SeaORM entities, no Redis client, no Tokio
tasks, and no direct database queries. It consumes stored record contracts from
`aether-data-contracts` and returns values that gateway/service crates persist
or log.

Source: `crates/aether-scheduler-core/Cargo.toml:9`.

```toml
[dependencies]
aether-ai-formats.workspace = true
aether-contracts.workspace = true
aether-data-contracts.workspace = true
aether-wallet.workspace = true
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
```

## Actual Layout

ABCoder MCP, called with `repo_name="aether-scheduler-core"`, reported one Rust
module with 18 files. The real source layout is:

```text
crates/aether-scheduler-core/
├── Cargo.toml
└── src/
    ├── affinity.rs
    ├── auth.rs
    ├── candidate/
    │   ├── capability.rs
    │   ├── enumeration.rs
    │   ├── mod.rs
    │   ├── selectability.rs
    │   └── types.rs
    ├── health.rs
    ├── lib.rs
    ├── model.rs
    ├── provider.rs
    ├── ranking/
    │   ├── format.rs
    │   ├── mod.rs
    │   ├── modes.rs
    │   ├── priority.rs
    │   ├── reasons.rs
    │   └── types.rs
    └── request_candidate.rs
```

Source: ABCoder `get_repo_structure({repo_name: "aether-scheduler-core"})`.

## Root Facade

Keep `src/lib.rs` as the public facade. All implementation modules are private:

```rust
mod affinity;
mod auth;
mod candidate;
mod health;
mod model;
mod provider;
mod ranking;
mod request_candidate;
```

Source: `crates/aether-scheduler-core/src/lib.rs:1`.

Only re-export stable scheduler APIs from `lib.rs`. For example, ranking exports
are grouped together:

```rust
pub use ranking::{
    apply_scheduler_candidate_ranking, SchedulerRankableCandidate, SchedulerRankingContext,
    SchedulerRankingMode, SchedulerRankingOutcome, SchedulerTunnelAffinityBucket,
    RANKING_REASON_CACHED_AFFINITY, RANKING_REASON_CROSS_FORMAT, RANKING_REASON_LOCAL_TUNNEL,
};
```

Source: `crates/aether-scheduler-core/src/lib.rs:48`.

Do not make implementation modules public just to reach a helper. Promote a
single helper through `lib.rs` only when it is a stable crate contract used by
gateway, AI serving, data tests, or another crate.

GitNexus context for `apply_scheduler_candidate_ranking` showed incoming calls
from `apps/aether-gateway/src/scheduler/candidate/ranking.rs`,
`apps/aether-gateway/src/ai_serving/planner/candidate_ranking.rs`, and
`crates/aether-ai-serving/src/candidate_ranking.rs`. That public API is shared;
internal comparator helpers should remain private.

## Module Responsibilities

| Module | Responsibility | Representative API |
| --- | --- | --- |
| `affinity.rs` | Build affinity cache keys and candidate identity hashes without leaking raw session data. | `build_scheduler_affinity_cache_key_for_api_key_id_with_client_session` at `src/affinity.rs:54`. |
| `auth.rs` | Apply scheduler-visible auth constraints over provider, API format, and model names. | `auth_constraints_allow_model_with_model_directives` at `src/auth.rs:75`. |
| `candidate/enumeration.rs` | Turn stored candidate rows into minimal scheduler candidates after auth, streaming, and model checks. | `enumerate_minimal_candidate_selection` at `src/candidate/enumeration.rs:10`. |
| `candidate/capability.rs` | Interpret requested capability JSON and compute missing-capability priority. | `requested_capability_priority_for_candidate` at `src/candidate/capability.rs:46`. |
| `candidate/selectability.rs` | Apply runtime skip checks such as quota block, cooldown, concurrency, circuit breaker, health, and RPM. | `candidate_runtime_skip_reason_with_state` at `src/candidate/selectability.rs:41`. |
| `health.rs` | Pure health, cooldown, active request, adaptive RPM, reservation, and bucket logic. | `provider_key_rpm_allows_request_since` at `src/health.rs:201`. |
| `model.rs` | Normalize API formats, resolve global/provider model names, regex model mappings, and format-specific priorities. | `resolve_provider_model_name_with_model_directives` at `src/model.rs:135`. |
| `provider.rs` | Provider quota and provider concurrent-limit helpers. | `build_provider_concurrent_limit_map` at `src/provider.rs:30`. |
| `ranking/` | Candidate ordering algorithms, tie-breakers, reason constants, and ranking outcome structs. | `apply_scheduler_candidate_ranking` at `src/ranking/mod.rs:66`. |
| `request_candidate.rs` | Build and repair request-candidate records and report context payloads. | `build_execution_request_candidate_seed` at `src/request_candidate.rs:308`. |

## Subdirectory Rules

Use a subdirectory only when a domain has multiple cooperating files with a
private `mod.rs` coordinator.

`candidate/` separates types, enumeration, runtime selectability, and
capability scoring. `candidate/enumeration.rs` imports only the public input and
output candidate structs from `candidate/types.rs`:

```rust
use super::types::{
    EnumerateMinimalCandidateSelectionInput, SchedulerMinimalCandidateSelectionCandidate,
};
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:6`.

`ranking/` separates comparable concerns: `format.rs`, `priority.rs`,
`modes.rs`, `reasons.rs`, and `types.rs`. `ranking/mod.rs` composes them and is
the only file that exports ranking APIs:

```rust
mod format;
mod modes;
mod priority;
mod reasons;
mod types;
```

Source: `crates/aether-scheduler-core/src/ranking/mod.rs:1`.

Do not add a new folder for a single helper. Put one-off pure helper families in
the existing flat file for that domain.

## Naming Conventions

Use domain prefixes in public APIs because the crate is re-exported through one
facade:

- `Scheduler*` for public structs and enums consumed by callers, such as
  `SchedulerRankingContext` and `SchedulerRequestCandidateStatusUpdate`.
- `provider_key_*` for key-level health, RPM, and bucket helpers.
- `count_recent_*` for pure counters over `StoredRequestCandidate` slices.
- `auth_constraints_allow_*` for auth gate predicates.
- `candidate_runtime_*` for runtime selectability and skip reason helpers.
- `build_*` for record/cache-key builders that return concrete output structs.
- `resolve_*` for model or candidate slot resolution with fallback order.

Example:

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

## Where New Code Belongs

Add new scheduling rules by the data they read and the decision they make:

- auth restrictions: `auth.rs`;
- model/provider model mapping: `model.rs`;
- static candidate row filtering: `candidate/enumeration.rs`;
- runtime request history, provider-key health, cooldown, circuit breaker, or
  RPM: `health.rs` and `candidate/selectability.rs`;
- candidate ordering, tie-breakers, and ranking explanations: `ranking/`;
- request-candidate persistence payload shape: `request_candidate.rs`;
- provider quota snapshots or provider-level concurrency maps: `provider.rs`.

If a change requires IO, app state, database connections, HTTP responses, or
tracing spans, the implementation belongs in a caller crate. This crate should
receive prepared records and return deterministic values.

## Anti-Patterns

DON'T add runtime concerns here:

```rust
// Wrong for this crate: scheduler-core has no stateful runtime boundary.
pub async fn load_candidates(db: &DatabaseConnection) -> Result<Vec<_>, DbErr> { ... }
```

DON'T expose every helper through `lib.rs`. Keep algorithm helpers private, like
`compare_candidate_identity_for_ranking`:

```rust
fn compare_candidate_identity_for_ranking(
    left: &SchedulerRankableCandidate,
    right: &SchedulerRankableCandidate,
) -> std::cmp::Ordering
```

Source: `crates/aether-scheduler-core/src/ranking/mod.rs:18`.
