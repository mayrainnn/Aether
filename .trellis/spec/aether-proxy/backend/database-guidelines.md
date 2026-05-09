# Database Guidelines

> Persistence and external state rules for `apps/aether-proxy`.

---

## Overview

`aether-proxy` does not own a SQL database. It has no SeaORM entities, no
migrations, no repositories, and no direct table queries. Do not add database
models to this crate.

The crate does have one optional external state integration: a Redis-backed
distributed semaphore from `aether-runtime-state`. It is used only for
cross-instance stream admission when `distributed_stream_limit` is configured.

Evidence:

```rust
// apps/aether-proxy/src/app.rs:228
if let Some(limit) = state.config.distributed_stream_limit {
    let redis_url = state
        .config
        .distributed_stream_redis_url
        .clone()
        .expect("distributed stream redis url should be validated");
    let runtime = RuntimeState::redis(
        RedisClientConfig {
            url: redis_url,
            key_prefix: state.config.distributed_stream_redis_key_prefix.clone(),
        },
        Some(state.config.distributed_stream_command_timeout_ms),
    )
    .await?;
}
```

This file stays in the spec because Redis is an external state backend used by
this binary. It should not be expanded into general SQL guidance.

---

## No SQL Ownership

Do not introduce any of these in `apps/aether-proxy`:

- SeaORM entities
- migrations
- repository structs
- SQL query builders
- direct PostgreSQL/MySQL/SQLite clients
- business data persistence

The proxy is an edge runtime. Business data and admin API persistence live in
gateway/data/admin crates. `aether-proxy` consumes control-plane APIs through
`registration/client.rs` and runtime contracts through shared crates.

DON'T add a `database.rs`, `repositories/`, or `entities/` module here.

---

## Redis Is Only For Distributed Admission

Distributed admission is optional and layered behind `aether-runtime-state`.
Config validation requires the Redis URL when the distributed gate is enabled:

```rust
// apps/aether-proxy/src/config.rs:699
if matches!(self.distributed_stream_limit, Some(0)) {
    anyhow::bail!("distributed_stream_limit must be > 0");
}
if self.distributed_stream_limit.is_some() && self.distributed_stream_redis_url.is_none() {
    anyhow::bail!(
        "distributed_stream_redis_url must be set when distributed_stream_limit is enabled"
    );
}
```

Lease and command timing are also fail-loud:

```rust
// apps/aether-proxy/src/config.rs:707
if self.distributed_stream_lease_ttl_ms == 0 {
    anyhow::bail!("distributed_stream_lease_ttl_ms must be > 0");
}
if self.distributed_stream_renew_interval_ms >= self.distributed_stream_lease_ttl_ms {
    anyhow::bail!(
        "distributed_stream_renew_interval_ms must be < distributed_stream_lease_ttl_ms"
    );
}
```

The Redis semaphore is acquired through shared state, not by direct Redis
commands:

```rust
// apps/aether-proxy/src/state.rs:364
let distributed = match &self.distributed_stream_gate {
    Some(gate) => Some(gate.try_acquire().await.map_err(|err| {
        match err {
            RuntimeSemaphoreError::Saturated { gate, limit } => {
                ProxyAdmissionError::Saturated { gate, limit }
            }
            RuntimeSemaphoreError::Unavailable { gate, limit, message } => {
                ProxyAdmissionError::Unavailable { gate, limit, message }
            }
            RuntimeSemaphoreError::InvalidConfiguration(message) => {
                ProxyAdmissionError::Unavailable {
                    gate: "proxy_streams_distributed",
                    limit: self.distributed_stream_gate.as_ref().map(|inner| inner.limit()).unwrap_or(0),
                    message,
                }
            }
        }
    })?),
    None => None,
};
```

DON'T import a raw Redis client into stream handlers. Use `RuntimeState` and
`RuntimeSemaphore`.

---

## Connection Handling

Redis connection setup occurs once during `app::run`, after config validation
and before tunnel pool managers are spawned. This keeps missing/invalid Redis
configuration out of hot request paths.

