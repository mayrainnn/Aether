# Quality Guidelines

The most important quality property in this crate is preserving transport
semantics across many provider families. Prefer explicit typed decisions,
stable reason codes, and focused pure helpers over broad mutable request logic.

## Public API Discipline

Keep external contracts in `src/lib.rs`. A helper should become public only when
gateway/admin/services need it directly. Otherwise keep it private or
`pub(crate)`.

Example:

```rust
// crates/aether-provider-transport/src/lib.rs:48
pub use network::{
    resolve_transport_execution_timeouts, resolve_transport_profile, resolve_transport_profile_id,
    resolve_transport_proxy_snapshot, resolve_transport_proxy_snapshot_with_tunnel_affinity,
};
```

`network.rs` itself keeps parsing helpers private, such as
`parse_transport_profile_value`, `effective_proxy_config`, and
`proxy_snapshot_from_value` (`crates/aether-provider-transport/src/network.rs:190`).

DON'T: expose helper modules wholesale to avoid adding a narrow re-export.

## Typed Parameter Objects

Use small input structs for functions with many related inputs. This avoids
boolean/argument ordering mistakes in request builders.

Example:

```rust
// crates/aether-provider-transport/src/same_format_provider/mod.rs:55
pub struct SameFormatProviderRequestBodyInput<'a> {
    pub body_json: &'a Value,
    pub mapped_model: &'a str,
    pub client_api_format: &'a str,
    pub provider_api_format: &'a str,
```

The same pattern appears in `StandardProviderRequestHeadersInput`,
`StandardPlanFallbackHeadersInput`, `TransportRequestUrlParams`, and
`GeminiFilesHeadersInput`.

DON'T: add more positional parameters to already complex request builders.

## Stable Ordering

Use `BTreeMap` for request headers, query maps, and OAuth cache data where tests
or diagnostics benefit from deterministic order.

Example:

```rust
// crates/aether-provider-transport/src/auth.rs:11
fn collect_passthrough_headers(
    headers: &http::HeaderMap,
    extra_headers: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
```

This crate repeatedly normalizes header keys to lowercase before insertion
(`crates/aether-provider-transport/src/auth.rs:20`). Preserve that behavior when
adding new header paths.

DON'T: switch to `HashMap` in request-building surfaces unless ordering truly
does not matter and tests are updated intentionally.

## Normalize at Boundaries

Trim and normalize provider_type, api_format, auth_type, header names, query
keys, and custom paths as soon as they enter a decision boundary.

Example:

```rust
// crates/aether-provider-transport/src/provider_types.rs:380
pub fn provider_runtime_policy(provider_type: &str) -> ProviderRuntimePolicy {
    if let Some(template) = fixed_provider_template(provider_type) {
        return template.runtime_policy;
    }
```

`fixed_provider_template` trims and lowercases provider types before matching
(`crates/aether-provider-transport/src/provider_types.rs:395`). URL builders
also normalize API-format aliases before branching
(`crates/aether-provider-transport/src/request_url/mod.rs:38`).

DON'T: compare raw provider strings directly in new policy code.

## Safe Auth-Config Absorption

Only absorb auth_config fields that are explicitly safe for local transport.
`auth_config.rs` blocks sensitive header/query names, sensitive top-level keys,
absolute paths, path templates, and control characters.

Example:

```rust
// crates/aether-provider-transport/src/auth_config.rs:6
const UNSAFE_AUTH_CONFIG_HEADER_NAMES: &[&str] = &[
    "api-key",
    "authorization",
    "content-length",
```

`parse_local_auth_config_object` returns `Err(())` for unknown or sensitive keys
instead of silently accepting them (`crates/aether-provider-transport/src/auth_config.rs:141`).

DON'T: add arbitrary auth_config passthrough fields. Secrets belong in
`decrypted_api_key` or controlled OAuth refresh metadata, not in generated
headers or URLs.

## Policy Before Build

Capability checks should be cheap and explicit. Builder functions should not be
the first place that discovers inactive records, unsupported rules, provider
type mismatches, or proxy/profile incompatibility.

Example:

```rust
// crates/aether-provider-transport/src/policy.rs:138
fn local_same_format_transport_unsupported_reason(
    transport: &GatewayProviderTransportSnapshot,
    api_format: &str,
    allow_network_passthrough: bool,
```

This helper checks active flags, API-format aliases, header/body rule support,
OAuth resolution, custom path behavior, proxy/profile support, provider type,
and endpoint kind before returning `None`.

DON'T: duplicate these checks inside every request builder. Add to policy first,
then keep builders focused on construction.

## Fixed Provider Templates

Fixed providers are declared in `provider_types.rs` with a template and runtime
policy. Add new fixed provider behavior there before adding provider-specific
request code.

Example:

```rust
// crates/aether-provider-transport/src/provider_types.rs:255
const CODEX_FIXED_PROVIDER_TEMPLATE: FixedProviderTemplate = FixedProviderTemplate {
    provider_type: "codex",
    version: 1,
    base_url: "https://chatgpt.com/backend-api/codex",
```

Templates can define endpoint defaults such as `upstream_stream_policy =
force_stream` for Codex image responses
(`crates/aether-provider-transport/src/provider_types.rs:260`).

DON'T: hardcode a fixed provider's default base URL or endpoint list in gateway
handlers.

## Diagnostics Must Be Sanitized

Diagnostics may include provider type, activity flags, rule support, proxy
presence, and resolved transport profile, but they must not include raw secrets
or full proxy URLs with credentials/path/query.

Example:

```rust
// crates/aether-provider-transport/src/diagnostics.rs:208
fn sanitize_trace_proxy_url(url: Option<&str>) -> Option<String> {
    let raw = url.map(str::trim).filter(|value| !value.is_empty())?;
    let parsed = url::Url::parse(raw).ok()?;
```

The sanitized proxy string includes only scheme, host, and port
(`crates/aether-provider-transport/src/diagnostics.rs:217`).

DON'T: put `decrypted_api_key`, `decrypted_auth_config`, access tokens, refresh
tokens, or raw proxy URLs into diagnostics.

## Testing Standards

Tests live next to the helpers they protect. They should assert exact URL,
header, policy, and snapshot behavior rather than only checking `is_some()`.

Examples:

1. Header filters assert that stainless and Anthropic headers are stripped while
   normal headers pass through (`crates/aether-provider-transport/src/headers.rs:84`).
2. Provider templates assert exact fixed endpoints and stream defaults
   (`crates/aether-provider-transport/src/provider_types.rs:536`).
3. OAuth coordinator tests assert cache reuse and forced-refresh behavior
   (`crates/aether-provider-transport/src/oauth_refresh/mod.rs:690`).
4. Network tests assert proxy precedence and tunnel owner enrichment
   (`crates/aether-provider-transport/src/network.rs:400`).

DON'T: rely on integration tests alone for provider quirks. Add unit tests in
the module where the branch or normalization rule lives.
