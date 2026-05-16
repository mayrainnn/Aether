# Streaming And Provider Compatibility

Streaming and provider-private compatibility are first-class responsibilities in
`aether-ai-formats`. The crate converts provider stream lines into canonical stream frames,
emits client-specific SSE, unwraps provider-private envelopes, and applies local proxy
header/body rules. Keep these paths deterministic, bounded, and free of logging side
effects.

## Stream Rewrite Mode Selection

`formats/shared/stream_rewrite.rs` selects a rewrite mode from report context. Report
context, not global state, decides whether to unwrap an envelope, convert OpenAI image
streams, run the standard matrix, or bridge Kiro event streams to Claude CLI shape:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_rewrite.rs:12-23
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeStreamRewriteMode {
    EnvelopeUnwrap,
    OpenAiImage,
    Standard,
    KiroToClaudeCli,
    KiroToClaudeCliThenStandard,
}

pub fn resolve_finalize_stream_rewrite_mode(
    report_context: &Value,
) -> Option<FinalizeStreamRewriteMode> { ... }
```

The decision logic uses normalized `needs_conversion`, `envelope_name`,
`provider_api_format`, and `client_api_format` fields:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_rewrite.rs:47-81
if needs_conversion
    && envelope_name.eq_ignore_ascii_case(KIRO_ENVELOPE_NAME)
    && provider_api_format == "claude:messages"
{
    return supports_standard_stream_rewrite(
        provider_api_format.as_str(),
        client_api_format.as_str(),
    )
    .then_some(FinalizeStreamRewriteMode::KiroToClaudeCliThenStandard);
}

if needs_conversion {
    return supports_standard_stream_rewrite(
        provider_api_format.as_str(),
        client_api_format.as_str(),
    )
    .then_some(FinalizeStreamRewriteMode::Standard);
}
```

Do not add a rewrite mode that relies on process-global flags or caller type inspection.
All mode input should be encoded in report context.

## Stateful Rewriters

`AiSurfaceStreamRewriter` buffers partial lines and delegates to state-specific parsers.
It must tolerate chunk boundaries that split an SSE line:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_rewrite.rs:95-152
pub struct AiSurfaceStreamRewriter<'a> {
    report_context: &'a Value,
    buffered: Vec<u8>,
    state: AiSurfaceStreamRewriteState,
}

pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
    match &mut self.state {
        AiSurfaceStreamRewriteState::OpenAiImage(state) => {
            state.push_chunk(self.report_context, chunk)
        }
        AiSurfaceStreamRewriteState::KiroToClaudeCliThenStandard { kiro, standard } => {
            let claude_bytes = kiro.push_chunk(self.report_context, chunk)?;
            transform_standard_bytes(standard, self.report_context, claude_bytes)
        }
        AiSurfaceStreamRewriteState::EnvelopeUnwrap
        | AiSurfaceStreamRewriteState::Standard(_) => {
            self.buffered.extend_from_slice(chunk);
            ...
        }
    }
}
```

Always implement `finish` for state machines. Buffered data must be flushed or explicitly
discarded by mode, and client emitters must finish their final frames.

## Standard Stream Matrix

`StreamingStandardFormatMatrix` is the shared provider-stream-to-client-stream bridge.
It lazily initializes provider and client sides from report context, handles provider error
frames, and emits canonical frames:

```rust
// crates/aether-ai-formats/src/formats/shared/stream_core/format_matrix.rs:20-45
#[derive(Default)]
pub struct StreamingStandardFormatMatrix {
    provider: Option<ProviderStreamParser>,
    client: Option<ClientStreamEmitter>,
    terminated: bool,
}

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

Use canonical frames for provider-to-client conversion. Do not have a Claude parser emit
OpenAI bytes directly, or an OpenAI parser emit Gemini bytes directly. The parser should
produce `CanonicalStreamFrame`; the client emitter should own the target wire format.

## SSE Encoding

Use the shared SSE helpers for JSON and done frames:

```rust
// crates/aether-ai-formats/src/formats/shared/sse.rs:23-40
pub fn encode_done_sse() -> Vec<u8> {
    b"data: [DONE]\n\n".to_vec()
}

pub fn encode_json_sse(
    event: Option<&str>,
    value: &Value,
) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
    let mut out = Vec::new();
    if let Some(event) = event.filter(|value| !value.trim().is_empty()) {
        out.extend_from_slice(b"event: ");
        out.extend_from_slice(event.as_bytes());
        out.push(b'\n');
    }
    out.extend_from_slice(b"data: ");
    out.extend(serde_json::to_vec(value).map_err(AiSurfaceFinalizeError::from)?);
    out.extend_from_slice(b"\n\n");
    Ok(out)
}
```

Do not hand-build JSON SSE with `format!("{value}")`. Use `serde_json::to_vec` so escaping
and object ordering behavior remain serde-controlled.

