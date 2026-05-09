# Error Handling

> Error conventions for `crates/aether-oauth/`.

---

## Overview

`aether-oauth` uses one crate-level error enum, `OAuthError`, for shared OAuth
failure modes. Public fallible APIs return `Result<T, OAuthError>`; parser
helpers that intentionally signal absence or unsupported payload shapes return
`Option`.

The crate does not convert errors into HTTP responses. It is a library crate:
callers in gateway/admin/data layers decide status codes, logging, persistence,
or user-facing messages.

```rust
// crates/aether-oauth/src/core/error.rs:3
#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("unsupported oauth provider: {0}")]
    UnsupportedProvider(String),
    #[error("invalid oauth request: {0}")]
    InvalidRequest(String),
    #[error("oauth state is invalid or expired")]
    InvalidState,
    #[error("oauth provider returned HTTP {status_code}: {body_excerpt}")]
    HttpStatus {
        status_code: u16,
        body_excerpt: String,
    },
    #[error("oauth provider returned invalid response: {0}")]
    InvalidResponse(String),
    #[error("oauth transport failed: {0}")]
    Transport(String),
    #[error("oauth storage failed: {0}")]
    Storage(String),
    #[error("oauth encryption failed")]
    EncryptionUnavailable,
}
```

## Error Types

Use existing variants by semantic boundary:

- `UnsupportedProvider`: registry misses or trait defaults for unsupported
  flows.
- `InvalidRequest`: caller/config/state is malformed before a provider accepts
  the request.
- `InvalidState`: OAuth state expired or mismatched. This variant currently
  exists as a shared contract even though state storage is external.
- `HttpStatus`: upstream provider returned a non-2xx status.
- `InvalidResponse`: upstream provider returned malformed or incomplete JSON.
- `Transport`: executor-level network or body-read failures.
- `Storage` and `EncryptionUnavailable`: reserved for callers that need the
  same error type while storing or decrypting OAuth state outside this crate.

Prefer helper constructors when converting string details:

```rust
// crates/aether-oauth/src/core/error.rs:26
impl OAuthError {
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self::InvalidRequest(detail.into())
    }

    pub fn invalid_response(detail: impl Into<String>) -> Self {
        Self::InvalidResponse(detail.into())
    }

    pub fn transport(detail: impl Into<String>) -> Self {
        Self::Transport(detail.into())
    }
}
```

## Propagation Pattern

Service methods are thin dispatchers. They resolve the adapter once, then
propagate adapter errors with `?` and `.await`.

```rust
// crates/aether-oauth/src/provider/service.rs:61
pub async fn exchange_code(
    &self,
    executor: &dyn OAuthHttpExecutor,
    ctx: &ProviderOAuthTransportContext,
    code: &str,
    state: &str,
    pkce_verifier: Option<&str>,
) -> Result<ProviderOAuthTokenSet, OAuthError> {
    self.adapter(&ctx.provider_type)?
        .exchange_code(executor, ctx, code, state, pkce_verifier)
        .await
}
```

The identity login helper follows the same linear `?` chain. Keep this shape
when adding new orchestration helpers so each step's error keeps its original
variant.

```rust
// crates/aether-oauth/src/identity/service.rs:104
pub async fn login_with_oauth(
    provider: &dyn IdentityOAuthProvider,
    executor: &dyn OAuthHttpExecutor,
    config: &IdentityOAuthProviderConfig,
    ctx: &IdentityOAuthExchangeContext,
) -> Result<OAuthLoginOutcome, OAuthError> {
    let tokens = provider.exchange_code(executor, config, ctx).await?;
    let identity = provider
        .fetch_identity(executor, config, &tokens, ctx.network.clone())
        .await?;
    let claims = provider.map_identity(config, identity)?;
    Ok(OAuthLoginOutcome {
        claims,
        is_new_external_identity: false,
    })
}
```

## Registry Misses and Unsupported Flows

Provider lookup failures become `UnsupportedProvider`. Do not return `None` to
public service callers.

```rust
// crates/aether-oauth/src/provider/service.rs:42
pub fn adapter(
    &self,
    provider_type: &str,
) -> Result<Arc<dyn ProviderOAuthAdapter>, OAuthError> {
    self.registry
        .get(provider_type)
        .ok_or_else(|| OAuthError::UnsupportedProvider(provider_type.to_string()))
}
```

Trait default methods also use `UnsupportedProvider` for flows that a concrete
adapter has not opted into:

```rust
// crates/aether-oauth/src/provider/adapter.rs:21
fn build_authorize_url(
    &self,
    _ctx: &ProviderOAuthTransportContext,
    _state: &str,
    _code_challenge: Option<&str>,
) -> Result<OAuthAuthorizeResponse, OAuthError> {
    Err(OAuthError::UnsupportedProvider(
        self.provider_type().to_string(),
    ))
}
```

## Request Validation

