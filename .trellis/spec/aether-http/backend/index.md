# aether-http Backend Guidelines

`aether-http` is a foundation crate for shared outbound HTTP client setup and
retry timing in Aether's Rust services. It is intentionally small: four Rust
source files, no internal Aether crate dependencies, and a public API exported
through `src/lib.rs`.

Use these guidelines before changing `crates/aether-http/` or adding a new
consumer of its helpers.

## Pre-Development Checklist

1. Confirm the change really belongs in `aether-http`, not in a caller-specific
   service crate.
2. Keep the crate leaf-level. It currently depends only on `reqwest` and
   `serde` in `crates/aether-http/Cargo.toml:8`.
3. Do not add database, cache, auth, gateway, proxy, scheduler, or domain
   dependencies to this crate.
4. Reuse `HttpClientConfig` for shared reqwest builder knobs instead of adding
   ad hoc helper functions in caller crates.
5. Reuse `HttpRetryConfig` plus `jittered_delay_for_retry` for shared retry
   backoff timing.
6. Preserve `src/lib.rs` as the only public facade for module exports.
7. Normalize retry config before long-running retry loops.
8. Let `reqwest::Error` bubble out of client-build helpers; do not wrap it in
   this crate unless a crate-local error type becomes necessary.
9. Keep tests near the tiny helper being protected.
10. Run `cargo test -p aether-http` after docs or code changes that describe or
    touch the crate behavior.

## Quick Source Map

```rust
// crates/aether-http/src/lib.rs:1
mod client;
mod config;
mod retry;

pub use client::{apply_http_client_config, build_http_client, build_http_client_with_headers};
pub use config::{HttpClientConfig, HttpRetryConfig};
pub use retry::jittered_delay_for_retry;
```

The exported API is deliberately limited to two config types and three helper
functions. Keep new module internals private until they are useful to more than
one crate.

## Guide Index

| Guide | Use For |
| --- | --- |
| [Directory Structure](./directory-structure.md) | File layout, module boundaries, and where new helpers belong. |
| [Error Handling](./error-handling.md) | `reqwest::Error` propagation, proxy validation, and retry timing failure rules. |
| [Quality Guidelines](./quality-guidelines.md) | Naming, visibility, type safety, tests, and forbidden patterns. |
| [Logging Guidelines](./logging-guidelines.md) | Why this crate stays log-free and where callers should log. |

## Non-Applicable Guides

`database-guidelines.md` was removed for this package. `aether-http` has no
SeaORM, Redis, migration, transaction, or connection-pool responsibilities.
Adding database guidance here would mislead future agents into treating a
foundation HTTP utility crate like a persistence layer.

## Tool Evidence Used To Build This Spec

GitNexus status reported the Aether index up to date at commit `209322b`.
GitNexus context found these important upstream consumers of `HttpClientConfig`:

- `crates/aether-testkit/src/http.rs:test_http_client_config`
- `apps/aether-proxy/src/setup/upgrade.rs:build_github_client`
- `apps/aether-proxy/src/registration/client.rs:AetherClient.new`
- `apps/aether-gateway/src/state/core.rs:AppState.build`
- `apps/aether-gateway/src/execution_runtime/transport.rs:build_client`
- `apps/aether-gateway/src/tunnel/embedded/control_plane.rs:ControlPlaneClient.new`

ABCoder MCP, run against the isolated `aether-http` AST, reported one module
named `aether-http`, four packages, and four files: `src/lib.rs`,
`src/client.rs`, `src/config.rs`, and `src/retry.rs`. It also confirmed the AST
nodes for `HttpClientConfig`, `HttpRetryConfig`,
`apply_http_client_config`, `build_http_client`,
`build_http_client_with_headers`, `HttpRetryConfig::normalized`,
`HttpRetryConfig::delay_for_retry`, and `jittered_delay_for_retry`.

## Quality Check

Before finishing work in this crate:

1. `rg -n "template marker|HTML comment" .trellis/spec/aether-http/backend`
   should return nothing after replacing those terms with the concrete legacy
   marker strings being checked.
2. `find .trellis/spec/aether-http/backend -maxdepth 1 -type f -name "*.md"`
   should list only the files in the Guide Index above.
3. `cargo test -p aether-http` should pass.
4. New docs must cite real source paths and line numbers.
5. New examples must match actual project code, not generic Rust snippets.
6. If a new dependency is proposed, verify it does not break the foundation
   layer rule in the PRD.