## Provider Adaptation Surfaces

Provider-private envelopes are declared in `provider_compat/surfaces.rs`, not scattered in
callers. The descriptor records provider type, envelope name, anchor API format, and
capabilities:

```rust
// crates/aether-ai-formats/src/provider_compat/surfaces.rs:15-28
#[derive(Debug, Clone, Copy)]
pub struct ProviderAdaptationDescriptor {
    pub surface: ProviderAdaptationSurface,
    pub provider_type: Option<&'static str>,
    pub envelope_name: &'static str,
    pub anchor_api_format: &'static str,
    pub supports_request_bridge: bool,
    pub supports_sync_finalize_bridge: bool,
    pub supports_stream_bridge: bool,
    pub requires_eventstream_accept: bool,
    pub unwraps_response_envelope: bool,
}
```

Capability tests must stay close to descriptor changes:

```rust
// crates/aether-ai-formats/src/provider_compat/surfaces.rs:147-187
#[test]
fn resolves_private_surface_anchor_contracts() {
    assert_eq!(
        provider_adaptation_anchor_api_format(KIRO_ENVELOPE_NAME, "claude:messages"),
        Some("claude:messages")
    );
}

#[test]
fn exposes_private_surface_capabilities() {
    assert!(provider_adaptation_requires_eventstream_accept(
        Some(KIRO_ENVELOPE_NAME),
        "claude:messages"
    ));
}
```

## Kiro Event Stream Decoder

Kiro stream compatibility includes an AWS event-stream decoder. It is bounded and recovers
from limited frame errors by draining bytes, then stops after repeated invalid frames:

```rust
// crates/aether-ai-formats/src/provider_compat/kiro_stream/state.rs:3-12
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
const MAX_BUFFER_SIZE: usize = MAX_MESSAGE_SIZE;
const MAX_ERRORS: usize = 5;

#[derive(Default)]
pub struct KiroToClaudeCliStreamState {
    decoder: EventStreamDecoder,
    state: KiroClaudeStreamState,
    started: bool,
}
```

```rust
// crates/aether-ai-formats/src/provider_compat/kiro_stream/stream/decoder.rs:25-58
pub(super) fn decode_available(&mut self) -> Result<Vec<AwsEventFrame>, String> {
    let mut out = Vec::new();
    if self.stopped {
        return Ok(out);
    }

    loop {
        match parse_frame(&self.buffer) {
            Ok(Some((frame, consumed))) => { ... }
            Ok(None) => break,
            Err(FrameParseError::Incomplete) => break,
            Err(FrameParseError::Invalid(message)) => {
                self.error_count += 1;
                if self.error_count >= MAX_ERRORS {
                    self.stopped = true;
                    return Err(message);
                }
                ...
            }
        }
    }
    Ok(out)
}
```

Do not remove size limits or CRC checks. The parser validates prelude CRC, message CRC,
header boundaries, and header value types before accepting a frame.

## Local Proxy Rules

Local proxy compatibility rules live in `provider_compat/proxy/rules.rs`. Public entry
points are intentionally small:

```rust
// crates/aether-ai-formats/src/provider_compat/proxy/rules.rs:30-57
pub fn header_rules_are_locally_supported(rules: Option<&Value>) -> bool { ... }
pub fn header_rules_have_enabled_rules(rules: Option<&Value>) -> bool { ... }
pub fn apply_local_header_rules(
    headers: &mut BTreeMap<String, String>,
    rules: Option<&Value>,
) { ... }
pub fn apply_local_header_rules_with_request_headers(
    headers: &mut BTreeMap<String, String>,
    original_request_headers: Option<&BTreeMap<String, String>>,
    rules: Option<&Value>,
) { ... }
```

Private helpers parse paths, wildcard ranges, conditions, placeholders, regex operations,
and nested JSON mutation. Keep these helpers private unless provider-transport needs a
new explicit capability check.

GitNexus process evidence for `repo="Aether"` shows provider query execution reaches
`header_rules_are_locally_supported` through `aether-provider-transport/src/policy.rs`.
That means compatibility checks affect whether gateway admin provider tests can execute
locally. Treat rule-support predicates as public contract, not internal cleanup targets.

## Do Not

- Do not stream raw provider bytes to a different client format without canonical frames.
- Do not log stream lines, tool arguments, image payloads, or provider-private envelopes.
- Do not add unbounded buffers in stream decoders or rewriters.
- Do not make provider-private envelope behavior depend on provider name alone; use
  envelope name plus anchor API format.
- Do not accept unsupported proxy rule shapes by returning true from
  `*_rules_are_locally_supported`; unsupported rules must keep local execution disabled.
- Do not hide parse errors by emitting partial final frames. Return `AiSurfaceFinalizeError`
  or a terminal summary parser error depending on the existing API.
