# aether-pool-core — Scheduler Domain Patterns

## Algorithm Overview

The scheduler follows a strict **group -> annotate or skip -> sort -> schedule** pipeline. There are no error returns -- every input candidate becomes either a `PoolScheduledCandidate` or a `PoolSkippedCandidate`.

```
run_pool_scheduler(candidates, runtime_by_provider, seed_nonce)
  |
  +-- Group candidates by PoolGroupKey (provider+endpoint+model+selected_model+format+singleton_key)
  |     |
  |     +-- Per group:
  |           |
  |           +-- If no pool_config: annotate directly (no pool_key_index)
  |           |
  |           +-- schedule_pool_group():
  |                 |
  |                 +-- Phase 1: SKIP FILTERS
  |                 |     account_blocked -> skipped
  |                 |     quota_exhausted (if skip_exhausted_accounts) -> skipped
  |                 |     cooldown_reason_by_key contains key -> skipped
  |                 |     cost_limit_per_key_tokens exceeded -> skipped
  |                 |
  |                 +-- Phase 2: STICKY EXTRACTION (if cache_affinity preset)
  |                 |     Remove sticky-hit candidate from available pool
  |                 |
  |                 +-- Phase 3: SORT (build_pool_sort_vectors)
  |                 |     Distribution mode vector first, then strategy presets
  |                 |     If no presets: LRU-only ranking
  |                 |
  |                 +-- Phase 4: ANNOTATE
  |                       Sticky candidate first, then sorted candidates
  |                       Each gets candidate_group_id + pool_key_index
```

## Input Contracts

### `PoolCandidateInput<Candidate>` (scheduler.rs:63)

Generic over `Candidate` so callers can embed their own domain type. The scheduler never inspects `Candidate` -- it only sorts and passes it through.

```rust
// crates/aether-pool-core/src/scheduler.rs:63-69
pub struct PoolCandidateInput<Candidate> {
    pub candidate: Candidate,
    pub facts: PoolCandidateFacts,
    pub pool_config: Option<PoolSchedulingConfig>,
    pub key_context: PoolMemberSignals,
}
```

- `pool_config = None` means the candidate is NOT in a pool -- it gets no reordering, just annotation.
- `pool_config = Some(...)` enables the full scheduling pipeline for that candidate.

### `PoolCandidateFacts` (scheduler.rs:47)

Identity fields used for grouping and sorting:

```rust
// crates/aether-pool-core/src/scheduler.rs:47-55
pub struct PoolCandidateFacts {
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
    pub selected_provider_model_name: String,
    pub provider_api_format: String,
    pub key_id: String,
    pub key_internal_priority: i32,
}
```

### `PoolRuntimeState` (scheduler.rs:25)

Per-provider runtime signals injected from outside:

```rust
// crates/aether-pool-core/src/scheduler.rs:25-32
pub struct PoolRuntimeState {
    pub sticky_bound_key_id: Option<String>,
    pub cooldown_reason_by_key: BTreeMap<String, String>,
    pub cost_window_usage_by_key: BTreeMap<String, u64>,
    pub latency_avg_ms_by_key: BTreeMap<String, f64>,
    pub lru_score_by_key: BTreeMap<String, f64>,
}
```

## Output Contracts

### `PoolSchedulerOutcome<Candidate>` (scheduler.rs:84)

```rust
// crates/aether-pool-core/src/scheduler.rs:84-87
pub struct PoolSchedulerOutcome<Candidate> {
    pub candidates: Vec<PoolScheduledCandidate<Candidate>>,
    pub skipped_candidates: Vec<PoolSkippedCandidate<Candidate>>,
}
```

**Guarantee**: `candidates.len() + skipped_candidates.len()` equals the input count (no candidate is lost).

### `PoolCandidateOrchestration` (scheduler.rs:57)

```rust
// crates/aether-pool-core/src/scheduler.rs:57-61
pub struct PoolCandidateOrchestration {
    pub candidate_group_id: Option<String>,
    pub pool_key_index: Option<u32>,
}
```

- `candidate_group_id`: format `provider={}|endpoint={}|model={}|selected_model={}|api_format={}|singleton_key={}`
- `pool_key_index`: position within the sorted pool group (0-based). `None` for non-pool candidates.

## Grouping

Candidates are grouped by `PoolGroupKey` (scheduler.rs:89-97), which includes `provider_id`, `endpoint_id`, `model_id`, `selected_provider_model_name`, and `provider_api_format`. When pooling is disabled for a candidate, the `key_id` is included in the group key via `singleton_key_id`, making each non-pool candidate its own group.

Groups preserve insertion order via `group_order` vector. Within each group, candidates are reordered by the scheduling algorithm. Groups are then emitted in the original group order.

**Test evidence** (`scheduler.rs:813-844`):
```rust
// Input: [pool-A, other, pool-B]  (pool-A and pool-B share a group)
// Output candidates: [key-pool-b, key-pool-a, key-other]
//   -- pool-b has lower LRU score (10.0 < 20.0) so it sorts first within the pool group
//   -- "other" is non-pool, forms its own group, emitted after the pool group
```

