# Aether Gateway Backend Guidelines

Backend coding guidance for `apps/aether-gateway`, the axum application crate that
fronts Aether AI APIs, admin/control routes, local execution runtime selection,
streaming response finalization, and persistence-backed support APIs.

These guides document the code that exists today. They are not generic Rust
rules and should be updated when the gateway execution path, data facade, or
route taxonomy changes.

## Scope

The crate is an application layer crate. It wires together lower-layer crates
such as `aether-ai-serving`, `aether-provider-transport`,
`aether-scheduler-core`, `aether-data`, `aether-runtime-state`,
`aether-usage-runtime`, and `aether-wallet`.

Relevant source entry points:

```rust
// apps/aether-gateway/src/lib.rs:27
mod admin_api;
mod ai_serving;
mod api;
mod async_task;
mod audit;
mod auth;
mod cache;
mod control;
mod data;
mod error;
mod execution_runtime;
mod executor;
mod handlers;
mod maintenance;
mod scheduler;
mod state;
```

Public crate surface is deliberately small:

```rust
// apps/aether-gateway/src/lib.rs:92
pub use self::router::{attach_static_frontend, build_router, build_router_with_state, serve_tcp};
pub(crate) use self::error::GatewayError;
pub use self::state::{AppState, FrontdoorCorsConfig};
```

## GitNexus Findings Used

GitNexus resource reads for repo `Aether` showed an indexed repository with
3140 files, 83229 symbols, and 300 execution flows. The gateway-specific traces
used here include:

- `Proxy_request -> Response_is_sse`: `proxy_request` flows through
  `build_local_overloaded_response`, `build_client_response_from_parts`,
  `apply_streaming_response_headers`, and `response_is_sse`.
- `Proxy_request -> Is_execution_runtime_candidate`: `proxy_request` flows
  through `insert_execution_runtime_candidate_headers` and
  `GatewayControlDecision::is_execution_runtime_candidate`.
- Clusters relevant to this crate include `Planner`, `Frontdoor`, `Auth`, and
  `Execution_runtime`.

The direct GitNexus query/context tools were unavailable in this run because
the MCP tool calls returned `user cancelled MCP tool call`; GitNexus resources
were still available and were cross-checked with source reads.

## ABCoder Availability

The PRD requested ABCoder MCP with `repo_name="aether-gateway"`. In this Codex
session, no ABCoder MCP namespace or resources were exposed, and the local CLI
check returned `abcoder: command not found`. These guidelines therefore use
GitNexus resources plus direct source inspection. If ABCoder is restored later,
refresh exact AST signature checks for the symbols cited here.

## Guidelines Index

| Guide | Purpose | Status |
| --- | --- | --- |
| [Directory Structure](./directory-structure.md) | Module layout, ownership boundaries, route organization | Filled |
| [Request Execution Guidelines](./request-execution-guidelines.md) | Control decisions, local execution, planner/fallback, response finalization, SSE | Filled |
| [Database Guidelines](./database-guidelines.md) | `GatewayDataState`, repository facade, migrations, Redis/runtime state | Filled |
| [Error Handling](./error-handling.md) | `GatewayError`, local responses, propagation and conversion | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Visibility, naming, tests, anti-patterns | Filled |
| [Logging Guidelines](./logging-guidelines.md) | `tracing` levels, structured fields, access/audit/ops patterns | Filled |

## Rules For Future Updates

- Keep source references as concrete file paths with line numbers.
- Prefer examples from `apps/aether-gateway/src` over examples from dependent
  crates unless the guideline is about an explicit boundary.
- Update `request-execution-guidelines.md` whenever a new AI public route,
  execution runtime step, or response finalization behavior is added.
- Update `database-guidelines.md` when persistence moves between local test
  stores, `GatewayDataState`, `aether_data`, and `RuntimeState`.
- Do not copy generic Rust advice into this directory. Every rule should be
  traceable to a current gateway pattern or an observed risk in this crate.
