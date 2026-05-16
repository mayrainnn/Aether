# Quality Guidelines

> Code quality standards for `crates/aether-oauth/`.

---

## Overview

`aether-oauth` is a shared library crate. Quality is defined by stable public
contracts, small adapter traits, injectable network behavior, safe token
handling, and focused provider tests. Prefer narrow additions that keep OAuth
logic reusable by gateway, admin, runtime, and data layers.

The crate intentionally keeps dependencies small:

```toml
# crates/aether-oauth/Cargo.toml:9
[dependencies]
aether-contracts.workspace = true
async-trait.workspace = true
base64.workspace = true
http.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
url.workspace = true
uuid.workspace = true
```

Do not add new dependencies without an explicit need that cannot be handled by
the existing workspace crates.

## Required Patterns

### Use Adapter Traits for Provider Behavior

Provider-account OAuth behavior belongs behind `ProviderOAuthAdapter`. This
keeps authorize, exchange, import, refresh, request-auth, fingerprint, and
probe behavior swappable and testable.

```rust
// crates/aether-oauth/src/provider/adapter.rs:15
#[async_trait]
pub trait ProviderOAuthAdapter: Send + Sync {
    fn provider_type(&self) -> &'static str;

    fn capabilities(&self) -> ProviderOAuthCapabilities;

    async fn import_credentials(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        input: ProviderOAuthImportInput,
    ) -> Result<ProviderOAuthTokenSet, OAuthError>;

    async fn refresh(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        account: &ProviderOAuthAccount,
    ) -> Result<ProviderOAuthTokenSet, OAuthError>;
}
```

Identity-login behavior belongs behind `IdentityOAuthProvider`:

```rust
// crates/aether-oauth/src/identity/adapter.rs:59
#[async_trait]
pub trait IdentityOAuthProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;

    fn build_authorize_url(
        &self,
        config: &IdentityOAuthProviderConfig,
        ctx: &IdentityOAuthStartContext,
    ) -> Result<OAuthAuthorizeResponse, OAuthError>;

    async fn exchange_code(
        &self,
        executor: &dyn OAuthHttpExecutor,
        config: &IdentityOAuthProviderConfig,
        ctx: &IdentityOAuthExchangeContext,
    ) -> Result<OAuthTokenSet, OAuthError>;
}
```

### Normalize Provider Types Through the Registry

Use `OAuthAdapterRegistry` rather than a local `HashMap` or ad hoc lowercase
logic. The registry trims and lowercases provider keys, stores adapters in
`Arc`, and returns cloned adapter handles.

```rust
// crates/aether-oauth/src/core/registry.rs:38
pub fn insert(&mut self, provider_type: &str, adapter: Arc<T>) {
    let key = provider_type.trim().to_ascii_lowercase();
    if !key.is_empty() {
        self.adapters.insert(key, adapter);
    }
}

// crates/aether-oauth/src/core/registry.rs:45
pub fn get(&self, provider_type: &str) -> Option<Arc<T>> {
    self.adapters
        .get(provider_type.trim().to_ascii_lowercase().as_str())
        .cloned()
}
```

### Keep Network Behavior Injectable

Adapters must build `OAuthHttpRequest` values and call the injected executor.
This allows tests to capture requests and lets higher layers decide proxy,
timeout, and client configuration.

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

The generic provider adapter follows this pattern:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:191
executor
    .execute(OAuthHttpRequest {
        request_id: request_id.clone(),
        method: reqwest::Method::POST,
        url: self.token_url(),
        headers: json_headers(),
        content_type: Some("application/json".to_string()),
        json_body: Some(Value::Object(body)),
        body_bytes: None,
        network: ctx.network.clone(),
    })
    .await?
```

### Preserve Token Flexibility and Existing Metadata

Token parsing must accept provider field variations and trim empty strings.
`OAuthTokenSet::from_token_payload` currently supports snake_case and
camelCase token fields:

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

Refresh flows must preserve stable metadata and existing refresh tokens when
providers do not rotate them:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:371
// Refresh responses often omit stable account metadata, and some providers
// do not rotate refresh_token on every refresh. Preserve the stored config
// as the base while letting the fresh token payload win.
if let Some(existing) = account.auth_config.as_object() {
    let mut merged = existing.clone();
    if let Some(updated) = refreshed.auth_config.as_object() {
        for (key, value) in updated {
            merged.insert(key.clone(), value.clone());
        }
    }
    if refreshed.token_set.refresh_token.is_none() {
        refreshed.token_set.refresh_token = Some(refresh_token.to_string());
        merged.insert("refresh_token".to_string(), json!(refresh_token));
    }
    refreshed.auth_config = Value::Object(merged);
}
```

### Use Deterministic Maps for Protocol Data

The crate uses `BTreeMap` for headers, identity maps, callback params, and
registry storage. Keep using deterministic map types where output stability or
test readability matters.

