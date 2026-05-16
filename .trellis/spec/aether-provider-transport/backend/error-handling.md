# Error Handling

This crate uses three distinct failure shapes:

1. typed `thiserror` errors for operational failures that need display text and
   source chaining,
2. small unsupported-reason enums or string reason codes for local transport
   capability decisions,
3. `Option` for absent support or builder failure where the caller already owns
   the fallback path.

Use the shape that matches the caller contract. Do not convert everything to
`anyhow::Error`; this crate does not depend on `anyhow`.

## Typed Operational Errors

Use `thiserror` when the caller must surface or convert a real transport failure.
`LocalOAuthRefreshError` is the canonical example. It distinguishes reqwest
transport errors, HTTP status errors, text-message transport failures, and
invalid responses.

Example:

```rust
// crates/aether-provider-transport/src/oauth_refresh/mod.rs:66
#[derive(Debug, Error)]
pub enum LocalOAuthRefreshError {
    #[error("{provider_type} oauth refresh request failed: {source}")]
    Transport {
        provider_type: &'static str,
        #[source]
        source: reqwest::Error,
    },
```

When wrapping another error, keep provider context in the variant and preserve
the source where possible. `ReqwestLocalOAuthHttpExecutor` maps `.send()` and
`.text()` failures to `LocalOAuthRefreshError::Transport`
(`crates/aether-provider-transport/src/oauth_refresh/mod.rs:133`).

DON'T: return a bare string for reqwest failures. The source chain is useful for
debugging TLS, proxy, timeout, and DNS failures.

## Cross-Crate Error Mapping

OAuth refresh bridges errors from `aether-oauth` into local transport errors and
back into `OAuthError` when the local executor implements the oauth executor
trait. Keep those mappings explicit and exhaustive.

Example:

```rust
// crates/aether-provider-transport/src/oauth_refresh/mod.rs:223
pub(crate) fn oauth_error_to_local_refresh_error(
    provider_type: &'static str,
    error: OAuthError,
) -> LocalOAuthRefreshError {
```

The reverse mapping is private and intentionally loses only the local
provider_type decoration when returning to `aether-oauth`
(`crates/aether-provider-transport/src/oauth_refresh/mod.rs:258`).

DON'T: stringify `OAuthError` early. Match the enum so HTTP status, invalid
state, encryption availability, and storage failures remain distinguishable.

## Snapshot and Data-Layer Errors

Snapshot reads propagate `DataLayerError` from
`aether-data-contracts`. This crate validates catalog consistency but does not
own database queries.

Example:

```rust
// crates/aether-provider-transport/src/snapshot.rs:97
pub async fn read_provider_transport_snapshot(
    state: &dyn ProviderTransportSnapshotSource,
    provider_id: &str,
    endpoint_id: &str,
    key_id: &str,
) -> Result<Option<GatewayProviderTransportSnapshot>, DataLayerError> {
```

Missing encryption keys or missing provider/endpoint/key rows are not errors;
they return `Ok(None)` (`crates/aether-provider-transport/src/snapshot.rs:103`).
Mismatched provider IDs are data corruption and return
`DataLayerError::UnexpectedValue` (`crates/aether-provider-transport/src/snapshot.rs:128`).

DON'T: silently accept endpoint/key provider mismatches. Policy checks rely on a
single coherent provider snapshot.

## Unsupported Reasons

Capability checks return `Option<&'static str>` with stable reason codes. This
lets gateway/admin code explain why a local candidate was skipped without
needing to parse logs.

Example:

```rust
// crates/aether-provider-transport/src/policy.rs:45
pub fn local_openai_chat_transport_unsupported_reason(
    transport: &GatewayProviderTransportSnapshot,
) -> Option<&'static str> {
    if !transport.provider.is_active {
        return Some("provider_inactive");
    }
```

Use this pattern for policy decisions that are expected and recoverable, such as
inactive records, unsupported header/body rules, proxy/profile limitations,
provider-type restrictions, custom-path limitations, and endpoint kind mismatch.

DON'T: log or error for normal unsupported candidates. Return a reason string
and let the caller choose a fallback.

## Small Local Error Enums

When a builder has a tiny closed set of failures and the caller needs to branch,
use a small enum without `thiserror`. `GeminiFilesRequestBodyError` distinguishes
binary body-rule incompatibility from body-rule application failure.

Example:

```rust
// crates/aether-provider-transport/src/gemini_files/mod.rs:14
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeminiFilesRequestBodyError {
    BodyRulesUnsupportedForBinaryUpload,
    BodyRulesApplyFailed,
}
```

Antigravity uses the same style for envelope support and unsupported reasons:
`AntigravityRequestEnvelopeSupport::Unsupported(...)`
(`crates/aether-provider-transport/src/antigravity/request.rs:20`).

DON'T: add display text to small local enums unless the error crosses a public
display boundary. Tests should compare variants directly.

## `Option` as Fallback Signal

Builders frequently return `Option` when failure means "this local path cannot
build the request, use another path." Examples include URL builders, auth
resolvers, header builders, and same-format request builders.

Example:

```rust
// crates/aether-provider-transport/src/same_format_provider/mod.rs:144
pub fn build_same_format_provider_request_body(
    input: SameFormatProviderRequestBodyInput<'_>,
) -> Option<Value> {
```

Inside these builders, return `None` on unsupported body shape, failed request
conversion, unsupported header rules, or missing mapped model. Preserve actual
operational errors for code paths that already return `Result`.

DON'T: use `unwrap()` in builders to force a request shape. Missing `model`,
non-object JSON bodies, invalid custom paths, and failed body rules are normal
candidate-filtering cases.

## Secret Decryption Errors

Key mapping must fail loudly for Fernet-shaped encrypted values that cannot be
decrypted, while accepting legacy plaintext key material only where explicitly
allowed.

Example:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:123
fn decrypt_secret(
    encryption_key: &str,
    fallback_encryption_keys: &[String],
    ciphertext: &str,
    field_name: &str,
) -> Result<String, DataLayerError> {
```

`should_use_plaintext_secret` allows plaintext `provider_api_keys.api_key` and
JSON-shaped `provider_api_keys.auth_config`, but not arbitrary auth_config text
(`crates/aether-provider-transport/src/snapshot_mapping.rs:168`).

DON'T: broaden plaintext fallback without tests. It changes whether corrupted
secrets fail loudly or pass into upstream auth.
