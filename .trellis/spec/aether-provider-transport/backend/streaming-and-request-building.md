# Streaming and Request Building

This crate's core job is to turn a provider snapshot plus client request inputs
into a provider-specific URL, body, and header set. The request builder must
preserve transport behavior exactly, especially for streaming and provider
family fallbacks.

## Classify Behavior First

`same_format_provider::classify_same_format_provider_request_behavior` decides
whether a request is Claude Code, Vertex, Kiro, Antigravity, or a normal same
format provider, and whether the upstream should stream.

Example:

```rust
// crates/aether-provider-transport/src/same_format_provider/mod.rs:96
pub fn classify_same_format_provider_request_behavior(
    transport: &GatewayProviderTransportSnapshot,
    params: SameFormatProviderRequestBehaviorParams<'_>,
) -> SameFormatProviderRequestBehavior {
```

The classifier uses endpoint config and provider capabilities to set
`upstream_is_stream` and `force_body_stream_field`, not just the incoming
boolean flags (`crates/aether-provider-transport/src/same_format_provider/mod.rs:108`).

DON'T: re-implement stream classification in call sites. Reuse the behavior
object.

## URL Construction Rules

`request_url/mod.rs` owns all provider URL selection, including custom paths,
provider hooks, and stream-specific Gemini `alt=sse` handling.

Example:

```rust
// crates/aether-provider-transport/src/request_url/mod.rs:30
pub fn build_transport_request_url(
    transport: &GatewayProviderTransportSnapshot,
    params: TransportRequestUrlParams<'_>,
) -> Option<String> {
```

Important rules:

1. provider-specific hooks run before custom-path expansion,
2. Gemini custom paths may rewrite `generateContent` to `streamGenerateContent`,
3. streaming Gemini URLs get `alt=sse` when the query does not already provide
   `alt`,
4. custom path templates expand `{model}` and `{action}` tokens when available,
5. unsupported provider formats return `None` instead of guessing a path.

Example:

```rust
// crates/aether-provider-transport/src/request_url/mod.rs:392
fn maybe_add_gemini_stream_alt_sse(
    upstream_url: String,
    provider_api_format: &str,
    upstream_is_stream: bool,
) -> String {
```

DON'T: append `alt=sse` blindly. Only add it for Gemini generate-content stream
requests and only when the query does not already contain `alt`.

## Header Construction Rules

Headers are built from passthrough headers plus provider-specific auth and
streaming requirements. `standard::build_standard_plan_fallback_headers` and
`same_format_provider::build_same_format_provider_headers` both preserve auth
headers and inject the right `accept` value for stream requests.

Example:

```rust
// crates/aether-provider-transport/src/standard/mod.rs:78
pub fn build_standard_plan_fallback_headers(
    input: StandardPlanFallbackHeadersInput<'_>,
) -> BTreeMap<String, String> {
```

The standard builder chooses a passthrough strategy based on whether the
provider format matches the client format, whether the provider is Claude-like,
and whether auth is present.

Example:

```rust
// crates/aether-provider-transport/src/same_format_provider/mod.rs:241
pub fn build_same_format_provider_headers(
    input: SameFormatProviderHeadersInput<'_>,
) -> Option<BTreeMap<String, String>> {
```

Use `ensure_upstream_auth_header` after rule application to restore required auth
headers. For streaming requests, set `accept: text/event-stream`
(`crates/aether-provider-transport/src/same_format_provider/mod.rs:302`).

DON'T: let header rules strip the auth header that the upstream still requires.
Pass the protected header list and re-insert auth after rules run.

## Body Construction Rules

Bodies should be built from a clone of the incoming JSON when the client API
format already matches the provider, otherwise from a conversion result. The
builder must keep provider-specific adjustments in one place.

Example:

```rust
// crates/aether-provider-transport/src/same_format_provider/mod.rs:144
pub fn build_same_format_provider_request_body(
    input: SameFormatProviderRequestBodyInput<'_>,
) -> Option<Value> {
```

Rules:

1. Kiro body building delegates to `build_kiro_provider_request_body`.
2. Claude Code bodies are sanitized after the shared body is built.
3. Provider-specific model directives can be applied after conversion.
4. Local body rules run before final stream-field enforcement.
5. Missing object bodies or failed conversions return `None`.

`gemini_files::build_gemini_files_request_body` is stricter for binary uploads:
if a base64 upload is present and body rules are enabled, it returns
`BodyRulesUnsupportedForBinaryUpload` instead of silently dropping the rules
(`crates/aether-provider-transport/src/gemini_files/mod.rs:70`).

DON'T: mutate the original request body in place. Clone first so diagnostics and
rule evaluation can still reference the original JSON.

## Family-Specific Streaming Behavior

Different provider families handle streaming differently:

1. Standard providers may use `TextEventStreamIfStreaming` or
   `ProviderEventStreamIfMissing` (`crates/aether-provider-transport/src/standard/mod.rs:40`).
2. Same-format providers force `accept: text/event-stream` when the upstream is
   streaming (`crates/aether-provider-transport/src/same_format_provider/mod.rs:302`).
3. Gemini generate-content URLs add `alt=sse` for stream mode
   (`crates/aether-provider-transport/src/request_url/mod.rs:392`).
4. Kiro and Antigravity can force streaming or use provider-specific request
   envelopes.

Example:

```rust
// crates/aether-provider-transport/src/standard/mod.rs:40
pub enum StandardPlanFallbackAcceptPolicy {
    None,
    TextEventStreamIfStreaming,
    TextEventStreamIfStreamingOrWildcard,
    TextEventStreamRequired,
    ProviderEventStreamIfMissing,
}
```

DON'T: assume one SSE rule fits every provider. The accept header and URL query
must match the provider protocol.

## Special Provider Paths

Claude Code, Vertex, Antigravity, and Kiro have provider-specific path and auth
hooks. Keep those hooks narrow and local.

Example:

```rust
// crates/aether-provider-transport/src/request_url/mod.rs:224
fn build_transport_hook_url(
    transport: &GatewayProviderTransportSnapshot,
    params: TransportRequestUrlParams<'_>,
) -> Option<String> {
```

That hook handles Kiro regional assistant URLs, Claude Code message URLs, Vertex
API-key query auth, and Antigravity internal URLs before the generic custom-path
logic runs.

DON'T: move provider hooks into the generic URL builder's default arm. The hook
order is part of the contract.

## Body and Header Rules Interaction

Header and body rule application can fail request construction, and that is
intentional. Builders return `None` or an error enum instead of producing a
malformed request.

Example:

```rust
// crates/aether-provider-transport/src/gemini_files/mod.rs:91
if provider_request_body_base64.is_some() && body_rules_have_enabled_rules(body_rules) {
    return Err(GeminiFilesRequestBodyError::BodyRulesUnsupportedForBinaryUpload);
}
```

The same pattern appears in `same_format_provider::build_same_format_provider_request_body`
and `same_format_provider::build_same_format_provider_headers`, both of which
short-circuit on unsupported rule application.

DON'T: try to "best effort" a failed body/header rule. If the provider request
cannot be made valid, return `None` or a typed unsupported error.
