# Logging Guidelines

> Structured logging rules for `apps/aether-proxy`.

---

## Overview

`aether-proxy` uses the `tracing` crate and initializes service logging through
`aether_runtime::init_reloadable_service_tracing`. Logs are operational: they
must help diagnose edge connectivity, registration, tunnel health, upstream
latency, backpressure, and shutdown without exposing tokens or request secrets.

The runtime logging config is constructed from `Config`:

```rust
// apps/aether-proxy/src/config.rs:788
pub fn service_runtime_config(&self) -> anyhow::Result<ServiceRuntimeConfig> {
    let mut config = ServiceRuntimeConfig::new("aether-proxy", "aether_proxy=info")
        .with_log_format(aether_runtime::LogFormat::Pretty)
        .with_log_destination(self.log_destination.into())
        .with_node_role("proxy")
        .with_instance_id(self.node_name.trim().to_string());
}
```

Remote config can hot-reload the log level:

```rust
// apps/aether-proxy/src/runtime.rs:97
if let Some(ref level) = remote.log_level {
    if *level != new_cfg.log_level {
        changed.push(format!("log_level -> {}", level));
        new_cfg.log_level = level.clone();
        if let Some(reloader) = LOG_RELOADER.get() {
            reloader(level);
        }
    }
}
```

---

## Log Levels

### `info!`

Use `info!` for lifecycle milestones, successful registration, tunnel pool
policy resolution, scale events, graceful drain, remote config application, and
successful proxy requests.

```rust
// apps/aether-proxy/src/app.rs:77
info!(
    version = env!("CARGO_PKG_VERSION"),
    node_name = %config.node_name,
    server_count = servers.len(),
    "aether-proxy starting (tunnel mode)"
);
```

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:206
info!(
    server = %ctx.server.server_label,
    stream_id = ctx.stream_id,
    method = %ctx.method,
    scheme = url.scheme(),
    host = request_log_host(url),
    port = request_log_port(url),
    path = request_log_path(url),
    query_present = url.query().is_some(),
    status,
    duration_ms = duration.as_millis() as u64,
    redirect_count = ctx.redirect_count,
    request_body_bytes = ctx.request_body_size,
    "proxy request completed"
);
```

### `warn!`

Use `warn!` for recoverable failures: registration retry, tunnel staleness,
invalid stream metadata, target blocks, writer congestion, socket option
failures, and upstream body read failures.

```rust
// apps/aether-proxy/src/app.rs:191
warn!(
    server = %label,
    url = %entry.aether_url,
    error = %e,
    "registration failed, will retry in background"
);
```

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:82
warn!(
    stale_ms = stale_timeout.as_millis(),
    "tunnel connection stale, no data received"
);
```

### `error!`

Use `error!` for failures that are not expected in healthy operation and require
operator attention, such as WebSocket read errors, tunnel connection errors,
writer panics, or unregister failures during shutdown.

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:97
error!(error = %e, "WebSocket read error");
```

```rust
// apps/aether-proxy/src/app.rs:302
if let Err(e) = server.aether_client.unregister(&node_id).await {
    error!(
        server = %server.server_label,
        error = %e,
        "unregister failed during shutdown"
    );
}
```

### `debug!`

Use `debug!` for retry internals, connection parameters, reconnect loops, frame
completion, ignored frame types, and non-fatal detail useful during diagnosis.

```rust
// apps/aether-proxy/src/registration/client.rs:219
debug!(
    attempt,
    status = %resp.status(),
    sleep_ms = sleep_for.as_millis(),
    label,
    "Aether request retrying"
);
```

### `trace!`

Use `trace!` sparingly for very high-volume internal events. Current usage is a
single WebSocket ping write:

```rust
// apps/aether-proxy/src/tunnel/writer.rs:141
trace!("sent WebSocket ping");
```

Do not add trace logs for every body chunk unless the log level defaults and
volume impact are explicitly considered.

---

## Structured Field Rules

Prefer structured fields over formatted strings. Use `%` for displayable values
and `?` only for debug-safe enums or compact internal state.

Good:

```rust
// apps/aether-proxy/src/tunnel/client.rs:121
debug!(
    conn = conn_idx,
    tcp_keepalive_secs = state.config.tunnel_tcp_keepalive_secs,
    tcp_nodelay = state.config.tunnel_tcp_nodelay,
    connect_timeout_ms = connect_timeout.as_millis(),
    stale_timeout_ms = stale_timeout.as_millis(),
    ping_interval_ms = ping_interval.as_millis(),
    "tunnel connected"
);
```

Good:

```rust
// apps/aether-proxy/src/runtime.rs:112
info!(
    version,
    changes = %changed.join(", "),
    "remote config applied"
);
```

For request logs, split URLs into safe fields. Use `scheme`, `host`, `port`,
`path`, and `query_present`; do not log a full URL with query parameters.

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:223
warn!(
    server = %ctx.server.server_label,
    stream_id = ctx.stream_id,
    method = %ctx.method,
    scheme = url.scheme(),
    host = request_log_host(url),
    port = request_log_port(url),
    path = request_log_path(url),
    query_present = url.query().is_some(),
    error = %error,
    duration_ms = duration.as_millis() as u64,
    "proxy request failed"
);
```

