# Protocol Contracts

> Wire-level compatibility rules for `aether-contracts`.

---

## Scope

This guide covers the contracts that are most likely to break downstream code:
execution plans, execution results, streaming frames, tunnel frames, and
standardized usage summaries. These types are shared across the Rust gateway,
proxy, testkit, AI serving, AI formats, video tasks, and Python-compatible
control-plane payloads.

The key rule is compatibility first. Add fields in a way that old payloads can
still deserialize, and do not change serialized names or raw tunnel byte values
without a coordinated migration.

---

## Execution Plans

`ExecutionPlan` is the main request contract passed to execution runtimes:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPlan {
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    pub provider_id: String,
    pub endpoint_id: String,
    pub key_id: String,
    pub method: String,
    #[serde(alias = "upstream_url")]
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: RequestBody,
    #[serde(default)]
    pub stream: bool,
    pub client_api_format: String,
    pub provider_api_format: String,
}
```

Source: `crates/aether-contracts/src/plan.rs:96`.

Guidelines:

- Keep required fields for identifiers and routing facts that every runtime
  needs: `request_id`, `provider_id`, `endpoint_id`, `key_id`, `method`, `url`,
  `body`, `client_api_format`, and `provider_api_format`.
- New fields should normally be optional with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- Use `BTreeMap<String, String>` for headers to keep snapshots deterministic.
- Preserve the `upstream_url` alias for legacy Python-compatible producers.

The compatibility test at `crates/aether-contracts/src/plan.rs:169` is the
model for adding fields that legacy control-plane payloads must ignore or map.

---

## Request Bodies

`RequestBody` supports three mutually exclusive body representations:

```rust
pub struct RequestBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_body: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_bytes_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_ref: Option<String>,
}
```

Source: `crates/aether-contracts/src/plan.rs:28`.

Use `RequestBody::from_json` for JSON payloads. It sets the other two fields to
`None` at `crates/aether-contracts/src/plan.rs:38`.

Do not put raw bytes in `json_body`. Use `body_bytes_b64` when the bytes need to
cross JSON boundaries, and use `body_ref` only when a higher layer has a stable
out-of-band body store.

---

## Transport and Proxy Snapshots

Execution headers for transport toggles are constants, not ad hoc strings:

```rust
pub const EXECUTION_REQUEST_FOLLOW_REDIRECTS_HEADER: &str =
    "x-aether-execution-follow-redirects";
pub const EXECUTION_REQUEST_HTTP1_ONLY_HEADER: &str =
    "x-aether-execution-http1-only";
pub const EXECUTION_REQUEST_ACCEPT_INVALID_CERTS_HEADER: &str =
    "x-aether-execution-accept-invalid-certs";
```

Source: `crates/aether-contracts/src/plan.rs:6`.

Transport backend and mode values are also constants:

- `TRANSPORT_BACKEND_REQWEST_RUSTLS` and `TRANSPORT_BACKEND_HYPER_RUSTLS` at
  `crates/aether-contracts/src/plan.rs:64`.
- `TRANSPORT_HTTP_MODE_AUTO` and `TRANSPORT_HTTP_MODE_HTTP1_ONLY` at
  `crates/aether-contracts/src/plan.rs:66`.
- `TRANSPORT_POOL_SCOPE_KEY` at `crates/aether-contracts/src/plan.rs:68`.

Use the constants in consumers. Do not duplicate literal strings in gateway,
proxy, or testkit code.

`ResolvedTransportProfile` has a serde default implementation at
`crates/aether-contracts/src/plan.rs:83`. If new fields are added, preserve the
ability to deserialize `{}` into a safe default transport profile.

---

## Execution Results

`ExecutionResult` is the sync result contract:

```rust
pub struct ExecutionResult {
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    pub status_code: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ResponseBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<ExecutionTelemetry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ExecutionError>,
}
```

Source: `crates/aether-contracts/src/result.rs:26`.

Rules:

- `status_code` is always present, even when `error` is populated.
- `body` may contain either JSON or base64 bytes via `ResponseBody` at
  `crates/aether-contracts/src/result.rs:18`.
- `ExecutionTelemetry` is optional and currently includes `ttfb_ms`,
  `elapsed_ms`, and `upstream_bytes` at `crates/aether-contracts/src/result.rs:8`.
- Do not put provider-specific telemetry fields directly on
  `ExecutionTelemetry` unless multiple runtimes need them.

---

## Stream Frames

`StreamFrame` is a JSON frame envelope used for NDJSON streaming:

```rust
pub struct StreamFrame {
    #[serde(rename = "type")]
    pub frame_type: StreamFrameType,
    pub payload: StreamFramePayload,
}
```

Source: `crates/aether-contracts/src/frame.rs:43`.

`StreamFramePayload` is internally tagged by `kind`:

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamFramePayload {
    Headers { status_code: u16, headers: BTreeMap<String, String> },
    Data { chunk_b64: Option<String>, text: Option<String> },
    Error { error: ExecutionError },
    Telemetry { telemetry: ExecutionTelemetry },
    Eof { summary: Option<ExecutionStreamTerminalSummary> },
}
```

Source: `crates/aether-contracts/src/frame.rs:17`.

Guidelines:

- Use `StreamFrame::eof()` for EOF without a summary and
  `StreamFrame::eof_with_summary(summary)` when terminal usage is known.
- Do not invent new ad hoc EOF payloads in callers.
- Do not change the serialized `"type"` field name. It is intentionally not
  `frame_type`.
- Preserve `chunk_b64` and `text` as optional fields so binary and text chunks
  can use the same `Data` variant.

