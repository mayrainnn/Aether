# Logging Guidelines

This crate logs sparingly. Most helpers are pure builders or policy checks and
should return values instead of logging. Logging is reserved for operational
events where the crate recovers from a runtime problem, delegates a refresh, or
repairs provider-specific request state.

The crate depends on `tracing` (`crates/aether-provider-transport/Cargo.toml:26`)
and currently uses `tracing::warn!`, `tracing::info!`, and one imported `warn!`.

## Log Only Runtime Events

Do not log normal unsupported transport decisions. Use reason strings instead.
For example, policy functions return `Some("transport_proxy_unsupported")` or
`None` and do not log (`crates/aether-provider-transport/src/policy.rs:45`).

Log when a runtime dependency fails and the code intentionally continues.

Example:

```rust
// crates/aether-provider-transport/src/network.rs:76
let owner = match lookup.lookup_tunnel_attachment_owner(node_id).await {
    Ok(owner) => owner,
    Err(error) => {
        warn!(error = %error, node_id = node_id, "failed to load tunnel attachment owner");
        None
    }
};
```

This is a warning because tunnel-affinity enrichment failed, but the proxy
snapshot can still be returned.

## Structured Fields

Use structured fields for key transport dimensions. OAuth refresh logging
includes `key_id`, `provider_id`, `endpoint_id`, and `provider_type`.

Example:

```rust
// crates/aether-provider-transport/src/generic_oauth/mod.rs:245
tracing::info!(
    key_id = %transport.key.id,
    provider_id = %transport.provider.id,
    endpoint_id = %transport.endpoint.id,
    provider_type,
    request_refresh_token_len = refresh_token.len(),
```

Keep fields stable and machine-readable. Prefer ids and normalized provider
types over display names.

DON'T: put structured context into message text only. Add fields so logs can be
filtered by provider/key/endpoint.

## Sensitive Data

Never log token values, API keys, auth_config JSON, or raw proxy URLs. When a
token-related value is useful, log length or boolean presence.

Example:

```rust
// crates/aether-provider-transport/src/generic_oauth/mod.rs:269
tracing::info!(
    key_id = %transport.key.id,
    provider_id = %transport.provider.id,
    endpoint_id = %transport.endpoint.id,
    provider_type,
    expires_at_unix_secs = ?refreshed.token_set.expires_at_unix_secs,
    response_has_refresh_token = refreshed.token_set.refresh_token.is_some(),
```

This logs expiration and refresh-token presence, not `access_token` or
`refresh_token` values.

DON'T: log `transport.key.decrypted_api_key`, `transport.key.decrypted_auth_config`,
OAuth response bodies, request bodies, `Authorization`, `x-api-key`, cookies, or
proxy credentials.

## Log Levels

Use `info!` for successful, important operational transitions that are not on
every request path. Generic OAuth refresh delegation and success are examples
(`crates/aether-provider-transport/src/generic_oauth/mod.rs:245` and
`crates/aether-provider-transport/src/generic_oauth/mod.rs:269`).

Use `warn!` when local transport continues after a recoverable infrastructure or
state problem:

1. missing refresh token in a refreshable auth_config
   (`crates/aether-provider-transport/src/generic_oauth/mod.rs:232`),
2. distributed OAuth refresh lock acquire failure
   (`crates/aether-provider-transport/src/oauth_refresh/mod.rs:466`),
3. distributed OAuth refresh lock release failure
   (`crates/aether-provider-transport/src/oauth_refresh/mod.rs:498`),
4. tunnel owner lookup failure (`crates/aether-provider-transport/src/network.rs:76`),
5. Kiro request history repair (`crates/aether-provider-transport/src/kiro/converter.rs:216`).

Avoid `error!` in this crate unless the crate itself consumes and suppresses a
non-recoverable failure. Most non-recoverable failures should be returned as
`Result::Err` to the caller.

## OAuth Refresh Lock Logging

Distributed-lock warnings must include the affected key and adapter provider
type. The code already follows this pattern:

```rust
// crates/aether-provider-transport/src/oauth_refresh/mod.rs:479
tracing::warn!(
    key_id = %key_id,
    provider_type = adapter.provider_type(),
    error = ?err,
    "gateway local oauth refresh distributed lock unavailable"
);
```

When adding lock or cache logging, avoid high-cardinality user/account labels
unless they are necessary for operational triage.

## Provider-Specific Repair Logging

Provider-specific repair code may use a compact warning when it changes request
shape to keep a provider protocol valid. Kiro history repair logs the number of
orphaned tool uses it removes.

Example:

```rust
// crates/aether-provider-transport/src/kiro/converter.rs:216
if !orphaned_tool_use_ids.is_empty() {
    warn!(
        "kiro: removing {} orphaned tool_use(s) from history",
        orphaned_tool_use_ids.len()
    );
```

Prefer structured fields for new logs even if older logs use format arguments.
For example, a new log should use `orphaned_tool_use_count = orphaned_tool_use_ids.len()`
rather than embedding only the number in the message.

## Anti-Patterns

DON'T: log inside hot pure helpers such as `build_transport_request_url`,
`build_passthrough_headers`, or `provider_runtime_policy`.

DON'T: log every unsupported candidate. Return reason strings for admin/gateway
diagnostics.

DON'T: log raw HTTP bodies or token responses. Summarize with booleans,
expiration timestamps, body excerpts already provided by upstream error types,
or safe status codes.

DON'T: use `println!` or `eprintln!` in library code. Use `tracing` so callers
can route logs consistently.
