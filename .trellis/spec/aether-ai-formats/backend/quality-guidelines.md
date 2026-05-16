# Quality Guidelines

The quality bar for `aether-ai-formats` is contract stability. This crate is consumed by
gateway, provider transport, runtime, and service code, so changes must preserve canonical
semantics, provider wire shapes, and format selection behavior. Prefer explicit enums,
small pure functions, and focused tests over runtime side effects or broad abstractions.

## Required Patterns

### Keep canonical structs provider-neutral

Provider modules must convert to or from `CanonicalRequest`, `CanonicalResponse`, and
canonical stream frames. Do not pass OpenAI, Claude, or Gemini raw payload shapes through
the registry as the shared internal model.

```rust
// crates/aether-ai-formats/src/protocol/canonical.rs:38-49
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalToolChoice {
    Auto,
    None,
    Required,
    Tool { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonicalContentBlock {
    Text { ... },
    Thinking { ... },
    Image { ... },
    ...
}
```

### Use serde attributes to preserve wire contracts

Canonical and contract structs use serde defaults and skip rules to keep JSON stable and
avoid emitting empty extension maps or absent optional fields. Example:

```rust
// crates/aether-ai-formats/src/contracts/auth_context.rs:7-18
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionRuntimeAuthContext {
    pub user_id: String,
    pub api_key_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_name: Option<String>,
    pub balance_remaining: Option<f64>,
    pub access_allowed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub api_key_is_standalone: bool,
}
```

Follow this style for new optional fields. Avoid changing existing serialization names
unless every caller contract is intentionally migrated.

### Normalize aliases before comparison

Never compare API format strings directly unless the values were already normalized in the
same scope. Use `normalize_api_format_alias`, `FormatId::parse`, or helper predicates.

```rust
// crates/aether-ai-formats/src/formats/id.rs:111-120
pub fn normalize_api_format_alias(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn api_format_alias_matches(left: &str, right: &str) -> bool {
    normalize_api_format_alias(left) == normalize_api_format_alias(right)
}
```

The canonical parser accepts route aliases as well as format names:

```rust
// crates/aether-ai-formats/src/formats/id.rs:91-106
fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "openai:chat" | "/v1/chat/completions" => Ok(Self::OpenAiChat),
        "openai:responses" | "/v1/responses" => Ok(Self::OpenAiResponses),
        "claude:messages" | "/v1/messages" => Ok(Self::ClaudeMessages),
        ...
        _ => Err(()),
    }
}
```

### Keep conversion matrices explicit

Cross-format routing should be table-like and readable. Do not hide provider priority in
ad hoc sort closures spread across caller crates.

```rust
// crates/aether-ai-formats/src/formats/matrix.rs:28-47
const NON_COMPACT_STANDARD_CANDIDATE_API_FORMATS: &[&str] = &[
    "openai:chat",
    "openai:responses",
    "claude:messages",
    "gemini:generate_content",
];
const EMBEDDING_CANDIDATE_API_FORMATS: &[&str] = &[
    "openai:embedding",
    "gemini:embedding",
    "jina:embedding",
    "doubao:embedding",
];
const RERANK_CANDIDATE_API_FORMATS: &[&str] = &["openai:rerank", "jina:rerank"];
```

If you add a format, update the identity enum, family/profile methods, route aliases,
candidate matrices, conversion kind functions, registry dispatch, API facade, and tests.

### Prefer private helpers and narrow visibility

Public items are part of the crate contract. Most parsing helpers stay private, and
stream internals use `pub(super)` only where sibling state modules need access.

```rust
// crates/aether-ai-formats/src/provider_compat/kiro_stream/stream/decoder.rs:9-25
impl EventStreamDecoder {
    pub(super) fn feed(&mut self, data: &[u8]) -> Result<(), String> { ... }

    pub(super) fn decode_available(&mut self) -> Result<Vec<AwsEventFrame>, String> { ... }
}

// crates/aether-ai-formats/src/provider_compat/kiro_stream/stream/decoder.rs:86
fn parse_frame(buffer: &[u8]) -> Result<Option<(AwsEventFrame, usize)>, FrameParseError> { ... }
```

Use `pub` only for symbols that cross module or crate boundaries. Use `pub(crate)` for
shared internal helpers like `FormatContext::mapped_model_or`.

## Forbidden Patterns

### Do not reintroduce retired aliases

The tests explicitly reject retired CLI/compact alias strings. Keep these failures unless
there is a deliberate migration across all callers.

