# Error Handling

> Error and miss-diagnostic conventions for `aether-ai-serving`.

---

## Scope

This crate does not define a monolithic application error enum. Most fallible serving
stages are generic over a `Port::Error` associated type. The crate owns the control flow
that decides whether a stage produced a response, an exhaustion outcome, a skipped
candidate, or no path. Adapter crates own concrete database, HTTP, or gateway errors.

The main rule: propagate adapter failures with `?`, but represent domain misses as
typed outcomes or stable reason strings.

---

## Port Errors Stay Generic

Async port traits expose an associated `Error` type and return
`Result<..., Self::Error>`. This keeps `aether-ai-serving` independent from Axum,
SeaORM, HTTP client, and storage error types.

Example from `crates/aether-ai-serving/src/attempt_loop.rs:18`:

```rust
#[async_trait]
pub trait AiAttemptLoopPort<Attempt>: Send + Sync
where
    Attempt: AiExecutionAttempt + Send + Sync + 'static,
{
    type Response: Send;
    type Exhaustion: Send;
    type Error: Send;

    async fn execute_attempt(
        &self,
        attempt: &Attempt,
    ) -> Result<Option<Self::Response>, Self::Error>;
}
```

Guideline: new async stage ports should use `type Error: Send`, not
`anyhow::Error`, `Box<dyn Error>`, or an application-specific error enum.

---

## Use `?` For Infrastructure Failure

Runner functions use `?` to stop the current stage when the port reports an actual
failure. They do not catch and translate adapter failures locally.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:125`:

```rust
for candidate in candidates {
    let Some(transport) = port.read_candidate_transport(&candidate).await? else {
        skipped.push(port.build_missing_transport_skipped_candidate(candidate));
        continue;
    };
}
```

The missing transport snapshot is a domain miss and becomes a skipped candidate. The
error returned by `read_candidate_transport` is infrastructure failure and propagates.

DON'T collapse both cases into `Result<Option<_>, String>` or log-and-continue.

---

## Domain Misses Are Outcomes, Not Errors

Execution paths distinguish response, exhaustion, and no path with explicit enums.
`NoPath` is not an error.

Example from `crates/aether-ai-serving/src/execution_path.rs:3`:

```rust
#[derive(Debug)]
pub enum AiServingExecutionOutcome<Response, Exhaustion> {
    Responded(Response),
    Exhausted(Exhaustion),
    NoPath,
}
```

The sync and stream runners preserve the last local exhaustion if fallback also has no
path. Example from `crates/aether-ai-serving/src/execution_path.rs:115`:

```rust
match port.execute_sync_plan_fallback(fallback_reason).await? {
    AiServingExecutionOutcome::Responded(response) => {
        Ok(AiServingExecutionOutcome::Responded(response))
    }
    AiServingExecutionOutcome::Exhausted(outcome) => {
        Ok(AiServingExecutionOutcome::Exhausted(outcome))
    }
    AiServingExecutionOutcome::NoPath => Ok(exhausted
        .map(AiServingExecutionOutcome::Exhausted)
        .unwrap_or(AiServingExecutionOutcome::NoPath)),
}
```

Guideline: add a new outcome variant only when callers must make a new domain
distinction. Do not use errors for expected "not eligible", "not supported", or "no
decision" paths.

---

## Candidate Skips Use Stable Reason Strings

Candidate gates return static skip reasons that downstream persistence and diagnostics
can record without depending on concrete error types.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:69`:

```rust
fn candidate_common_skip_reason(
    &self,
    candidate: &Self::Candidate,
    transport: &Self::Transport,
    requested_model: Option<&str>,
) -> Option<&'static str>;
```

Pool scheduler skip reasons are exported constants. Example from
`crates/aether-ai-serving/src/pool_scheduler.rs:5`:

```rust
pub const AI_POOL_ACCOUNT_BLOCKED_SKIP_REASON: &str = "pool_account_blocked";
pub const AI_POOL_ACCOUNT_EXHAUSTED_SKIP_REASON: &str = "pool_account_exhausted";
pub const AI_POOL_COOLDOWN_SKIP_REASON: &str = "pool_cooldown";
pub const AI_POOL_COST_LIMIT_REACHED_SKIP_REASON: &str = "pool_cost_limit_reached";
```

Guideline: if a new skip reason needs to cross crate boundaries, define it as a public
constant. If it is only used inside one adapter, keep it adapter-local.

DON'T allocate dynamic skip reason strings in hot loops unless the caller truly needs
dynamic text. Most existing skip reasons are `&'static str`.

