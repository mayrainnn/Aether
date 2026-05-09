# Error Handling

> Error contracts and propagation rules for `aether-contracts`.

---

## Scope

This crate defines serialized error data and protocol decode errors. It does
not own gateway error responses, HTTP status mapping, retry loops, logging, or
user-facing messages. Those decisions live in higher-level crates such as
`apps/aether-gateway`.

There are two distinct error surfaces:

1. `ExecutionError` in `src/error.rs`, which is a transportable data contract
   embedded in `ExecutionResult` and `StreamFramePayload::Error`.
2. `ProtocolError` in `src/tunnel.rs`, which is a local Rust error returned by
   binary frame decoding.

Keep these surfaces separate. Do not make `ExecutionError` implement
`std::error::Error` unless a real caller needs it; today it is serialized data.

---

## Serialized Execution Errors

`ExecutionErrorKind` classifies failures with stable snake_case JSON values.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionErrorKind {
    ConnectTimeout,
    FirstByteTimeout,
    ReadTimeout,
    Upstream4xx,
    Upstream5xx,
    TlsError,
    ProxyError,
    ProtocolError,
    Cancelled,
    Internal,
}
```

Source: `crates/aether-contracts/src/error.rs:3`.

`ExecutionPhase` gives the stage where the failure happened and is also
serialized as snake_case. It starts at `crates/aether-contracts/src/error.rs:18`
and currently covers `Connect`, `Handshake`, `Write`, `FirstByte`,
`StreamRead`, `Decode`, and `Finalize`.

`ExecutionError` itself is a plain struct:

```rust
pub struct ExecutionError {
    pub kind: ExecutionErrorKind,
    pub phase: ExecutionPhase,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub failover_recommended: bool,
}
```

Source: `crates/aether-contracts/src/error.rs:30`.

Guidelines:

- Add new `ExecutionErrorKind` variants only when callers need a stable branch.
- Keep variant names semantic, not implementation-specific.
- Use `upstream_status` only for upstream HTTP status, not internal gateway
  statuses.
- Preserve `retryable` and `failover_recommended` defaults so old payloads
  decode as false.
- Keep messages human-readable but safe to log. Do not put tokens, API keys,
  raw request bodies, or proxy credentials in `message`.

---

## Error Embedding

`ExecutionResult` embeds the serialized execution error as an optional field:

```rust
pub struct ExecutionResult {
    pub request_id: String,
    pub status_code: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ExecutionError>,
}
```

Source: `crates/aether-contracts/src/result.rs:26`.

`StreamFramePayload::Error` carries the same `ExecutionError` in streaming
NDJSON frames:

```rust
pub enum StreamFramePayload {
    Error {
        error: ExecutionError,
    },
}
```

Source: `crates/aether-contracts/src/frame.rs:17`.

If a new execution failure needs to appear in both sync and streaming paths,
extend `ExecutionError` or `ExecutionErrorKind` once and reuse it in both
`ExecutionResult.error` and `StreamFramePayload::Error`. Do not add separate
sync-only and stream-only error payload shapes.

---

## Protocol Decode Errors

`ProtocolError` is different. It is a Rust error used by the binary tunnel
decoder:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("frame too short: expected {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },
    #[error("frame incomplete: expected {expected} bytes, got {actual}")]
    Incomplete { expected: usize, actual: usize },
    #[error("unknown message type: 0x{0:02x}")]
    UnknownMsgType(u8),
}
```

Source: `crates/aether-contracts/src/tunnel.rs:162`.

`Frame::decode` returns `Result<Self, ProtocolError>` and performs three
ordered checks:

1. Verify at least `HEADER_SIZE` bytes are present.
2. Verify the advertised payload length is available.
3. Convert the raw byte into `MsgType` with `MsgType::from_u8`.

Source: `crates/aether-contracts/src/tunnel.rs:129`.

Use `ProtocolError` for binary frame shape errors only. Do not reuse it for
upstream HTTP failures, JSON decoding of execution plans, or business logic.

---

## String Errors in Legacy Helpers

`decode_payload` currently returns `Result<Vec<u8>, String>` because it is a
simple helper over an already parsed `FrameHeader` and must report gzip decode
messages without exporting another public error enum:

```rust
pub fn decode_payload(data: &[u8], header: &FrameHeader) -> Result<Vec<u8>, String> {
    let payload = frame_payload_by_header(data, header)
        .ok_or_else(|| "incomplete frame payload".to_string())?;
    if header.flags & FLAG_GZIP_COMPRESSED != 0 {
        ...
            .map_err(|err| format!("failed to decompress payload: {err}"))?;
        Ok(decoded)
    } else {
        Ok(payload.to_vec())
    }
}
```

Source: `crates/aether-contracts/src/tunnel.rs:273`.

Do not introduce new `String` error APIs for complex parsing. Prefer
`thiserror` when callers need structured variants, as `ProtocolError` does.

---

## Serde Validation Errors

Serde custom validation is used for tunnel request timeouts. The timeout field
accepts integer seconds and integer-like floats, but rejects negative,
non-finite, fractional, and too-large values:

```rust
fn deserialize_timeout<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum TimeoutValue {
        Int(u64),
        Float(f64),
    }
    ...
}
```

Source: `crates/aether-contracts/src/tunnel.rs:201`.

The tests at `crates/aether-contracts/src/tunnel.rs:330` and
`crates/aether-contracts/src/tunnel.rs:337` pin compatibility for integer and
integer-like float timeout payloads. Add tests before tightening this behavior.

---

## How Errors Surface to Callers

This crate should not log or convert errors into HTTP responses. It gives
callers typed data. Examples from consumers:

- `apps/aether-gateway/src/async_task/runtime.rs` imports
  `ExecutionErrorKind` and classifies failed refresh results.
- `apps/aether-gateway/src/execution_runtime/ndjson.rs` decodes
  `StreamFrame` and maps serde failures into `GatewayError`.
- `apps/aether-gateway/src/execution_runtime/stream/error.rs` logs stream
  error frames at the gateway layer, not inside `aether-contracts`.

Keep that layering. If a caller needs a status code, add mapping code in that
caller, not in this foundation crate.

---

## Anti-Patterns

Do not add `anyhow::Error` fields to serialized contracts:

```rust
// DON'T: cannot be serialized into a stable cross-process contract.
pub struct ExecutionError {
    pub source: anyhow::Error,
}
```

Do not make protocol decode errors carry full payload bytes:

```rust
// DON'T: leaks data and makes errors heavy.
Incomplete { payload: Vec<u8>, expected: usize }
```

Do not log from error constructors in this crate. A contract type may be
created in tests, sync execution, stream execution, proxy tunnels, and legacy
compatibility paths. Logging belongs where the caller knows the trace id,
request id, provider, and redaction policy.
