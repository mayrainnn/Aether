# Directory Structure

> Backend organization rules for the `aether-crypto` crate.

---

## Scope

`aether-crypto` is a foundation crate under `crates/aether-crypto/`. It is a
leaf utility crate: it owns cryptographic compatibility helpers and exposes a
small public API to higher layers, but it does not call Aether domain, data,
HTTP, admin, or runtime crates.

Evidence:

```toml
# crates/aether-crypto/Cargo.toml:1
[package]
name = "aether-crypto"

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

Keep this crate in the foundation layer. New code here should be deterministic,
mostly pure, synchronous, and free from application state.

## Actual Layout

```text
crates/aether-crypto/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── python_fernet.rs
```

`src/lib.rs` is only the public facade. It declares the private module and
re-exports the compatibility surface:

```rust
// crates/aether-crypto/src/lib.rs:1
mod python_fernet;

// crates/aether-crypto/src/lib.rs:3
pub use python_fernet::{
    decrypt_python_fernet_ciphertext, derive_python_fernet_key,
    encrypt_python_fernet_plaintext, looks_like_python_fernet_ciphertext,
    warm_python_fernet_secret, PythonFernetCompat, PythonFernetError,
    APP_SALT_HEX, APP_SALT_SEED, DEVELOPMENT_ENCRYPTION_KEY,
};
```

`src/python_fernet.rs` contains all implementation details for the Python Fernet
compatibility format: constants, key derivation, cache management, AES-CBC,
HMAC verification, double base64 wrapping, public helpers, and crate-local
tests.

## Module Boundary Rules

Keep algorithm-specific implementations in their own private module and expose
only stable helpers from `lib.rs`. The existing module is named by wire-format
compatibility, not by generic crypto primitives.

Use this pattern:

```rust
// crates/aether-crypto/src/lib.rs:1
mod python_fernet;

// crates/aether-crypto/src/lib.rs:3
pub use python_fernet::{encrypt_python_fernet_plaintext, PythonFernetError};
```

Do not expose a catch-all `crypto` module or make callers import internals from
`aether_crypto::python_fernet`. The module is intentionally private so callers
depend on the facade.

## Public API Shape

The public API uses explicit Python Fernet names because the format is a legacy
compatibility contract. The exported helpers are named by operation:

- `derive_python_fernet_key(secret: &str) -> String`
- `decrypt_python_fernet_ciphertext(secret: &str, ciphertext: &str) -> Result<String, PythonFernetError>`
- `encrypt_python_fernet_plaintext(secret: &str, plaintext: &str) -> Result<String, PythonFernetError>`
- `looks_like_python_fernet_ciphertext(ciphertext: &str) -> bool`
- `warm_python_fernet_secret(secret: &str)`
- `PythonFernetCompat::from_secret(secret: &str) -> Self`

ABCoder parsed these functions from `src/python_fernet.rs` at lines 111, 116,
129, 217, 221, 228, 244, and 251. Keep new public helpers similarly explicit
and easy to grep.

## Internal Organization Inside `python_fernet.rs`

The file is organized in this order:

1. Standard library imports, then third-party imports.
2. Wire-format constants and exported development constants.
3. Static cache and derived salt initialization.
4. Type aliases for AES-CBC and HMAC primitives.
5. Small helper functions for base64 length math.
6. Private cache struct and methods.
7. Public error enum.
8. Public compatibility struct and methods.
9. Public facade functions.
10. Private key decoding and base64 helpers.
11. `#[cfg(test)] mod tests`.

Keep new code in the nearest existing section. For example, a new decoding
helper belongs beside `decode_urlsafe` and `decode_with_engine_fallback`, not
near the public re-export block.

## Dependency Direction

Callers live above this crate. `aether-provider-transport` decrypts provider
catalog secrets through the public helpers:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:1
use aether_crypto::{decrypt_python_fernet_ciphertext, looks_like_python_fernet_ciphertext};

// crates/aether-provider-transport/src/snapshot_mapping.rs:133
match decrypt_python_fernet_ciphertext(encryption_key, ciphertext) {
```

`aether-gateway` warms the derived-key cache during startup configuration:

```rust
// apps/aether-gateway/src/main.rs:11
use aether_crypto::warm_python_fernet_secret;

// apps/aether-gateway/src/main.rs:417
match self.effective_encryption_key() {
    Some(value) => {
        warm_python_fernet_secret(&value);
```

Do not add imports from these higher-layer crates back into `aether-crypto`.
If a higher-layer concern seems needed here, put the adapter in the higher
layer and keep this crate focused on bytes, strings, keys, and errors.

## Naming Conventions

Use `python_fernet` for the module and `PythonFernet*` for public types. This
disambiguates the legacy Python-compatible double-wrapped Fernet format from
generic Rust crypto or standard Fernet libraries.

Use protocol constants in uppercase:

```rust
// crates/aether-crypto/src/python_fernet.rs:15
const FERNET_VERSION: u8 = 0x80;
const HMAC_SIZE: usize = 32;
const IV_SIZE: usize = 16;
```

Use private type aliases to keep cryptographic primitive calls readable:

```rust
// crates/aether-crypto/src/python_fernet.rs:37
type Aes128CbcDec = Decryptor<aes::Aes128>;
type Aes128CbcEnc = Encryptor<aes::Aes128>;
type HmacSha256 = Hmac<Sha256>;
```

## Tests Live With The Module

The crate keeps unit tests in `python_fernet.rs` because they need access to
private helpers such as `encrypt_token` for deterministic IV and timestamp
fixtures:

```rust
// crates/aether-crypto/src/python_fernet.rs:321
#[cfg(test)]
mod tests {
    use super::{
        decrypt_python_fernet_ciphertext, derive_python_fernet_key,
        encrypt_python_fernet_plaintext, looks_like_python_fernet_ciphertext,
        PythonFernetCompat, PythonFernetError, APP_SALT_HEX,
        DEVELOPMENT_ENCRYPTION_KEY,
    };
```

Add compatibility tests beside the implementation when they validate private
wire-format steps. Add higher-layer integration tests in the calling crate when
the behavior includes database records, provider snapshots, or HTTP handlers.

## Do Not Add

Do not add these to `aether-crypto`:

- Axum handlers or request/response types.
- SeaORM entities, repositories, or migrations.
- Redis, cache backends, or runtime state.
- Tokio tasks or async APIs.
- Logging side effects for secrets or plaintext.
- Provider-specific policy beyond Python Fernet compatibility.

If a change needs any of those, it belongs in a higher layer and should call
the public helpers from this crate.
