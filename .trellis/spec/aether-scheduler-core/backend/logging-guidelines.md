# Logging Guidelines

> Observability boundary for `crates/aether-scheduler-core`.

## Current Rule: No Logging In This Crate

`aether-scheduler-core` currently contains no `tracing` dependency and no
logging macros. A source scan under `crates/aether-scheduler-core/src` found no
`trace!`, `debug!`, `info!`, `warn!`, `error!`, `#[instrument]`, `println!`,
`dbg!`, or `eprintln!` usage.

That is intentional. The crate is pure scheduler logic and does not own request
ids, authenticated user context, HTTP response mapping, redaction policy,
database transactions, or retry execution. Callers should log decisions after
they attach the correct operational context.

## Return Structured Observability Data

Instead of logging, return stable values that caller crates can persist or log.

Runtime selectability returns skip reasons:

```rust
if provider_quota_blocks_requests {
    return Some("provider_quota_blocked");
}
if account_quota_exhausted {
    return Some("account_quota_exhausted");
}
if oauth_invalid {
    return Some("oauth_invalid");
}
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:56`.

More skip reasons are emitted for cooldown, concurrency, circuit breaker,
zero-health, and RPM exhaustion:

```rust
return Some("recent_failure_cooldown");
...
return Some("key_rpm_exhausted");
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:72` and
`crates/aether-scheduler-core/src/candidate/selectability.rs:125`.

Ranking returns a vector of outcomes instead of logging comparison decisions:

```rust
pub struct SchedulerRankingOutcome {
    pub original_index: usize,
    pub ranking_index: usize,
    pub priority_mode: SchedulerPriorityMode,
    pub ranking_mode: SchedulerRankingMode,
    pub priority_slot: i32,
    pub promoted_by: Option<&'static str>,
    pub demoted_by: Option<&'static str>,
}
```

Source: `crates/aether-scheduler-core/src/ranking/types.rs:122`.

Request-candidate reporting converts scheduler metadata into `extra_data` fields
that downstream persistence and logs can inspect:

```rust
if let Some(ranking_mode) = ranking_mode {
    extra_data.insert("ranking_mode".to_string(), Value::String(ranking_mode));
}
if let Some(priority_slot) = priority_slot {
    extra_data.insert(
        "priority_slot".to_string(),
        Value::Number(priority_slot.into()),
    );
}
if let Some(promoted_by) = promoted_by {
    extra_data.insert("promoted_by".to_string(), Value::String(promoted_by));
}
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:791`.

## Caller-Side Logging Contract

Callers should log around scheduler-core APIs with their own context. For
example, a gateway caller can log request id, user id, provider id, model name,
ranking outcome, and skip reason because it owns redaction and routing context.
Scheduler-core should not attempt to guess those fields.

Good scheduler-core pattern:

```rust
pub fn candidate_is_selectable_with_runtime_state(
    input: CandidateRuntimeSelectabilityInput<'_>,
) -> bool {
    candidate_runtime_skip_reason_with_state(input).is_none()
}
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:35`.

Caller-side logging can then use the reason function when a candidate is
rejected.

## Log Levels For Callers

These levels are recommendations for crates that call scheduler-core:

- `debug`: normal candidate filtering, ranking outcomes, and affinity decisions
  during request planning.
- `info`: final chosen provider/key when useful for request-level audit logs,
  after sensitive fields are redacted.
- `warn`: exhausted quota, circuit-open, repeated cooldown, or no selectable
  candidate when the request will fail over or return overload.
- `error`: malformed persisted scheduler data that surfaces as
  `DataLayerError::UnexpectedValue`, because it indicates a data-contract or
  migration problem.

Do not implement these logs in `aether-scheduler-core` itself.

## Sensitive Data

Do not log raw API keys, authorization headers, raw client session keys, full
unredacted upstream URLs, or model request bodies from this crate.

`affinity.rs` already avoids raw session key disclosure by hashing session keys
before composing v2 affinity cache keys:

```rust
let session_hash = hash_session_key(session_key);

Some(format!(
    "scheduler_affinity:v2:{api_key_id}:{api_format}:{global_model_name}:{client_family}:{session_hash}"
))
```

Source: `crates/aether-scheduler-core/src/affinity.rs:89`.

The test locks that raw session fragments are not present:

```rust
assert!(!cache_key.contains("conversation-123"));
assert!(!cache_key.contains("agent-7"));
```

Source: `crates/aether-scheduler-core/src/affinity.rs:211`.

Preserve this rule if adding observability metadata. Stable identifiers like
provider id, endpoint id, key id, candidate id, and ranking reason are safer
than raw credentials or session material.

## Structured Fields To Preserve

When adding new scheduler explanations, prefer stable machine fields over prose.
Existing examples include:

- skip reasons such as `"provider_quota_blocked"` and `"key_rpm_exhausted"`;
- ranking reasons exported as constants:

```rust
pub const RANKING_REASON_CACHED_AFFINITY: &str = "cached_affinity";
pub const RANKING_REASON_LOCAL_TUNNEL: &str = "local_tunnel";
pub const RANKING_REASON_CROSS_FORMAT: &str = "cross_format";
```

Source: `crates/aether-scheduler-core/src/ranking/reasons.rs:5`.

- request-candidate phase marker:

```rust
extra_data.insert("gateway_execution_runtime".to_string(), Value::Bool(true));
extra_data.insert("phase".to_string(), Value::String("3c_trial".to_string()));
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:721`.

If a caller needs log-friendly text, let the caller translate these fields.

## Anti-Patterns

DON'T add `tracing` to this crate just to debug one scheduling branch:

```rust
// Wrong boundary.
tracing::warn!(key_id = %candidate.key_id, "key rpm exhausted");
```

Return `Some("key_rpm_exhausted")` and let the runtime log it with request
context.

DON'T use `println!`, `dbg!`, or `eprintln!` in scheduler helpers. They cannot
be filtered by request id and can leak data in tests or production.

DON'T log raw report context. `request_candidate.rs` may carry `upstream_url`,
`header_rules`, `body_rules`, proxy metadata, and upstream response details in
`extra_data`. Caller crates must decide what is safe to emit.