```rust
// crates/aether-ai-formats/src/formats/id.rs:152-160
#[test]
fn retired_api_formats_do_not_parse() {
    assert_eq!(FormatId::parse("openai:cli"), None);
    assert_eq!(FormatId::parse("openai:compact"), None);
    assert_eq!(FormatId::parse("claude:chat"), None);
    assert_eq!(FormatId::parse("claude:cli"), None);
    assert_eq!(FormatId::parse("gemini:chat"), None);
    assert_eq!(FormatId::parse("gemini:cli"), None);
}
```

### Do not cross chat, embedding, and rerank boundaries

Embedding and rerank candidates are intentionally separate from chat/generation formats.
The matrix tests lock this:

```rust
// crates/aether-ai-formats/src/formats/matrix.rs:516-558
#[test]
fn embedding_candidate_registry_never_crosses_chat_generation_boundary() {
    let embedding_formats = ["openai:embedding", "gemini:embedding", "jina:embedding", "doubao:embedding"];
    let standard_formats = ["openai:chat", "openai:responses", "claude:messages", "gemini:generate_content"];

    for embedding_api_format in embedding_formats {
        for standard_api_format in standard_formats {
            assert_eq!(
                request_candidate_api_format_preference(embedding_api_format, standard_api_format),
                None
            );
        }
    }
}
```

### Do not mutate caller-owned JSON unless the API says so

Registry emit functions clone canonical values before applying mapped-model overrides:

```rust
// crates/aether-ai-formats/src/formats/registry.rs:43-52
pub fn emit_request(
    target_format: &str,
    request: &CanonicalRequest,
    ctx: &FormatContext,
) -> Result<Value, FormatError> {
    let target = parse_format(target_format)?;
    let mut request = request.clone();
    if let Some(mapped_model) = ctx.mapped_model.as_deref().filter(|value| !value.trim().is_empty()) {
        request.model = mapped_model.to_string();
    }
    ...
}
```

If a function needs to mutate a JSON body, make it explicit with `&mut Value`, as local
proxy rules do.

### Do not add runtime dependencies casually

`crates/aether-ai-formats/Cargo.toml:9-19` depends on data/parsing crates:
`aether-contracts`, `base64`, `http`, `regex`, `serde`, `serde_json`, `sha1`, `sha2`,
`url`, and `uuid`. There is no `tokio`, `axum`, `reqwest`, `sqlx`, `redis`, or `tracing`
dependency in this crate. New dependencies require a clear pure-format reason.

## Testing Requirements

Put tests next to the module that owns the behavior. This crate uses `#[cfg(test)] mod
tests` broadly, with many exact JSON-shape assertions.

Examples of expected test coverage:

- Identity and alias tests in `formats/id.rs`.
- Candidate and conversion matrix tests in `formats/matrix.rs`.
- Registry conversion tests in `formats/registry.rs`.
- Provider-specific wire shape tests in provider request/response/stream modules.
- Provider compatibility rule tests in `provider_compat/proxy/rules.rs`.
- Sanitization tests in `formats/shared/routing.rs`.

Registry tests should prove real conversion output, not only that functions return `Ok`:

```rust
// crates/aether-ai-formats/src/formats/registry.rs:182-196
#[test]
fn converts_openai_chat_to_responses_via_registry() {
    let body = json!({
        "model": "gpt-source",
        "messages": [{"role": "user", "content": "hello"}]
    });
    let ctx = FormatContext::default().with_mapped_model("gpt-target");

    let converted = convert_request("openai:chat", "openai:responses", &body, &ctx)
        .expect("request conversion should succeed");

    assert_eq!(converted["model"], "gpt-target");
    assert_eq!(converted["input"][0]["type"], "message");
}
```

When changing this crate, run:

```bash
cargo test -p aether-ai-formats
```

For documentation-only changes, this test is still useful when examples mention concrete
module names because it catches source drift in the crate.

## Review Checklist

- Did public API changes go through `lib.rs` or `api.rs` intentionally?
- Did a new format update identity, matrix, registry, canonical conversion, and tests?
- Are provider-specific raw fields preserved through `extensions` where needed?
- Are optional JSON fields represented with serde defaults and skip rules?
- Are aliases normalized before comparison?
- Are sensitive query parameters sanitized before becoming trace/report metadata?
- Are parser failures represented as `Option` locally and typed errors at boundaries?
- Are streaming state machines bounded and tested for partial chunks, finalization, and
  error frames?
