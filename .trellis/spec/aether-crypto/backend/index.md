# Backend Development Guidelines

> Entry point for backend work in the `aether-crypto` crate.

---

## Package Summary

`aether-crypto` is the Aether foundation crate for Python Fernet compatibility
helpers. It owns key derivation, token encryption, token decryption, token shape
detection, and warm-up of the small derived-key cache. It is synchronous, has no
internal Aether crate dependencies, and should remain a leaf utility crate.

Evidence:

```toml
# crates/aether-crypto/Cargo.toml:1
[package]
name = "aether-crypto"
description = "Shared crypto compatibility helpers for Rust migration"
```

The current public surface is re-exported from `src/lib.rs`:

```rust
// crates/aether-crypto/src/lib.rs:3
pub use python_fernet::{
    decrypt_python_fernet_ciphertext, derive_python_fernet_key,
    encrypt_python_fernet_plaintext, looks_like_python_fernet_ciphertext,
    warm_python_fernet_secret, PythonFernetCompat, PythonFernetError,
    APP_SALT_HEX, APP_SALT_SEED, DEVELOPMENT_ENCRYPTION_KEY,
};
```

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Crate layout, facade exports, private module organization, and dependency direction | Filled |
| [Error Handling](./error-handling.md) | `PythonFernetError`, typed `Result` propagation, caller conversion, and compatibility failure modes | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Visibility, type-safety, crypto operation order, cache bounds, and required tests | Filled |
| [Logging Guidelines](./logging-guidelines.md) | No-logging stance for secret-handling code and caller-side observability boundaries | Filled |

`database-guidelines.md` was removed because this crate has no database, ORM,
Redis, transaction, migration, or connection handling code. Database behavior
belongs to higher-layer crates such as `aether-data` and callers that persist
encrypted values.

## Pre-Development Checklist

Before editing `crates/aether-crypto/`, verify:

- The change is about cryptographic compatibility or a directly related helper.
- The behavior cannot live in a higher-layer caller.
- The public API remains small and explicit.
- Existing Python Fernet ciphertext compatibility is preserved.
- The change does not introduce async, HTTP, database, runtime state, or
  provider policy dependencies.
- The change does not log or expose secrets, plaintext, ciphertext, keys, IVs,
  or signatures.

## Package Boundary

The dependency direction is one-way. Higher layers call this crate:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:1
use aether_crypto::{decrypt_python_fernet_ciphertext, looks_like_python_fernet_ciphertext};

// apps/aether-gateway/src/main.rs:11
use aether_crypto::warm_python_fernet_secret;
```

This crate must not import from `aether-provider-transport`, `aether-gateway`,
`aether-data`, `aether-admin`, or any other Aether layer.

## Public Contract

The public contract is Python-compatible Fernet wrapping:

- Direct 32-byte Fernet keys are accepted.
- Non-key secrets derive a Fernet key via PBKDF2-HMAC-SHA256.
- Tokens use version `0x80`, timestamp, IV, AES-128-CBC, PKCS7 padding, and
  HMAC-SHA256.
- Encoded payloads are URL-safe base64 wrapped twice for Python compatibility.
- Shape detection returns a boolean and must not throw or log.

Representative implementation:

```rust
// crates/aether-crypto/src/python_fernet.rs:217
pub fn derive_python_fernet_key(secret: &str) -> String {
    URL_SAFE.encode(raw_fernet_key(secret))
}

// crates/aether-crypto/src/python_fernet.rs:228
pub fn looks_like_python_fernet_ciphertext(ciphertext: &str) -> bool {
    let ciphertext = ciphertext.trim();
    if ciphertext.is_empty() || ciphertext.len() < minimum_wrapped_token_len() {
        return false;
    }
```

## Quality Gate

Minimum verification for crate changes:

```bash
cargo test -p aether-crypto
```

For public helper changes, also run or compile the relevant caller tests in
`aether-provider-transport` or `aether-gateway`, because these layers decrypt
catalog secrets and encrypt sensitive admin system config values.

Caller examples:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:133
match decrypt_python_fernet_ciphertext(encryption_key, ciphertext) {

// apps/aether-gateway/src/handlers/admin/system/shared/configs.rs:85
value = json!(encrypt_python_fernet_plaintext(encryption_key, plaintext)
    .map_err(|err| GatewayError::Internal(err.to_string()))?);
```

## Review Focus

Reviewers should spend most time on:

- Compatibility with legacy Python Fernet ciphertext.
- Key derivation fallback behavior.
- HMAC verification before decryption.
- Error variants and caller-facing error strings.
- Secret redaction and absence of logging.
- Cache bounds and lock handling.
- Tests using deterministic vectors for format details.

## Non-Goals

This spec intentionally does not cover:

- Database schema or migrations.
- SeaORM repository patterns.
- HTTP request/response mapping.
- Redis or runtime-state cache policies.
- Provider scheduling or model selection.
- Frontend behavior.

If a task touches those areas, load the corresponding package spec instead of
adding that guidance here.
