# Directory Structure

> Backend organization rules for `apps/aether-proxy`.

---

## Scope

`aether-proxy` is a binary crate, not a reusable library crate. Its public
surface is the executable behavior exposed by `main.rs`, CLI subcommands, and
the WebSocket tunnel protocol. Keep modules private unless another internal
module in this crate needs a symbol.

Evidence:

```rust
// apps/aether-proxy/src/main.rs:3
mod app;
mod config;
mod egress_proxy;
mod hardware;
mod net;
mod registration;
mod runtime;
mod setup;
mod state;
mod target_filter;
mod tunnel;
mod upstream_client;
```

There is no `src/lib.rs`. Do not add one just to re-export internals unless a
real external crate needs a stable API.

---

## Actual Layout

```text
apps/aether-proxy/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs
    ├── app.rs
    ├── config.rs
    ├── egress_proxy.rs
    ├── hardware.rs
    ├── net.rs
    ├── runtime.rs
    ├── safe_dns.rs
    ├── state.rs
    ├── target_filter.rs
    ├── upstream_client.rs
    ├── registration/
    │   ├── mod.rs
    │   └── client.rs
    ├── setup/
    │   ├── mod.rs
    │   ├── service.rs
    │   ├── tui.rs
    │   └── upgrade.rs
    └── tunnel/
        ├── mod.rs
        ├── client.rs
        ├── dispatcher.rs
        ├── heartbeat.rs
        ├── protocol.rs
        ├── stream_handler.rs
        └── writer.rs
```

GitNexus process resources confirm that startup crosses `main.rs`, `config.rs`,
and config migration helpers before entering runtime. That is the current
architectural spine; do not bypass it with alternate startup paths.

---

## Module Responsibilities

### `main.rs`

Owns binary bootstrapping: rustls provider installation, config-file loading,
clap subcommands, fallback to setup on missing required config, and the
`run_proxy` handoff.

```rust
// apps/aether-proxy/src/main.rs:54
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls CryptoProvider"))?;
}
```

Add new subcommands here only when they are executable-level behavior. Runtime
or tunnel logic belongs elsewhere.

### `app.rs`

Owns lifecycle orchestration. It turns validated config into shared
`AppState`, registers each `ServerEntry`, starts tunnel pool managers, starts
background registration recovery, and performs graceful unregister during
shutdown.

```rust
// apps/aether-proxy/src/app.rs:162
let server_contexts: Arc<Mutex<Vec<Arc<ServerContext>>>> = Arc::new(Mutex::new(Vec::new()));
let mut failed_entries: Vec<(String, ServerEntry)> = Vec::new();
```

Use `app.rs` for cross-module coordination. Keep low-level protocol parsing and
network handshake logic in the specialized modules.

### `config.rs`

Owns clap/TOML/environment configuration, validation, removed-key rejection,
legacy-key migration constraints, and `ServiceRuntimeConfig` construction.

```rust
// apps/aether-proxy/src/config.rs:1137
fn parse_config_file_content(content: &str) -> anyhow::Result<ConfigFile> {
    reject_removed_config_keys(content)?;
    let mut value: toml::Value = toml::from_str(content)?;
    promote_server_scoped_upstream_proxy_url(&mut value)?;
    Ok(value.try_into()?)
}
```

Configuration changes must be fail-loud. Do not silently ignore stale keys or
accept ambiguous per-server values.

### `registration/`

`registration/client.rs` owns control-plane HTTP calls for proxy-node register,
unregister, and remote config DTOs.

```rust
// apps/aether-proxy/src/registration/client.rs:47
pub struct AetherClient {
    http: Client,
    base_url: String,
    token: String,
    retry: HttpRetryConfig,
}
```

Keep control-plane retry and response-body redaction here. Do not make tunnel
code post directly to `/api/admin/proxy-nodes/*`.

### `tunnel/`

`tunnel/mod.rs` owns reconnect loops, startup staggering, and drain-aware slot
exit. `tunnel/client.rs` owns WebSocket connect/auth/upgrade. `dispatcher.rs`
reads incoming frames. `stream_handler.rs` executes a single request stream.
`writer.rs` is the only place that writes outbound WebSocket messages.

