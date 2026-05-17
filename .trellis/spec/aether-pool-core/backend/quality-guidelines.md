# aether-pool-core — Quality Guidelines

## Design Boundary

Keep `crates/aether-pool-core` a pure, synchronous computation crate. It owns
provider-independent scheduling and scoring rules only. It must not load
catalog rows, call repositories, spawn tasks, emit logs, or know provider
brands.

Cargo evidence:

```toml
# crates/aether-pool-core/Cargo.toml:9-11
[dependencies]
aether-data-contracts.workspace = true
serde_json.workspace = true
```

Those two dependencies are the intended boundary:

- `aether-data-contracts` supplies stored score contract types such as
  `PoolMemberHardState`, `PoolMemberIdentity`, `PoolMemberProbeStatus`, and
  `PoolScoreScope`.
- `serde_json` is used only for deterministic `score_reason` JSON construction.

Do not add dependencies for async runtimes, HTTP clients, database access,
logging, background workers, random-number generators, provider adapters, or
gateway state. Put those concerns in caller crates and pass plain facts into
this crate.

## Public Surface Discipline

All public API goes through `src/lib.rs`; the modules themselves remain private:

```rust
// crates/aether-pool-core/src/lib.rs:1-17
mod scheduler;
mod scoring;

pub use scheduler::{
    normalize_enabled_pool_presets, run_pool_scheduler, PoolCandidateFacts,
    PoolCandidateInput, PoolRuntimeState, PoolSchedulerOutcome,
    PoolSchedulingConfig, PoolSchedulingPreset,
};
pub use scoring::{
    score_pool_member, score_pool_member_with_rules, PoolMemberScoreInput,
    PoolMemberScoreOutput, PoolMemberScoreRules, PoolMemberScoreWeights,
    POOL_SCORE_VERSION,
};
```

When adding a public type or constant, add it in `scheduler.rs` or `scoring.rs`
and then explicitly re-export it from `lib.rs`. Do not expose `pub mod
scheduler;` or `pub mod scoring;` as a shortcut; that would turn helper
functions into API by accident.

## Deterministic Scheduling

Scheduler output order is user-visible through gateway dispatch, so every
ordering path needs explicit tie-breakers:

- `run_pool_scheduler` preserves group insertion order with `group_order` while
  using `BTreeMap` for deterministic lookup (`scheduler.rs:110-125`).
- Pool groups sort by lexicographic rank vectors and then `original_index`
  (`scheduler.rs:298-303`).
- LRU fallback also ends with `original_index` (`scheduler.rs:305-311`).
- Generic rank construction sorts by signal presence, score, and original index
  (`scheduler.rs:631-654`).

Do not replace these ordered maps or tie-breakers with unordered collection
iteration. If a new preset needs randomness, follow `load_balance_ranks`: derive
a deterministic hash from `group_sort_seed` plus `key_id`
(`scheduler.rs:550-589`) and test the nonce-driven ordering.

## Scheduler Test Expectations

The scheduler tests are executable examples for policy behavior. New scheduling
logic should add or update tests in `scheduler.rs:809-1301`, following the
existing pattern:

```rust
// crates/aether-pool-core/src/scheduler.rs:830-834
let outcome = run_pool_scheduler(
    vec![pool_first, other, pool_second],
    &runtime_by_provider,
    "seed",
);
```

Coverage expectations:

- Grouping and internal key reordering: `pool_scheduler_groups_interleaved_candidates_and_reorders_internal_keys`
- Skip reasons: `pool_scheduler_skips_cooldown_and_cost_exhausted_keys`
- Sticky promotion: `pool_scheduler_promotes_sticky_hit_before_other_sorted_keys`
- Load-balance dominance over sticky and priority: `load_balance_distribution_ignores_sticky_hit`,
  `load_balance_distribution_is_not_overridden_by_priority_strategy`
- Plan and strategy preset ordering: `pool_scheduler_uses_plan_preset_with_catalog_context`,
  `pool_scheduler_applies_distribution_mode_before_strategy_presets`
- Distribution-mode normalization: `normalizes_distribution_mode_before_strategy_presets`,
  `normalizes_lru_as_mutually_exclusive_distribution_mode`

Tests should assert both scheduled order and skipped candidate reasons. A test
that only checks length is not enough for a scheduler change.

## Scoring Versioning

`POOL_SCORE_VERSION` is part of the stored score contract:

