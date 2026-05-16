# Error Handling

> Error handling conventions for the `aether-crypto` crate.

---

## Scope

`aether-crypto` exposes a single public error type, `PythonFernetError`, and
uses typed `Result` values at every fallible crypto boundary. It does not use
`anyhow`, does not log errors, and does not convert errors into HTTP responses.
Callers decide whether to map failures into data-layer, gateway, or test errors.

## Public Error Type

The error enum is defined with `thiserror` and has one variant for each
observable failure class in the Python Fernet compatibility format:

```rust
// crates/aether-crypto/src/python_fernet.rs:86
#[derive(Debug, thiserror::Error)]
pub enum PythonFernetError {
    #[error("invalid Python Fernet outer base64 payload")]
    InvalidOuterBase64,
    #[error("invalid Python Fernet inner base64 payload")]
    InvalidInnerBase64,
    #[error("invalid Python Fernet token structure")]
    InvalidTokenStructure,
    #[error("unsupported Python Fernet token version: {0:#x}")]
    UnsupportedTokenVersion(u8),
    #[error("invalid Python Fernet token signature")]
    InvalidTokenSignature,
    #[error("invalid Python Fernet token padding")]
    InvalidPadding,
    #[error("invalid Python Fernet plaintext utf-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}
```

Do not collapse these into string errors. The variants are the contract callers
and tests use to distinguish malformed wrapping, signature failure, padding
failure, and invalid plaintext encoding.

## Result Signatures

Public fallible helpers return `Result<_, PythonFernetError>` directly:

```rust
// crates/aether-crypto/src/python_fernet.rs:221
pub fn decrypt_python_fernet_ciphertext(
    secret: &str,
    ciphertext: &str,
) -> Result<String, PythonFernetError> {
    PythonFernetCompat::from_secret(secret).decrypt_ciphertext(ciphertext)
}

// crates/aether-crypto/src/python_fernet.rs:244
pub fn encrypt_python_fernet_plaintext(
    secret: &str,
    plaintext: &str,
) -> Result<String, PythonFernetError> {
    PythonFernetCompat::from_secret(secret).encrypt_plaintext(plaintext)
}
```

Keep new public functions equally narrow: return the crate error type, not
`GatewayError`, `DataLayerError`, `Box<dyn Error>`, or `anyhow::Error`.

## Empty Input Semantics

Decryption treats an empty ciphertext as an empty plaintext, not as an error:

```rust
// crates/aether-crypto/src/python_fernet.rs:116
pub fn decrypt_ciphertext(&self, ciphertext: &str) -> Result<String, PythonFernetError> {
    if ciphertext.is_empty() {
        return Ok(String::new());
    }
```

Preserve this behavior unless all consumers are migrated together. Higher
layers may trim before calling; this crate only checks the exact string it is
given in `decrypt_ciphertext`.

## Error Mapping Pattern

External library errors are mapped at the boundary where semantic information
is known:

```rust
// crates/aether-crypto/src/python_fernet.rs:121
let outer =
    decode_urlsafe(ciphertext).map_err(|_| PythonFernetError::InvalidOuterBase64)?;
let token =
    decode_urlsafe_bytes(&outer).map_err(|_| PythonFernetError::InvalidInnerBase64)?;
let plaintext = self.decrypt_token_bytes(token)?;
String::from_utf8(plaintext).map_err(PythonFernetError::InvalidUtf8)
```

The inner token validator also maps HMAC and padding failures to explicit
variants:

```rust
// crates/aether-crypto/src/python_fernet.rs:159
let mut mac = HmacSha256::new_from_slice(&self.signing_key)
    .map_err(|_| PythonFernetError::InvalidTokenSignature)?;
mac.update(signed);
mac.verify_slice(signature)
    .map_err(|_| PythonFernetError::InvalidTokenSignature)?;

// crates/aether-crypto/src/python_fernet.rs:172
Aes128CbcDec::new((&self.encryption_key).into(), (&iv[..]).into())
    .decrypt_padded_mut::<Pkcs7>(ciphertext)
    .map_err(|_| PythonFernetError::InvalidPadding)?
```