Use `InvalidRequest` when the caller or stored config lacks required inputs.
The generic refresh-token import path trims the token and rejects missing or
empty values before making a network request:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:338
async fn import_credentials(
    &self,
    executor: &dyn OAuthHttpExecutor,
    ctx: &ProviderOAuthTransportContext,
    input: ProviderOAuthImportInput,
) -> Result<ProviderOAuthTokenSet, OAuthError> {
    let refresh_token = input
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OAuthError::invalid_request("refresh_token is required"))?;
    self.exchange_grant(executor, ctx, "refresh_token", refresh_token, None, None)
        .await
}
```

For identity OIDC, an invalid or missing `userinfo_url` is also a request
problem because config is incomplete:

```rust
// crates/aether-oauth/src/identity/providers/custom_oidc.rs:107
let userinfo_url = config
    .userinfo_url
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| OAuthError::invalid_request("userinfo_url is required"))?;
```

## Provider HTTP Statuses

Adapters convert non-2xx upstream responses to `OAuthError::HttpStatus` and
include only a bounded body excerpt. Keep body excerpts short because OAuth
providers can echo sensitive details.

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:238
if !(200..300).contains(&response.status_code) {
    return Err(OAuthError::HttpStatus {
        status_code: response.status_code,
        body_excerpt: truncate_body(&response.body_text),
    });
}

// crates/aether-oauth/src/provider/providers/generic.rs:432
fn truncate_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        "-".to_string()
    } else {
        body.chars().take(500).collect()
    }
}
```

Kiro and identity OIDC use the same 500-character cap inline:

```rust
// crates/aether-oauth/src/provider/providers/kiro.rs:305
if !(200..300).contains(&response.status_code) {
    return Err(OAuthError::HttpStatus {
        status_code: response.status_code,
        body_excerpt: response.body_text.chars().take(500).collect(),
    });
}
```

## Invalid Provider Responses

Use `InvalidResponse` when a provider responds successfully but the payload is
not JSON or is missing required fields.

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:244
let payload = response
    .json_body
    .or_else(|| serde_json::from_str::<Value>(&response.body_text).ok())
    .ok_or_else(|| OAuthError::invalid_response("token response is not json"))?;
self.token_set_from_payload(payload)

// crates/aether-oauth/src/provider/providers/generic.rs:251
let token_set = OAuthTokenSet::from_token_payload(payload.clone())
    .ok_or_else(|| OAuthError::invalid_response("token response missing access_token"))?;
```

`OAuthTokenSet::from_token_payload` returns `Option` because it is a parser,
not the policy layer that knows how to word the error:

```rust
// crates/aether-oauth/src/core/token.rs:15
pub fn from_token_payload(payload: Value) -> Option<Self> {
    let access_token = non_empty_string(payload.get("access_token"))
        .or_else(|| non_empty_string(payload.get("accessToken")))?;
    let expires_at_unix_secs = json_u64(
        payload
            .get("expires_in")
            .or_else(|| payload.get("expiresIn")),
    )
    .map(|expires_in| current_unix_secs().saturating_add(expires_in))
    .or_else(|| {
        json_u64(
            payload
                .get("expires_at")
                .or_else(|| payload.get("expiresAt")),
        )
    });

    Some(Self {
        access_token,
        refresh_token: non_empty_string(
            payload
                .get("refresh_token")
                .or_else(|| payload.get("refreshToken")),
        ),
        token_type: non_empty_string(
            payload
                .get("token_type")
                .or_else(|| payload.get("tokenType")),
        ),
        scope: non_empty_string(payload.get("scope")),
        expires_at_unix_secs,
        raw_payload: Some(payload),
    })
}
```

## Transport Errors

The reqwest executor maps network and body-read failures to `Transport`. It
does not decide whether a non-2xx provider response is an error; adapters own
that policy because they know the OAuth flow.

```rust
// crates/aether-oauth/src/network/executor.rs:58
let response = builder
    .send()
    .await
    .map_err(|err| OAuthError::transport(err.to_string()))?;
let status_code = response.status().as_u16();
let body_text = response
    .text()
    .await
    .map_err(|err| OAuthError::transport(err.to_string()))?;
```

## Common Mistakes

Do not use `anyhow::Result` in this crate's public API. `Cargo.toml` does not
depend on `anyhow`, and callers need the stable `OAuthError` variants.

Do not log or include raw tokens in error details. This is acceptable because
it names the missing field:

```rust
// crates/aether-oauth/src/provider/providers/kiro.rs:467
.ok_or_else(|| OAuthError::invalid_response("kiro auth_config missing access_token"))?
```

This is not acceptable:

```rust
// DON'T: exposes raw OAuth material in an error string.
return Err(OAuthError::invalid_request(format!(
    "bad auth_config: {auth_config:?}"
)));
```

Do not collapse `InvalidRequest`, `InvalidResponse`, and `Transport` into one
generic variant. The current split lets callers distinguish local config
problems, provider payload drift, and network failures.

Do not return `Ok(None)` from a public service method to hide an unsupported
provider. Only optional probes return `Result<Option<ProviderOAuthProbeResult>,
OAuthError>`; registry and auth flows fail loudly.
