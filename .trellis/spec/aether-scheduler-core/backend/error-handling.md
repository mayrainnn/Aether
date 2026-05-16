# Error Handling

> Error-return conventions for `crates/aether-scheduler-core`.

## Scope

`aether-scheduler-core` does not define a crate-local error enum. Most scheduler
decisions are pure predicates or builders that return `bool`, `Option<T>`, or
plain structs. The only public `Result` paths currently surface
`aether_data_contracts::DataLayerError`, because malformed stored JSON should be
treated as a data-contract violation rather than silently ignored.

Evidence from source scan:

- `DataLayerError` appears in `src/model.rs` and `src/candidate/enumeration.rs`.
- There is no `anyhow`, `thiserror`, `tracing`, `DbErr`, SeaORM, Redis, or SQL
  usage under `crates/aether-scheduler-core/src`.
- `expect(...)` appears only in `#[cfg(test)]` modules.

## Result Errors

Use `Result<T, DataLayerError>` only when the helper is validating persisted
data shape and callers must see the invariant break.

Candidate enumeration returns `DataLayerError` because it calls
`extract_global_priority_for_format` while building each scheduler candidate:

```rust
pub fn enumerate_minimal_candidate_selection(
    input: EnumerateMinimalCandidateSelectionInput<'_>,
) -> Result<Vec<SchedulerMinimalCandidateSelectionCandidate>, DataLayerError> {
    enumerate_minimal_candidate_selection_inner(input, false)
}
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:10`.

The inner loop propagates priority parsing failures with `?`:

```rust
key_global_priority_for_format: crate::extract_global_priority_for_format(
    row.key_global_priority_by_format.as_ref(),
    normalized_api_format,
)?,
```

Source: `crates/aether-scheduler-core/src/candidate/enumeration.rs:87`.

The parser is intentionally strict. A non-object `global_priority_by_format`
returns `UnexpectedValue`, and non-integer entries also return `UnexpectedValue`:

```rust
let Some(object) = raw.as_object() else {
    return Err(DataLayerError::UnexpectedValue(
        "provider_api_keys.global_priority_by_format is not a JSON object".to_string(),
    ));
};
```

Source: `crates/aether-scheduler-core/src/model.rs:316`.

```rust
Err(DataLayerError::UnexpectedValue(
    "provider_api_keys.global_priority_by_format contains a non-integer value".to_string(),
))
```

Source: `crates/aether-scheduler-core/src/model.rs:347`.

Do not replace these errors with `Ok(None)`. Missing data means no priority;
malformed data means the data layer violated the expected contract.

## Option For Inapplicable Decisions

Use `Option<T>` when a decision cannot be made because required input is missing
or blank. This crate uses `None` as a non-error "no usable value" signal.

Affinity cache keys reject blank components:

```rust
let api_key_id = api_key_id.trim();
if api_key_id.is_empty() {
    return None;
}
let api_format = crate::normalize_api_format(api_format);
let global_model_name = global_model_name.trim();
if api_format.is_empty() || global_model_name.is_empty() {
    return None;
}
```

Source: `crates/aether-scheduler-core/src/affinity.rs:60`.

Report slot resolution also returns `None` when the metadata does not contain a
request id. That lets callers decide whether to create or skip a candidate
record:

```rust
let request_id = request_id?;
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:225`.

Local status record building requires a non-empty candidate id and a candidate
index; both are represented with `?` on `Option`:

```rust
let candidate_id = plan
    .candidate_id
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())?;
let metadata = parse_request_candidate_report_context(report_context)?;
let candidate_index = metadata.candidate_index?;
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:451`.

## Boolean Gates And Skip Reasons

Use `bool` for simple allow/block predicates when the caller already knows the
context.

Example:

```rust
pub fn provider_key_rpm_allows_request(
    key: &StoredProviderCatalogKey,
    recent_candidates: &[StoredRequestCandidate],
    now_unix_secs: u64,
    is_cached_user: bool,
) -> bool
```

Source: `crates/aether-scheduler-core/src/health.rs:186`.

Use `Option<&'static str>` when the caller needs a stable machine-readable skip
reason. `candidate_runtime_skip_reason_with_state` returns the first blocking
reason in policy order:

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

The matching convenience predicate delegates to the reason function:

```rust
pub fn candidate_is_selectable_with_runtime_state(
    input: CandidateRuntimeSelectabilityInput<'_>,
) -> bool {
    candidate_runtime_skip_reason_with_state(input).is_none()
}
```

Source: `crates/aether-scheduler-core/src/candidate/selectability.rs:35`.

Do not create a second copy of the skip policy in a bool helper. Keep the
reason-returning function as the single source of truth.

## Execution Errors Are Converted, Not Propagated

This crate receives `aether_contracts::ExecutionError` only to extract stable
candidate status fields. It does not return the execution error object.

```rust
pub fn execution_error_details(
    error: Option<&ExecutionError>,
    body_json: Option<&Value>,
) -> (Option<String>, Option<String>) {
    match error {
        Some(error) => (
            Some(format!("{:?}", error.kind)),
            Some(error.message.trim().to_string()).filter(|value| !value.is_empty()),
        ),
        None => (
            None,
            body_json
                .and_then(extract_error_message)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        ),
    }
}
```

Source: `crates/aether-scheduler-core/src/request_candidate.rs:113`.

Keep this boundary narrow. Scheduler-core should shape persisted candidate
status, not decide HTTP response bodies.

## No API Error Responses

There are no Axum response builders or HTTP status response types in this crate.
If an error must be returned to a client, implement that mapping in gateway,
admin, or another application/service crate.

DON'T add this kind of API response code here:

```rust
// Wrong boundary for scheduler-core.
return (StatusCode::BAD_REQUEST, Json(json!({"detail": "invalid scheduler input"})));
```

The scheduler-core equivalent should be a pure return value, for example
`None`, `Some("key_rpm_exhausted")`, or `Err(DataLayerError::UnexpectedValue(_))`
depending on whether the condition is missing input, policy denial, or broken
stored data.

## Common Mistakes

DON'T silently accept malformed JSON from stored records:

```rust
// Wrong: hides corrupted provider_api_keys.global_priority_by_format.
let value = raw.and_then(|value| value.as_object()).and_then(...);
```

Use the current strict parser in `extract_global_priority_for_format` instead.

DON'T use `unwrap()` or `expect()` in production helpers. Test modules use
`expect("candidate should build")` after contract constructors because invalid
fixtures should fail loudly:

```rust
StoredRequestCandidate::new(...).expect("candidate should build")
```

Source: `crates/aether-scheduler-core/src/health.rs:655`.

DON'T log errors inside this crate. Return enough structured context for callers
to log with request id, user id, provider id, and redaction context.
