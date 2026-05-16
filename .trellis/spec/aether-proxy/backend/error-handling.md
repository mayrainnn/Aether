# Error Handling

> How `apps/aether-proxy` represents, propagates, logs, and converts failures.

---

## Overview

`aether-proxy` uses several error styles because it spans CLI startup, network
protocol handshakes, WebSocket tunnel frames, upstream HTTP execution, and
optional distributed admission.

Use each style where the crate already uses it:

- `anyhow::Result<T>` for binary lifecycle, config loading, setup commands,
  registration, and tunnel session orchestration.
- `thiserror::Error` for typed internal errors that callers need to pattern
  match, such as `ProxyAdmissionError`.
- Small custom enums with `Display` for testable policy failures, such as
  `FilterError`.
- `Result<T, String>` for stream-local upstream execution failures that become
  tunnel `StreamError` payloads.
- `io::Result<T>` for raw TCP, HTTP CONNECT, SOCKS5, and socket option work.

Do not collapse these into one global error enum. This binary has multiple
callers: the CLI, the tunnel dispatcher, stream handlers, metrics snapshots,
and service-management commands.

---

## Startup And Config Errors Use `anyhow`

The executable entry points return `anyhow::Result<()>` so startup can use `?`
across clap, TOML, rustls, filesystem, service, and network errors.

```rust
// apps/aether-proxy/src/main.rs:55
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls CryptoProvider"))?;
}
```

Config validation is fail-loud with human-readable `anyhow::bail!` messages:

```rust
// apps/aether-proxy/src/config.rs:618
pub fn validate(&self) -> anyhow::Result<()> {
    if self.heartbeat_interval == 0 {
        anyhow::bail!("heartbeat_interval must be > 0");
    }
    if self.allowed_ports.is_empty() {
        anyhow::bail!("allowed_ports must not be empty");
    }
}
```

Legacy config handling must reject removed keys before deserialization:

```rust
// apps/aether-proxy/src/config.rs:1190
fn reject_removed_config_keys(content: &str) -> anyhow::Result<()> {
    let value: toml::Value = toml::from_str(content)?;
    let Some(table) = value.as_table() else {
        return Ok(());
    };
}
```

DON'T silently accept old keys such as top-level `aether_url` or
`management_token` in config files. GitNexus process resources identify
`reject_removed_config_keys` as part of the startup execution flow, so bypassing
it changes real runtime semantics.

---

## Control-Plane HTTP Errors Are Redacted

Registration and unregistration use `anyhow` because they are lifecycle
operations, but control-plane response bodies are summarized instead of logged
or returned raw.

```rust
// apps/aether-proxy/src/registration/client.rs:134
let status = resp.status();
if !status.is_success() {
    let text = resp.text().await.unwrap_or_default();
    let summary = summarize_text_payload(&text);
    anyhow::bail!(
        "register failed (HTTP {}): response body redacted (bytes={}, sha256={})",
        status,
        summary.bytes,
        summary.sha256
    );
}
```

The same pattern is used for unregister failures and logs only the byte count
and sha256 digest:

```rust
// apps/aether-proxy/src/registration/client.rs:181
error!(
    status = %status,
    body_bytes = summary.bytes,
    body_sha256 = %summary.sha256,
    "unregister failed"
);
```

DON'T include raw response bodies in errors or logs. Control-plane error bodies
may contain deployment data, tokens, provider IDs, or upstream diagnostics.

---

## Target Filtering Uses A Small Policy Error Enum

Target validation needs precise tests and concise user-facing messages, so it
uses `FilterError` rather than `anyhow`.

```rust
// apps/aether-proxy/src/target_filter.rs:86
#[derive(Debug)]
pub enum FilterError {
    PrivateIp(IpAddr),
    PortNotAllowed(u16),
    DnsResolutionFailed(String),
    NoPublicAddrs(String),
}
```

`Display` defines the text that stream handlers embed into `target blocked:`
messages:

