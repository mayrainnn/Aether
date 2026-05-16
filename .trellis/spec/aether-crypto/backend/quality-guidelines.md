# Quality Guidelines

> Code quality standards for the `aether-crypto` crate.

---

## Design Posture

`aether-crypto` is a small compatibility crate. Quality here means preserving
wire-format compatibility, keeping the public API narrow, and avoiding side
effects that could expose secrets or pull higher layers into a foundation crate.

The current package description states the intent:

```toml
# crates/aether-crypto/Cargo.toml:7
description = "Shared crypto compatibility helpers for Rust migration"
```

Treat the existing Python Fernet behavior as a migration contract. Changes must
be proven with deterministic vectors and round-trip tests.

## Visibility Rules

Only stable helpers and compatibility types are re-exported from `lib.rs`:

```rust
// crates/aether-crypto/src/lib.rs:3
pub use python_fernet::{
    decrypt_python_fernet_ciphertext, derive_python_fernet_key,
    encrypt_python_fernet_plaintext, looks_like_python_fernet_ciphertext,
    warm_python_fernet_secret, PythonFernetCompat, PythonFernetError,
    APP_SALT_HEX, APP_SALT_SEED, DEVELOPMENT_ENCRYPTION_KEY,
};
```

Keep internals private by default. Examples:

```rust
// crates/aether-crypto/src/python_fernet.rs:58
struct RawFernetKeyCache {
    entries: HashMap<Arc<str>, [u8; 32]>,
    insertion_order: VecDeque<Arc<str>>,
}

// crates/aether-crypto/src/python_fernet.rs:255
fn raw_fernet_key(secret: &str) -> [u8; 32] {
```

Do not make cache internals, primitive aliases, or token parsing helpers public
for test convenience. Tests are inside the module so they can exercise private
helpers without expanding the public API.

## Type Safety Rules

Use fixed-size arrays for keys and IVs once sizes are known:

```rust
// crates/aether-crypto/src/python_fernet.rs:104
#[derive(Debug, Clone)]
pub struct PythonFernetCompat {
    signing_key: [u8; SIGNING_KEY_SIZE],
    encryption_key: [u8; ENCRYPTION_KEY_SIZE],
}

// crates/aether-crypto/src/python_fernet.rs:183
fn encrypt_token(
    &self,
    plaintext: &str,
    timestamp: u64,
    iv: [u8; IV_SIZE],
) -> Result<String, PythonFernetError> {
```

This prevents callers and tests from passing incorrectly sized key material
after derivation. Keep boundary functions accepting `&str` for secrets and
ciphertexts because callers store these values as configuration strings.

## Allocation And Buffer Rules

Pre-size buffers when the encoded or signed size is known:

```rust
// crates/aether-crypto/src/python_fernet.rs:196
let mut signed = Vec::with_capacity(1 + 8 + IV_SIZE + ciphertext.len() + HMAC_SIZE);

// crates/aether-crypto/src/python_fernet.rs:208
let mut inner = String::with_capacity(base64_encoded_len(signed.len()));
URL_SAFE.encode_string(&signed, &mut inner);
```

Do not repeatedly concatenate strings for token construction. Use `Vec` and
`String::with_capacity` when the byte layout is predictable.

## Cache Rules

The derived-key cache is intentionally small and FIFO:

```rust
// crates/aether-crypto/src/python_fernet.rs:22
const MAX_CACHED_DERIVED_KEYS: usize = 16;

// crates/aether-crypto/src/python_fernet.rs:74
if self.entries.len() >= MAX_CACHED_DERIVED_KEYS {
    if let Some(oldest) = self.insertion_order.pop_front() {
        self.entries.remove(oldest.as_ref());
    }
}
```

Do not replace this with an unbounded global map. Secrets may be tenant- or
environment-specific, and the cache should not grow with request volume.

## Cryptographic Operation Rules

Preserve the existing Fernet byte layout:

```rust
// crates/aether-crypto/src/python_fernet.rs:196
let mut signed = Vec::with_capacity(1 + 8 + IV_SIZE + ciphertext.len() + HMAC_SIZE);
signed.push(FERNET_VERSION);
signed.extend_from_slice(&timestamp.to_be_bytes());
signed.extend_from_slice(&iv);
signed.extend_from_slice(ciphertext);
```

Verify HMAC before decryption:

