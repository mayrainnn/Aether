# Logging Guidelines

`aether-ai-formats` currently has no logging or tracing calls. This is intentional. The
crate is a pure format, protocol, and provider-compatibility library; callers own request
lifecycle logging, spans, HTTP status logging, and persistence. This crate should return
structured values that callers can log safely after applying their own policy.

## Current State

Source search under `crates/aether-ai-formats/src` finds no `tracing`, `debug!`, `info!`,
`warn!`, `error!`, `trace!`, or `span!` calls. The crate-level dependencies also omit
`tracing`:

```toml
# crates/aether-ai-formats/Cargo.toml:9-19
[dependencies]
aether-contracts.workspace = true
base64.workspace = true
http.workspace = true
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
sha1 = "0.10"
sha2.workspace = true
url.workspace = true
uuid.workspace = true
```

The workspace has `tracing` available for other crates at `Cargo.toml:86-87`, but this
crate does not opt into it. Do not add `tracing` to `aether-ai-formats` just to observe a
local parse branch.

## Why This Crate Stays Silent

- It is called from gateway, provider transport, model fetch, and runtime paths where the
  caller already has request IDs and span context.
- It frequently sees raw AI request bodies, response bodies, headers, image inputs,
  provider-private envelopes, tool arguments, and auth context fields.
- Logging from this crate would risk duplicating or leaking payloads before caller-level
  redaction has happened.
- Tests rely on deterministic return values rather than logs.

When a caller needs observability, return data that is safe to summarize. For example,
`StreamingStandardTerminalObserver` stores terminal summaries instead of logging stream
parser details:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_core/format_matrix.rs:98-144
#[derive(Default)]
pub struct StreamingStandardTerminalObserver {
    provider: Option<ProviderStreamParser>,
    latest_summary: Option<ExecutionStreamTerminalSummary>,
}

pub fn disable_with_error(&mut self, parser_error: impl Into<String>) {
    let parser_error = parser_error.into();
    if let Some(summary) = self.latest_summary.as_mut() {
        if summary.parser_error.is_none() {
            summary.parser_error = Some(parser_error);
        }
    } else {
        self.latest_summary = Some(ExecutionStreamTerminalSummary {
            parser_error: Some(parser_error),
            ..ExecutionStreamTerminalSummary::default()
        });
    }
    self.provider = None;
}
```

## Safe Metadata Instead Of Logs

When this crate prepares metadata that may be logged by callers, sanitize it before it
leaves the crate. Request path and query helpers keep only a small allowlist:

```rust
// crates/aether-ai-formats/src/formats/shared/routing.rs:327-367
pub fn sanitize_request_query_string(query: &str) -> Option<String> {
    let query = query.trim().trim_start_matches('?').trim();
    if query.is_empty() {
        return None;
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        if request_query_key_is_safe_to_trace(key.as_ref()) {
            serializer.append_pair(key.as_ref(), value.as_ref());
        }
    }
    let sanitized = serializer.finish();
    (!sanitized.is_empty()).then_some(sanitized)
}

fn request_query_key_is_safe_to_trace(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "alt" | "view" | "pagesize" | "page_size" | "limit" | "offset"
    )
}
```

Tests lock this redaction behavior:

```rust
// crates/aether-ai-formats/src/formats/shared/routing.rs:689-708
#[test]
fn request_path_metadata_sanitizer_drops_sensitive_query_parameters() {
    assert_eq!(
        sanitize_request_query_string("?key=secret&alt=sse&pageSize=10&token=hidden")
            .as_deref(),
        Some("alt=sse&pageSize=10")
    );
    assert_eq!(
        sanitize_request_path_and_query(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?key=secret&alt=sse",
            None
        )
        .as_deref(),
        Some("/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse")
    );
}
```

## If Logging Is Ever Added

Adding logging to this crate should be a rare exception. If it becomes necessary:

- Keep logs at branch/contract level, never payload level.
- Use structured fields that are already sanitized, such as `client_api_format`,
  `provider_api_format`, `plan_kind`, `report_kind`, `envelope_name`, and sanitized path.
- Never log `headers`, `body_json`, `body_base64`, tool arguments, image data, bearer
  tokens, API keys, raw query strings, auth context, or full provider-private envelopes.
- Prefer returning an `ExecutionStreamTerminalSummary` or typed error over logging parser
  recovery details.
- Add tests for any helper that prepares loggable metadata.

## Do Not

Do not log inside provider request parsers such as OpenAI chat parsing:

```rust
// crates/aether-ai-formats/src/formats/openai/chat/request.rs:27-35
pub fn from_raw(body_json: &Value) -> Option<CanonicalRequest> {
    let request = body_json.as_object()?;
    let mut canonical = CanonicalRequest {
        model: request
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..CanonicalRequest::default()
    };
    ...
}
```

This function sees the full request body. It should return `None` for unsupported shape
and let `formats/registry.rs` convert that into a typed error. The caller can then log a
safe, high-level failure with its own request context.

Do not add debug logs to stream rewriters for every line or frame. These code paths handle
SSE data, reasoning deltas, tool calls, and binary chunks. If investigation needs details,
write targeted tests using representative frames instead.
