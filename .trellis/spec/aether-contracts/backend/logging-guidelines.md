# Logging Guidelines

> Logging boundaries for the `aether-contracts` crate.

---

## Scope

`aether-contracts` does not log. It has no `tracing` dependency and no runtime
execution context. Its job is to define payloads such as `ExecutionPlan`,
`ExecutionResult`, `StreamFrame`, tunnel frames, and standardized usage
summaries.

This is intentional. The caller knows the trace id, request id, provider id,
user context, redaction policy, and whether an event is expected. A contract
crate does not.

Evidence:

- `crates/aether-contracts/Cargo.toml:9` lists `bytes`, `flate2`, `serde`,
  `serde_json`, and `thiserror`; it does not list `tracing`.
- `crates/aether-contracts/src/error.rs:30` stores error data but does not emit
  logs.
- `crates/aether-contracts/src/tunnel.rs:129` returns `ProtocolError` from
  `Frame::decode` instead of logging malformed frames.

---

## Rule: No Logging Inside Contracts

Do not add `tracing::{debug, info, warn, error}` imports to this crate.

```rust
// DON'T in aether-contracts:
pub fn decode(mut data: Bytes) -> Result<Self, ProtocolError> {
    if data.len() < HEADER_SIZE {
        tracing::warn!(actual = data.len(), "frame too short");
        return Err(ProtocolError::TooShort {
            expected: HEADER_SIZE,
            actual: data.len(),
        });
    }
    ...
}
```

Use the existing return-value style:

```rust
pub fn decode(mut data: Bytes) -> Result<Self, ProtocolError> {
    if data.len() < HEADER_SIZE {
        return Err(ProtocolError::TooShort {
            expected: HEADER_SIZE,
            actual: data.len(),
        });
    }
    ...
}
```

Source: `crates/aether-contracts/src/tunnel.rs:129`.

---

## Where Logging Belongs

Log at the runtime or application boundary that handles the result.

Examples from current consumers:

- `apps/aether-gateway/src/execution_runtime/stream/error.rs` logs when the
  execution runtime emits an error frame while collecting an error body.
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs` logs upstream
  execution warnings while it still has the `ExecutionPlan` and request context.
- `apps/aether-gateway/src/executor/candidate_loop.rs` creates debug spans for
  candidate execution and includes trace id fields.
- `apps/aether-gateway/src/tunnel/embedded/hub.rs` logs tunnel heartbeat and
  attachment failures with proxy connection context.

These higher layers can decide what to redact and what severity to use.
`aether-contracts` should only return structured values.

---

## Data That Callers May Log

The following fields are usually safe to include after the caller applies its
own redaction policy:

- `ExecutionPlan.request_id` at `crates/aether-contracts/src/plan.rs:98`.
- `ExecutionPlan.candidate_id` at `crates/aether-contracts/src/plan.rs:99`.
- `ExecutionPlan.provider_id` at `crates/aether-contracts/src/plan.rs:103`.
- `ExecutionPlan.endpoint_id` at `crates/aether-contracts/src/plan.rs:104`.
- `ExecutionResult.status_code` at `crates/aether-contracts/src/result.rs:31`.
- `ExecutionTelemetry.ttfb_ms` and `elapsed_ms` at
  `crates/aether-contracts/src/result.rs:8`.
- `ExecutionError.kind`, `phase`, `retryable`, and `failover_recommended` at
  `crates/aether-contracts/src/error.rs:31`.
- Tunnel `FrameHeader.stream_id`, `msg_type`, `flags`, and `payload_len` at
  `crates/aether-contracts/src/tunnel.rs:66`.

Prefer structured log fields in callers:

```rust
warn!(
    request_id = %plan.request_id,
    provider_id = %plan.provider_id,
    error_kind = ?error.kind,
    "execution failed"
);
```

This is caller-side guidance. Do not put that code in `aether-contracts`.

---

## Data That Must Not Be Logged

Never log these fields raw:

- `ExecutionPlan.headers` at `crates/aether-contracts/src/plan.rs:109`, because
  it may contain `authorization`, `x-api-key`, `api-key`, `x-goog-api-key`, or
  `proxy-authorization`.
- `RequestBody.json_body` and `body_bytes_b64` at
  `crates/aether-contracts/src/plan.rs:28`, because prompts, files, messages,
  and provider payloads can contain user content.
- `ExecutionResult.body` at `crates/aether-contracts/src/result.rs:34`, because
  upstream responses can contain generated content or provider diagnostics.
- `StreamFramePayload::Data.text` and `chunk_b64` at
  `crates/aether-contracts/src/frame.rs:25`, because stream chunks may contain
  user or model content.
- `ProxySnapshot.url` at `crates/aether-contracts/src/plan.rs:58`, because proxy
  URLs can embed credentials.
- `RequestMeta.headers` at `crates/aether-contracts/src/tunnel.rs:182`, for the
  same header-secret reason.
- Tunnel frame payload bytes at `crates/aether-contracts/src/tunnel.rs:94`.

The crate should continue to make these values available to callers, but
callers must redact before logging.

---

## Error Message Hygiene

`ExecutionError.message` is serializable data at
`crates/aether-contracts/src/error.rs:34`. It may later be logged by gateway or
proxy code. When constructing this field in consumers, keep it concise and
sanitized.

Good message:

```text
upstream returned HTTP 503 during first byte
```

Bad message:

```text
authorization=Bearer sk-... body={"messages":[...]}
```

Do not add helper constructors in this crate that automatically include raw
URLs, headers, or payload excerpts in messages.

---

## Log Level Guidance For Callers

Because this crate is log-free, levels are caller-side policy:

- `debug`: contract conversion decisions, selected execution formats, candidate
  filtering, tunnel protocol state that is useful during troubleshooting.
- `info`: lifecycle events such as startup, migration, successful tunnel
  registration, or long-running background task milestones.
- `warn`: recoverable execution, tunnel, compression, upstream, or storage
  failures where the request can still fail over or the service can continue.
- `error`: unrecoverable startup failures, dirty migration state, or data loss
  risk.

Use `ExecutionError.retryable` and `failover_recommended` from
`crates/aether-contracts/src/error.rs:37` to guide caller log severity, but do
not treat those booleans as a logging API.

---

## Review Checklist

Before approving a logging-related change:

- Confirm `aether-contracts` still has no `tracing` dependency.
- Confirm decode and validation functions return errors instead of logging.
- Confirm new serialized message fields cannot accidentally contain secrets.
- Confirm caller-side logs use stable identifiers and redacted context.
- Confirm tests assert contract behavior, not log output.
