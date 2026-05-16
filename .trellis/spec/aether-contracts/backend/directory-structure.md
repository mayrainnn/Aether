# Directory Structure

> Backend organization rules for the `aether-contracts` crate.

---

## Scope

`aether-contracts` is a foundation crate under `crates/aether-contracts/`.
It owns wire contracts shared by the Rust gateway, proxy, testkit, AI serving,
AI formats, video task core, and legacy Python-compatible execution payloads.

This crate is intentionally small. It does not contain route handlers,
storage code, service orchestration, or database repositories. New code belongs
here only when it defines a stable cross-crate or cross-process data contract.

Evidence from ABCoder's `aether-contracts` AST shows one Rust module with six
source packages: `error`, `frame`, `plan`, `result`, `tunnel`, and `usage`.
GitNexus indexed the repo at commit `209322b`, and its graph lists the same
symbols under `crates/aether-contracts/src/`.

---

## Actual Layout

```text
crates/aether-contracts/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── frame.rs
    ├── plan.rs
    ├── result.rs
    ├── tunnel.rs
    └── usage.rs
```

`Cargo.toml` declares this as `aether-contracts` and describes the crate as
"Shared contracts for Python and Rust Aether components" at
`crates/aether-contracts/Cargo.toml:1`. The dependency list is deliberately
limited to `bytes`, `flate2`, `serde`, `serde_json`, and `thiserror` at
`crates/aether-contracts/Cargo.toml:9`.

---

## Module Responsibilities

`src/lib.rs` is the public facade. It keeps `error`, `frame`, `plan`, `result`,
and `usage` private while re-exporting their stable types at
`crates/aether-contracts/src/lib.rs:1`. The exception is `pub mod tunnel`,
because callers need namespace access to protocol helpers such as
`aether_contracts::tunnel::RequestMeta` and tunnel header constants.

`src/error.rs` defines execution failure classification:
`ExecutionErrorKind`, `ExecutionPhase`, and `ExecutionError` at
`crates/aether-contracts/src/error.rs:3`. These are data contracts, not
runtime errors thrown by this crate.

`src/plan.rs` defines the upstream execution request contract:
`RequestBody`, `ProxySnapshot`, `ResolvedTransportProfile`,
`ExecutionTimeouts`, `ExecutionPlan`, and execution header constants. The
central struct begins at `crates/aether-contracts/src/plan.rs:96`.

`src/result.rs` defines the execution response contract returned by runtimes:
`ExecutionTelemetry`, `ResponseBody`, and `ExecutionResult` at
`crates/aether-contracts/src/result.rs:8`.

`src/frame.rs` defines NDJSON stream frame contracts used by the gateway stream
pump: `StreamFrameType`, `StreamFramePayload`, and `StreamFrame`. The
externally visible `"type"` JSON field is pinned by
`#[serde(rename = "type")]` at `crates/aether-contracts/src/frame.rs:43`.

`src/tunnel.rs` owns the binary tunnel frame protocol and tunnel control
metadata. It contains message type constants, `FrameHeader`, `Frame`,
`ProtocolError`, `RequestMeta`, `ResponseMeta`, compression helpers, and
control-frame encoders. `HEADER_SIZE` is fixed at 10 bytes at
`crates/aether-contracts/src/tunnel.rs:8`.

`src/usage.rs` standardizes token usage and terminal stream summaries:
`StandardizedUsage` starts at `crates/aether-contracts/src/usage.rs:5`, and
`ExecutionStreamTerminalSummary` starts at
`crates/aether-contracts/src/usage.rs:123`.

---

## Public Surface Pattern

Keep most modules private and re-export stable contracts from `lib.rs`.

```rust
mod error;
mod frame;
mod plan;
mod result;
pub mod tunnel;
mod usage;

pub use error::{ExecutionError, ExecutionErrorKind, ExecutionPhase};
pub use frame::{StreamFrame, StreamFramePayload, StreamFrameType};
pub use plan::{ExecutionPlan, ExecutionTimeouts, ProxySnapshot, RequestBody};
```