```rust
// crates/aether-crypto/src/python_fernet.rs:159
let mut mac = HmacSha256::new_from_slice(&self.signing_key)
    .map_err(|_| PythonFernetError::InvalidTokenSignature)?;
mac.update(signed);
mac.verify_slice(signature)
    .map_err(|_| PythonFernetError::InvalidTokenSignature)?;
```

Do not reorder these steps. Any change to version, timestamp byte order, IV
length, signature location, wrapping, or padding must be treated as a breaking
compatibility change.

## Base64 Compatibility Rules

The crate accepts both padded and unpadded URL-safe base64 for decoded tokens:

```rust
// crates/aether-crypto/src/python_fernet.rs:309
fn decode_with_engine_fallback(value: &[u8]) -> Result<Vec<u8>, base64::DecodeError> {
    let mut decoded = Vec::with_capacity(decoded_len_estimate(value.len()));
    match URL_SAFE.decode_vec(value, &mut decoded) {
        Ok(()) => Ok(decoded),
        Err(_) => {
            decoded.clear();
            URL_SAFE_NO_PAD.decode_vec(value, &mut decoded)?;
            Ok(decoded)
        }
    }
}
```

Do not simplify this to a single engine unless legacy ciphertext samples prove
the fallback is no longer needed.

## Testing Requirements

Every behavior change needs unit tests in `src/python_fernet.rs`. The existing
tests cover:

- PBKDF2 derivation for `DEVELOPMENT_ENCRYPTION_KEY`.
- Direct Fernet key pass-through.
- Legacy Python ciphertext decryption.
- Unpadded direct-key fallback behavior.
- Deterministic outer-wrapped ciphertext round trip.
- Shape detection for encrypted and plaintext values.
- Tampered token rejection.
- Normal encrypt/decrypt round trip.

Example deterministic vector:

```rust
// crates/aether-crypto/src/python_fernet.rs:365
let crypto = PythonFernetCompat::from_secret(DEVELOPMENT_ENCRYPTION_KEY);
let ciphertext = crypto
    .encrypt_token(
        "{\"api_key\":\"sk-test\",\"provider\":\"openai\"}",
        1_710_000_000,
        *b"fixed-fernet-iv!",
    )
    .expect("ciphertext should build");
```

Use fixed timestamps and IVs for compatibility vectors. Use public
`encrypt_python_fernet_plaintext` only for round-trip and consumer tests where
non-deterministic IVs are acceptable.

## Caller Contract Tests

Higher layers already exercise the public helpers. Provider snapshot mapping
uses decrypt and shape detection:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:168
fn should_use_plaintext_secret(ciphertext: &str, field_name: &str) -> bool {
    let ciphertext = ciphertext.trim();
    if ciphertext.is_empty() {
        return false;
    }
```

System config writes encrypt sensitive values before storing them:

```rust
// apps/aether-gateway/src/handlers/admin/system/shared/configs.rs:72
if is_sensitive_admin_system_config_key(&normalized_key)
    && value.as_str().is_some_and(|raw| !raw.is_empty())
{
```

When changing public helper semantics, run the crate unit tests plus at least
the relevant caller tests or compile target. For this crate's own spec updates,
`cargo test -p aether-crypto` is the minimum verification.

## Forbidden Patterns

DON'T add new dependencies without a strong compatibility reason. This crate
currently depends only on crypto primitives, base64, `thiserror`, and `uuid`.

DON'T add async, Tokio, Axum, SeaORM, Redis, HTTP, or provider policy code here.
The public API should remain synchronous:

```rust
// crates/aether-crypto/src/python_fernet.rs:244
pub fn encrypt_python_fernet_plaintext(
    secret: &str,
    plaintext: &str,
) -> Result<String, PythonFernetError> {
```

DON'T log or print secrets. No `tracing`, `log`, `println!`, or `dbg!` calls
belong in this crate.

DON'T expose plaintext in errors. Use variants such as
`InvalidTokenSignature`, not messages containing the offending token.

DON'T remove the `looks_like_python_fernet_ciphertext` shape guard. Higher
layers use it to preserve plaintext legacy records for selected fields.

## Review Checklist

Before accepting changes to `aether-crypto`, verify:

- Public exports in `lib.rs` are intentional and minimal.
- Errors remain typed as `PythonFernetError`.
- HMAC verification still happens before AES-CBC decryption.
- Direct-key and PBKDF2 fallback behavior are both tested.
- Padded and unpadded base64 decoding remain covered.
- Tests do not rely on wall-clock timestamps unless they are pure round trips.
- No higher-layer crate imports were introduced.
- No secrets are logged, formatted into errors, or written to stdout.
