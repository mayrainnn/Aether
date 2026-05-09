# Logging Guidelines

> Observability conventions for the pure `aether-ai-serving` service crate.

---

## Current State

This crate intentionally does not call `tracing::trace!`, `debug!`, `info!`, `warn!`,
`error!`, `span!`, or `#[instrument]`. A source scan of `crates/aether-ai-serving/src`
found no tracing macro usage. `Cargo.toml` also does not depend on `tracing`.

That silence is a design boundary. `aether-ai-serving` is a pure orchestration and
contract crate. Request spans, headers, user IDs, HTTP status, database timings, and
transport errors live in adapter/application crates such as the gateway. This crate
returns structured facts that callers can log.

---

## Prefer Structured Return Data Over Logs

Instead of logging candidate decisions, runners return outcomes, skipped candidates,
report context, and diagnostics.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:159`:

```rust
Ok(AiCandidateResolutionOutcome {
    eligible_candidates: ranked,
    skipped_candidates: skipped,
})
```

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:319`:

```rust
AiPoolSchedulerOutcome {
    candidates: annotate_ai_pool_candidates(ordered, candidate_group_id, true),
    skipped_candidates: skipped,
}
```

Guideline: add fields to an outcome, report context, or diagnostic object when the
information is needed downstream. Do not add a log line just to make an internal
decision visible.

---

## Report Context Is The Logging Payload Boundary

`report_context.rs` constructs the structured object that gateway/runtime layers can
persist or log. It includes request, candidate, provider, model, conversion, origin,
ranking, and optional provider request data.

Example from `crates/aether-ai-serving/src/report_context.rs:51`:

```rust
pub fn build_ai_execution_report_context(parts: AiExecutionReportContextParts<'_>) -> Value {
    let mut object = Map::new();
    object.insert(
        "user_id".to_string(),
        Value::String(parts.auth_context.user_id.clone()),
    );
    object.insert(
        "api_key_id".to_string(),
        Value::String(parts.auth_context.api_key_id.clone()),
    );
```

Guideline: if a new serving fact must be visible in usage reports or gateway logs, add
it to `AiExecutionReportContextParts` and populate the JSON object there. Keep the
field machine-readable and snake_case.

DON'T emit a `tracing::info!` with the same data from this crate. That would create a
parallel observability channel that tests and downstream persistence cannot inspect.

---

## Runtime Miss Diagnostics Carry Reasons

Runtime miss state is mutated through a diagnostic port. This lets gateway/runtime code
decide where to store or log diagnostics while this crate owns the reason transitions.

Example from `crates/aether-ai-serving/src/runtime_miss.rs:52`:

```rust
pub fn apply_ai_runtime_candidate_evaluation_progress_to_diagnostic<Diagnostic>(
    diagnostic: &mut Diagnostic,
    candidate_count: usize,
) where
    Diagnostic: AiRuntimeMissDiagnosticFields,
{
    diagnostic.set_candidate_count(candidate_count);
    diagnostic.set_reason(if candidate_count == 0 {
        "candidate_list_empty".to_string()
    } else {
        "candidate_evaluation_incomplete".to_string()
    });
}
```

Use this style for new miss states: a stable reason string plus structured counters.
Do not hide miss reasons inside log text.

---

## Candidate Skip Reasons Are Observability Data

Pool and candidate skip reasons are stable fields that callers can aggregate.

Example from `crates/aether-ai-serving/src/pool_scheduler.rs:221`:

```rust
if item.key_context.account_blocked {
    skipped.push(AiPoolSkippedCandidate {
        candidate: item.candidate,
        skip_reason: AI_POOL_ACCOUNT_BLOCKED_SKIP_REASON,
    });
    continue;
}
```

Guideline: when a new skip condition is added, return it as an `AiPoolSkippedCandidate`
or adapter-built skipped candidate. The caller can then persist it, surface it in
diagnostics, or log it in the request span.

DON'T use `warn!` for normal skip conditions such as account cooldown, cost limit, or
unsupported transport pair. These are expected scheduling outcomes.

---

## Request Body Diagnostics Are Extra Data

Request body conversion failures produce JSON extra data. That extra data can be saved
with candidate diagnostics or logged by callers.

Example from `crates/aether-ai-serving/src/request_body_diagnostics.rs:7`:

```rust
pub fn request_body_build_failure_extra_data(
    body_json: &Value,
    client_api_format: &str,
    provider_api_format: &str,
) -> Option<Value> {
    let diagnostic =
        diagnose_request_body_build_failure(body_json, client_api_format, provider_api_format)?;
```

Guideline: keep field paths and source contexts in the diagnostic payload. Do not add a
log statement that only says "request body build failed"; callers need the structured
path and format context.

---

## Sensitive Data Boundaries

This crate handles auth headers and provider request bodies in DTOs and report context.
Treat those as sensitive even when this crate is not logging.

Examples:

- `AiExecutionDecision.auth_value` in `crates/aether-ai-serving/src/dto.rs:90`.
- `provider_request_headers` in `crates/aether-ai-serving/src/dto.rs:109`.
- `provider_request_body` and `provider_request_body_base64` in
  `crates/aether-ai-serving/src/dto.rs:112`.
- `original_headers` inserted into report context at
  `crates/aether-ai-serving/src/report_context.rs:124`.

Guideline: if logging is ever added at an adapter boundary, redact auth values and
request bodies by default. This crate should not add dependency-level logging that
accidentally serializes DTOs with secrets.

---

## When Logging Belongs In Another Crate

Add tracing in the caller crate when the log needs:

- HTTP request ID, route, status, or latency.
- User/account identity beyond the structured auth context.
- Database transaction or Redis command timing.
- Transport attempt timing or upstream response status.
- Span correlation with a gateway proxy request.

`aether-ai-serving` can expose the facts. The caller owns the log line.

DON'T add `tracing` as a dependency to this crate just to debug a stage. First add or
extend a unit test that asserts the stage order, skip reason, diagnostic reason, or
report-context field.

---

## Testing Observability Facts

Because this crate returns observability data directly, tests should assert returned
fields rather than capture logs.

Example from `crates/aether-ai-serving/src/candidate_materialization.rs:145`:

```rust
assert_eq!(outcome.attempts, ["attempt-a", "attempt-b"]);
assert_eq!(outcome.candidate_count, 4);
assert_eq!(
    port.calls.lock().unwrap().as_slice(),
    [
        "resolve:candidate-a",
        "decorate:pre-skip",
        "decorate:resolved-skip",
    ]
);
```

For new observability behavior, prefer exact assertions on returned JSON, skip reasons,
candidate counts, or call traces. This keeps the crate deterministic and logger-free.