```rust
// crates/aether-oauth/src/core/pkce.rs:25
pub fn parse_oauth_callback_params(callback_url: &str) -> BTreeMap<String, String> {
    let mut merged = BTreeMap::new();
    let Ok(url) = Url::parse(callback_url.trim()) else {
        return merged;
    };

    for (key, value) in form_urlencoded::parse(url.query().unwrap_or_default().as_bytes()) {
        merged.insert(key.into_owned(), value.into_owned());
    }
    if let Some(fragment) = url.fragment() {
        for (key, value) in form_urlencoded::parse(fragment.trim_start_matches('#').as_bytes()) {
            merged.insert(key.into_owned(), value.into_owned());
        }
    }
}
```

## Forbidden Patterns

Do not instantiate reqwest clients inside adapters:

```rust
// DON'T: adapter code should not own client construction.
let client = reqwest::Client::new();
let response = client.post(url).send().await?;
```

Use an injected executor instead:

```rust
// crates/aether-oauth/src/network/executor.rs:37
impl ReqwestOAuthHttpExecutor {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}
```

Do not expose raw secrets in fingerprints, debug strings, or error text. Use a
bounded digest for account fingerprints:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:441
fn secret_fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut fingerprint = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(&mut fingerprint, "{byte:02x}");
    }
    fingerprint
}
```

Do not hardcode provider-specific branches in `ProviderOAuthService` beyond
builtin registration. Provider-specific behavior belongs in the adapter:

```rust
// crates/aether-oauth/src/provider/providers/codex.rs:34
fn build_authorize_url(
    &self,
    ctx: &crate::provider::ProviderOAuthTransportContext,
    state: &str,
    code_challenge: Option<&str>,
) -> Result<crate::core::OAuthAuthorizeResponse, crate::core::OAuthError> {
    let mut response = self.inner.build_authorize_url(ctx, state, code_challenge)?;
    let mut url = url::Url::parse(&response.authorize_url)
        .map_err(|_| crate::core::OAuthError::invalid_response("invalid authorize_url"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("prompt", "login");
        query.append_pair("id_token_add_organizations", "true");
        query.append_pair("codex_cli_simplified_flow", "true");
    }
    response.authorize_url = url.to_string();
    Ok(response)
}
```

Do not add database entities or persistence policy here. `ProviderOAuthAccount`
is an input/output DTO, not a SeaORM model.

Do not silently ignore malformed required input. Use `ok_or_else` with
`OAuthError::invalid_request` or `OAuthError::invalid_response` as appropriate.

## Testing Requirements

Put tests next to the owning module under `#[cfg(test)]`. Use sync `#[test]`
for pure helpers and `#[tokio::test]` for adapter flows that cross an async
executor.

Existing pure helper tests:

```rust
// crates/aether-oauth/src/core/pkce.rs:79
#[test]
fn pkce_s256_is_url_safe() {
    let value = pkce_s256("verifier");
    assert!(!value.contains('+'));
    assert!(!value.contains('/'));
    assert!(!value.contains('='));
}
```

Existing async adapter tests use a fake executor that captures the request:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:626
#[derive(Debug, Clone)]
struct StaticExecutor {
    seen_request: Arc<Mutex<Option<OAuthHttpRequest>>>,
}

// crates/aether-oauth/src/provider/providers/generic.rs:631
#[async_trait]
impl OAuthHttpExecutor for StaticExecutor {
    async fn execute(
        &self,
        request: OAuthHttpRequest,
    ) -> Result<OAuthHttpResponse, crate::core::OAuthError> {
        *self.seen_request.lock().expect("mutex should lock") = Some(request);
        Ok(OAuthHttpResponse {
            status_code: 200,
            body_text: json!({
                "access_token": "new-access-token",
                "expires_in": 3600
            })
            .to_string(),
            json_body: None,
        })
    }
}
```

Minimum source verification for this crate:

```bash
cargo test -p aether-oauth
```

When changing public types or re-exports, also run at least:

```bash
cargo check -p aether-oauth
rg -n "ProviderOAuthService|IdentityOAuthService|OAuthTokenSet|OAuthHttpExecutor" crates apps -g '*.rs'
```

## Code Review Checklist

Reviewers should check:

- New provider behavior is behind the correct trait, not embedded in a service
  dispatcher.
- `provider_type` strings are normalized by registry lookup.
- OAuth HTTP calls use `OAuthHttpExecutor` and carry the caller-provided
  `OAuthNetworkContext`.
- Non-2xx upstream responses become bounded `OAuthError::HttpStatus` errors.
- Required missing input uses `InvalidRequest`; malformed upstream payloads use
  `InvalidResponse`.
- Token parser changes retain snake_case and camelCase compatibility.
- Refresh behavior preserves existing metadata and non-rotated refresh tokens.
- New code does not log or format access tokens, refresh tokens, client
  secrets, auth_config blobs, or userinfo payloads.
- New helper visibility is no wider than needed.
- Tests cover provider-specific query params, request bodies, metadata merge,
  fallback defaults, and error cases.