## Skip Filters

Applied in order within `schedule_pool_group` (scheduler.rs:211-263). A candidate that fails any filter is immediately moved to `skipped`:

| Filter | Condition | Skip Reason |
|--------|-----------|-------------|
| Account blocked | `key_context.account_blocked == true` | `POOL_ACCOUNT_BLOCKED_SKIP_REASON` |
| Quota exhausted | `pool_config.skip_exhausted_accounts && key_context.quota_exhausted` | `POOL_ACCOUNT_EXHAUSTED_SKIP_REASON` |
| Cooldown | `runtime.cooldown_reason_by_key.contains_key(&key_id)` | `POOL_COOLDOWN_SKIP_REASON` |
| Cost limit | `runtime_cost_usage(runtime, key_id) >= pool_config.cost_limit_per_key_tokens` | `POOL_COST_LIMIT_REACHED_SKIP_REASON` |

**Test evidence** (`scheduler.rs:848-893`):
```rust
// Input: [key-ready, key-cooldown, key-cost] with cost_limit=100
// Runtime: cooldown for "key-cooldown", cost usage=100 for "key-cost"
// Output candidates: [key-ready]
// Skipped: [(key-cooldown, "pool_cooldown"), (key-cost, "pool_cost_limit_reached")]
```

## Distribution Modes

Distribution modes are mutually exclusive preset types that control how candidates are ordered. Exactly one can be active per scheduling pass. They form a "distribution mode" mutex group.

| Preset | Behavior | Mutex Group |
|--------|----------|-------------|
| `lru` | LRU-only ranking (enables `lru_enabled` path, not a sort vector preset) | distribution_mode |
| `cache_affinity` | LRU ranking + sticky affinity promotion | distribution_mode |
| `load_balance` | Deterministic random shuffle via stable hash | distribution_mode |
| `single_account` | Priority-first, then reverse-LRU for tie-breaking | distribution_mode |

### `lru` mode

When `lru` is the only enabled preset, it is filtered out by normalization (scheduler.rs:776: `filter(|_, preset, _| preset != "lru")`). Instead, the code falls through to the LRU-only path at scheduler.rs:304-312, which uses `lru_rank_indices` directly.

### `cache_affinity` mode

Enables sticky affinity (see below). Uses `cache_affinity_ranks` (which are LRU ranks with `descending=true`) as its sort vector.

### `load_balance` mode

Generates a stable pseudo-random score using `stable_hash_score(format!("{seed}:{key_id}"))`. The seed is composed of provider+endpoint+model+selected_model+nonce. This ensures the same key gets a different position for different requests while remaining deterministic within a single request.

**Key property**: Load balance ignores sticky affinity. When `load_balance` is the distribution mode, `pool_sticky_enabled` returns false (scheduler.rs:402-406).

**Test evidence** (`scheduler.rs:937-977`):
```rust
// Two keys with load_balance preset, nonce chosen so key-b hashes before key-a
// Even though sticky_bound_key_id = Some("key-a"), key-b comes first
// Result: [key-b, key-a]
```

### `single_account` mode

Orders by `key_internal_priority` ascending (lower = higher priority), then by reverse-LRU rank (most recently used first), then by original index. This ensures the highest-priority, most-recently-used key is tried first.

**Test evidence** (`scheduler.rs:1125-1193`):
```rust
// Keys: priority-old (prio=10, LRU=10), priority-recent (prio=10, LRU=200), lower-priority-recent (prio=50, LRU=500)
// Single account sorts by: priority ascending, then reverse-LRU (higher LRU = more recent = first)
// Result: [priority-recent, priority-old, lower-priority-recent]
```

## Strategy Presets

Strategy presets are non-distribution presets that add additional sort dimensions. They are applied AFTER the distribution mode vector, creating a composite sort key (tuple of ranks).

| Preset | Sort Metric | Source |
|--------|------------|--------|
| `priority_first` | `key_internal_priority` ascending | `PoolCandidateFacts` |
| `plus_first` | Plan tier (plus/pro preferred) | `PoolMemberSignals.plan_tier` |
| `pro_first` | Plan tier (pro preferred) | `PoolMemberSignals.plan_tier` |
| `free_first` | Plan tier (free preferred) | `PoolMemberSignals.plan_tier` |
| `team_first` | Plan tier (team preferred) | `PoolMemberSignals.plan_tier` |
| `health_first` | Health score descending | `PoolMemberSignals.health_score` |
| `latency_first` | Latency ascending (lower = better) | `PoolMemberSignals.latency_avg_ms` |
| `cost_first` | Cost penalty ascending | `PoolRuntimeState.cost_window_usage_by_key` |
| `quota_balanced` | Quota usage ratio ascending | `PoolMemberSignals.quota_usage_ratio` |
| `recent_refresh` | Quota reset time ascending (soonest first) | `PoolMemberSignals.quota_reset_seconds` |

