# Quality Guidelines

> Code standards and review rules for `apps/aether-proxy`.

---

## Overview

`aether-proxy` is a long-running edge binary in a hostile network path. Quality
is mostly about bounded concurrency, explicit config validation, egress safety,
sanitized diagnostics, reconnect behavior, and keeping tunnel frame flow from
blocking globally.

The crate already accepts one Clippy exception at the binary root:

```rust
// apps/aether-proxy/src/main.rs:1
#![allow(clippy::large_enum_variant)]
```

Do not add broad new allow-lints. Local exceptions such as
`#[allow(clippy::too_many_arguments)]` are used where orchestration functions
would become less clear if wrapped in artificial structs.

---

## Required Patterns

### Validate Config Before Runtime Work

`app::run` validates before tracing, registration, DNS cache creation, Redis,
or tunnel tasks:

```rust
// apps/aether-proxy/src/app.rs:73
pub async fn run(mut config: Config, servers: Vec<ServerEntry>) -> anyhow::Result<()> {
    config.validate()?;
    init_tracing(&config);
}
```

New config fields must be validated in `Config::validate` when bad values can
cause hangs, unsafe egress, silent no-ops, or misleading logs.

### Keep Config Migration Fail-Loud

`parse_config_file_content` rejects removed keys and only promotes
`upstream_proxy_url` when values are unambiguous:

```rust
// apps/aether-proxy/src/config.rs:1151
fn promote_server_scoped_upstream_proxy_url(value: &mut toml::Value) -> anyhow::Result<()> {
    const KEY: &str = "upstream_proxy_url";
    let mut promoted = root.get(KEY).cloned();
}
```

DON'T silently drop unknown or removed config fields. The current code uses
`#[serde(deny_unknown_fields)]` for server entries and explicit removed-key
checks for legacy top-level keys.

### Share Runtime State Through `Arc`

The runtime is task-heavy. Shared state is intentionally wrapped in `Arc`,
atomics, `Mutex`, `RwLock`, `watch`, and `ArcSwap`.

```rust
// apps/aether-proxy/src/state.rs:18
pub struct AppState {
    pub config: Arc<Config>,
    pub dns_cache: Arc<DnsCache>,
    pub upstream_client_pool: UpstreamClientPool,
    pub tunnel_tls_config: Arc<rustls::ClientConfig>,
    pub stream_gate: Option<Arc<ConcurrencyGate>>,
    pub distributed_stream_gate: Option<Arc<RuntimeSemaphore>>,
}
```

Prefer immutable snapshots and small locked sections. Remote config reads use
`ArcSwap` so stream code can load current values without a hot-path lock:

```rust
// apps/aether-proxy/src/runtime.rs:40
pub type SharedDynamicConfig = Arc<ArcSwap<DynamicConfig>>;
```

### Bound All Shared Loops

The dispatcher should never block all streams on one slow stream handler. It
uses per-stream channels, dispatch timeouts, periodic handle cleanup, and a
separate writer task.

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

When adding a new queue, channel, lock, retry, or network operation, define the
timeout or capacity next to the code and add a test if stalling is possible.

### Use Priority Queues For Outbound Frames

`writer.rs` gives control frames priority over response bodies:

```rust
// apps/aether-proxy/src/tunnel/writer.rs:162
fn classify_frame_priority(frame: &Frame) -> FramePriority {
    match frame.msg_type {
        MsgType::ResponseHeaders
        | MsgType::StreamError
        | MsgType::Ping
        | MsgType::Pong
        | MsgType::GoAway
        | MsgType::HeartbeatData
        | MsgType::HeartbeatAck => FramePriority::High,
        MsgType::RequestHeaders
        | MsgType::RequestBody
        | MsgType::ResponseBody
        | MsgType::StreamEnd => FramePriority::Normal,
    }
}
```

DON'T send directly to the WebSocket sink from stream handlers or heartbeat
code. All outbound frames go through `FrameSender`.

### Validate Egress Targets Before Building Requests

The stream handler validates host, port, private IP policy, and DNS before
calling the upstream client:

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:742
let dns_start = Instant::now();
{
    let allowed_ports = Arc::clone(&server.dynamic.load().allowed_ports);
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
}
```

DON'T build new upstream execution paths that skip `target_filter`.

### Preserve Redirect And Header Safety

Hop-by-hop and security-sensitive headers are blocked before upstream requests:

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:48
const BLOCKED_HEADERS: &[&str] = &[
    "connection",
    "content-length",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];
```

