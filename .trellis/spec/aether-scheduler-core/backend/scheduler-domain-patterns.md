# Scheduler Domain Patterns

> Scheduler-specific implementation patterns for `crates/aether-scheduler-core`.

## Candidate Enumeration Pipeline

Candidate enumeration is a pure filtering and projection pass over
`StoredMinimalCandidateSelectionRow` values. Keep the order of gates explicit:
empty API format, auth API format, auth model, provider auth, streaming support,
provider model resolution, and finally priority extraction.

The public entry point delegates to the shared inner function:

```rust
pub fn enumerate_minimal_candidate_selection(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    enumerate_minimal_candidate_selection_inner(input, false)
}
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:10`.

The model-directive variant keeps the same policy but passes an explicit flag:

```rust
pub fn enumerate_minimal_candidate_selection_with_model_directives(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
    enable_model_directives: bool,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    enumerate_minimal_candidate_selection_inner(input, enable_model_directives)
}
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:16`.

Preserve the early empty returns. They mean "no eligible candidates", not an
error:

```rust
if normalized_api_format.is_empty() {
    return Ok(Vec::new());
}
if !crate::auth_constraints_allow_api_format(auth_constraints, normalized_api_format) {
    return Ok(Vec::new());
}
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:37`.

Do not sort at enumeration time. `enumerate_minimal_candidate_selection_inner`
preserves theoretical candidate order and leaves final ordering to `ranking/`.

## Auth And Model Resolution

Auth matching is exact and normalized where appropriate. Providers can match id,
name, or type after trimming and case-insensitive comparison:

```rust
!allowed_value.is_empty()
    && (allowed_value.eq_ignore_ascii_case(provider_id.trim())
        || allowed_value.eq_ignore_ascii_case(provider_name.trim())
        || allowed_value.eq_ignore_ascii_case(provider_type.trim()))
```

Source: `crates/aether-scheduler-core/src/auth.rs:14`.

API formats are normalized through `aether-ai-formats`:

```rust
pub fn normalize_api_format(value: &str) -> String {
    aether_ai_formats::normalize_api_format_alias(value)
}
```

Source: `crates/aether-scheduler-core/src/model.rs:352`.

Model directive support must stay explicitly gated. The auth layer only accepts
the base model when `enable_model_directives` is true:

```rust
let base_model = enable_model_directives
    .then(|| aether_ai_formats::model_directive_base_model(requested_model_name))
    .flatten();
```

Source: `crates/aether-scheduler-core/src/auth.rs:86`.

Model mapping regexes are full-text anchored and case-insensitive:

```rust
let regex_pattern = format!("^(?:{pattern})$");
let Ok(compiled) = RegexBuilder::new(&regex_pattern)
    .case_insensitive(true)
    .build()
else {
    return false;
};
compiled.is_match(model_name)
```

Source: `crates/aether-scheduler-core/src/model.rs:299`.

DON'T use partial substring matching for model mappings. The tests at
`src/model.rs:403` lock that `"gpt-4o"` does not match `"gpt-4o-mini"`.

## Runtime Selectability

Runtime selectability applies blocking checks in a deliberate order. Preserve
this order because callers may surface the first skip reason:

1. provider quota blocks requests;
2. account quota exhausted;
3. OAuth invalid;
4. recent failure cooldown;
5. provider concurrency limit;
6. provider-key concurrency limit;
7. provider-key circuit breaker;
8. provider-key health score zero;
9. provider-key RPM exhausted.

The implementation starts with request-independent provider/account checks:

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

It then applies request-history and provider-key state:

```rust
if crate::is_candidate_in_recent_failure_cooldown(
    recent_candidates,
    candidate.provider_id.as_str(),
    candidate.endpoint_id.as_str(),
    candidate.key_id.as_str(),
    now_unix_secs,
) {
    return Some("recent_failure_cooldown");
}
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:65`.

Do not return prose skip reasons. Use stable snake_case strings because they
flow into persisted candidate metadata and caller logs.

## Cooldown And Active Request Windows

Cooldown is based on recent terminal failures for the same provider, endpoint,
and key. A recent success clears the cooldown immediately:

```rust
match candidate.status {
    RequestCandidateStatus::Success => return false,
    RequestCandidateStatus::Failed | RequestCandidateStatus::Cancelled => {
        recent_failures += 1;
        if recent_failures >= FAILURE_COOLDOWN_THRESHOLD {
            return true;
        }
    }
    ...
}
```

Source: `crates/aether-scheduler-core/src/health.rs:72`.

Active request counters only count unfinished `Pending` and `Streaming` records
inside `ACTIVE_REQUEST_WINDOW_SECS`:

```rust
if candidate.finished_at_unix_ms.is_some() {
    return false;
}

if !matches!(
    candidate.status,
    RequestCandidateStatus::Pending | RequestCandidateStatus::Streaming
) {
    return false;
}
```

Source: `crates/aether-scheduler-core/src/health.rs:496`.

Use `saturating_sub` for timestamp comparisons. Do not subtract raw unsigned
timestamps directly.

## RPM And Health State

Fixed `rpm_limit` takes precedence over learned limits:

```rust
if let Some(limit) = key.rpm_limit.filter(|limit| *limit > 0) {
    return usize::try_from(limit).ok();
}
```

Source: `crates/aether-scheduler-core/src/health.rs:127`.

Learned RPM limits are enforced only after adaptive confidence reaches the
threshold:

```rust
if provider_key_adaptive_learning_confidence(key, now_unix_secs)
    < ENFORCEMENT_CONFIDENCE_THRESHOLD
{
    return None;
}
```

