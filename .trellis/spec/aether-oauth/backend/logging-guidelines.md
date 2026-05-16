# Logging Guidelines

> Observability rules for `crates/aether-oauth/`.

---

## Overview

`aether-oauth` currently has no logging dependency and no tracing/logging macro
calls. A repository scan of this crate finds no `tracing`, `log::`, `info!`,
`debug!`, `warn!`, `error!`, `println!`, or `eprintln!` usage. Keep that
stance unless there is a strong crate-level reason to add instrumentation.

This is intentional. The crate handles access tokens, refresh tokens,
client secrets, identity payloads, and provider auth_config blobs. Higher
layers that know request IDs, user IDs, storage IDs, and redaction policy
should perform logging around calls into this crate.

The network boundary carries a safe request identifier that callers can log
without exposing secrets:

```rust
// crates/aether-oauth/src/network/executor.rs:8
pub struct OAuthHttpRequest {
    pub request_id: String,
    pub method: reqwest::Method,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub content_type: Option<String>,
    pub json_body: Option<Value>,
    pub body_bytes: Option<Vec<u8>>,
    pub network: OAuthNetworkContext,
}
```

## Current Log Levels

There are no crate-local log levels today. If instrumentation is added later,
use the following policy:

- `debug`: provider type, request_id, high-level OAuth step, sanitized status,
  and whether proxy context was present.
- `info`: successful high-level completion only when a caller-level operation
  has durable business meaning. Prefer caller-side info logs.
- `warn`: recoverable provider drift, unexpected but handled provider status,
  or degraded optional probe behavior. Prefer caller-side warn logs.
- `error`: only for failures that cannot be returned as `OAuthError`. Most
  failures in this crate should be returned, not logged.

Do not add logs just to trace every method entry. Service methods are thin
dispatchers:

```rust
// crates/aether-oauth/src/provider/service.rs:85
pub async fn refresh(
    &self,
    executor: &dyn OAuthHttpExecutor,
    ctx: &ProviderOAuthTransportContext,
    account: &super::ProviderOAuthAccount,
) -> Result<ProviderOAuthTokenSet, OAuthError> {
    self.adapter(&ctx.provider_type)?
        .refresh(executor, ctx, account)
        .await
}
```

Caller-side spans around `ProviderOAuthService::refresh` are more useful than
duplicating logs inside this dispatcher.

## Structured Fields

When a caller logs around this crate, prefer stable, non-secret fields:

- `oauth_flow`: `identity_start`, `identity_exchange`, `identity_userinfo`,
  `provider_exchange`, `provider_import`, `provider_refresh`, or
  `provider_probe`.
- `provider_type`: normalized provider type such as `codex`, `kiro`, or
  `custom_oidc`.
- `request_id`: the explicit OAuth request id built by adapters.
- `status_code`: upstream HTTP status for failed provider calls.
- `network_policy`: direct/system proxy/provider-operation proxy, if the
  caller exposes it.
- `has_proxy`: boolean, not full proxy details.

Adapters already provide request ids:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:145
let request_id = match grant_type {
    "authorization_code" => "provider-oauth:exchange-code".to_string(),
    "refresh_token" => "provider-oauth:refresh-token".to_string(),
    _ => format!(
        "provider-oauth:{}:{grant_type}",
        self.template.provider_type
    ),
};
```

Identity OIDC follows a provider-scoped request-id convention:

```rust
// crates/aether-oauth/src/identity/providers/custom_oidc.rs:74
let response = executor
    .execute(OAuthHttpRequest {
        request_id: format!("identity-oauth:{}:exchange-code", config.provider_type),
        method: reqwest::Method::POST,
        url: config.token_url.clone(),
        headers: form_headers(),
        content_type: Some("application/x-www-form-urlencoded".to_string()),
        json_body: None,
        body_bytes: Some(body_bytes),
        network: ctx.network.clone(),
    })
    .await?;
```

## What to Log in Callers

Log the operation lifecycle outside this crate:

- OAuth flow start with provider type and generated state id, never the state
  secret itself if the caller treats it as sensitive.
- Provider HTTP attempt with `request_id`, provider type, method, and host if
  host logging is allowed.
- Non-2xx provider result with `status_code` and a sanitized body excerpt if
  the caller has redaction in place.
- Final mapped identity/account result using stable internal IDs, not raw
  identity payloads.
- Probe result as booleans or summary fields, not raw quota or auth_config
  objects unless explicitly redacted.

Use `OAuthError` variants to decide caller log level:

```rust
// crates/aether-oauth/src/core/error.rs:11
#[error("oauth provider returned HTTP {status_code}: {body_excerpt}")]
HttpStatus {
    status_code: u16,
    body_excerpt: String,
},

// crates/aether-oauth/src/core/error.rs:18
#[error("oauth transport failed: {0}")]
Transport(String),
```

Provider status and transport failures are usually caller-side `warn` or
`error` depending on retry policy. `InvalidRequest` is often a caller bug or
configuration problem.

## What Not to Log

Never log these fields raw:

- `OAuthTokenSet.access_token`
- `OAuthTokenSet.refresh_token`
- `ProviderOAuthAccount.access_token`
- `ProviderOAuthAccount.auth_config`
- `ProviderOAuthTransportContext.decrypted_api_key`
- `ProviderOAuthTransportContext.decrypted_auth_config`
- `IdentityOAuthProviderConfig.client_secret`
- `OAuthHttpRequest.headers`
- `OAuthHttpRequest.json_body`
- `OAuthHttpRequest.body_bytes`
- userinfo `raw` payloads and identity maps

Actual sensitive fields are visible in the type definitions:

```rust
// crates/aether-oauth/src/provider/account.rs:28
pub struct ProviderOAuthTransportContext {
    pub provider_id: String,
    pub provider_type: String,
    pub endpoint_id: Option<String>,
    pub key_id: Option<String>,
    pub auth_type: Option<String>,
    pub decrypted_api_key: Option<String>,
    pub decrypted_auth_config: Option<String>,
    pub provider_config: Option<Value>,
    pub endpoint_config: Option<Value>,
    pub key_config: Option<Value>,
    pub network: OAuthNetworkContext,
}
```

```rust
// crates/aether-oauth/src/core/token.rs:4
pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    pub expires_at_unix_secs: Option<u64>,
    pub raw_payload: Option<Value>,
}
```

Use bounded fingerprints where correlation is needed. The generic and Kiro
provider adapters hash secret material and keep only the first eight bytes of
the SHA-256 digest:

```rust
// crates/aether-oauth/src/provider/providers/kiro.rs:540
fn secret_fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
```

## If Logging Is Added Later

Before adding a `tracing` dependency or macros:

1. Confirm caller-side logging cannot capture the needed signal.
2. Add redaction tests for every structured field that could contain token,
   header, auth_config, userinfo, or client secret material.
3. Keep logs at boundaries such as `OAuthHttpExecutor::execute`, not inside
   every parser/helper.
4. Prefer request ids and provider types over URLs, headers, and bodies.
5. Update this guide and `Cargo.toml` together.

Do not add temporary `println!` or `dbg!` statements. This crate's test fakes
already capture requests for assertions:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:636
*self.seen_request.lock().expect("mutex should lock") = Some(request);
```