Redirect replay is budgeted through `redirect_replay_budget_bytes`. Do not
buffer unbounded request bodies to follow redirects.

---

## Forbidden Patterns

Do not introduce global mutable state for tunnel or stream state. Use `AppState`,
`ServerContext`, `TunnelMetrics`, `watch` channels, and task handles.

Do not hold `std::sync::Mutex` guards across `.await`. Current mutex use guards
short in-memory structures, for example the recent error deque and upstream
client map. If a new lock must be held across async work, use a tokio primitive
and justify it.

Do not add new dependencies without a strong crate-local reason. This binary
already depends on tokio, hyper, reqwest, tokio-tungstenite, rustls, ratatui,
sysinfo, tar/flate2, socket2, and shared Aether crates.

Do not create direct database access or SeaORM models in this binary. Optional
distributed admission goes through `aether-runtime-state`, not raw Redis code.

Do not log secrets or full URLs with sensitive query strings. Use structured
host/path/query-present fields or redacted URLs.

Do not assume a single Aether server. `run_proxy` supports config-file
`[[servers]]`, labels each server, and starts one tunnel pool per context.

Do not make connection 1..N send heartbeats. `connect_and_run` only starts the
real heartbeat task for `conn_idx == 0` to avoid corrupting shared metrics.

---

## Testing Requirements

Add module-local unit tests for pure policy code:

```rust
// apps/aether-proxy/src/app.rs:812
#[test]
fn desired_tunnel_connections_expands_when_load_crosses_high_water() {
    let policy = TunnelPoolPolicy {
        min_connections: 1,
        max_connections: 6,
        max_streams_per_tunnel: 1024,
        scale_check_interval: Duration::from_secs(1),
        scale_up_threshold_percent: 70,
        scale_down_threshold_percent: 35,
        scale_down_grace: Duration::from_secs(15),
    };
    assert_eq!(desired_tunnel_connections(2_000, &policy), 3);
}
```

Add async tests for backpressure and tunnel-frame behavior:

```rust
// apps/aether-proxy/src/tunnel/dispatcher.rs:387
#[tokio::test]
async fn dispatch_stream_frame_times_out_when_handler_stops_draining() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1);
    tx.send(Frame::new(7, MsgType::RequestBody, 0, Bytes::from_static(b"first")))
        .await
        .expect("first frame should enqueue");
}
```

Add fake-server tests for lifecycle behavior instead of hitting real control
planes:

```rust
// apps/aether-proxy/src/app.rs:774
spawn_registration_recovery_tasks(
    Arc::clone(&state),
    Arc::clone(&server_contexts),
    failed,
    "127.0.0.1".to_string(),
    sample_hardware_info(),
    tunnel_pool_policy,
    shutdown_rx.clone(),
    Arc::clone(&tunnel_handles),
    Arc::clone(&retry_handles),
)
.await;
```

For proxy protocol changes, add tests in `egress_proxy.rs` or
`upstream_client.rs` using local sockets/fakes. Do not rely on external proxy
services.

---

## Review Checklist

Check that new code:

- Runs after `Config::validate` or adds validation for new fields.
- Has bounded timeouts, queue capacities, retry caps, or drain behavior.
- Uses `FrameSender` instead of writing directly to WebSocket sinks.
- Preserves DNS/private-IP/allowed-port validation for all upstream paths.
- Redacts proxy credentials and control-plane response bodies.
- Maintains multi-server labels and per-server metrics.
- Does not add SQL or SeaORM ownership to this binary.
- Includes tests for policy math, error branches, and async backpressure.
- Uses `ArcSwap` or snapshots for runtime config changes, not long-held locks.

---

## Common Mistakes

Treating `aether-proxy` like a normal web API service leads to wrong designs.
There are no request handlers registered with axum in production runtime; axum
appears only in tests.

Adding a helper that returns a raw `reqwest::Client` for upstream traffic would
bypass `UpstreamClientPool`, validated DNS, transport profiles, and timing
instrumentation.

Following redirects by default would change current semantics. Tests show
redirects are explicit, replay-budget-aware behavior in `stream_handler.rs`.

Expanding log detail without redaction can leak management tokens, proxy
credentials, or provider request metadata.

Using unbounded mpsc channels or unbounded body buffering can let one slow
network path exhaust the edge process.
