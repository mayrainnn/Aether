# aether-oauth Backend Guidelines

> Entry point for backend work in `crates/aether-oauth/`.

---

## Package Summary

`aether-oauth` is the shared OAuth abstraction crate for Aether identity login
flows and provider account authentication flows. It is a Rust library crate,
not an axum route crate and not a persistence crate. Its public API is grouped
around reusable OAuth contracts, pluggable adapters, network execution
boundaries, and provider-specific token refresh/authentication behavior.

Evidence:

```toml
# crates/aether-oauth/Cargo.toml:1
[package]
name = "aether-oauth"
description = "Shared OAuth abstractions for Aether identity and provider account flows"

# crates/aether-oauth/Cargo.toml:9
[dependencies]
aether-contracts.workspace = true
async-trait.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

The crate facade exposes four module families:

```rust
// crates/aether-oauth/src/lib.rs:1
pub mod core;
pub mod identity;
pub mod network;
pub mod provider;

// crates/aether-oauth/src/lib.rs:6
pub use core::{
    current_unix_secs, generate_oauth_nonce, generate_pkce_verifier,
    parse_oauth_callback_params, pkce_s256, OAuthAdapterRegistry,
    OAuthAuthorizeRequest, OAuthAuthorizeResponse, OAuthCallback, OAuthError,
    OAuthProviderMetadata, OAuthTokenSet,
};
```

GitNexus confirmed the Aether index is current for `repo="Aether"` at commit
`209322b` and contains 3,140 files, 83,229 symbols, and 300 execution flows.
The `aether-oauth` crate is mostly a library boundary: GitNexus symbol queries
show service and adapter methods but no top-level app process anchored inside
this crate.

ABCoder MCP for `repo_name="aether-oauth"` parsed one module with the following
package families:

```text
aether-oauth::core
aether-oauth::network
aether-oauth::identity
aether-oauth::identity::providers
aether-oauth::provider
aether-oauth::provider::providers
```

## Guidelines Index

| Guide | Description | Status |
| --- | --- | --- |
| [Directory Structure](./directory-structure.md) | Module layout, facade exports, adapter ownership, and where new OAuth code belongs. | Filled |
| [Error Handling](./error-handling.md) | `OAuthError`, propagation, HTTP status/body handling, token parsing failures, and caller boundaries. | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Adapter traits, registry normalization, token safety, dependency boundaries, and test patterns. | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Current no-logging stance, safe caller-side observability, and request-id conventions. | Filled |

`database-guidelines.md` is intentionally absent. This crate has no SeaORM,
SQLx, Redis, migrations, repositories, connection pools, transactions, or
database query code. OAuth credential storage belongs to higher-level data or
runtime-state crates; this crate only models in-memory token/auth_config values
and receives an injected HTTP executor.

## Pre-Development Checklist

Before editing `crates/aether-oauth/`, verify:

- The change belongs to shared OAuth mechanics, not an application route,
  database repository, billing policy, or provider scheduling rule.
- New provider-account behavior implements `ProviderOAuthAdapter` and is
  registered through `ProviderOAuthService::with_builtin_adapters` when it
  should be builtin.
- New identity-login behavior implements `IdentityOAuthProvider` and is
  registered through `IdentityOAuthService::with_builtin_providers` when it
  should be builtin.
- Provider-type strings are normalized through `OAuthAdapterRegistry`, not by
  open-coded maps at call sites.
- Network calls go through `OAuthHttpExecutor`; adapters do not instantiate
  reqwest clients directly except for the executor implementation.
- Secrets are stored only in token/auth_config values and never formatted into
  logs, error messages, debug helpers, or fingerprints.
- Token parsing preserves provider payload flexibility, including snake_case
  and camelCase fields.
- Refresh logic preserves stable metadata and existing refresh tokens when a
  provider omits rotation data.
- New behavior has focused unit tests in the owning module.

## Public Contract Anchors

Use these source locations as stable examples:

- `crates/aether-oauth/src/core/error.rs:3`: `OAuthError` enum and helper
  constructors.
- `crates/aether-oauth/src/core/token.rs:4`: `OAuthTokenSet` with optional
  refresh, expiry, scope, and raw payload data.
- `crates/aether-oauth/src/core/registry.rs:5`: generic
  `OAuthAdapterRegistry<T: ?Sized>` backed by lowercased provider keys and
  `Arc<T>`.
- `crates/aether-oauth/src/network/executor.rs:27`: injectable
  `OAuthHttpExecutor` trait.
- `crates/aether-oauth/src/provider/adapter.rs:15`: provider-account OAuth
  adapter trait.
- `crates/aether-oauth/src/provider/service.rs:9`: provider adapter registry
  service.
- `crates/aether-oauth/src/identity/adapter.rs:59`: identity-login OAuth
  provider trait.
- `crates/aether-oauth/src/identity/service.rs:9`: identity provider registry
  service.

## Quality Gate

Minimum checks after changing only these spec docs:

```bash
rg -n 'template residue|HTML comment marker' .trellis/spec/aether-oauth/backend
find .trellis/spec/aether-oauth/backend -maxdepth 1 -name '*.md' -print0 | xargs -0 wc -l
```

Minimum checks after changing Rust source in this crate:

```bash
cargo fmt --check -p aether-oauth
cargo test -p aether-oauth
```

For public contract or provider behavior changes, also search direct consumers:

```bash
rg -n "ProviderOAuthService|IdentityOAuthService|OAuthTokenSet|OAuthHttpExecutor" crates apps -g '*.rs'
```

## Non-Goals

Do not add the following to this crate:

- Database access, migration code, Redis commands, repositories, or
  transactions.
- axum handlers, admin APIs, or gateway routing.
- Provider scheduling, billing, quota persistence, or usage accounting.
- Background workers or tokio task orchestration.
- Logging of tokens, refresh tokens, client secrets, auth_config blobs, or
  userinfo payloads.
- Provider-specific behavior that can be represented by
  `GenericProviderOAuthTemplate` without a custom adapter.