```rust
// apps/aether-proxy/src/target_filter.rs:94
impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrivateIp(ip) => write!(f, "target IP {} is in private/reserved range", ip),
            Self::PortNotAllowed(port) => write!(f, "port {} not in allowed list", port),
            Self::DnsResolutionFailed(host) => write!(f, "DNS resolution failed for {}", host),
            Self::NoPublicAddrs(host) => write!(f, "all resolved addresses for {} are private/reserved", host),
        }
    }
}
```

Keep new target policy failures in this enum so tests can assert exact variants:

```rust
// apps/aether-proxy/src/target_filter.rs:340
#[tokio::test]
async fn test_port_not_allowed() {
    let result = validate_target("8.8.8.8", 22, &ports(), false, &cache()).await;
    assert!(matches!(result, Err(FilterError::PortNotAllowed(22))));
}
```

---

## Admission Errors Are Typed With `thiserror`

Local and distributed concurrency admission are unified as
`ProxyAdmissionError`. This is the one typed error in shared state because
stream handlers need to distinguish saturation from unavailability.

```rust
// apps/aether-proxy/src/state.rs:305
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProxyAdmissionError {
    #[error("proxy stream admission saturated at {limit} for gate {gate}")]
    Saturated { gate: &'static str, limit: usize },
    #[error("proxy stream admission unavailable for gate {gate}: {message}")]
    Unavailable {
        gate: &'static str,
        limit: usize,
        message: String,
    },
}
```

`AppState::try_acquire_stream_permit` maps both local `ConcurrencyError` and
Redis-backed `RuntimeSemaphoreError` into this enum:

```rust
// apps/aether-proxy/src/state.rs:341
pub async fn try_acquire_stream_permit(
    &self,
) -> Result<Option<AdmissionPermit>, ProxyAdmissionError> {
    let local = match &self.stream_gate {
        Some(gate) => Some(gate.try_acquire().map_err(|err| {
            match err {
                ConcurrencyError::Saturated { gate, limit } => {
                    ProxyAdmissionError::Saturated { gate, limit }
                }
                ConcurrencyError::Closed { gate } => ProxyAdmissionError::Unavailable {
                    gate,
                    limit: self.stream_gate.as_ref().map(|inner| inner.snapshot().limit).unwrap_or(0),
                    message: "local stream gate is closed".to_string(),
                },
            }
        })?),
        None => None,
    };
}
```

Stream handlers convert the typed admission failure into a short tunnel error:

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:1067
let permit = match state.try_acquire_stream_permit().await {
    Ok(permit) => permit,
    Err(err) => {
        let message = match err {
            crate::state::ProxyAdmissionError::Saturated { .. } => "proxy overloaded",
            crate::state::ProxyAdmissionError::Unavailable { .. } => "proxy admission unavailable",
        };
        send_error(&frame_tx, stream_id, message).await;
        return;
    }
};
```

DON'T leak Redis command errors or internal gate names into client-facing tunnel
payloads. Log or metric the detail separately when needed.

---

## Tunnel Session Errors Trigger Reconnects

`tunnel/client.rs` returns `TunnelOutcome` for normal outcomes and
`anyhow::Error` for errors that should be counted and retried by the caller.

```rust
// apps/aether-proxy/src/tunnel/client.rs:18
pub enum TunnelOutcome {
    Shutdown,
    Disconnected,
}
```

The WebSocket connect path wraps timeouts with explicit messages:

```rust
// apps/aether-proxy/src/tunnel/client.rs:97
let (ws_stream, _response) = tokio::time::timeout(
    handshake_timeout,
    tokio_tungstenite::client_async_tls_with_config(request, tcp_stream, Some(ws_config), connector),
)
.await
.map_err(|_| {
    anyhow::anyhow!(
        "tunnel WebSocket handshake timeout ({}ms)",
        handshake_timeout.as_millis()
    )
})??;
```

The tunnel loop records errors and reconnects:

```rust
// apps/aether-proxy/src/tunnel/client.rs:176
match result {
    Ok(()) => Ok(TunnelOutcome::Disconnected),
    Err(e) => {
        server.tunnel_metrics.record_error("dispatcher_error", &e.to_string());
        Err(e)
    }
}
```

DON'T `panic!` on normal network failures. Panics are reserved for impossible
compile-target cases or test helper assertions.

---

## Dispatcher Errors Become Stream Frames When Possible

The dispatcher usually recovers from malformed stream input by sending
`StreamError` frames and continuing the tunnel read loop.

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:162
let meta: RequestMeta = match serde_json::from_slice(&payload) {
    Ok(m) => m,
    Err(e) => {
        warn!(stream_id = frame.stream_id, error = %e, "invalid request metadata");
        if frame_tx.try_send(Frame::new(
            frame.stream_id,
            MsgType::StreamError,
            0,
            Bytes::from(format!("invalid request metadata: {e}")),
        )).is_err() {
            warn!(stream_id = frame.stream_id, "writer channel full, StreamError dropped");
        }
        continue;
    }
};
```

