# Error Handling

`aether-dispatch-core` has one explicit error type: `PoolDispatchError<Error>`.

## `PoolDispatchError<Error>`

```rust
// crates/aether-dispatch-core/src/pool.rs:50
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolDispatchError<Error> {
    #[error("pool dispatch port failed")]
    Port(Error),
}
```

### What it means

- The cursor algorithm itself is not a validation layer.
- Exhaustion is not an error.
- Empty pages are not an error.
- Only the backing port can fail, so all errors are wrapped as `Port(Error)`.

### Where it is raised

```rust
// crates/aether-dispatch-core/src/pool.rs:86
let page = port
    .read_page(offset, limit)
    .await
    .map_err(PoolDispatchError::Port)?;
```

```rust
// crates/aether-dispatch-core/src/pool.rs:103
let window = port
    .rank_and_filter_window(page, config.window_size)
    .await
    .map_err(PoolDispatchError::Port)?;
```

### Why the variant is generic

The port chooses its own error type. The core crate does not know whether that error comes from:

- repository access
- cache access
- runtime state access
- test doubles

The generic wrapper keeps the crate reusable and avoids leaking gateway-specific error enums into the core API.

## `thiserror` Pattern

The derive is intentionally minimal:

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolDispatchError<Error> {
    #[error("pool dispatch port failed")]
    Port(Error),
}
```

Guidelines:

- Keep the display string static.
- Do not inline the inner error into the display message.
- Preserve `Clone`, `PartialEq`, and `Eq` so tests can compare the error value directly.
- Keep the enum to a single variant unless a new cursor failure mode truly exists.

## Error Propagation Path

The error bubbles out of `run_pool_dispatch_cursor` to the caller of the port implementation:

```rust
// crates/aether-dispatch-core/src/pool.rs:74
pub async fn run_pool_dispatch_cursor<Port>(
    port: &mut Port,
    config: PoolWindowConfig,
) -> Result<PoolDispatchCursorOutcome<Port::Candidate>, PoolDispatchError<Port::Error>>
where
    Port: PoolDispatchPort + Send,
{ ... }
```

GitNexus and source inspection show that the production gateway does not currently call this function. It has its own `PoolKeyCursor` implementation in `apps/aether-gateway/src/dispatch/pool_scheduler.rs`. So the practical downstream effect today is limited to:

- crate-local tests inside `aether-dispatch-core`
- any future caller that adopts the port/cursor contract

## Non-Error Outcomes

Successful exhaustion is returned as data, not an error:

```rust
// crates/aether-dispatch-core/src/pool.rs:91
if page.is_empty() {
    return Ok(PoolDispatchCursorOutcome {
        candidates: Vec::new(),
        scanned_count,
        exhausted: true,
    });
}
```

This distinction matters:

- `exhausted: true` means the port had nothing more to provide.
- `candidates.is_empty()` can mean either a frozen window produced no survivors or the scan ended with no remaining pages.
- Only `Err(PoolDispatchError::Port(...))` means the backing port failed.

## Anti-Patterns

- Do not add `InvalidConfig`, `NoCandidates`, or `WindowTooSmall` errors. Normalize config instead.
- Do not convert exhaustion into an error.
- Do not expose gateway-specific error enums in this crate.
- Do not log inside error constructors or cursor code.