```rust
// crates/aether-pool-core/src/scoring.rs:6-11
pub const POOL_SCORE_VERSION: u64 = 1;
pub const PROBE_FRESHNESS_TTL_SECONDS: u64 = 30 * 60;
pub const UNSCHEDULABLE_SCORE_CAP: f64 = 0.05;
pub const PROBE_FAILURE_PENALTY: f64 = 0.05;
pub const REQUEST_FAILURE_PENALTY: f64 = 0.005;
pub const PROBE_FAILURE_COOLDOWN_THRESHOLD: u64 = 3;
```

Bump `POOL_SCORE_VERSION` when changing the persisted scoring semantics:

- adding, removing, or reweighting score factors
- changing hard-state derivation
- changing default penalties, caps, TTLs, or cooldown thresholds
- changing `score_reason` JSON shape that downstream storage or analytics reads

Do not bump the version for test-only refactors, comments, or internal helper
renames that preserve output for the same input.

## Score Reason Contract

`score_pool_member_with_rules` builds a structured reason JSON with weights,
factors, rules, penalties, hard state, and score version:

```rust
// crates/aether-pool-core/src/scoring.rs:204-231
PoolMemberScoreOutput {
    score,
    hard_state,
    score_reason: json!({
        "weights": weights.as_reason_json(),
        "factors": {
            "manual_priority": manual_priority,
            "health": health,
            "probe_freshness": probe_freshness,
            "quota_remaining": quota_remaining,
            "latency": latency,
            "cost_lru": cost_lru
        },
        "score_version": POOL_SCORE_VERSION
    }),
}
```

Keep this JSON additive where possible. If a field is renamed or removed, update
gateway score upsert consumers and tests because `apps/aether-gateway/src/ai_serving/planner/pool_scores.rs:30-39`
stores `output.score`, `output.hard_state`, `POOL_SCORE_VERSION`, and
`output.score_reason`.

## Graceful Input Handling

This crate should fail closed into explicit output data, not panics or dynamic
errors:

- Missing health and quota signals degrade to neutral `0.5`
  (`scoring.rs:173`, `scoring.rs:180-183`).
- Missing or stale probe data returns freshness `0.0`
  (`scoring.rs:296-315`).
- Non-finite and negative weights are sanitized before normalization
  (`scoring.rs:37-63`, `scoring.rs:235-241`).
- Unschedulable hard states cap score to `UNSCHEDULABLE_SCORE_CAP`
  (`scoring.rs:199-201`).
- Scheduler runtime issues become static skip reasons in
  `PoolSkippedCandidate`, not `Result` errors.

Avoid `unwrap()` or `expect()` on caller-provided data. The existing production
`expect("group should exist")` in `run_pool_scheduler` (`scheduler.rs:136-140`)
is structural: a group exists only after at least one candidate was inserted.

## Provider-Agnostic Guardrail

The gateway architecture test explicitly checks this crate's public scheduler
surface and forbids provider-specific terms:

```rust
// apps/aether-gateway/src/tests/architecture/ai_serving.rs:1367-1384
let pool_core_scheduler = read_workspace_file("crates/aether-pool-core/src/scheduler.rs");
for forbidden in ["codex", "kiro", "chatgpt_web", "provider_type"] {
    assert!(
        !pool_core_scheduler.contains(forbidden) && !pool_core_lib.contains(forbidden),
        "aether-pool-core should stay provider-agnostic and not embed provider behavior {forbidden}"
    );
}
```

Do not add provider-specific preset names, provider-type branches, or model
brand logic here. Caller crates can translate provider-specific catalog facts
into `PoolCandidateFacts`, `PoolMemberSignals`, `PoolSchedulingConfig`, and
`PoolRuntimeState`.

## Anti-Patterns

- Adding `async fn`, `tokio`, channels, background queues, or timers.
- Adding `tracing`, `log`, metrics, or side-effectful diagnostics.
- Adding database repositories, gateway state, HTTP clients, or file access.
- Introducing dynamic skip messages or localized strings for scheduler reasons.
- Making sorting depend on `HashMap` iteration order or wall-clock time.
- Changing scoring weights, penalties, or reason JSON without tests and a
  `POOL_SCORE_VERSION` decision.
- Encoding provider names or product-specific account tiers outside the generic
  `plan_tier` and preset inputs.

## Quality Gate

Run the crate-local test suite after changes:

```bash
cargo test -p aether-pool-core
```

For public API, scoring JSON, or provider-agnostic boundary changes, also run
the downstream architecture test:

```bash
cargo test -p aether-gateway --test architecture -- ai_serving
```
