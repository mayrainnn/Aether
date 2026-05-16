# Logging Guidelines

`aether-runtime-state` is intentionally quiet. Normal KV, lock, stream, and
rate-limit operations return typed errors instead of logging. The crate logs only
when asynchronous semaphore cleanup or lease renewal fails after the caller can no
longer receive a direct result.

## Logging Library

Use the project-wide `tracing` crate. This crate currently imports only
`tracing::warn`:

```rust
// crates/aether-runtime-state/src/lib.rs:24
use tracing::warn;
```

Do not introduce `println!`, `eprintln!`, or ad hoc logging. Aether's runtime
crate owns tracing initialization and formatting; this crate should emit
structured events only when needed.

## Current Warn Events

`RuntimeSemaphorePermit::drop` aborts the renewal task and spawns asynchronous
release. Because `Drop` cannot return a `Result`, release failures are logged:

```rust
// crates/aether-runtime-state/src/lib.rs:1418
impl Drop for RuntimeSemaphorePermit {
    fn drop(&mut self) {
        self.renew_task.abort();
        let state = Arc::clone(&self.state);
        let token = self.token.clone();
        tokio::spawn(async move {
            if let Err(err) = state.release(&token).await {
                warn!(
                    gate = state.gate,
                    error = %err,
                    "failed to release runtime semaphore permit"
                );
            }
        });
    }
}
```

Lease renewal failures are also logged because they happen in a background task
created after a permit is acquired:

```rust
// crates/aether-runtime-state/src/lib.rs:1463
let renew_task = tokio::spawn(async move {
    let interval = Duration::from_millis(renew_state.config.renew_interval_ms);
    loop {
        tokio::time::sleep(interval).await;
        if let Err(err) = renew_state.renew(&renew_token).await {
            warn!(
                gate = renew_state.gate,
                error = %err,
                "failed to renew runtime semaphore permit"
            );
            break;
        }
    }
});
```

Keep the structured fields `gate` and `error` on these events. They are the
minimum context needed to identify the saturated or unavailable admission gate.

## Log Levels

Use `warn!` only for background cleanup or renewal failures that may leak or
expire a distributed permit unexpectedly.

Return `DataLayerError` for foreground Redis failures. Do not both log and return
the same error from ordinary methods such as `kv_get`, `read_group`, or
`lock_try_acquire`; callers at service boundaries should decide how to report
those failures.

Use `debug!` or `trace!` only if a future diagnostic need is proven and the log
does not include keys, values, tokens, Redis URLs, payload bodies, or PII.

Use `info!` only for lifecycle events owned by applications or workers, not for
low-level state operations in this crate.

## Structured Fields

When logging inside this crate, use tracing fields instead of string
interpolation for machine-readable context:

```rust
// Preferred style from crates/aether-runtime-state/src/lib.rs:1425
warn!(
    gate = state.gate,
    error = %err,
    "failed to release runtime semaphore permit"
);
```

If adding a new background task, include stable low-cardinality fields such as
`gate`, `operation`, or `stream_kind`. Avoid high-cardinality raw keys and member
IDs unless the event is already a sanitized admin/debug-only path.

## What Not To Log

Do not log Redis URLs from `RedisClientConfig.url`
(`src/redis/client.rs:8`). URLs can embed credentials.

Do not log KV values, stream JSON payloads, OAuth token cache entries, lock
tokens, or semaphore tokens. Tokens are generated with UUIDs and used as
ownership proofs:

```rust
// crates/aether-runtime-state/src/redis/lock.rs:93
let token = format!("{owner}:{}", Uuid::new_v4());
```

Do not log raw namespaced keys by default. A key may include user IDs, provider
IDs, or other operational identifiers. Prefer a stable gate or operation name.

## Error Propagation Instead Of Logs

Foreground Redis operations include operation names in timeout errors:

```rust
// crates/aether-runtime-state/src/lib.rs:1731
tokio::time::timeout(Duration::from_millis(timeout_ms), future)
    .await
    .map_err(|_| {
        DataLayerError::TimedOut(format!("{operation} exceeded {timeout_ms}ms timeout"))
    })?
```

This is the primary observability surface for callers. Keep operation names clear
and specific, for example `"runtime rate limit check"`,
`"runtime score rank trim"`, or `"redis stream reclaim"`.

## DO NOT Patterns

DO NOT add success logs for hot operations. KV, score, rate-limit, and queue
methods can be called per request; success logging would be high-volume noise.

DO NOT log and suppress errors in foreground methods. If a method returns
`Result`, let the caller handle the error.

DO NOT log secrets or payload bodies to make debugging easier. Add sanitized
fields or improve error messages instead.