---

## Tunnel Frames

The binary tunnel frame header is fixed at 10 bytes:

```rust
pub const HEADER_SIZE: usize = 10;

pub struct FrameHeader {
    pub stream_id: u32,
    pub msg_type: u8,
    pub flags: u8,
    pub payload_len: u32,
}
```

Source: `crates/aether-contracts/src/tunnel.rs:8` and
`crates/aether-contracts/src/tunnel.rs:66`.

The byte layout is:

1. `stream_id` as big-endian `u32`.
2. `msg_type` as one byte.
3. `flags` as one byte.
4. `payload_len` as big-endian `u32`.
5. Payload bytes.

`Frame::encode` writes this exact layout at
`crates/aether-contracts/src/tunnel.rs:119`; `Frame::decode` reads it at
`crates/aether-contracts/src/tunnel.rs:129`.

Do not change `HEADER_SIZE`, field order, or endian behavior. Any change would
break gateway-proxy interoperability.

---

## Tunnel Message Types and Flags

`MsgType` is the authoritative source for raw message values:

```rust
#[repr(u8)]
pub enum MsgType {
    RequestHeaders = 0x01,
    RequestBody = 0x02,
    ResponseHeaders = 0x03,
    ResponseBody = 0x04,
    StreamEnd = 0x05,
    StreamError = 0x06,
    Ping = 0x10,
    Pong = 0x11,
    GoAway = 0x12,
    HeartbeatData = 0x13,
    HeartbeatAck = 0x14,
}
```

Source: `crates/aether-contracts/src/tunnel.rs:17`.

The raw constants below the enum, such as `REQUEST_HEADERS`, `PONG`, and
`HEARTBEAT_ACK`, mirror those values for consumers that work with raw bytes.
Keep enum variants and constants in sync.

Flags are defined in the nested `flags` module at
`crates/aether-contracts/src/tunnel.rs:12` and re-exported as raw constants at
`crates/aether-contracts/src/tunnel.rs:63`:

- `END_STREAM = 0x01`.
- `GZIP_COMPRESSED = 0x02`.

Do not assign a flag bit that is already in use.

---

## Compression

`compress_payload` compresses only when the payload is at least 512 bytes and
the compressed form is smaller:

```rust
pub fn compress_payload(data: Bytes) -> (Bytes, u8) {
    if data.len() >= COMPRESS_MIN_SIZE {
        if let Ok(compressed) = compress_gzip(&data) {
            if compressed.len() < data.len() {
                return (compressed, flags::GZIP_COMPRESSED);
            }
        }
    }
    (data, 0)
}
```

Source: `crates/aether-contracts/src/tunnel.rs:296`.

`COMPRESS_MIN_SIZE` is private at `crates/aether-contracts/src/tunnel.rs:307`.
Do not expose it unless multiple callers need to reason about the threshold.

If compression fails, `compress_payload` intentionally falls back to the
original payload and no gzip flag. If decompression fails, `decode_payload` or
`decompress_if_gzip` returns an error. Preserve this asymmetry: compression is
an optimization, decompression is required when the flag is set.

---

## Usage Summaries

`StandardizedUsage` is the provider-neutral usage payload:

```rust
pub struct StandardizedUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_creation_ephemeral_5m_tokens: i64,
    pub cache_creation_ephemeral_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_storage_token_hours: f64,
    pub request_count: i64,
    pub dimensions: BTreeMap<String, serde_json::Value>,
}
```

Source: `crates/aether-contracts/src/usage.rs:5`.

Rules:

- Use named fields for common metrics that billing or analytics need.
- Put provider-specific extras into `dimensions`.
- Keep `request_count` defaulting to 1 through `StandardizedUsage::new` at
  `crates/aether-contracts/src/usage.rs:19`.
- Use `normalize_cache_creation_breakdown` when providers send only the 5m/1h
  cache creation breakdown at `crates/aether-contracts/src/usage.rs:75`.
- Use `choose_more_complete` when two parsers produce different signal levels
  at `crates/aether-contracts/src/usage.rs:111`.

`ExecutionStreamTerminalSummary` wraps usage and terminal stream facts at
`crates/aether-contracts/src/usage.rs:123`. Keep zero and absent values sparse;
`unknown_event_count` skips zero through `is_zero_u64`.

---

## Compatibility Anti-Patterns

Do not rename serialized fields:

```rust
// DON'T: breaks JSON consumers that read "provider_api_format".
pub provider_format: String,
```

Add a new optional field or serde alias instead.

Do not remove old tunnel byte constants even if a typed enum exists:

```rust
// DON'T: callers use REQUEST_HEADERS when reading raw frame bytes.
pub const REQUEST_HEADERS: u8 = MsgType::RequestHeaders as u8;
```

Do not change `StreamFramePayload` tagging:

```rust
// DON'T: changes wire shape from {"kind":"data",...}.
#[serde(untagged)]
pub enum StreamFramePayload { ... }
```

Do not store secrets in contract extension fields. `ProxySnapshot.extra`,
`ResolvedTransportProfile.extra`, and `StandardizedUsage.dimensions` are
serialized and can be persisted or logged by callers.

---

## Review Checklist

For every protocol contract change:

- Add or update an owning-module unit test.
- Check whether old JSON payloads deserialize.
- Check whether new optional fields skip serialization when absent.
- Search consumers before renaming fields or variants.
- Keep raw tunnel byte values stable.
- Keep text and binary bodies separate.
- Keep usage metrics numeric and dimensions deterministic.