---

## Redaction And Sensitive Data

Proxy URLs with credentials must be redacted before logging:

```rust
// apps/aether-proxy/src/egress_proxy.rs:110
pub(crate) fn redacted_url(&self) -> String {
    let Ok(mut parsed) = Url::parse(&self.raw) else {
        return "<invalid>".to_string();
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("****");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("****"));
    }
    parsed.to_string()
}
```

Control-plane response bodies must be summarized:

```rust
// apps/aether-proxy/src/registration/client.rs:136
let text = resp.text().await.unwrap_or_default();
let summary = summarize_text_payload(&text);
anyhow::bail!(
    "register failed (HTTP {}): response body redacted (bytes={}, sha256={})",
    status,
    summary.bytes,
    summary.sha256
);
```

DON'T log these values:

- `management_token`
- `Authorization` headers
- `Proxy-Authorization` headers
- SOCKS or HTTP proxy credentials
- raw control-plane response bodies
- full upstream URLs when query strings are present
- request or response body bytes

Node IDs and server labels are safe to log. Node names are safe when they come
from config or remote config, but avoid adding free-form user payloads to logs.

---

## What To Log

Log startup and resolved tunnel policy:

```rust
// apps/aether-proxy/src/app.rs:134
info!(
    tunnel_connections_initial = tunnel_pool_policy.min_connections,
    tunnel_connections_max = tunnel_pool_policy.max_connections,
    tunnel_max_streams = tunnel_pool_policy.max_streams_per_tunnel,
    scale_check_interval_ms = tunnel_pool_policy.scale_check_interval.as_millis(),
    scale_up_threshold_percent = tunnel_pool_policy.scale_up_threshold_percent,
    scale_down_threshold_percent = tunnel_pool_policy.scale_down_threshold_percent,
    scale_down_grace_secs = tunnel_pool_policy.scale_down_grace.as_secs(),
    auto_sizing = config.tunnel_connections.is_none(),
    "resolved tunnel pool policy"
);
```

Log registration success and retry failures:

```rust
// apps/aether-proxy/src/app.rs:185
Ok(node_id) => {
    info!(server = %label, node_id = %node_id, url = %entry.aether_url, node_name = %node_name, "registered");
}
```

Log tunnel drain and stale connection events:

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:60
if draining && streams.is_empty() {
    info!("tunnel drained after in-flight streams completed");
    break None;
}
```

Log upstream request success/failure through `log_stream_success` and
`log_stream_failure`; do not scatter ad hoc request logs across helpers.

Log metrics-related errors by calling `server.tunnel_metrics.record_error` in
addition to tracing when the error should appear in heartbeat metrics.

---

## What Not To Log

Do not add unstructured `format!` logs for retry loops. Use fields so operators
can filter by `server`, `conn`, `stream_id`, `attempt`, and `status`.

Do not log every WebSocket frame at info/debug in production paths. Frame volume
can be high; use aggregate metrics and targeted warn/error events.

Do not log request bodies, response bodies, or decompressed tunnel payloads.
Malformed metadata can be logged as an error string only.

Do not log unredacted `aether_proxy_url` or `upstream_proxy_url`. Use
`UpstreamProxyConfig::redacted_url`.

Do not log query strings. Current request logs intentionally use
`query_present = url.query().is_some()`.

---

## Review Checklist

For every new log:

- Is the level consistent with current lifecycle/retry/tunnel semantics?
- Are fields structured and named like existing fields?
- Are credentials and body payloads absent or redacted?
- Does high-volume code avoid info/debug spam?
- Does the event also update `TunnelMetrics` if it affects heartbeat-reported
  tunnel health?
- Is the message stable and grep-friendly?
