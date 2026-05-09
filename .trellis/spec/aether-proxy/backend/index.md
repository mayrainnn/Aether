# Backend Development Guidelines

> Entry point for `apps/aether-proxy`, the Aether edge proxy binary.

---

## What This Crate Does

`aether-proxy` is a layer-5 Rust binary. It runs on edge hosts, registers
itself with one or more Aether control planes, keeps WebSocket tunnels open,
receives multiplexed request frames, validates target hosts, executes upstream
HTTP requests, and relays responses back through the tunnel.

The crate is intentionally not an axum API server. Its CLI subcommands handle
setup, service lifecycle, logs, and self-upgrade, while the default command path
starts tunnel mode:

```rust
// apps/aether-proxy/src/main.rs:29
fn build_command() -> clap::Command {
    Config::command()
        .subcommand(clap::Command::new("setup").about("Interactive setup wizard (TUI)"))
        .subcommand(clap::Command::new("start").about("Start the installed service"))
        .subcommand_negates_reqs(true)
}
```

GitNexus MCP resources for repo `Aether` report 3,140 files, 83,229 symbols,
and 300 execution flows. The process resource shows the config startup flow
`main -> handle_setup_result -> run_proxy -> ConfigFile::load ->
parse_config_file_content -> reject_removed_config_keys`, plus the sibling
flow ending in `promote_server_scoped_upstream_proxy_url`. Those process traces
match the binary facade and config-migration rules documented here.

ABCoder MCP was requested with `repo_name="aether-proxy"`, but the current
Codex session did not expose an ABCoder MCP namespace. `abcoder` was also not
available on PATH, and `npx abcoder --help` failed because the package name was
not found in npm. These specs are therefore grounded in GitNexus MCP resources
plus direct source reads from `apps/aether-proxy`.

---

## Read These Guides

| Guide | Purpose |
|-------|---------|
| [Directory Structure](./directory-structure.md) | Module boundaries, ownership, and where new proxy behavior belongs. |
| [Error Handling](./error-handling.md) | `anyhow`, `thiserror`, `String`, `io::Error`, tunnel frame errors, and shutdown behavior. |
| [Quality Guidelines](./quality-guidelines.md) | Type-safety, concurrency, visibility, tests, egress filtering, and forbidden shortcuts. |
| [Logging Guidelines](./logging-guidelines.md) | Structured `tracing` fields, log levels, redaction, and runtime log reloads. |
| [Database Guidelines](./database-guidelines.md) | No SQL ownership; optional Redis-backed distributed stream admission only. |

Keep all files in this directory synchronized. If a future change adds a new
major subsystem, add a guide for that subsystem and update this table.

---

## Architectural Rules

Keep `main.rs` as a binary facade. It declares modules, wires clap
subcommands, loads TOML into environment defaults, and delegates runtime work to
`app::run`.

Keep `app.rs` as the lifecycle owner. It validates config, initializes tracing,
collects hardware, registers servers, builds shared state, starts tunnel pool
managers, retries failed registration, and unregisters during shutdown:

```rust
// apps/aether-proxy/src/app.rs:73
pub async fn run(mut config: Config, servers: Vec<ServerEntry>) -> anyhow::Result<()> {
    config.validate()?;
    init_tracing(&config);
    /* registration, shared state, tunnel pool, shutdown */
}
```

Keep request execution under `tunnel/`. The dispatcher decodes WebSocket frames
and spawns per-stream handlers; the stream handler validates targets, builds
an upstream request, relays body frames, and sends response frames; the writer
serializes all outbound frames through priority queues.

Keep direct egress proxy protocol code in `egress_proxy.rs` and Hyper client
pooling in `upstream_client.rs`. Do not spread CONNECT or SOCKS handshakes into
stream handlers.

Keep persistence out of this crate. `aether-proxy` has no SeaORM entities,
migrations, or SQL repositories. The only external state backend is an optional
Redis semaphore supplied by `aether-runtime-state` for distributed admission.

---

## Pre-Development Checklist

1. Read [Directory Structure](./directory-structure.md) before adding a file or
   moving behavior between `app`, `tunnel`, `setup`, and `upstream_client`.
2. Read [Error Handling](./error-handling.md) before adding new fallible paths.
3. Read [Quality Guidelines](./quality-guidelines.md) before touching frame
   flow, DNS validation, redirect replay, or tunnel pool sizing.
4. Read [Logging Guidelines](./logging-guidelines.md) before logging URLs,
   proxy URLs, control-plane responses, node tokens, or request metadata.
5. Read [Database Guidelines](./database-guidelines.md) before using Redis or
   claiming this crate owns database behavior.

---

## Verification Commands

Run the narrowest command that covers your change. Examples:

```bash
cargo test -p aether-proxy target_filter
cargo test -p aether-proxy tunnel::dispatcher
cargo test -p aether-proxy tunnel::writer
cargo test -p aether-proxy app
```

For broad changes to the binary, tunnel protocol, or runtime-state integration:

```bash
cargo test -p aether-proxy
cargo check -p aether-proxy
```

Also check the spec itself after doc edits:

```bash
rg -n "fill-me|html-comment-marker|starter-copy" .trellis/spec/aether-proxy/backend
find .trellis/spec/aether-proxy/backend -maxdepth 1 -name "*.md" -exec wc -l {} +
```

---

## Do Not

Do not add generic Rust advice to these files. Every rule should cite concrete
`aether-proxy` code paths, names, or behaviors.

Do not document `aether-proxy` as a database or API-handler crate. It is an edge
binary with a control-plane client and WebSocket tunnel runtime.

Do not invent ABCoder results when the server is unavailable. Use GitNexus
resources and source evidence, then state the tooling limit exactly.

Do not log management tokens, raw Authorization headers, full upstream query
strings, raw control-plane error bodies, or unredacted proxy URLs.
