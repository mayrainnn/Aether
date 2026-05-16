# Error Handling

`aether-ai-formats` uses small typed errors at public boundaries and `Option` for local
parse/emit failure inside provider modules. The crate does not log errors and does not map
errors to HTTP responses directly. Callers decide how to report failures, but this crate
owns provider-shaped JSON error bodies for AI surface finalization.

## Format Registry Errors

The central sync conversion error type is `FormatError`. It is deliberately compact:

```rust
// crates/aether-ai-formats/src/formats/context.rs:50-57
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    UnsupportedFormat(String),
    RequestParseFailed { format: String },
    RequestEmitFailed { format: String },
    ResponseParseFailed { format: String },
    ResponseEmitFailed { format: String },
}
```

`FormatError` implements `Display` and `std::error::Error`, but does not use `thiserror`.
Do not add a dependency just to derive this error. The current implementation keeps error
messages stable:

```rust
// crates/aether-ai-formats/src/formats/context.rs:59-77
impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat(format) => write!(f, "unsupported AI format: {format}"),
            Self::RequestParseFailed { format } => write!(f, "failed to parse {format} request"),
            Self::RequestEmitFailed { format } => write!(f, "failed to emit {format} request"),
            Self::ResponseParseFailed { format } => write!(f, "failed to parse {format} response"),
            Self::ResponseEmitFailed { format } => write!(f, "failed to emit {format} response"),
        }
    }
}
```

The registry converts `Option` from provider parse/emit modules into `FormatError` at the
boundary. Preserve this pattern:

```rust
// crates/aether-ai-formats/src/formats/registry.rs:15-37
pub fn parse_request(
    source_format: &str,
    body: &Value,
    ctx: &FormatContext,
) -> Result<CanonicalRequest, FormatError> {
    let source = parse_format(source_format)?;
    match source {
        FormatId::OpenAiChat => openai_chat::request::from(body, ctx),
        FormatId::ClaudeMessages => claude_messages::request::from(body, ctx),
        FormatId::GeminiGenerateContent => gemini_generate_content::request::from(body, ctx),
        ...
    }
    .ok_or_else(|| FormatError::RequestParseFailed {
        format: source.as_str().to_string(),
    })
}
```

For composed conversion, propagate registry errors with `?` and do not erase the variant:

```rust
// crates/aether-ai-formats/src/formats/registry.rs:71-79
pub fn convert_request(
    source_format: &str,
    target_format: &str,
    body: &Value,
    ctx: &FormatContext,
) -> Result<Value, FormatError> {
    let request = parse_request(source_format, body, ctx)?;
    emit_request(target_format, &request, ctx)
}
```

## Provider Module Parse Failures

Provider modules usually return `Option<CanonicalRequest>` or `Option<Value>` for local
shape matching. This makes unsupported payload shape cheap and lets the registry add the
format-specific boundary error.

Example from OpenAI chat request parsing:

```rust
// crates/aether-ai-formats/src/formats/openai/chat/request.rs:27-40
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

Do not return `Result<_, String>` from simple provider parse functions unless callers need
a specific user-facing reason. Let invalid wire shape become `RequestParseFailed` or
`ResponseParseFailed` at the registry.

## Stream Finalization Errors

Streaming and finalize paths use `AiSurfaceFinalizeError`. It is a single-message error
with conversion from serialization and base64 decode failures:

```rust
// crates/aether-ai-formats/src/formats/shared/mod.rs:28-55
#[derive(Debug)]
pub struct AiSurfaceFinalizeError(pub String);

