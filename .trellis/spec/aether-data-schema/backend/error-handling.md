# Error Handling

> How `aether-data-schema` reports failures and keeps schema generation fail-loud.

---

## Overview

This crate uses a single domain error type, `SchemaError`, for all file IO,
TOML parse, generation write, and validation failures. It does not define HTTP
or API response shapes because it is a build-time and maintenance tool, not a
service endpoint.

The main rule is simple: if schema content, generated output, or required SQL
shape is wrong, return an error immediately and stop. The code should not try
to recover silently or skip broken files.

GitNexus context shows the error type is the hub for the whole crate. Every
public workflow function returns `Result<_, SchemaError>` except the CLI
`main`, which erases the type to `Box<dyn std::error::Error>` so clap can bubble
up failures directly.

---

## Error Types

### `SchemaError`

Defined in `crates/aether-data-schema/src/lib.rs:7`.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("failed to read schema file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse schema file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to write generated schema file {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("schema validation failed: {0}")]
    Validation(String),
}
```

The variants are intentionally concrete:

- `Read` captures the path and the IO error.
- `Parse` captures the file path and TOML parser error.
- `Write` captures the target path and the IO error.
- `Validation` stores a human readable schema rule failure.

This makes the failure actionable without needing a second log channel.

---

## Error Handling Patterns

### Propagate with `?` and `map_err`

`load_schema_sources` converts each IO and parse failure into `SchemaError`
before bubbling up. That keeps the low-level path detail intact.

Example from `crates/aether-data-schema/src/lib.rs:200`:

```rust
let mut paths = fs::read_dir(root.as_ref())
    .map_err(|source| SchemaError::Read {
        path: root.as_ref().to_path_buf(),
        source,
    })?
    .map(|entry| entry.map(|entry| entry.path()))
    .collect::<Result<Vec<_>, _>>()
    .map_err(|source| SchemaError::Read {
        path: root.as_ref().to_path_buf(),
        source,
    })?;
```

Later the same function wraps `fs::read_to_string` and `toml::from_str` with
path-aware variants:

```rust
let text = fs::read_to_string(&path).map_err(|source| SchemaError::Read {
    path: path.clone(),
    source,
})?;
let file: SchemaFile = toml::from_str(&text).map_err(|source| SchemaError::Parse {
    path: path.clone(),
    source,
})?;
```

### Validate in one place

`validate_schema` is the central rule gate. It rejects empty schemas, empty
tables, duplicate columns, duplicate constraint names, unknown columns, and
bad identifiers.

Example from `crates/aether-data-schema/src/lib.rs:266`:

```rust
if schema.tables.is_empty() {
    return Err(SchemaError::Validation(
        "logical schema must define at least one table".to_string(),
    ));
}
```

### Use `expect` only for proven invariants

There are a few `expect` calls where the code has already established the
invariant by construction. For example, `ordered_table_names` and the emitters
expect the table name to exist in the merged schema map. These should stay
paired with validation and source loading that guarantees the invariant.

Example from `crates/aether-data-schema/src/lib.rs:45`:

```rust
let left_table = self
    .tables
    .get(left)
    .expect("ordered table should exist in schema");
```

Do not replace these with silent `Option` fallbacks. That would hide internal
corruption.

### Return errors from the CLI

The binary does not invent its own error hierarchy. It lets the library surface
domain errors and uses a boxed error only as the top-level shell contract.

Example from `crates/aether-data-schema/src/bin/aether-schema.rs:46`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
```

---

## API Error Responses

Not applicable. This crate does not expose HTTP handlers or JSON error bodies.
Its user-facing surface is:

- `cargo run -p aether-data-schema --bin aether-schema -- generate`
- `cargo run -p aether-data-schema --bin aether-schema -- check`
- `cargo run -p aether-data-schema --bin aether-schema -- print --driver postgres`

Any failure should be shown as a plain CLI error or a failed test.

---

## Common Mistakes

### Do not swallow file path context

Bad:

```rust
// DON'T: this hides which file failed.
let text = fs::read_to_string(path)?;
```

Preferred:

```rust
let text = fs::read_to_string(&path).map_err(|source| SchemaError::Read {
    path: path.clone(),
    source,
})?;
```

### Do not downgrade schema validation to warnings

If a table references an unknown column, the crate should fail. The tests in
`crates/aether-data-schema/src/lib.rs:809` deliberately assert that behavior.

### Do not mix generated drift errors with runtime retries

If `check_generated_dir` finds a stale file, that is not a retryable condition.
It means the logical source and the generated output are out of sync.

### Do not invent API error envelopes

There is no `{"error": ...}` response format in this crate. If you add one, you
are probably editing the wrong layer.
