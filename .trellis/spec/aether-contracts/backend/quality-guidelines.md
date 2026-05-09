# Quality Guidelines

> Code quality standards for `aether-contracts`.

---

## Scope

Quality in this crate means stable contracts, predictable serialization, narrow
dependencies, and tests that protect compatibility. The crate is used across
runtime, gateway, proxy, AI serving, AI formats, video task, OAuth, model
fetch, and testkit code. A small field rename here can break multiple layers.

GitNexus symbol queries show `ExecutionPlan`, `StreamFrame`, `StandardizedUsage`,
and tunnel protocol types as the important surfaces under
`crates/aether-contracts/src/`. ABCoder confirms this crate has only contract
types and helper functions, with no repository, service, or route modules.

---

## Required Derives

Data contracts that cross process or crate boundaries should derive the same
basic traits used by existing contracts:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPlan {
    pub request_id: String,
    ...
}
```

Source: `crates/aether-contracts/src/plan.rs:96`.

Use this pattern for payload structs:

- `Debug` for test failures and caller diagnostics.
- `Clone` because higher layers often need to retain a plan or result while
  continuing fallback logic.
- `Serialize` and `Deserialize` for JSON boundary payloads.
- `PartialEq` for focused unit tests.
- `Eq` only when all fields support it, as with `ExecutionTimeouts` at
  `crates/aether-contracts/src/plan.rs:11` and `ExecutionError` at
  `crates/aether-contracts/src/error.rs:30`.

Do not derive `Copy` on structs that contain `String`, `Bytes`, `Value`, maps,
or other owned payloads. `MsgType` and `FrameHeader` are small copyable protocol
values, so they derive `Copy` at `crates/aether-contracts/src/tunnel.rs:17` and
`crates/aether-contracts/src/tunnel.rs:66`.

---

## Serde Compatibility

Use serde defaults and skip-empty serialization for optional fields:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub proxy: Option<ProxySnapshot>,
```

Source: `crates/aether-contracts/src/plan.rs:122`.

This pattern appears across `ExecutionPlan`, `ExecutionResult`, `ResponseBody`,
`StreamFramePayload`, `ProxySnapshot`, and `ExecutionStreamTerminalSummary`.
It allows older producers and consumers to omit fields without breaking decode.

Use `#[serde(default)]` on maps and booleans when empty or false is the desired
wire default. Examples:

- `ExecutionPlan.headers` defaults to an empty `BTreeMap` at
  `crates/aether-contracts/src/plan.rs:109`.
- `ExecutionPlan.stream` defaults to false at
  `crates/aether-contracts/src/plan.rs:116`.
- `ExecutionStreamTerminalSummary.observed_finish` defaults to false at
  `crates/aether-contracts/src/usage.rs:133`.

Use aliases for known legacy names rather than duplicating fields:

```rust
#[serde(alias = "upstream_url")]
pub url: String,
```

Source: `crates/aether-contracts/src/plan.rs:107`.

Do not remove an alias without checking legacy producers. The plan tests include
`deserializes_python_control_plane_plan_shape` at
`crates/aether-contracts/src/plan.rs:169` specifically to preserve Python
control-plane compatibility.

---

## Deterministic Maps

Prefer `BTreeMap` in JSON contracts where stable ordering helps tests and
snapshot comparisons. Current examples:

- `ExecutionPlan.headers` at `crates/aether-contracts/src/plan.rs:109`.
- `ExecutionResult.headers` at `crates/aether-contracts/src/result.rs:32`.
- `StandardizedUsage.dimensions` at `crates/aether-contracts/src/usage.rs:16`.

`RequestMeta.headers` in `tunnel.rs` uses `HashMap` at
`crates/aether-contracts/src/tunnel.rs:182` because it is tunnel metadata, not
a canonical JSON snapshot. Preserve this distinction unless a test or consumer
requires deterministic tunnel metadata ordering.

---

## Type Safety

