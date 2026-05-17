# aether-pool-core — Error Handling

## Core Principle: No Result Types

This crate does not use `Result<T, E>` for its main scheduling and scoring functions. All outputs are deterministic data structures that capture every possible outcome without error propagation.

```rust
// crates/aether-pool-core/src/scheduler.rs:84-87
pub struct PoolSchedulerOutcome<Candidate> {
    pub candidates: Vec<PoolScheduledCandidate<Candidate>>,
    pub skipped_candidates: Vec<PoolSkippedCandidate<Candidate>>,
}
```

Every input candidate ends up in exactly one of these two lists. No candidate is silently dropped.

## How Errors Manifest as Skipped Candidates

Instead of returning errors, the scheduler converts runtime problems into `PoolSkippedCandidate` entries with static string reasons:

```rust
// crates/aether-pool-core/src/scheduler.rs:77-81
pub struct PoolSkippedCandidate<Candidate> {
    pub candidate: Candidate,
    pub skip_reason: &'static str,
}
```

The `&'static str` type is intentional -- skip reasons are not format strings or dynamic messages. They are compile-time constants used by consumers for logging, metrics, and decision-making.

## Skip Reason Constants

Defined at the top of `scheduler.rs`:

```rust
// crates/aether-pool-core/src/scheduler.rs:5-8
pub const POOL_ACCOUNT_BLOCKED_SKIP_REASON: &str = "pool_account_blocked";
pub const POOL_ACCOUNT_EXHAUSTED_SKIP_REASON: &str = "pool_account_exhausted";
pub const POOL_COOLDOWN_SKIP_REASON: &str = "pool_cooldown";
pub const POOL_COST_LIMIT_REACHED_SKIP_REASON: &str = "pool_cost_limit_reached";
```

## Skip Conditions and Order

Within `schedule_pool_group` (scheduler.rs:211-263), skip filters are applied in strict order. A candidate that matches the first applicable condition is skipped immediately -- subsequent filters are not evaluated:

| Priority | Condition | Skip Reason | Source |
|----------|-----------|-------------|--------|
| 1 | `key_context.account_blocked == true` | `POOL_ACCOUNT_BLOCKED_SKIP_REASON` | scheduler.rs:219-224 |
| 2 | `pool_config.skip_exhausted_accounts && key_context.quota_exhausted` | `POOL_ACCOUNT_EXHAUSTED_SKIP_REASON` | scheduler.rs:227-233 |
| 3 | `runtime.cooldown_reason_by_key.contains_key(&key_id)` | `POOL_COOLDOWN_SKIP_REASON` | scheduler.rs:235-241 |
| 4 | `runtime_cost_usage(runtime, key_id) >= cost_limit_per_key_tokens` | `POOL_COST_LIMIT_REACHED_SKIP_REASON` | scheduler.rs:243-252 |

The order matters because:
- Account-blocked is a hard gate (never recoverable within a scheduling cycle)
- Quota-exhausted is semi-permanent but only checked when `skip_exhausted_accounts` is enabled
- Cooldown is transient (key may recover)
- Cost limit is a soft cap with explicit token threshold

## Scoring Error Handling

Scoring also avoids Result types. Instead, problematic inputs degrade gracefully:

### Hard State Derivation (scoring.rs:243-277)

`derive_hard_state` returns an enum that determines schedulability:

```rust
// Derived from PoolMemberHardState in aether-data-contracts:
// Inactive -> not schedulable
// Banned -> not schedulable
// AuthInvalid -> not schedulable
// QuotaExhausted -> not schedulable
// Cooldown -> not schedulable
// Unknown -> schedulable (optimistic)
// Available -> schedulable
```

When `!hard_state.schedulable()`, the score is capped at `UNSCHEDULABLE_SCORE_CAP` (0.05) regardless of other factors:

```rust
// crates/aether-pool-core/src/scoring.rs:199-201
if !hard_state.schedulable() {
    score = score.min(rules.unschedulable_score_cap);
}
```

### Graceful Degradation Patterns

| Missing Input | Fallback | Location |
|---------------|----------|----------|
| `health_score = None` | `0.5` (neutral) | scoring.rs:173 |
| `quota_usage_ratio = None` | `0.5` (neutral) | scoring.rs:183 |
| `success_count = 0` or `total_response_time_ms = 0` | `0.5` latency (neutral) | scoring.rs:317-319 |
| `last_used_at = None` | `0.25` LRU bonus (unused key gets a boost) | scoring.rs:338-340 |
| `last_probe_success_at = None` | `0.0` probe freshness | scoring.rs:305 |
| `probe_status != Ok` | `0.0` probe freshness | scoring.rs:302 |
| Non-finite weight values | Clamped to `0.0` | scoring.rs:235-241 |
| All-zero weights | Left as-is (no division by zero) | scoring.rs:52-54 |

### Weight Normalization Safety

```rust
// crates/aether-pool-core/src/scoring.rs:37-63
pub fn normalized(self) -> Self {
    // ...sanitize each weight to finite_non_negative...
    let total = sanitized.manual_priority + /* ... */;
    if total <= f64::EPSILON {
        return sanitized;  // all zeros stays all zeros, no division by zero
    }
    // divide each by total to normalize to sum=1.0
}
```

### Rules Sanitization

`PoolMemberScoreRules::effective()` (scoring.rs:101-128) replaces invalid values with defaults:
- `probe_freshness_ttl_seconds = 0` falls back to `PROBE_FRESHNESS_TTL_SECONDS` (1800)
- Non-finite `unschedulable_score_cap` falls back to default (0.05)
- Non-finite penalty values fall back to their respective defaults
- All values are clamped to valid ranges (0.0-1.0 for scores and penalties)

## Anti-Patterns

- **Never** add `Result<T, E>` to scheduler or scoring functions. The design intentionally uses sum-type outputs.
- **Never** use format strings for skip reasons. They must remain `&'static str` so consumers can match on them.
- **Never** panic on invalid input. All inputs degrade gracefully to neutral defaults.
- **Never** add `unwrap()` or `expect()` on user-controlled data. The only `.expect()` in production code is `group.first().expect("group should exist")` (scheduler.rs:138), which is structurally guaranteed because groups are only created when candidates are pushed into them.