impl AiSurfaceFinalizeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<serde_json::Error> for AiSurfaceFinalizeError {
    fn from(source: serde_json::Error) -> Self {
        Self(source.to_string())
    }
}
```

Use `map_err(AiSurfaceFinalizeError::from)` for serde/base64 boundaries and
`AiSurfaceFinalizeError::new(err.to_string())` when the source error is not already
covered. Example:

```rust
// crates/aether-ai-formats/src/formats/shared/sse.rs:27-40
pub fn encode_json_sse(
    event: Option<&str>,
    value: &Value,
) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
    let mut out = Vec::new();
    ...
    out.extend(serde_json::to_vec(value).map_err(AiSurfaceFinalizeError::from)?);
    out.extend_from_slice(b"\n\n");
    Ok(out)
}
```

Stateful stream rewriters should return empty bytes for benign no-op states and only error
on real parse/serialization failures:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_core/format_matrix.rs:27-45
pub fn transform_line(
    &mut self,
    report_context: &Value,
    line: Vec<u8>,
) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
    if self.terminated {
        return Ok(Vec::new());
    }
    self.ensure_initialized(report_context);
    if let Some(error_body) = build_client_error_body_for_line(report_context, &line) {
        self.terminated = true;
        return self.emit_error(error_body);
    }
    ...
}
```

## Provider-Shaped API Error Bodies

This crate builds JSON bodies shaped for the client API format. The enum is provider-neutral:

```rust
// crates/aether-ai-formats/src/formats/shared/error_body.rs:3-13
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalCoreSyncErrorKind {
    InvalidRequest,
    Authentication,
    PermissionDenied,
    NotFound,
    RateLimit,
    ContextLengthExceeded,
    Overloaded,
    ServerError,
}
```

`build_core_error_body_for_client_format` maps the same kind to OpenAI, Claude, and Gemini
JSON shapes:

```rust
// crates/aether-ai-formats/src/formats/shared/error_body.rs:31-82
pub fn build_core_error_body_for_client_format(
    client_api_format: &str,
    message: &str,
    code: Option<&str>,
    kind: LocalCoreSyncErrorKind,
) -> Option<Value> {
    match aether_ai_formats::normalize_api_format_alias(client_api_format).as_str() {
        "openai:chat" | "openai:responses" | "openai:responses:compact" => { ... }
        "claude:messages" => { ... }
        "gemini:generate_content" => { ... }
        _ => None,
    }
}
```

Tests lock both shape and mapping:

```rust
// crates/aether-ai-formats/src/formats/shared/error_body.rs:145-158
#[test]
fn builds_openai_core_error_body() {
    let body = build_core_error_body_for_client_format(
        "openai:chat",
        "bad request",
        Some("invalid_request"),
        LocalCoreSyncErrorKind::InvalidRequest,
    )
    .expect("body should build");

    assert_eq!(body["error"]["type"], "invalid_request_error");
}
```

## Specialized Request Errors

When the caller needs status code, provider error type, and error code, use a specialized
typed error. OpenAI image to ChatGPT-Web bridging is the precedent:

```rust
// crates/aether-ai-formats/src/formats/openai/image/request.rs:55-82
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatGptWebImageRequestError {
    pub status_code: u16,
    pub error_type: &'static str,
    pub code: &'static str,
    pub message: String,
}

impl ChatGptWebImageRequestError {
    pub fn to_error_json(&self) -> Value {
        json!({
            "error": {
                "message": self.message,
                "type": self.error_type,
                "code": self.code,
                "param": Value::Null,
            }
        })
    }
}
```

Tests should assert exact status and provider type for these errors:

```rust
// crates/aether-ai-formats/src/formats/openai/image/request.rs:1455-1470
let err = build_chatgpt_web_image_request_body(&parts, &body, None)
    .expect_err("oversized request should fail");
assert_eq!(err.status_code, 400);
assert_eq!(err.error_type, "invalid_request_error");
assert!(err.message.contains("ChatGPT-Web"));
```

## Do Not

- Do not log inside this crate when returning an error. Return typed data to the caller.
- Do not collapse `UnsupportedFormat`, parse failure, and emit failure into one string.
- Do not use `unwrap()` for request body shape parsing. Use `?`, `and_then`, `unwrap_or_default`,
  or explicit typed errors depending on the boundary.
- Do not silently invent provider-specific error fields outside
  `formats/shared/error_body.rs` or a specialized module that already owns that surface.
- Do not expose internal parser recovery errors unless callers need to act on them.
