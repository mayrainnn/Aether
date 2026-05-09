# Directory Structure

> How `crates/aether-oauth/` is organized and where new OAuth code belongs.

---

## Overview

`aether-oauth` is organized by OAuth responsibility, not by web route or
database table. The crate has four public module families:

- `core`: shared OAuth data structures, PKCE helpers, token parsing, registry,
  and `OAuthError`.
- `network`: network policy/context types plus the injectable HTTP executor
  trait and reqwest-backed executor.
- `identity`: end-user login and external identity binding flows.
- `provider`: AI-provider account OAuth, import, refresh, request-auth, and
  account-probe flows.

The top-level facade makes those families explicit:

```rust
// crates/aether-oauth/src/lib.rs:1
pub mod core;
pub mod identity;
pub mod network;
pub mod provider;

// crates/aether-oauth/src/lib.rs:11
pub use network::{
    NetworkRequirement, OAuthHttpExecutor, OAuthHttpRequest, OAuthHttpResponse,
    OAuthNetworkContext, OAuthNetworkPolicy, OAuthTimeouts,
};
```

ABCoder MCP reported the actual module/package structure for
`repo_name="aether-oauth"` as `aether-oauth::core`, `aether-oauth::network`,
`aether-oauth::identity`, `aether-oauth::identity::providers`,
`aether-oauth::provider`, and `aether-oauth::provider::providers`.

## Directory Layout

```text
crates/aether-oauth/
|-- Cargo.toml
`-- src/
    |-- lib.rs
    |-- core/
    |   |-- error.rs
    |   |-- flow.rs
    |   |-- mod.rs
    |   |-- pkce.rs
    |   |-- registry.rs
    |   `-- token.rs
    |-- network/
    |   |-- context.rs
    |   |-- executor.rs
    |   `-- mod.rs
    |-- identity/
    |   |-- adapter.rs
    |   |-- mod.rs
    |   |-- service.rs
    |   `-- providers/
    |       |-- custom_oidc.rs
    |       |-- linuxdo.rs
    |       `-- mod.rs
    `-- provider/
        |-- account.rs
        |-- adapter.rs
        |-- mod.rs
        |-- service.rs
        `-- providers/
            |-- antigravity.rs
            |-- codex.rs
            |-- generic.rs
            |-- kiro.rs
            `-- mod.rs
```

## Module Ownership

`src/core/` owns reusable OAuth primitives. Put shared request/response shapes
in `flow.rs`, token payload parsing in `token.rs`, PKCE and callback parsing in
`pkce.rs`, provider registry mechanics in `registry.rs`, and crate-wide errors
in `error.rs`.

```rust
// crates/aether-oauth/src/core/mod.rs:1
mod error;
mod flow;
mod pkce;
mod registry;
mod token;

// crates/aether-oauth/src/core/mod.rs:7
pub use error::OAuthError;
pub use pkce::{
    generate_oauth_nonce, generate_pkce_verifier, parse_oauth_callback_params,
    pkce_s256,
};
```

`src/network/` defines the transport boundary. OAuth adapters should build
`OAuthHttpRequest` values and receive an injected `OAuthHttpExecutor`; they
should not own reqwest clients directly.

```rust
// crates/aether-oauth/src/network/executor.rs:27
#[async_trait]
pub trait OAuthHttpExecutor: Send + Sync {
    async fn execute(&self, request: OAuthHttpRequest) -> Result<OAuthHttpResponse, OAuthError>;
}
```

`src/identity/` is for user-facing identity OAuth. `adapter.rs` defines
provider contracts and identity claim mapping helpers; `service.rs` owns the
registry and start/login/bind orchestration; `providers/` contains concrete
identity providers.

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

`src/provider/` is for AI-provider account OAuth. `account.rs` owns account,
capability, token-set, import-input, and request-auth DTOs; `adapter.rs` owns
the provider-account adapter trait; `service.rs` dispatches through the
registry; `providers/` contains concrete provider implementations.

```rust
// crates/aether-oauth/src/provider/mod.rs:1
mod account;
mod adapter;
pub mod providers;
mod service;

// crates/aether-oauth/src/provider/mod.rs:6
pub use account::{
    ProviderOAuthAccount, ProviderOAuthAccountState, ProviderOAuthCapabilities,
    ProviderOAuthImportInput, ProviderOAuthRequestAuth, ProviderOAuthTokenSet,
    ProviderOAuthTransportContext,
};
```

## Facade and Visibility Rules

Each submodule keeps implementation files private by default and exports only
the stable surface through its local `mod.rs`. The provider and identity
`providers` modules are public because callers may need provider constructors
or test helpers; inner files remain module-owned.

