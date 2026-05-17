# aether-pool-core — Backend Spec Index

## Package Summary

**Crate**: `aether-pool-core` (v0.1.0)
**Location**: `crates/aether-pool-core/`
**Description**: Provider-independent pool scheduling and scoring primitives for Aether
**Evidence**: `crates/aether-pool-core/Cargo.toml` — `description = "Provider-independent pool scheduling and scoring primitives for Aether"`

## Analysis Evidence

- GitNexus `context(name="run_pool_scheduler", repo="Aether")` resolves the symbol at
  `crates/aether-pool-core/src/scheduler.rs:105-166`. Its direct production caller
  is `apps/aether-gateway/src/dispatch/pool_scheduler.rs:860-932`
  (`apply_local_execution_pool_scheduler_with_runtime_map`), with this crate's
  scheduler tests as additional callers.
- GitNexus `context(name="score_pool_member_with_rules", repo="Aether")` resolves the
  symbol at `crates/aether-pool-core/src/scoring.rs:165-233`. Its production caller
  is `apps/aether-gateway/src/ai_serving/planner/pool_scores.rs:13-53`
  (`build_provider_key_pool_score_upsert`).
- ABCoder MCP functions were not exposed in this Codex tool surface, but the local
  ABCoder UniAST artifact exists at `/Users/mayrain/abcoder-asts/aether-pool-core-ast.json`.
  It reports `ASTVersion v0.1.4`, `ToolVersion v0.3.1`, path
  `/Volumes/mayrain/workspace/Aether/crates/aether-pool-core`, one module
  (`aether-pool-core`), three packages (`aether-pool-core`,
  `aether-pool-core::scheduler`, `aether-pool-core::scoring`), and the three source
  files `src/lib.rs`, `src/scheduler.rs`, and `src/scoring.rs`.

### Dependencies

| Dependency | Purpose |
|------------|---------|
| `aether-data-contracts` | Stored type contracts (`PoolMemberHardState`, `PoolMemberIdentity`, `PoolScoreScope`, etc.) |
| `serde_json` | Score reason JSON construction |

### Constraints

- Pure computation crate: **no I/O, no async, no database, no logging**
- Build and test: `cargo test -p aether-pool-core`

## Public API Surface

### Scheduler (re-exported from `scheduler.rs`)

| Symbol | Kind | Description |
|--------|------|-------------|
| `run_pool_scheduler` | fn | Top-level entry: groups candidates, applies presets, produces `PoolSchedulerOutcome` |
| `normalize_enabled_pool_presets` | fn | Deduplicates and orders enabled presets |
| `PoolCandidateInput<Candidate>` | struct | Input wrapper: candidate + facts + pool_config + key_context |
| `PoolCandidateFacts` | struct | Per-candidate identity (provider, endpoint, model, key, priority) |
| `PoolCandidateOrchestration` | struct | Output annotation: group_id + pool_key_index |
| `PoolMemberSignals` | struct | Per-candidate runtime signals (plan tier, quota, health, latency, LRU) |
| `PoolRuntimeState` | struct | Per-provider runtime state (sticky key, cooldowns, cost usage, latency, LRU) |
| `PoolScheduledCandidate<Candidate>` | struct | Successful output: candidate + orchestration |
| `PoolSkippedCandidate<Candidate>` | struct | Skipped output: candidate + skip_reason |
| `PoolSchedulerOutcome<Candidate>` | struct | Combined result: candidates + skipped_candidates |
| `PoolSchedulingConfig` | struct | Scheduling behavior config (presets, LRU, skip exhausted, cost limit) |
| `PoolSchedulingPreset` | struct | Single preset entry (name, enabled, mode) |
| `POOL_ACCOUNT_BLOCKED_SKIP_REASON` | const | `"pool_account_blocked"` |
| `POOL_ACCOUNT_EXHAUSTED_SKIP_REASON` | const | `"pool_account_exhausted"` |
| `POOL_COOLDOWN_SKIP_REASON` | const | `"pool_cooldown"` |
| `POOL_COST_LIMIT_REACHED_SKIP_REASON` | const | `"pool_cost_limit_reached"` |

