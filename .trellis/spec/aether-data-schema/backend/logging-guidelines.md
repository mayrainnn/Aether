# Logging Guidelines

> Logging and observability rules for `aether-data-schema`.

---

## Overview

`aether-data-schema` currently has no `tracing`, `log`, or metrics dependency.
That is intentional. The crate is a deterministic schema compiler and check
tool, not a long-running service. Normal diagnostics are carried through
`SchemaError`, CLI exit status, generated SQL output, and shell-script status
lines.

The practical rule is:

- Library code should stay silent.
- CLI `print` may write generated SQL to stdout.
- Shell orchestration may print short status lines.
- Failures should be returned as errors, not hidden behind logs.

This keeps generated output stable and makes CI checks easy to reason about.

---

## Current Logging Surface

### Library code

There are no logging macros in `src/lib.rs` or `src/dialect/*`. The library
reports failures through `Result<_, SchemaError>`.

Example from `crates/aether-data-schema/src/lib.rs:442`:

```rust
pub fn check_generated_dir(
    loaded: &LoadedSchema,
    output_root: impl AsRef<Path>,
) -> Result<(), SchemaError> {
    let output_root = output_root.as_ref();
    assert_file_contents(output_root.join("README.md"), &generated_readme())?;
    assert_generated_root_files(output_root)?;
```

### CLI code

The binary only prints generated SQL for `Command::Print`. It does not print
progress during `generate` or `check`.

Example from `crates/aether-data-schema/src/bin/aether-schema.rs:65`:

```rust
Command::Print { schema_dir, driver } => {
    let schema = load_schema_sources(schema_dir)?.schema;
    let output = match driver {
        Driver::Postgres => postgres::emit_schema(&schema),
        Driver::Mysql => mysql::emit_schema(&schema),
        Driver::Sqlite => sqlite::emit_schema(&schema),
    };
    print!("{output}");
}
```

### Shell orchestration

`crates/aether-data/schema/compose_schema.sh` prints short success lines after
checks and generated composition steps. This belongs in the orchestration
script, not in the library.

Example from `crates/aether-data/schema/compose_schema.sh:31`:

```bash
(cd "${root}/../.." && cargo run -q -p aether-data-schema --bin aether-schema -- check "${args[@]}")
printf 'ok generated logical schema\n'
```

---

## Log Levels

There are no current log levels in this crate.

If future work introduces logging, follow these boundaries:

- `error`: avoid in the library. Return `SchemaError` instead. A CLI wrapper may
  let the process error display it.
- `warn`: avoid for validation. Validation failures must be errors.
- `info`: acceptable only in CLI orchestration for explicit user-facing
  progress, never in generated SQL output.
- `debug` or `trace`: acceptable only behind explicit CLI flags if diagnosing
  schema compilation. Do not enable by default.

Do not add logs just because another Aether crate uses `tracing`. This crate has
a different shape.

---

## Structured Context

Structured context belongs in error variants and validation messages, not log
fields.

Example from `crates/aether-data-schema/src/lib.rs:19`:

```rust
#[error("failed to write generated schema file {path}: {source}")]
Write {
    path: PathBuf,
    source: std::io::Error,
},
```

Example from `crates/aether-data-schema/src/lib.rs:461`:

```rust
return Err(SchemaError::Validation(format!(
    "logical schema is missing definitions required by {}: {}",
    path.display(),
    details.join("; ")
)));
```

Use the same style for future diagnostics: include the path, table, column,
driver, or generated file that makes the error actionable.

---

## What to Log

By default, nothing in library code.

For future CLI-only work, acceptable user-facing messages are:

- Which schema directory is being compiled, if a verbose flag exists.
- Which output directory is being checked, if a verbose flag exists.
- A final success line in a shell wrapper.

These should not appear in emitted SQL. `aether-schema print` must remain a clean
stdout stream so users can redirect it to a file.

---

## What Not to Log

Do not log:

- Full schema files on validation failure.
- Generated SQL in `generate` or `check` mode.
- Secrets from future schema fields.
- Environment variables.
- Database URLs.
- Auth tokens.
- User PII from logical table examples.

The current logical schema contains identity, API key, OAuth, wallet, payment,
usage, and task tables. Even though this crate processes definitions rather than
runtime rows, generated SQL can reveal domain shape. Keep output intentional.

---

## Anti-Patterns

Bad:

```rust
// DON'T: library code should not print progress while generating files.
println!("writing {}", path.display());
```

Bad:

```rust
// DON'T: validation must fail, not warn and continue.
tracing::warn!(table = table_name, "unknown column ignored");
```

Preferred:

```rust
return Err(SchemaError::Validation(format!(
    "table {table_name} {context} references unknown column {column_name}"
)));
```

This preferred pattern already exists in
`crates/aether-data-schema/src/lib.rs:376`.

---

## Review Checklist

Before accepting logging-related changes:

- Confirm generated SQL output is byte-for-byte unchanged unless the feature
  intentionally changes schema output.
- Confirm `aether-schema print` writes only SQL to stdout.
- Confirm validation problems are returned as errors.
- Confirm any new progress output is CLI-only and opt-in unless it is a shell
  wrapper success line.
- Confirm no sensitive values are printed.
