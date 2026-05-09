# aether-contracts Backend Guidelines

> Development guide for the foundation contract crate at
> `crates/aether-contracts/`.

---

## Overview

`aether-contracts` defines stable Rust and wire-level contracts shared across
Aether's runtime layers. It contains no HTTP routes, database code, background
workers, orchestration services, or business logic.

The crate's current source surface is:

- `src/lib.rs`: public facade and re-exports.
- `src/error.rs`: serialized execution error data.
- `src/plan.rs`: upstream execution request plans.
- `src/result.rs`: execution results and telemetry.
- `src/frame.rs`: NDJSON stream frame contracts.
- `src/tunnel.rs`: binary tunnel protocol contracts and helpers.
- `src/usage.rs`: standardized token usage and stream terminal summaries.

The deleted `database-guidelines.md` template is intentionally absent. This
crate has no SeaORM, SQLx, Redis, migrations, connection pools, repositories,
or database transactions. Storage guidance belongs to data crates, not this
foundation contract crate.

---

## Guides

| Guide | Use It For |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Module layout, public facade rules, crate boundaries, and where new contracts belong. |
| [Error Handling](./error-handling.md) | `ExecutionError`, `ExecutionErrorKind`, `ExecutionPhase`, `ProtocolError`, serde validation, and caller-facing error boundaries. |
| [Protocol Contracts](./protocol-contracts.md) | Wire compatibility rules for execution plans, results, stream frames, tunnel frames, and standardized usage. |
| [Quality Guidelines](./quality-guidelines.md) | Required derives, serde compatibility, deterministic maps, type safety, tests, and review checklist. |
| [Logging Guidelines](./logging-guidelines.md) | Why this crate stays log-free and how callers should log contract data safely. |

---

## Pre-Development Checklist

Before editing `crates/aether-contracts/`, verify:

1. The change is a contract change, not runtime behavior.
2. The new API is needed by at least one consumer outside this crate.
3. The field, enum variant, or helper has a clear compatibility story.
4. Old payloads still deserialize, or a breaking change has an explicit
   migration plan.
5. Sensitive data is not made easier to log accidentally.
6. The public surface in `src/lib.rs` remains intentional.
7. `cargo test -p aether-contracts` is the minimum verification target.

---

## Important Source Examples

Use these source locations as anchors:

- `crates/aether-contracts/src/lib.rs:1`: private modules plus public
  re-exports.
- `crates/aether-contracts/src/error.rs:3`: snake_case error enums.
- `crates/aether-contracts/src/error.rs:30`: serialized `ExecutionError`.
- `crates/aether-contracts/src/plan.rs:28`: `RequestBody` variants for JSON,
  base64 bytes, and body references.
- `crates/aether-contracts/src/plan.rs:96`: `ExecutionPlan` contract.
- `crates/aether-contracts/src/result.rs:26`: `ExecutionResult` contract.
- `crates/aether-contracts/src/frame.rs:17`: tagged `StreamFramePayload`.
- `crates/aether-contracts/src/frame.rs:43`: `StreamFrame` serializes its type
  field as `"type"`.
- `crates/aether-contracts/src/tunnel.rs:17`: `MsgType` raw byte values.
- `crates/aether-contracts/src/tunnel.rs:129`: `Frame::decode` error checks.
- `crates/aether-contracts/src/tunnel.rs:201`: serde timeout validation.
- `crates/aether-contracts/src/usage.rs:5`: `StandardizedUsage` numeric and
  dimension fields.
- `crates/aether-contracts/src/usage.rs:123`: stream terminal summary payload.

---

## Contract Consumers

This crate has many downstream consumers. When changing a contract, search for
the concrete type before editing:

```bash
rg -n "ExecutionPlan|ExecutionResult|StreamFrame|StandardizedUsage|RequestMeta" crates apps -g '*.rs'
```

Known consumers include:

- `apps/aether-gateway/src/execution_runtime/ndjson.rs` for `StreamFrame`.
- `apps/aether-gateway/src/execution_runtime/stream_pump.rs` for streaming
  frames, EOF summaries, and tunnel response metadata.
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs` for
  `ExecutionPlan`, `ExecutionResult`, and telemetry.
- `crates/aether-ai-serving/src/attempt_plan.rs` for plan construction and
  decision conversion.
- `crates/aether-ai-formats/src/formats/shared/stream_core/format_matrix.rs`
  for `StandardizedUsage` and terminal summaries.
- `crates/aether-video-tasks-core/src/types.rs` for stored task execution
  plans.
- `crates/aether-testkit/src/bin/gateway_tunnel_stream_baseline.rs` for tunnel
  request and response metadata.

---

## Quality Gate

Minimum checks after changing only spec docs:

```bash
rg -n "<template placeholders or HTML comment markers>" .trellis/spec/aether-contracts/backend
find .trellis/spec/aether-contracts/backend -name '*.md' -maxdepth 1 -print0 | xargs -0 wc -l
```

Minimum checks after changing Rust source in this crate:

```bash
cargo fmt --check -p aether-contracts
cargo check -p aether-contracts
cargo test -p aether-contracts
```

For public contract changes, also run checks for directly affected consumers.
Examples are `cargo check -p aether-gateway`, `cargo test -p aether-ai-serving`,
or targeted tests in `aether-ai-formats` when stream usage semantics change.

---

## Non-Goals

Do not use this crate for:

- Database access, migrations, transactions, or connection handling.
- HTTP clients or axum extractors.
- Tokio tasks, background workers, retries, or scheduling.
- Logging or tracing.
- Provider-specific runtime behavior.
- Business rules for quotas, billing, admin APIs, or OAuth.

Those belong in higher-level crates. `aether-contracts` should remain a stable,
low-dependency contract layer.
