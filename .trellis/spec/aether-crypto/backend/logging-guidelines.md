# Logging Guidelines

> Logging and observability rules for the `aether-crypto` crate.

---

## Current State

`aether-crypto` does not use `tracing`, `log`, `println!`, or `dbg!`. That is
intentional. The crate handles secrets, ciphertext, plaintext, derived keys,
signatures, and IVs, so the safest observability contract is to return typed
errors and let higher layers decide what to log.

Evidence from dependencies:

```toml
# crates/aether-crypto/Cargo.toml:9
[dependencies]
aes.workspace = true
base64.workspace = true
cbc.workspace = true
hmac.workspace = true
pbkdf2.workspace = true
sha2.workspace = true
thiserror.workspace = true
uuid.workspace = true
```

There is no `tracing` dependency in this crate. Do not add one just to observe
normal encrypt/decrypt activity.

## Error Reporting Instead Of Logging

The crate reports failures through `PythonFernetError`:

```rust
// crates/aether-crypto/src/python_fernet.rs:86
#[derive(Debug, thiserror::Error)]
pub enum PythonFernetError {
    #[error("invalid Python Fernet outer base64 payload")]
    InvalidOuterBase64,
    #[error("invalid Python Fernet token signature")]
    InvalidTokenSignature,
    #[error("invalid Python Fernet plaintext utf-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}
```

These messages are safe because they name failure classes and omit the
encrypted value, decrypted plaintext, and secret. Keep any new error message at
that same level of detail.

## What To Log

Inside `aether-crypto`: nothing by default.

In callers: log only contextual metadata that is already safe for the caller's
layer. For example, provider snapshot mapping converts decrypt failures to a
data-layer error with the field name, not the secret:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:143
Err(DataLayerError::UnexpectedValue(format!(
    "failed to decrypt {field_name}: {error}"
)))
```

If a caller logs that error, it will include the field name and error class.
That is the intended boundary.

## What Not To Log

Never log these values in this crate:

- `secret`
- `ciphertext`
- `plaintext`
- raw derived keys
- signing keys
- encryption keys
- IV bytes
- HMAC signatures
- decrypted provider API keys
- decrypted auth config JSON

The implementation works with all of those values directly:

```rust
// crates/aether-crypto/src/python_fernet.rs:221
pub fn decrypt_python_fernet_ciphertext(
    secret: &str,
    ciphertext: &str,
) -> Result<String, PythonFernetError> {
    PythonFernetCompat::from_secret(secret).decrypt_ciphertext(ciphertext)
}
```

Because both inputs are sensitive, do not add debug logs around this public
function.

## Startup Warming

Startup warming is initiated by `aether-gateway`, not by this crate:

```rust
// apps/aether-gateway/src/main.rs:417
match self.effective_encryption_key() {
    Some(value) => {
        warm_python_fernet_secret(&value);
        config.with_encryption_key(value)
    }
    None => config,
}
```

`warm_python_fernet_secret` deliberately returns `()` and logs nothing:

```rust
// crates/aether-crypto/src/python_fernet.rs:251
pub fn warm_python_fernet_secret(secret: &str) {
    let _ = raw_fernet_key(secret);
}
```

Do not add a log such as "warmed key <value>". Even logging a hash or length of
the secret creates a new side channel and is not needed for operation.

## Shape Detection

`looks_like_python_fernet_ciphertext` is a safe boolean helper for callers that
need to distinguish encrypted-looking values from legacy plaintext:

```rust
// crates/aether-crypto/src/python_fernet.rs:228
pub fn looks_like_python_fernet_ciphertext(ciphertext: &str) -> bool {
    let ciphertext = ciphertext.trim();
    if ciphertext.is_empty() || ciphertext.len() < minimum_wrapped_token_len() {
        return false;
    }
```

The function should remain silent. Do not log why a candidate failed the shape
check; the candidate may be a plaintext API key or JSON auth config.

## Acceptable Future Observability

If observability is required in the future, prefer one of these caller-side
patterns:

- Count failures by `PythonFernetError` variant after the caller has mapped the
  error into its own telemetry layer.
- Log field names such as `provider_api_keys.api_key`, never values.
- Emit startup configuration warnings in `aether-gateway`, not in this crate.
- Use tests to validate compatibility vectors instead of logs to inspect data.

Example caller context that is acceptable to include in an error:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:123
fn decrypt_secret(
    encryption_key: &str,
    fallback_encryption_keys: &[String],
    ciphertext: &str,
    field_name: &str,
) -> Result<String, DataLayerError> {
```

Only `field_name` should survive into user-visible or logged diagnostics.

## Forbidden Patterns

DON'T add this:

```rust
// DON'T: crates/aether-crypto/src/python_fernet.rs
tracing::debug!(ciphertext, "decrypting Python Fernet ciphertext");
```

DON'T add this:

```rust
// DON'T: crates/aether-crypto/src/python_fernet.rs
println!("derived key for {secret}");
```

DON'T add this:

```rust
// DON'T: crates/aether-crypto/src/python_fernet.rs
dbg!(&plaintext);
```

The crate's current no-logging stance is part of its security posture. If a
future change adds logging, reviewers should require a written justification,
redaction rules, and tests or linting that prevent secret values from reaching
the log event.

## Review Checklist

For any change touching this crate, check:

- No `tracing`, `log`, `println!`, `eprintln!`, or `dbg!` usage was added.
- Error messages describe classes, not values.
- Caller conversions do not include ciphertext or plaintext.
- Tests do not print generated secrets or encrypted payloads.
- Documentation examples avoid real secrets and use sanitized literals only.