All strategy presets degrade gracefully: if no candidates have the relevant signal (all `None`), `neutral_rank_indices` returns rank 0 for everyone (no effect on sort order).

### Composite Sort Vectors

`build_pool_sort_vectors` (scheduler.rs:352-400) constructs a `BTreeMap<String, Vec<usize>>` -- a vector of ranks per key_id. The first entry is the distribution mode rank (or LRU rank if `lru_enabled`), followed by one rank per strategy preset in order. The final sort compares these vectors lexicographically.

**Test evidence** (`scheduler.rs:1009-1065`):
```rust
// Keys: cache-hit (prio=50, LRU=200) and high-priority (prio=10, LRU=10)
// Presets: [cache_affinity, priority_first]
// cache_affinity sorts by LRU descending: cache-hit gets rank 0
// priority_first sorts by priority: high-priority gets rank 0
// Composite vector: cache-hit=[0,1], high-priority=[1,0]
// cache_affinity (distribution mode) wins first dimension: cache-hit comes first
```

### Load-balance overrides priority

When `load_balance` is the distribution mode, its random hash is the first sort dimension. Even if `priority_first` is also enabled, the random hash dominates:

**Test evidence** (`scheduler.rs:1069-1122`):
```rust
// Keys: random-first (prio=50) and high-priority (prio=10)
// Presets: [load_balance, priority_first]
// Nonce chosen so random-first hashes lower than high-priority
// Result: [random-first, high-priority] -- random distribution overrides priority
```

## Preset Normalization

`normalize_enabled_pool_presets` (scheduler.rs:731-788) processes the raw preset list:

1. **Deduplicate**: trims, lowercases, skips duplicates and empty strings
2. **Classify**: distribution modes (lru, cache_affinity, load_balance, single_account) vs strategy presets
3. **Select distribution mode**: earliest-indexed enabled distribution mode wins (others ignored)
4. **Special case**: `lru` distribution mode is filtered out (it activates the LRU-only code path instead of a sort vector)
5. **Order**: distribution mode first, then strategy presets in their original order

**Test evidence** (`scheduler.rs:1196-1244`):
```rust
// Input: [lru(disabled), single_account(enabled), cache_affinity(enabled), priority_first(enabled)]
// Result: ["single_account", "priority_first"]
//   -- lru is disabled, single_account wins distribution mode, cache_affinity loses mutex
```

## Sticky Affinity

Sticky affinity is activated by the `cache_affinity` preset. When enabled:

1. `pool_sticky_enabled` returns true (scheduler.rs:402-406)
2. If `runtime.sticky_bound_key_id` matches an available candidate, that candidate is removed from the sort pool
3. After sorting, the sticky candidate is prepended to the front of the ordered list
4. This ensures the previously-used key is tried first for cache locality

**Test evidence** (`scheduler.rs:896-933`):
```rust
// Two keys: key-a and key-b, both with cache_affinity preset
// Runtime: sticky_bound_key_id = "key-a", LRU scores: key-a=50.0, key-b=10.0
// key-a is extracted as sticky, remaining pool sorts key-b first (lower LRU)
// Final order: [key-a (sticky), key-b (sorted)]
```

## Preset-Plan Interaction

Plan presets (`plus_first`, `pro_first`, `free_first`, `team_first`) use `plan_priority_score` (scheduler.rs:688-729) to map plan tiers to numeric scores. The mapping is different per mode:

| Mode | Score 0.0 (best) | Score 0.3 | Score 0.5 | Score 0.6 | Score 0.7 | Score 0.8 |
|------|-----------|------|------|------|------|------|
| `plus_only` | plus/pro | -- | free/team | -- | other | None |
| `pro_only` | pro | plus | enterprise/business | -- | free/team/other | None |
| `free_only` | free | -- | team | enterprise/business | plus/pro | other/None |
| `team_only` | team | -- | free | enterprise/business | plus/pro | other/None |

**Test evidence** (`scheduler.rs:980-1007`):
```rust
// Keys: key-free (plan=free) and key-plus (plan=plus), preset=plus_first
// plus_first gives plus plan score 0.0 (best), free plan score 0.7
// Result: [key-plus, key-free]
```

## Test Patterns

Tests in `scheduler.rs:809-1301` follow a consistent pattern:

1. **Build candidates** using `sample_candidate(provider, endpoint, key_id, priority, pool_enabled)`
2. **Customize** with `.with_presets(...)`, `.with_plan(...)`, `.with_cost_limit(...)`
3. **Build runtime** as `BTreeMap<String, PoolRuntimeState>`
4. **Call `run_pool_scheduler`** with candidates, runtime, and seed nonce
5. **Assert** on `outcome.candidates` (order and content) and `outcome.skipped_candidates` (reasons)

The `TestCandidateExt` trait (scheduler.rs:1275-1300) provides builder-style helpers for test setup.
