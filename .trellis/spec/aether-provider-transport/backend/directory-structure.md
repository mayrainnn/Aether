# Directory Structure

`crates/aether-provider-transport` is a Rust library crate with a thin public
facade and many small provider/request helper modules. The crate is deliberately
not a web layer: it receives `GatewayProviderTransportSnapshot` data and returns
request pieces, support decisions, diagnostics, or refreshed auth state.

## Public Facade

Use `src/lib.rs` as the public API contract. It declares provider modules that
external crates may need (`antigravity`, `claude_code`, `kiro`, `oauth_refresh`,
`policy`, `provider_types`, `same_format_provider`, `snapshot`, `url`, `vertex`)
and keeps implementation helpers private (`auth_config`, `cache`, `diagnostics`,
`headers`, `network`, `request_url`, `standard`, and `video`).

Example:

```rust
// crates/aether-provider-transport/src/lib.rs:1
pub mod antigravity;
pub mod auth;
mod auth_config;
mod cache;
```

The re-export block in `lib.rs` is the stable consumption surface. Gateway code
should import helpers such as `build_same_format_provider_headers`,
`supports_local_standard_transport`, or `read_provider_transport_snapshot` from
the crate root instead of reaching into private modules.

Example:

```rust
// crates/aether-provider-transport/src/lib.rs:83
pub use same_format_provider::{
    build_same_format_provider_headers, build_same_format_provider_request_body,
    build_same_format_provider_upstream_url, classify_same_format_provider_request_behavior,
};
```

## Module Groups

The actual layout follows these groups:

| Module | Role |
|--------|------|
| `snapshot.rs` and `snapshot_mapping.rs` | Convert data-contract catalog records into transport snapshots and decrypt key material |
| `policy.rs`, `provider_types.rs`, `conversion.rs` | Decide whether local transport or conversion is supported and why |
| `auth.rs`, `auth_config.rs`, `oauth_refresh/`, `generic_oauth/` | Resolve direct auth, absorb safe auth-config subsets, and refresh OAuth entries |
| `request_url/`, `url.rs` | Build provider URLs, custom paths, provider hooks, and query merging |
| `standard/`, `same_format_provider/`, `openai_image/`, `gemini_files/`, `video/` | Build request headers and bodies for shared transport families |
| `claude_code/`, `kiro/`, `vertex/`, `antigravity/` | Provider-specific adapters with auth, policy, request, URL, and credential details |
| `headers.rs`, `network.rs`, `diagnostics.rs`, `cache.rs`, `rules.rs` | Cross-cutting transport helpers |

## Provider-Specific Modules

Create a provider directory when the provider needs more than one helper class of
logic. Existing provider directories keep their submodules private and re-export
only the provider API from `mod.rs`.

Example:

```rust
// crates/aether-provider-transport/src/kiro/mod.rs:1
mod auth;
mod converter;
mod credentials;
mod headers;
mod policy;
mod refresh;
mod request;
mod url;
```

Follow that pattern for any provider that needs separate auth, request, URL, and
policy logic. Do not put all provider logic in `lib.rs` or in a generic shared
file just to avoid a directory.

## Shared Helpers

Shared helpers live at crate root and should stay narrow:

1. `auth.rs` builds passthrough headers and direct auth values.
2. `headers.rs` owns request-header filters.
3. `request_url/mod.rs` owns all URL composition, custom-path expansion, and
   provider URL hooks.
4. `network.rs` owns proxy, timeout, transport profile, and tunnel-affinity
   resolution.
5. `diagnostics.rs` builds JSON diagnostics without leaking secrets.

Example:

```rust
// crates/aether-provider-transport/src/request_url/mod.rs:30
pub fn build_transport_request_url(
    transport: &GatewayProviderTransportSnapshot,
    params: TransportRequestUrlParams<'_>,
) -> Option<String> {
```

Keep shared modules free of provider-only state unless the provider is truly a
hook in the shared request path, as with Kiro region URLs, Claude Code messages,
Vertex API-key query auth, and Antigravity internal URLs
(`crates/aether-provider-transport/src/request_url/mod.rs:224`).

## Snapshot Boundary

`snapshot.rs` is the only module that knows about repository contract structs.
It defines `ProviderTransportSnapshotSource` and asks the source for providers,
endpoints, and keys by ID. The crate does not import SeaORM or database
connection traits.

Example:

```rust
// crates/aether-provider-transport/src/snapshot.rs:77
#[async_trait]
pub trait ProviderTransportSnapshotSource: Send + Sync {
    fn encryption_key(&self) -> Option<&str>;
```

`snapshot_mapping.rs` then maps those records into `GatewayProviderTransport*`
structs and normalizes legacy JSON values. Keep new snapshot fields in these two
files instead of spreading mapping rules into request builders.

## Naming Rules

Use snake_case file names and function names. Provider directories use provider
type names (`claude_code`, `generic_oauth`, `openai_image`). Public structs and
enums should include the transport family in the name, for example
`SameFormatProviderRequestBehavior`, `StandardPlanFallbackAcceptPolicy`, and
`LocalOAuthRefreshCoordinator`.

Reason strings are stable machine-readable snake_case strings such as
`transport_api_format_mismatch`, `transport_proxy_unsupported`, and
`transport_oauth_resolution_unsupported` (`crates/aether-provider-transport/src/policy.rs:45`).
Do not replace these with prose.

## Where New Code Belongs

1. New fixed provider template: start in `provider_types.rs`, then add a
   provider directory only when request/auth behavior is not standard.
2. New URL shape: add it to `request_url/mod.rs` or provider-specific `url.rs`.
3. New auth shape: add provider-specific auth parsing under the provider
   directory; expose only a typed resolver through `mod.rs`.
4. New request body transformation: add it to the specific family module
   (`same_format_provider`, `standard`, `gemini_files`, `video`) unless it is
   unique to a provider.
5. New snapshot field: add it to `GatewayProviderTransportSnapshot`, mapping,
   and tests before using it in policy/builders.

## Anti-Patterns

DON'T: add route-handler logic, axum extractors, or database queries here. The
crate is a transport domain crate, not an application service.

DON'T: expose private implementation modules just because a caller needs one
helper. Add a crate-root re-export in `lib.rs` when the helper is part of the
contract.

DON'T: duplicate URL or header construction in provider modules when
`request_url`, `auth`, or `headers` already owns the shared invariant.

DON'T: create provider modules named after display names. Use provider_type
tokens that match runtime policy checks, for example `claude_code`, `kiro`,
`gemini_cli`, `vertex_ai`, and `antigravity`.