Use `?` after the mapping has converted the dependency error into
`PythonFernetError`. Do not leak `base64::DecodeError`, block-mode padding
errors, or HMAC verification errors from public APIs.

## Validation Order

Token validation is deliberately ordered:

1. Check minimum token size.
2. Check Fernet version byte.
3. Verify HMAC signature before decrypting.
4. Decrypt AES-CBC and validate PKCS7 padding.
5. Convert plaintext bytes to UTF-8.

Evidence:

```rust
// crates/aether-crypto/src/python_fernet.rs:148
fn decrypt_token_bytes(&self, mut token: Vec<u8>) -> Result<Vec<u8>, PythonFernetError> {
    if token.len() < MIN_TOKEN_SIZE {
        return Err(PythonFernetError::InvalidTokenStructure);
    }
    if token[0] != FERNET_VERSION {
        return Err(PythonFernetError::UnsupportedTokenVersion(token[0]));
    }
```

Do not decrypt before signature verification. That would change the security
boundary and could surface different failure classes for tampered tokens.

## Direct Key Decode Fallback

`raw_fernet_key` first accepts an existing 32-byte Fernet key. If direct decode
fails, it derives a key from the secret using PBKDF2:

```rust
// crates/aether-crypto/src/python_fernet.rs:271
let raw_key = match decode_direct_fernet_key(secret) {
    Ok(raw_key) => raw_key,
    Err(_) => {
        let mut raw_key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(
            secret.as_bytes(),
            &*APP_SALT,
            PBKDF2_ITERATIONS,
            &mut raw_key,
        );
        raw_key
    }
};
```

This fallback intentionally discards the direct-decode error. Do not surface
that error to callers unless the compatibility contract is redesigned.

## Caller Conversion Pattern

Callers convert `PythonFernetError` at their own layer. Provider snapshot
mapping wraps decrypt failures in `DataLayerError::UnexpectedValue` and includes
the field name:

```rust
// crates/aether-provider-transport/src/snapshot_mapping.rs:133
match decrypt_python_fernet_ciphertext(encryption_key, ciphertext) {
    Ok(value) => Ok(value),
    Err(error) => {
        for fallback_encryption_key in fallback_encryption_keys {
            if let Ok(value) =
                decrypt_python_fernet_ciphertext(fallback_encryption_key, ciphertext)
            {
                return Ok(value);
            }
        }
        Err(DataLayerError::UnexpectedValue(format!(
            "failed to decrypt {field_name}: {error}"
        )))
    }
}
```

Gateway callers map errors into `GatewayError::Internal` only after crossing
the application boundary:

```rust
// apps/aether-gateway/src/handlers/admin/system/shared/configs.rs:85
value = json!(encrypt_python_fernet_plaintext(encryption_key, plaintext)
    .map_err(|err| GatewayError::Internal(err.to_string()))?);
```

Keep that layering. `aether-crypto` should not know about HTTP status codes,
database error enums, or localized response bodies.

## Tests For Error Behavior

Error behavior is locked with unit tests. The tampered-signature test accepts
the small set of failure variants that can result after corrupting the double
base64 wrapped token:

```rust
// crates/aether-crypto/src/python_fernet.rs:403
let err = decrypt_python_fernet_ciphertext(DEVELOPMENT_ENCRYPTION_KEY, &ciphertext)
    .expect_err("tampered ciphertext should fail");
assert!(matches!(
    err,
    PythonFernetError::InvalidInnerBase64
        | PythonFernetError::InvalidTokenSignature
        | PythonFernetError::InvalidPadding
));
```

When adding a new failure mode, add a unit test that forces that exact variant.

## Common Mistakes

DON'T use `unwrap()` or `expect()` in production crypto paths for attacker- or
operator-controlled input. The only production `expect` calls in this crate are
for poisoned lock handling on the static key cache:

```rust
// crates/aether-crypto/src/python_fernet.rs:256
RAW_FERNET_KEY_CACHE
    .read()
    .expect("raw fernet key cache should lock")
```

DON'T log, print, or return plaintext secrets in error messages. Error messages
should describe the failure class, not include `secret`, `ciphertext`, or
`plaintext` values.

DON'T replace `PythonFernetError` with `String`. Callers rely on the enum in
tests and get better failure boundaries from typed variants.