Source: `crates/aether-scheduler-core/src/health.rs:139`.

New users reserve part of the effective limit, but cached users may use the full
limit:

```rust
if is_cached_user {
    return current_usage < effective_limit;
}

let available_for_new = available_provider_key_rpm_slots_for_new_user(
    key,
    current_usage,
    effective_limit,
    now_unix_secs,
);
current_usage < available_for_new
```

Source: `crates/aether-scheduler-core/src/health.rs:221`.

Health scores are clamped and then bucketed:

```rust
Some(score.clamp(0.0, 1.0))
```

Source: `crates/aether-scheduler-core/src/health.rs:243`.

```rust
if score < HEALTH_LOW_THRESHOLD {
    return Self::Low;
}
if score < HEALTH_DEGRADED_THRESHOLD {
    return Self::Degraded;
}
Self::Healthy
```

Source: `crates/aether-scheduler-core/src/health.rs:35`.

## Ranking Modes

`SchedulerRankingMode` has three modes: `FixedOrder`, `CacheAffinity`, and
`LoadBalance`.

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

All modes compare capability misses before provider priority. Fixed order then
applies cross-format demotion, priority, format preference, identity, and
original index:

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

Cache affinity can promote cached candidates and local tunnel candidates. The
reason constants are exported:

```rust
pub const RANKING_REASON_CACHED_AFFINITY: &str = "cached_affinity";
pub const RANKING_REASON_LOCAL_TUNNEL: &str = "local_tunnel";
pub const RANKING_REASON_CROSS_FORMAT: &str = "cross_format";
```

Source: `crates/aether-scheduler-core/src/ranking/reasons.rs:5`.

Load balance first sorts by the same base criteria, then rotates within equal
priority/capability/format groups:

```rust
let group_len = end - start;
if group_len > 1 {
    let offset = usize::try_from(context.load_balance_seed).unwrap_or(0) % group_len;
    sorted_indices[start..end].rotate_left(offset);
}
```

Source: `crates/aether-scheduler-core/src/ranking/modes.rs:109`.

Keep rotation deterministic. Do not call randomness inside scheduler-core.

## Applying Ranking To Caller Items

Ranking accepts a separate `items` slice and `SchedulerRankableCandidate` slice.
The function computes outcomes first, then reorders `items` in place:

```rust
let outcomes = scheduler_ranking_outcomes(candidates, context);
apply_order(
    items,
    outcomes
        .iter()
        .map(|outcome| outcome.original_index)
        .collect(),
);
outcomes
```

Source: `crates/aether-scheduler-core/src/ranking/mod.rs:71`.

Callers must keep `items` and `candidates` aligned by original index. Do not
pass a filtered candidate slice that no longer matches the item slice.

## Affinity

Affinity cache keys normalize API format and reject blank required components:

```rust
let api_format = crate::normalize_api_format(api_format);
let global_model_name = global_model_name.trim();
if api_format.is_empty() || global_model_name.is_empty() {
    return None;
}
```

Source: `crates/aether-scheduler-core/src/affinity.rs:64`.

Session-aware keys use a v2 prefix and hash raw session keys:

```rust
let client_family = client_session_affinity
    .client_family
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_ascii_lowercase)
    .unwrap_or_else(|| "generic".to_string());
let session_hash = hash_session_key(session_key);
```

Source: `crates/aether-scheduler-core/src/affinity.rs:82`.

Candidate affinity hashes include the affinity key and candidate identity:

```rust
hasher.update(affinity_key.as_bytes());
hasher.update(b":");
hasher.update(candidate.provider_id.as_bytes());
hasher.update(b":");
hasher.update(candidate.endpoint_id.as_bytes());
hasher.update(b":");
hasher.update(candidate.key_id.as_bytes());
```

Source: `crates/aether-scheduler-core/src/affinity.rs:107`.

Do not store or log raw client session keys in scheduler metadata.

## Request-Candidate Reporting

Request-candidate helpers bridge scheduler decisions into persistence records
without owning the repository. `build_execution_request_candidate_seed` accepts
an `ExecutionPlan`, optional report context, timestamp, and generated id:

```rust
pub fn build_execution_request_candidate_seed(
    plan: &ExecutionPlan,
    report_context: Option<&Value>,
    started_at_unix_ms: u64,
    generated_candidate_id: String,
) -> SchedulerExecutionRequestCandidateSeed
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:308`.

The helper writes plan identity back into the report context:

```rust
context.insert(
    "provider_id".to_string(),
    Value::String(plan.provider_id.clone()),
);
context.insert(
    "endpoint_id".to_string(),
    Value::String(plan.endpoint_id.clone()),
);
context.insert("key_id".to_string(), Value::String(plan.key_id.clone()));
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:336`.

Extra data always marks gateway execution runtime and phase:

```rust
extra_data.insert("gateway_execution_runtime".to_string(), Value::Bool(true));
extra_data.insert("phase".to_string(), Value::String("3c_trial".to_string()));
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:721`.

Terminal status record building repairs epoch or missing timestamps by falling
back to started, finished, or `now_unix_ms`:

```rust
let created_at_unix_ms = non_epoch_unix_ms(slot.created_at_unix_ms)
    .or_else(|| started_at_unix_ms.and_then(non_epoch_unix_ms))
    .or_else(|| finished_at_unix_ms.and_then(non_epoch_unix_ms))
    .unwrap_or(terminal_unix_secs);
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:536`.

When adding new report metadata, parse it in
`SchedulerRequestCandidateReportContext`, include it in
`ReportCandidateExtraDataInput`, and cover the round trip in
`request_candidate.rs` tests.
