# Logging Guidelines

`apps/aether-gateway` uses the `tracing` crate with structured fields. Logs are
part of the gateway contract because they connect request admission, route
classification, local execution, fallback, audit, and maintenance behavior.

## Required Shape

Structured gateway logs normally include:

- `event_name`: stable machine-readable event identifier.
- `log_type`: broad channel such as `access`, `audit`, `event`, `ops`, or
  `debug`.
- `trace_id`: the request trace id when the event belongs to a request.
- Route/execution fields when available: `route_class`, `route_family`,
  `route_kind`, `execution_path`.
- Request identity fields when available: `request_id`, `user_id`, `api_key_id`.
- Context fields specific to the event.

Example:

```rust
// apps/aether-gateway/src/handlers/proxy/finalize.rs:105
if response.status().is_server_error() {
    warn!(
        event_name = "http_request_failed",
        log_type = "access",
        status = "failed",
        status_code,
        trace_id = %trace_id,
        request_id,
        remote_addr = %remote_addr,
        method = %method,
        path = %path_and_query,
        user_id,
        api_key_id,
        route_class,
        execution_path,
        dependency_reason = dependency_reason.as_str(),
        local_execution_runtime_miss_reason = local_execution_runtime_miss_reason.as_str(),
        elapsed_ms,
        "gateway request failed"
    );
}
```

## Access Logs

There are two access-log layers:

- `middleware/access_log.rs` logs generic request start/completion for routes
  that do not emit a richer final gateway log.
- `handlers/proxy/finalize.rs` logs final frontdoor request outcomes and marks
  the response with `RequestLogEmitted` so middleware does not duplicate it.

Trace id is generated or propagated in the middleware.

```rust
// apps/aether-gateway/src/middleware/access_log.rs:63
let trace_id = extract_or_generate_trace_id(request.headers());
if !request.headers().contains_key(TRACE_ID_HEADER) {
    request.headers_mut().insert(
        HeaderName::from_static(TRACE_ID_HEADER),
        HeaderValue::from_str(&trace_id).expect("trace id should be a valid header value"),
    );
}
trace!(
    event_name = "http_request_started",
    log_type = "access",
    status = "started",
    trace_id = %trace_id,
    request_id = "-",
    method = %method,
    path = %path,
    route_class = "pending",
    execution_path = "pending",
    "gateway request started"
);
```

High-frequency read endpoints are downgraded to `trace` after completion.

```rust
// apps/aether-gateway/src/middleware/access_log.rs:29
pub(crate) fn should_downgrade_access_log(method: &Method, path: &str) -> bool {
    if method != Method::GET {
        return false;
    }
    let normalized_path = path.split('?').next().unwrap_or(path);
    matches!(
        normalized_path,
        "/api/admin/usage/active"
            | "/api/users/me/usage/active"
            | "/api/admin/usage/records"
            | "/api/admin/usage/stats"
            | "/api/admin/usage/cache-affinity/interval-timeline"
    ) || is_usage_detail_path(normalized_path)
        || normalized_path.starts_with("/api/admin/monitoring/trace/")
}
```

## Levels

Use the levels as they are used in the crate today:

- `trace!`: request start and high-frequency successful polling/read endpoints.
- `debug!`: local execution attempt loop details and non-operator diagnostics.
- `info!`: successful meaningful events, normal access completions, audit
  completions, scheduled maintenance summaries.
- `warn!`: failed requests, rejected loops, invalid permissions, unavailable
  upstream/control paths, background worker failures, failed audit events.

Candidate loops use a debug span plus a debug event.

```rust
// apps/aether-gateway/src/executor/candidate_loop.rs:45
let span = tracing::debug_span!(
    "candidates",
    trace_id = %trace_id,
    plan_kind,
    candidate_count,
);

async move {
    tracing::debug!(
        event_name = "candidate_loop_started",
        log_type = "event",
        trace_id = %trace_id,
        plan_kind,
        candidate_count,
        first_provider = first_provider.as_str(),
        "candidate loop started"
    );
```

## Audit Logs

Admin audit logs are emitted from response finalization after the handler has
attached an optional `AdminAuditEvent`. The audit log includes admin identity,
route metadata, action and target.

```rust
// apps/aether-gateway/src/audit/admin.rs:71
let (audit_status, log_level) = classify_admin_audit_response(method, response.status());
if log_level == AdminAuditLogLevel::Info {
    info!(
        event_name,
        log_type = "audit",
        status = audit_status,
        status_code,
        trace_id = %trace_id,
        admin_user_id = admin_principal.user_id.as_str(),
        admin_user_role = admin_principal.user_role.as_str(),
        route_family,
        route_kind,
        method = %method,
        path = %path_and_query,
        action,
        target_type,
        target_id = %target_id,
        "admin audit event"
    );
}
```

For admin mutations, prefer attaching a specific `AdminAuditEvent` when the
target id is known; otherwise finalization will fall back to route-derived
action/target metadata.

## Ops Logs

Use `log_type = "ops"` for operational failures and background jobs.

```rust
// apps/aether-gateway/src/maintenance/runtime/runners.rs:150
pub(super) async fn run_db_maintenance_once(data: &GatewayDataState) -> Result<(), DataLayerError> {
    let summary = perform_db_maintenance_once(data).await?;
    if summary.attempted > 0 {
        info!(
            event_name = "db_maintenance_completed",
            log_type = "ops",
            worker = "db_maintenance",
            attempted = summary.attempted,
            succeeded = summary.succeeded,
            failed = summary.attempted.saturating_sub(summary.succeeded),
            "gateway finished db maintenance"
        );
    }
    Ok(())
}
```

For recoverable background failures, log and keep the worker alive.

```rust
// apps/aether-gateway/src/maintenance/runtime/workers.rs:60
loop {
    interval.tick().await;
    if let Err(err) = run_audit_cleanup_once(&data).await {
        log_maintenance_worker_failure("audit_cleanup", "tick", &err);
    }
}
```

## Sensitive Data Rules

Do not log:

- `Authorization`, `x-api-key`, `api-key`, `x-goog-api-key`, management token,
  OAuth token, refresh token, cookie, or session secret values.
- Raw request/response bodies for AI calls.
- Full request ids when `short_request_id` is already used by an access log.
- Provider credential headers forwarded through proxy/tunnel paths.

It is acceptable to log stable ids such as `trace_id`, `request_id`,
`provider_id`, `endpoint_id`, `key_id`, `user_id`, `api_key_id`, and `route_kind`
when they are needed to debug routing or execution decisions.

## DON'T

Do not use unstructured string interpolation when fields can be structured.

```rust
// DON'T
warn!("gateway failed for trace {trace_id}: {err}");
```

Prefer:

```rust
// apps/aether-gateway/src/handlers/proxy/mod.rs:341
warn!(
    trace_id = %request_context.trace_id,
    provider_id = %target.provider_id,
    endpoint_id = %target.endpoint_id,
    key_id = %target.key_id,
    error = ?err,
    "gateway failed to read provider transport for tunnel affinity forward"
);
```

Do not add duplicate access logs inside handlers. Return through
`finalize_gateway_response_with_context` so finalization emits the canonical
access event and inserts `RequestLogEmitted`.