### Scoring (re-exported from `scoring.rs`)

| Symbol | Kind | Description |
|--------|------|-------------|
| `score_pool_member` | fn | Score with default rules |
| `score_pool_member_with_rules` | fn | Score with custom rules |
| `probe_freshness_score` | fn | Probe-based freshness (30-min TTL) |
| `probe_freshness_score_with_ttl` | fn | Probe freshness with configurable TTL |
| `PoolMemberScoreInput` | struct | 22-field scoring input |
| `PoolMemberScoreOutput` | struct | Score + hard_state + score_reason JSON |
| `PoolMemberScoreWeights` | struct | 6 weight factors (default: 0.30/0.20/0.15/0.15/0.10/0.10) |
| `PoolMemberScoreRules` | struct | Rules bundle: weights + penalties + thresholds |
| `POOL_SCORE_VERSION` | const | `1u64` |
| `PROBE_FRESHNESS_TTL_SECONDS` | const | `1800` (30 min) |
| `UNSCHEDULABLE_SCORE_CAP` | const | `0.05` |
| `PROBE_FAILURE_PENALTY` | const | `0.05` |
| `REQUEST_FAILURE_PENALTY` | const | `0.005` |
| `PROBE_FAILURE_COOLDOWN_THRESHOLD` | const | `3` |

## Guidelines Index

| File | Contents |
|------|----------|
| `directory-structure.md` | Crate layout, module ownership, expansion rules |
| `scheduler-domain-patterns.md` | Scheduling algorithm flow, distribution modes, presets, sticky affinity, I/O contracts |
| `error-handling.md` | Skip-reason contracts, how errors manifest as skipped candidates |
| `quality-guidelines.md` | Pure-function discipline, scoring weight versioning, test coverage, anti-patterns |

## Known Consumers

| Consumer | File | Usage |
|----------|------|-------|
| `aether-gateway` | `apps/aether-gateway/src/dispatch/pool_scheduler.rs:860` (`apply_local_execution_pool_scheduler_with_runtime_map`) | Calls `run_pool_scheduler` with runtime map for local execution |
| `aether-gateway` | `apps/aether-gateway/src/ai_serving/planner/pool_scores.rs:30` | Calls `score_pool_member_with_rules` for provider key pool score upsert |
| `aether-ai-serving` | `crates/aether-ai-serving/src/lib.rs:25-52` | Re-exports entire public API with `Ai`-prefixed aliases |
| `aether-provider-pool` | `crates/aether-provider-pool/src/service.rs:123`, `src/provider.rs:4`, `src/presets.rs:3` | Builds pool member signals and preset contracts |
| `aether-gateway` architecture tests | `apps/aether-gateway/src/tests/architecture/ai_serving.rs:1354-1384` | Verifies public scheduler exports and forbids provider-specific terms in this crate |

## Pre-development Checklist

Before modifying this crate:

- [ ] No `async`, `tokio`, `tracing`, or DB imports added
- [ ] All new functions remain pure (inputs in, outputs out, no side effects)
- [ ] New scheduling presets added to `build_pool_sort_vectors` match-case and `pool_preset_mutex_group`
- [ ] New skip reasons are `&'static str` constants, not format strings
- [ ] Scoring weight changes bump `POOL_SCORE_VERSION`
- [ ] Tests added alongside new logic (scheduler tests at `scheduler.rs:808+`, scoring tests at `scoring.rs:346+`)
- [ ] Provider-specific strings such as `codex`, `kiro`, `chatgpt_web`, or `provider_type` stay out of `scheduler.rs` and `lib.rs`
- [ ] `cargo test -p aether-pool-core` passes

## Quality Gate

```bash
cargo test -p aether-pool-core
```

Expected: all tests pass. Test count: ~500 lines of scheduler tests (8 test functions) + ~190 lines of scoring tests (6 test functions).