```rust
// apps/aether-proxy/src/app.rs:242
let distributed_gate = runtime.semaphore(
    "proxy_streams_distributed",
    limit,
    RuntimeSemaphoreConfig {
        lease_ttl_ms: state.config.distributed_stream_lease_ttl_ms,
        renew_interval_ms: state.config.distributed_stream_renew_interval_ms,
        command_timeout_ms: Some(state.config.distributed_stream_command_timeout_ms),
    },
)?;
```

The semaphore is stored in `AppState`:

```rust
// apps/aether-proxy/src/state.rs:27
/// Optional per-process stream admission gate.
pub stream_gate: Option<Arc<ConcurrencyGate>>,
/// Optional cross-instance stream admission gate.
pub distributed_stream_gate: Option<Arc<RuntimeSemaphore>>,
```

Use `Arc<RuntimeSemaphore>` when sharing the gate. Do not reconnect to Redis per
stream, per tunnel, or per request.

---

## Key Naming And Namespace

`aether-proxy` passes a fixed semaphore name and an optional configured
`key_prefix` into `aether-runtime-state`.

```rust
// apps/aether-proxy/src/app.rs:243
"proxy_streams_distributed",
```

If a future distributed primitive is added, name it by the runtime resource it
guards, not by a call site. Use names like `proxy_streams_distributed`; do not
use ambiguous names like `lock`, `semaphore`, or `global_limit`.

Keep Redis key formatting in `aether-runtime-state`. This crate should pass
semantic names and config, not concatenate Redis keys manually.

---

## Transactions And Migrations

There are no transactions or migrations in this crate.

Do not add migrations for proxy runtime state. Distributed admission state is
ephemeral lease state and should be managed by `RuntimeSemaphore`.

Do not add SQL transaction guidance here. If a future feature needs persistent
configuration, implement persistence in the control plane and expose it through
registration or heartbeat contracts.

---

## Failure Behavior

Invalid distributed admission configuration fails startup:

```rust
// apps/aether-proxy/src/config.rs:702
if self.distributed_stream_limit.is_some() && self.distributed_stream_redis_url.is_none() {
    anyhow::bail!(
        "distributed_stream_redis_url must be set when distributed_stream_limit is enabled"
    );
}
```

Runtime Redis/semaphore failures are mapped to `ProxyAdmissionError::Unavailable`
and stream handlers turn that into `"proxy admission unavailable"`:

```rust
// apps/aether-proxy/src/tunnel/stream_handler.rs:1067
let permit = match state.try_acquire_stream_permit().await {
    Ok(permit) => permit,
    Err(err) => {
        let message = match err {
            crate::state::ProxyAdmissionError::Saturated { .. } => "proxy overloaded",
            crate::state::ProxyAdmissionError::Unavailable { .. } => {
                "proxy admission unavailable"
            }
        };
        send_error(&frame_tx, stream_id, message).await;
        return;
    }
};
```

This is intentional: clients get a short status-like message, while internal
state carries gate and limit details.

---

## Common Mistakes

Do not call this a SeaORM crate because the wider Aether project uses SeaORM.
`apps/aether-proxy` itself does not.

Do not treat the DNS cache as persistence. `DnsCache` is an in-memory safety and
performance helper in `target_filter.rs`.

Do not add retry loops around Redis acquisition in stream handlers. Let
`RuntimeSemaphore` own command timeouts and lease behavior.

Do not create one Redis connection per `ServerContext` or tunnel connection.
Distributed admission is process-wide in `AppState`.

Do not store management tokens, node IDs, metrics, or tunnel errors in Redis
from this crate. Registration/heartbeat/control-plane APIs own durable state.

---

## Review Checklist

Before approving changes touching external state:

- Is there still no SQL or SeaORM dependency in `apps/aether-proxy/Cargo.toml`?
- Does any Redis behavior go through `aether-runtime-state`?
- Does `Config::validate` reject missing or impossible Redis settings?
- Is the semaphore initialized once in `app::run`?
- Are stream handlers using `try_acquire_stream_permit` rather than raw Redis?
- Are Redis failures mapped into short client-facing messages?
- Are key names semantic and centralized in the runtime-state layer?