Source: `crates/aether-contracts/src/lib.rs:1`.

Use this pattern when adding a new non-tunnel contract family:

1. Put the implementation in a private `src/<family>.rs` module.
2. Re-export only the intended stable types from `src/lib.rs`.
3. Keep helper functions private unless another crate already needs them.
4. Add round-trip serde tests in the module that owns the contract.

Do not expose new top-level modules by default. `tunnel` is public because the
protocol helpers are naturally namespaced and consumers use constants like
`aether_contracts::tunnel::TUNNEL_RELAY_FORWARDED_BY_HEADER`.

---

## Dependency Direction

The crate must stay in the foundation layer. It should not depend on Aether
application or domain crates.

Allowed local dependencies: none.
Allowed external dependency categories: serialization, byte buffers,
compression, and lightweight error display for protocol decode errors.

Do not add dependencies such as `tokio`, `axum`, `reqwest`, `sea-orm`, `sqlx`,
`redis`, or `tracing` to this crate. Runtime behavior belongs in higher layers
such as `apps/aether-gateway`, `apps/aether-proxy`, or
`crates/aether-ai-serving`.

Consumer evidence:

- `apps/aether-gateway/src/execution_runtime/ndjson.rs` imports
  `StreamFrame` for line-delimited stream frames.
- `crates/aether-ai-serving/src/attempt_plan.rs` imports `ExecutionPlan` and
  `RequestBody` to convert between planning DTOs and execution plans.
- `crates/aether-ai-formats/src/formats/shared/stream_core/format_matrix.rs`
  imports `ExecutionStreamTerminalSummary` and `StandardizedUsage` for
  provider stream normalization.
- `crates/aether-testkit/src/bin/gateway_tunnel_stream_baseline.rs` imports
  tunnel `RequestMeta` and `ResponseMeta` through the public `tunnel` module.

---

## File Naming Rules

Use noun-based module names that match the contract family:

- `plan.rs` for request plans.
- `result.rs` for execution results.
- `frame.rs` for NDJSON stream frames.
- `tunnel.rs` for binary tunnel protocol.
- `usage.rs` for token usage summaries.
- `error.rs` for serialized execution failure descriptors.

Avoid vague names such as `types.rs`, `models.rs`, `common.rs`, or `utils.rs`.
This crate already has a tight one-file-per-contract-family layout; keep that
shape when extending it.

---

## Adding New Contracts

Before adding a new field to an existing contract, check whether the field is
required by consumers and whether old payloads must still deserialize. Existing
optional fields consistently use serde defaults and skip-empty serialization,
for example `ExecutionPlan.proxy` at `crates/aether-contracts/src/plan.rs:122`
and `ExecutionResult.telemetry` at `crates/aether-contracts/src/result.rs:36`.

For compatibility with legacy payload names, prefer targeted serde aliases over
parallel fields. Examples:

- `ExecutionPlan.url` accepts `upstream_url` at
  `crates/aether-contracts/src/plan.rs:107`.
- `ProxySnapshot.url` accepts `proxy_url` at
  `crates/aether-contracts/src/plan.rs:58`.

Do not create a second struct just to preserve an old JSON key if a serde alias
can maintain the contract without duplicating data.

---

## Directory Anti-Patterns

Do not put implementation concerns in this crate:

```rust
// DON'T: a contract crate should not execute HTTP requests.
pub async fn execute(plan: ExecutionPlan) -> anyhow::Result<ExecutionResult> {
    reqwest::Client::new().post(plan.url).send().await?;
    todo!()
}
```

The correct placement is a runtime crate or gateway module. `aether-contracts`
should define the shape of `ExecutionPlan`, not decide how to execute it.

Do not create `src/db.rs`, migrations, or repository traits here. Database
contracts live in `aether-data-contracts` or concrete storage code in
`aether-data`.