---

## Local Validation Can Return Static Errors

Small pure helpers use `Result<T, &'static str>` when failure is a stable local contract
violation.

Example from `crates/aether-ai-serving/src/candidate_preparation.rs:8`:

```rust
pub fn prepare_ai_header_authenticated_candidate(
    direct_auth: Option<(String, String)>,
    oauth_header_auth: Option<(String, String)>,
    selected_provider_model_name: &str,
) -> Result<AiPreparedHeaderAuthenticatedCandidate, &'static str> {
    let Some((auth_header, auth_value)) = direct_auth.or(oauth_header_auth) else {
        return Err("transport_auth_unavailable");
    };
    let mapped_model = resolve_ai_candidate_mapped_model(selected_provider_model_name)?;
```

Use this pattern only for stable machine-readable reasons such as
`transport_auth_unavailable` or `mapped_model_missing`. If a new helper needs rich
context, return a structured diagnostic value instead.

---

## JSON Diagnostics Are Structured Extra Data

Request body and runtime miss diagnostics are returned as JSON values or mutated through
diagnostic ports. They are not logged or thrown in this crate.

Example from `crates/aether-ai-serving/src/request_body_diagnostics.rs:7`:

```rust
pub fn request_body_build_failure_extra_data(
    body_json: &Value,
    client_api_format: &str,
    provider_api_format: &str,
) -> Option<Value> {
    let diagnostic =
        diagnose_request_body_build_failure(body_json, client_api_format, provider_api_format)?;
    Some(
        diagnostic
            .formats(client_api_format, provider_api_format)
            .source(request_body_build_source(
                client_api_format,
                provider_api_format,
            ))
            .to_extra_data(),
    )
}
```

Runtime misses use an adapter-supplied diagnostic type. Example from
`crates/aether-ai-serving/src/runtime_miss.rs:1`:

```rust
pub trait AiRuntimeMissDiagnosticPort: Send + Sync {
    type Decision: Send + Sync;
    type Diagnostic: Send;

    fn build_runtime_miss_diagnostic(
        &self,
        decision: &Self::Decision,
        plan_kind: &str,
        requested_model: Option<&str>,
        reason: &str,
    ) -> Self::Diagnostic;
}
```

Guideline: diagnostic helpers should preserve machine-readable fields. Avoid converting
diagnostics into prose-only errors inside this crate.

---

## Serialization Errors Surface Where Serialization Happens

The crate generally avoids serializing except for contract payload shaping. When it does
serialize potentially fallible data, return the serde error instead of inventing a crate
error.

Example from `crates/aether-ai-serving/src/dto.rs:147`:

```rust
pub fn augment_sync_report_context(
    report_context: Option<serde_json::Value>,
    provider_request_headers: &BTreeMap<String, String>,
    _provider_request_body: &serde_json::Value,
) -> serde_json::Result<Option<serde_json::Value>> {
```

`report_context.rs` uses `expect` only where serialization is considered infallible for
control-owned maps. Example from `crates/aether-ai-serving/src/report_context.rs:125`:

```rust
object.insert(
    "original_headers".to_string(),
    serde_json::to_value(parts.original_headers).expect("control headers should serialize"),
);
```

Use `expect` only for internal invariants that tests can lock. Do not use `unwrap` or
`expect` for adapter-returned fallible data in production code paths.

---

## Tests Use Infallible Ports

Unit tests usually set `type Error = std::convert::Infallible` so they can focus on
stage behavior rather than error construction.

Example from `crates/aether-ai-serving/src/candidate_resolution.rs:236`:

```rust
impl AiCandidateResolutionPort for TestPort {
    type Candidate = &'static str;
    type Transport = &'static str;
    type Eligible = String;
    type Skipped = String;
    type Error = std::convert::Infallible;
}
```

When adding tests for error propagation, use a small explicit test error type and assert
that the runner returns it through `?`. For normal ordering tests, keep `Infallible`.

---

## Common Mistakes To Avoid

DON'T add `thiserror` or `anyhow` to this crate for stage runners. There is no current
dependency on either in `crates/aether-ai-serving/Cargo.toml`.

DON'T treat `None` as an error when the domain model already uses it as "no decision",
"no candidate", or "no diagnostic".

DON'T log adapter failures here. Return them to the gateway/admin layer where request
context and tracing spans exist.

DON'T use user-facing error strings as skip reasons. Skip reasons should be stable,
snake_case, and suitable for persisted diagnostics.