```rust
// crates/aether-oauth/src/provider/providers/mod.rs:1
mod antigravity;
mod codex;
mod generic;
mod kiro;

// crates/aether-oauth/src/provider/providers/mod.rs:6
pub use antigravity::AntigravityProviderOAuthAdapter;
pub use codex::CodexProviderOAuthAdapter;
pub use generic::{
    GenericProviderOAuthAdapter, GenericProviderOAuthTemplate,
    GENERIC_PROVIDER_OAUTH_TEMPLATES,
};
```

Use `pub(crate)` for helpers that are shared within a module family but should
not become crate API. The identity adapter mapping helpers are a good example:

```rust
// crates/aether-oauth/src/identity/adapter.rs:91
pub(crate) fn mapped_string(
    raw: &Value,
    mapping: Option<&Value>,
    logical_key: &str,
) -> Option<String> {
    let mapped_key = mapping
        .and_then(Value::as_object)
        .and_then(|object| object.get(logical_key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(logical_key);
    find_string(raw, mapped_key)
}
```

## Adding New Code

For a new provider-account OAuth type:

1. Prefer adding a `GenericProviderOAuthTemplate` entry in
   `provider/providers/generic.rs` when the provider uses standard authorize
   and token endpoints.
2. Create `provider/providers/<provider>.rs` only when the provider needs
   custom authorization parameters, refresh semantics, request-auth shape, or
   account probing.
3. Export the adapter in `provider/providers/mod.rs`.
4. Register builtin support in
   `ProviderOAuthService::with_builtin_adapters`.
5. Add tests in the provider module or service module.

The builtin registry pattern is explicit:

```rust
// crates/aether-oauth/src/provider/service.rs:19
pub fn with_builtin_adapters() -> Self {
    use super::providers::{
        AntigravityProviderOAuthAdapter, CodexProviderOAuthAdapter,
        GenericProviderOAuthAdapter, KiroProviderOAuthAdapter,
    };

    let mut service = Self::new()
        .with_adapter(Arc::new(KiroProviderOAuthAdapter::default()))
        .with_adapter(Arc::new(CodexProviderOAuthAdapter::default()))
        .with_adapter(Arc::new(AntigravityProviderOAuthAdapter::default()));
    for provider_type in ["claude_code", "chatgpt_web", "gemini_cli"] {
        if let Some(adapter) = GenericProviderOAuthAdapter::for_provider_type(provider_type) {
            service = service.with_adapter(Arc::new(adapter));
        }
    }
    service
}
```

For a new identity-login provider:

1. Add the concrete provider under `identity/providers/`.
2. Implement `IdentityOAuthProvider`.
3. Export it from `identity/providers/mod.rs`.
4. Register it in `IdentityOAuthService::with_builtin_providers` if it is a
   builtin provider.

```rust
// crates/aether-oauth/src/identity/service.rs:31
pub fn with_builtin_providers() -> Self {
    use super::providers::{CustomOidcIdentityOAuthProvider, LinuxDoIdentityOAuthProvider};

    Self::new()
        .with_provider(Arc::new(LinuxDoIdentityOAuthProvider::default()))
        .with_provider(Arc::new(CustomOidcIdentityOAuthProvider))
}
```

## Naming Conventions

Use the crate's existing vocabulary:

- Core shared types start with `OAuth`: `OAuthTokenSet`,
  `OAuthAuthorizeResponse`, `OAuthNetworkContext`.
- Provider-account types start with `ProviderOAuth`: `ProviderOAuthAdapter`,
  `ProviderOAuthService`, `ProviderOAuthAccount`.
- Identity-login types start with `IdentityOAuth`: `IdentityOAuthProvider`,
  `IdentityOAuthService`, `IdentityOAuthProviderConfig`.
- Provider type constants and strings are lowercase snake_case:
  `"codex"`, `"chatgpt_web"`, `"gemini_cli"`, `"custom_oidc"`, `"kiro"`.
- Test-only override methods make their purpose obvious, such as
  `with_token_url_for_tests`.

## Common Mistakes

Do not put application persistence into this crate. A `ProviderOAuthAccount`
contains `auth_config` and `identity` values, but it is not a database entity:

```rust
// crates/aether-oauth/src/provider/account.rs:49
pub struct ProviderOAuthAccount {
    pub provider_type: String,
    pub access_token: String,
    pub auth_config: Value,
    pub expires_at_unix_secs: Option<u64>,
    pub identity: BTreeMap<String, Value>,
}
```

Do not bypass module facades with ad hoc paths from callers. Add stable
exports through the owning `mod.rs` if a type is truly public.

Do not add custom provider files when a template entry is enough. The generic
template table already covers standard authorization-code providers:

```rust
// crates/aether-oauth/src/provider/providers/generic.rs:15
pub struct GenericProviderOAuthTemplate {
    pub provider_type: &'static str,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub use_pkce: bool,
    pub uses_json_payload: bool,
}
```
