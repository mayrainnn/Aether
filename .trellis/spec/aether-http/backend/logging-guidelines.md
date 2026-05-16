# Logging Guidelines

`aether-http` currently contains no logging or tracing calls, and that is the
right default. The crate has no operation label, tenant, route, node id, token
scope, or user context. It only configures `reqwest` clients and computes retry
delays. Logging belongs at the caller boundary where those fields exist.

## Crate-Level Rule

Do not add `tracing`, `log`, or `println!` to `crates/aether-http`.

The crate's dependency list is intentionally small:

```toml
# crates/aether-http/Cargo.toml:8
[dependencies]
reqwest.workspace = true
serde.workspace = true
```

Adding a logging dependency here would make every consumer pay for a concern
the foundation crate cannot label correctly.

## What aether-http Should Do Instead

Expose data and return errors. Let callers decide what to log.

`build_http_client_with_headers` returns `reqwest::Error` and does not emit a
log when proxy validation or client construction fails:

```rust
// crates/aether-http/src/client.rs:43
pub fn build_http_client_with_headers(
    config: &HttpClientConfig,
    default_headers: HeaderMap,
) -> Result<reqwest::Client, reqwest::Error> {
```

`jittered_delay_for_retry` returns a `Duration` and does not log each jitter
calculation:

```rust
// crates/aether-http/src/retry.rs:5
pub fn jittered_delay_for_retry(config: HttpRetryConfig, retry_index: u32) -> Duration {
```

Guideline: if a helper has no context to produce a high-signal structured log,
keep it silent.

## Caller Logging Pattern: Public IP Detection

`apps/aether-proxy/src/net.rs` logs success at `info` because it has the public
IP and source endpoint. It logs failed attempts at `debug` because the function
tries multiple providers and failure of one provider is expected:

```rust
// apps/aether-proxy/src/net.rs:23
match client.get(*endpoint).send().await {
    Ok(resp) if resp.status().is_success() => {
        let ip = resp.text().await?.trim().to_string();
        if !ip.is_empty() {
            info!(ip = %ip, source = %endpoint, "detected public IP");
            return Ok(ip);
        }
    }
    Ok(resp) => {
        debug!(endpoint = %endpoint, status = %resp.status(), "IP detection failed");
    }
    Err(e) => {
        debug!(endpoint = %endpoint, error = %e, "IP detection failed");
    }
}
```

This is the model for consumers: log the operation and domain fields in the
caller, not in `aether-http`.

## Caller Logging Pattern: Registration Retry

The proxy registration client uses `jittered_delay_for_retry` and logs retry
decisions with attempt number, status or error, sleep duration, and label:

```rust
// apps/aether-proxy/src/registration/client.rs:217
if should_retry_status(resp.status()) && attempt < self.retry.max_attempts {
    let sleep_for = jittered_delay_for_retry(self.retry, attempt - 1);
    debug!(
        attempt,
        status = %resp.status(),
        sleep_ms = sleep_for.as_millis(),
        label,
        "Aether request retrying"
    );
```

For transport errors, the same function logs the error and next sleep:

```rust
// apps/aether-proxy/src/registration/client.rs:231
Err(e) => {
    if attempt < self.retry.max_attempts {
        let sleep_for = jittered_delay_for_retry(self.retry, attempt - 1);
        debug!(
            attempt,
            error = %e,
            sleep_ms = sleep_for.as_millis(),
            label,
            "Aether request retrying"
        );
```

Guideline: retry logs should be emitted by the retry loop, not by the delay
helper. The loop knows whether a delay follows a `429`, `408`, `5xx`, network
error, registration failure, or shutdown retry.

## Caller Logging Pattern: Degraded Shutdown

When unregistering a proxy node fails, the caller logs at `error` with redacted
body metadata. This is caller-owned because `aether-http` cannot know whether a
response body contains secrets:

```rust
// apps/aether-proxy/src/registration/client.rs:177
Ok(r) => {
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    let summary = summarize_text_payload(&text);
    error!(
        status = %status,
        body_bytes = summary.bytes,
        body_sha256 = %summary.sha256,
        "unregister failed"
    );
```

Do not move response body logging, redaction, or hashing into `aether-http`.
Different callers have different sensitivity rules.

## Log Levels

Use these levels in callers that consume `aether-http`:

- `debug`: expected retries, best-effort detection provider failures, fallback
  attempts, and transient HTTP failures where the caller will continue.
- `info`: successful domain events such as detected public IP, detected region,
  registered node id, or shutdown during retry.
- `warn`: repeated or user-visible retry failure where the caller keeps running.
- `error`: operation failure that aborts or reports a failed lifecycle step.

`aether-http` itself should use none of these levels unless it gains a
context-rich operation of its own.

## Sensitive Data Rules

Do not log these from `aether-http` or its callers without explicit redaction:

- Bearer tokens and management tokens.
- Proxy URLs that may include credentials.
- Request or response bodies.
- Header maps.
- Full endpoint URLs when query strings may include secrets.

The proxy registration client demonstrates the correct pattern: response body
content is summarized instead of logged directly:

```rust
// apps/aether-proxy/src/registration/client.rs:136
let text = resp.text().await.unwrap_or_default();
let summary = summarize_text_payload(&text);
anyhow::bail!(
    "register failed (HTTP {}): response body redacted (bytes={}, sha256={})",
```

## DON'T Patterns

DON'T log generic builder settings inside `apply_http_client_config`:

```rust
// DON'T: no operation context and proxy_url may be sensitive.
debug!(?config, "building HTTP client");
```

DON'T log every retry delay calculation:

```rust
// DON'T: the helper does not know the operation being retried.
info!(retry_index, "sleeping before retry");
```

DON'T print from this library:

```rust
// DON'T: library code must not write to stdout or stderr.
println!("configured proxy");
```

If future functionality truly needs logs inside `aether-http`, require a design
change first: add `tracing` explicitly, define allowed fields, and prove the
crate has enough context to avoid low-signal logs and secret leakage.
