# aether-pool-core — Directory Structure

## Crate Layout

```
crates/aether-pool-core/
├── Cargo.toml              (11 lines — name, version, 2 deps)
└── src/
    ├── lib.rs              (17 lines — re-exports only, no logic)
    ├── scheduler.rs        (1301 lines — pool scheduling algorithm + ~500 lines of tests)
    └── scoring.rs          (540 lines — member scoring + ~190 lines of tests)
```

Total source size: 1,858 lines including tests (`wc -l` on `src/lib.rs`,
`src/scheduler.rs`, and `src/scoring.rs`).

ABCoder UniAST evidence from `/Users/mayrain/abcoder-asts/aether-pool-core-ast.json`
matches this layout: one module named `aether-pool-core`, three packages
(`aether-pool-core`, `aether-pool-core::scheduler`, `aether-pool-core::scoring`),
and three source files (`src/lib.rs`, `src/scheduler.rs`, `src/scoring.rs`).

## Module Ownership

### `lib.rs` — Re-export surface

Pure re-exports. No logic, no types defined here. Every public symbol comes from `scheduler` or `scoring`:

```rust
// crates/aether-pool-core/src/lib.rs
pub use scheduler::{ run_pool_scheduler, normalize_enabled_pool_presets, /* ... */ };
pub use scoring::{ score_pool_member, probe_freshness_score, /* ... */ };
```

### `scheduler.rs` — Pool Scheduling Engine

**Owner**: scheduling algorithm and candidate lifecycle

| Section | Lines | Responsibility |
|---------|-------|---------------|
| Types & constants | 1-107 | Public I/O types (`PoolCandidateInput`, `PoolScheduledCandidate`, etc.) and skip-reason constants |
| `run_pool_scheduler` | 105-167 | Top-level entry: groups by `PoolGroupKey`, dispatches to `schedule_pool_group` |
| `schedule_pool_group` | 195-324 | Core algorithm: skip filters, sticky extraction, sort, annotate |
| Sort-vector builders | 352-400 | `build_pool_sort_vectors` — assembles composite sort keys from presets |
| Preset rank functions | 402-567 | One function per preset: `priority_first_ranks`, `load_balance_ranks`, etc. |
| Utility functions | 569-729 | `stable_hash_score`, `plan_priority_score`, `runtime_lru_score`, `cost_penalty`, etc. |
| Preset normalization | 731-807 | `normalize_enabled_pool_presets`, mutex group logic, dedup |
| Tests | 809-1301 | 8 test functions covering grouping, skipping, sticky, distribution modes, presets |

### `scoring.rs` — Pool Member Scoring

**Owner**: health/score computation for individual pool members

| Section | Lines | Responsibility |
|---------|-------|---------------|
| Constants | 1-11 | `POOL_SCORE_VERSION`, `PROBE_FRESHNESS_TTL_SECONDS`, weight defaults |
| `PoolMemberScoreWeights` | 13-75 | Weight struct + `normalized()` + `as_reason_json()` |
| `PoolMemberScoreRules` | 77-128 | Rules struct + `effective()` (sanitizes defaults) |
| I/O types | 130-159 | `PoolMemberScoreInput` (22 fields), `PoolMemberScoreOutput` |
| `score_pool_member_with_rules` | 161-233 | Main scoring: 6 weighted factors + penalties + hard-state cap |
| Helper functions | 235-344 | `derive_hard_state`, `manual_priority_score`, `probe_freshness_score_with_ttl`, `latency_score`, `cost_lru_score` |
| Tests | 346-540 | 6 test functions covering hard state, penalties, custom rules, weight normalization |

## Expansion Rules

### When to add a new scheduling preset

1. Add a rank function (e.g., `my_preset_ranks`) following the pattern in `scheduler.rs:402-567`
2. Register the preset name in `build_pool_sort_vectors` match-arm (`scheduler.rs:374-388`)
3. If it is a distribution mode, add it to `pool_preset_mutex_group` (`scheduler.rs:790-795`)
4. Add a test in the `#[cfg(test)] mod tests` block
5. Re-export any new public types from `lib.rs` if needed

### When to add a new scoring factor

1. Add the factor as a new field on `PoolMemberScoreWeights` (`scoring.rs:14`)
2. Add the factor to the `normalized()` method (`scoring.rs:37`)
3. Add the factor to the `as_reason_json()` method (`scoring.rs:65`)
4. Compute the raw factor in `score_pool_member_with_rules` (`scoring.rs:165`)
5. Add it to the weighted sum (`scoring.rs:187`)
6. Bump `POOL_SCORE_VERSION` (`scoring.rs:6`)
7. Add the factor to the "factors" section of `score_reason` JSON (`scoring.rs:207`)

### When to add a new skip reason

1. Define a `pub const POOL_*_SKIP_REASON: &str = "pool_*";` near `scheduler.rs:6-8`
2. Add the skip check in `schedule_pool_group` before the "available" push (`scheduler.rs:210-263`)
3. Re-export from `lib.rs`
4. Add a test verifying the skip reason appears in `skipped_candidates`

### What NOT to add

- No new files without strong justification (the crate is intentionally 3 files)
- No `async` functions, `tokio`, or any async runtime dependency
- No `tracing`, `log`, or any logging dependency
- No database or repository traits
- No network or I/O code