Use `try_send` inside the dispatcher for immediate control/error replies so a
congested writer cannot block the shared WebSocket read loop.

`dispatch_stream_frame` bounds request-body backpressure:

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:314
async fn dispatch_stream_frame(tx: &mpsc::Sender<Frame>, frame: Frame) -> StreamDispatchStatus {
    match tokio::time::timeout(stream_frame_dispatch_timeout(), tx.send(frame)).await {
        Ok(Ok(())) => StreamDispatchStatus::Delivered,
        Ok(Err(_)) => StreamDispatchStatus::Closed,
        Err(_) => StreamDispatchStatus::TimedOut,
    }
}
```

DON'T await indefinitely while routing a single stream's body frames.

---

## Stream Execution Uses `String` Errors

Within `stream_handler.rs`, failures become tunnel payloads and structured logs,
so many helpers return `Result<T, String>`.

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:736
) -> Result<UpstreamResponseContext, String> {
    let host = current_url
        .host_str()
        .ok_or_else(|| "missing host in URL".to_string())?;
}
```

Target filtering is folded into a short string:

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:745
if let Err(error) = target_filter::validate_target(
    host,
    port,
    &allowed_ports,
    state.config.allow_private_targets,
    &state.dns_cache,
)
.await
{
    server.metrics.dns_failures.fetch_add(1, Ordering::Release);
    return Err(format!("target blocked: {error}"));
}
```

DON'T return large debug dumps in these strings. They may become client-visible
`StreamError` frames.

---

## Egress Proxy Protocol Uses `io::Error`

Raw proxy handshakes use `io::Result<T>` because callers need standard socket
error behavior and timeout mapping.

```rust
// apps/aether-proxy/src/egress_proxy.rs:204
pub(crate) async fn http_connect(
    stream: &mut TcpStream,
    target_authority: &str,
    proxy: &UpstreamProxyConfig,
) -> io::Result<()> {
    let mut request = format!(
        "CONNECT {target_authority} HTTP/1.1\r\nHost: {target_authority}\r\nProxy-Connection: Keep-Alive\r\n"
    );
}
```

SOCKS5 maps protocol-level failures to `io::Error::other` with concise text:

```rust
// apps/aether-proxy/src/egress_proxy.rs:293
if response[1] != 0x00 {
    return Err(io::Error::other(format!(
        "SOCKS5 connect failed: {}",
        socks5_reply_message(response[1])
    )));
}
```

DON'T convert these low-level errors into `anyhow` inside `egress_proxy.rs`.
Let the tunnel client decide whether to wrap them with context.

---

## Common Mistakes

Do not use `unwrap()` on remote data, config file contents, frame payloads, URLs,
or HTTP headers. Use `?`, `ok_or_else`, `map_err`, or a tunnel `StreamError`.

Do not log and then swallow lifecycle errors unless shutdown is explicitly
best-effort, as unregister is during cleanup.

Do not use raw `format!("{err:?}")` for client-visible errors. Use short,
sanitized messages.

Do not treat invalid request metadata as a tunnel-fatal error. Current behavior
warns, sends `StreamError`, and keeps the connection alive.

Do not add a broad crate-level error enum unless several modules truly need the
same typed variants. Prefer the local style already used by the owning module.