```rust
// apps/aether-proxy/src/tunnel/client.rs:30
pub async fn connect_and_run(
    state: &Arc<AppState>,
    server: &Arc<ServerContext>,
    conn_idx: usize,
    shutdown: &mut watch::Receiver<bool>,
    drain: watch::Receiver<bool>,
) -> Result<TunnelOutcome, anyhow::Error>
```

Add tunnel features to the narrowest file. For example, frame priority belongs
in `writer.rs`, target validation belongs in `stream_handler.rs` plus
`target_filter.rs`, and reconnect policy belongs in `tunnel/mod.rs`.

### `target_filter.rs`, `safe_dns.rs`, and `upstream_client.rs`

`target_filter.rs` validates ports, IP ranges, and DNS results. The DNS cache
is shared with `upstream_client::ValidatedResolver` so the Hyper connector uses
previously validated addresses and avoids a DNS rebinding gap.

```rust
// apps/aether-proxy/src/target_filter.rs:265
pub async fn validate_target(
    host: &str,
    port: u16,
    allowed_ports: &HashSet<u16>,
    allow_private: bool,
    dns_cache: &DnsCache,
) -> Result<Vec<SocketAddr>, FilterError>
```

Keep target filtering separate from request execution. This makes it testable
without a tunnel or HTTP server.

### `egress_proxy.rs` and `upstream_client.rs`

`egress_proxy.rs` owns manual proxy protocols: HTTP CONNECT, SOCKS5, SOCKS5h,
redacted proxy URLs, proxy auth headers, and TCP socket setup. `upstream_client`
owns Hyper client pooling, TLS connection instrumentation, and transport-profile
keys.

```rust
// apps/aether-proxy/src/egress_proxy.rs:131
pub(crate) async fn connect_target_via_proxy(
    proxy: &UpstreamProxyConfig,
    target_host: &str,
    target_port: u16,
    options: ProxyConnectOptions,
) -> io::Result<TcpStream>
```

Do not put proxy handshake bytes in `stream_handler.rs`.

### `setup/`

`setup/tui.rs` owns interactive config capture. `setup/service.rs` owns systemd
and OpenRC service management. `setup/upgrade.rs` owns GitHub release download,
checksum verification, extraction, atomic replacement, and service restart.

```rust
// apps/aether-proxy/src/setup/service.rs:84
pub fn install_service(config_path: &Path) -> anyhow::Result<()> {
    let manager = detect_service_manager()
        .ok_or_else(|| anyhow::anyhow!("no supported service manager detected (systemd/OpenRC)"))?;
}
```

Keep host-service side effects in `setup/service.rs`; do not run `systemctl` or
`rc-service` from `main.rs` directly.

---

## Naming And Visibility

Use module names by subsystem, not implementation detail. `tunnel/dispatcher.rs`
and `tunnel/stream_handler.rs` are good examples because they name runtime
roles.

Use `pub(crate)` for helpers shared inside the binary but not meant as external
API, as in `UpstreamProxyConfig`:

```rust
// apps/aether-proxy/src/egress_proxy.rs:18
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpstreamProxyConfig {
    raw: String,
    scheme: UpstreamProxyScheme,
}
```

Use private structs/enums for file-local policies, such as `TunnelPoolPolicy`
and `ManagedTunnel` in `app.rs`.

Use explicit, transport-domain names: `FrameSender`, `TunnelOutcome`,
`ProxyConnectOptions`, `ProxyAdmissionError`, `RemoteConfig`, and
`TunnelPoolSizing`.

---

## Tests Stay Near The Owning Module

Unit tests are embedded in the module that owns the behavior:

```rust
// apps/aether-proxy/src/target_filter.rs:340
#[tokio::test]
async fn test_port_not_allowed() {
    let cache = cache();
    let result = validate_target("8.8.8.8", 22, &ports(), false, &cache).await;
    assert!(matches!(result, Err(FilterError::PortNotAllowed(22))));
}
```

Integration-like tests still live in the owning module when they need local
private helpers. `app.rs` starts a fake axum gateway to prove registration
recovery survives startup failures and connects later.

---

## Do Not

Do not add a catch-all `utils.rs`. Add a named module only when a new subsystem
has a clear owner.

Do not make `tunnel` modules public for convenience. Wire new behavior through
the existing binary runtime.

Do not move control-plane HTTP calls into stream handlers. Registration and
unregistration stay in `registration/client.rs`.

Do not add database modules to this crate. Document Redis admission in
`database-guidelines.md`; SQL belongs in data/gateway crates.