Use enums for closed protocol value sets. `MsgType` is a `#[repr(u8)]` enum and
has a single conversion function from raw bytes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgType {
    RequestHeaders = 0x01,
    RequestBody = 0x02,
    ...
}
```

Source: `crates/aether-contracts/src/tunnel.rs:17`.

`MsgType::from_u8` returns `Option<Self>` at
`crates/aether-contracts/src/tunnel.rs:33`, and `Frame::decode` turns unknown
values into `ProtocolError::UnknownMsgType` at
`crates/aether-contracts/src/tunnel.rs:149`.

Do not scatter raw message byte matches throughout caller code. Add or update
the enum and constants in `tunnel.rs`, then make callers depend on those shared
symbols.

---

## Helper Methods

Helper methods should encode contract semantics, not runtime policy.

Good examples:

- `RequestBody::from_json` constructs a mutually exclusive JSON request body at
  `crates/aether-contracts/src/plan.rs:38`.
- `StreamFrame::eof_with_summary` creates a canonical EOF frame at
  `crates/aether-contracts/src/frame.rs:55`.
- `StandardizedUsage::normalize_cache_creation_breakdown` derives aggregate
  cache creation tokens from the two ephemeral buckets at
  `crates/aether-contracts/src/usage.rs:75`.
- `StandardizedUsage::choose_more_complete` chooses the payload with more
  observed token signals at `crates/aether-contracts/src/usage.rs:111`.

Do not add methods that perform IO, call services, read environment variables,
or inspect database state. Those are runtime concerns.

---

## Testing Requirements

Every contract change must have a local unit test in the owning module.
Existing examples show the expected style:

```rust
#[test]
fn deserializes_python_control_plane_plan_shape() {
    let raw = serde_json::json!({
        "request_id": "req-1",
        "provider_name": "openai",
        "url": "https://example.com/v1/chat/completions",
        "body": {"json_body": {"model": "gpt-4.1"}}
    });

    let plan: ExecutionPlan =
        serde_json::from_value(raw).expect("python payload should deserialize");
    assert_eq!(plan.provider_name.as_deref(), Some("openai"));
}
```

Source: `crates/aether-contracts/src/plan.rs:169`.

Required test coverage by change type:

- New optional field: serialization skip behavior and old-payload deserialization.
- New alias: deserialize payload using the old name and assert the canonical
  field is populated.
- New enum variant: serde snake_case output and binary/raw conversion if it is
  a tunnel message type.
- New tunnel frame behavior: encode/decode round trip and incomplete input.
- New usage field: `get`, `set`, and signal scoring behavior.

Run at minimum:

```bash
cargo test -p aether-contracts
cargo check -p aether-contracts
```

For cross-crate contract changes, also run the affected consumer package tests
or checks, commonly `aether-gateway`, `aether-ai-serving`, `aether-ai-formats`,
or `aether-testkit`.

---

## Forbidden Patterns

Do not add broad dependencies:

```toml
# DON'T in crates/aether-contracts/Cargo.toml
tokio.workspace = true
reqwest.workspace = true
sea-orm.workspace = true
tracing.workspace = true
```

Current dependencies are intentionally narrow at
`crates/aether-contracts/Cargo.toml:9`.

Do not use `serde_json::Value` for fields that have known stable meaning.
`Value` is appropriate for payload bodies and provider-specific extension
fields such as `RequestBody.json_body` at
`crates/aether-contracts/src/plan.rs:30`, `ProxySnapshot.extra` at
`crates/aether-contracts/src/plan.rs:60`, and
`StandardizedUsage.dimensions` at `crates/aether-contracts/src/usage.rs:16`.
It is not appropriate for core identifiers such as `request_id`, `provider_id`,
or `status_code`.

Do not replace typed enums with strings for closed sets:

```rust
// DON'T: loses compile-time coverage and decode validation.
pub struct Frame {
    pub msg_type: String,
}
```

Use `MsgType` and the existing raw-byte constants instead.

Do not silently swallow decode errors in protocol functions:

```rust
// DON'T: hides corrupt or partial tunnel frames.
let msg_type = MsgType::from_u8(raw).unwrap_or(MsgType::StreamError);
```

Use `ProtocolError::UnknownMsgType` as `Frame::decode` does at
`crates/aether-contracts/src/tunnel.rs:149`.

---

## Review Checklist

Before approving changes in this crate, verify:

- The new API belongs in a foundation contract crate, not a runtime crate.
- `lib.rs` exposes only intentional public items.
- Optional fields have serde defaults and skip-empty behavior.
- Legacy JSON names are preserved with aliases or explicit migration tests.
- New binary protocol values update `MsgType`, raw constants, and decode tests.
- Error data does not contain secrets or raw request/response bodies.
- Unit tests cover both serialization and deserialization where relevant.
- `cargo test -p aether-contracts` passes.
