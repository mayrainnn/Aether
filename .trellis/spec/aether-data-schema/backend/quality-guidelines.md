# Quality Guidelines

> Code quality standards for the `aether-data-schema` logical schema compiler.

---

## Overview

Quality in this crate means deterministic, auditable schema output. The same
logical TOML inputs must produce the same generated SQL on every machine, and
check mode must fail when any generated file, manifest, or required SQL shape
drifts.

The crate is intentionally narrow:

- No runtime database access.
- No HTTP framework.
- No global mutable state.
- No broad internal dependency chain.
- No new dependencies unless the generator cannot stay correct without them.

ABCoder's graph for `repo-id=aether-data-schema` shows the core symbols are
plain Rust functions and data types, not service objects. Keep future changes
similarly direct.

---

## Required Patterns

### Use deterministic collections for public output

Schema tables and parsed SQL shapes use `BTreeMap` and `BTreeSet` so output and
error reporting stay stable.

Example from `crates/aether-data-schema/src/lib.rs:28`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalSchema {
    pub tables: BTreeMap<String, Table>,
}
```

Example from `crates/aether-data-schema/src/lib.rs:514`:

```rust
pub fn extract_table_shapes(sql: &str) -> BTreeMap<String, BTreeSet<String>> {
    const PREFIX: &str = "CREATE TABLE IF NOT EXISTS ";
    let mut tables = BTreeMap::<String, BTreeSet<String>>::new();
```

### Sort schema files and table names

`load_schema_sources` sorts logical TOML paths before reading. `LogicalSchema`
sorts table names by optional `order`, then by name. Do not rely on filesystem
iteration order.

Example from `crates/aether-data-schema/src/lib.rs:45`:

```rust
pub fn ordered_table_names(&self) -> Vec<String> {
    let mut names = self.tables.keys().cloned().collect::<Vec<_>>();
    names.sort_by(|left, right| {
        let left_table = self
            .tables
            .get(left)
            .expect("ordered table should exist in schema");
        let right_table = self
            .tables
            .get(right)
            .expect("ordered table should exist in schema");
        left_table
            .order
            .unwrap_or(u32::MAX)
            .cmp(&right_table.order.unwrap_or(u32::MAX))
            .then_with(|| left.cmp(right))
    });
    names
}
```

### Keep the logical model strongly typed

The schema format is typed through Rust structs and enums, not loose
`serde_json::Value` maps. Unknown TOML fields are denied for table, column,
constraint, and override objects.

Example from `crates/aether-data-schema/src/lib.rs:92`:

```rust
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub logical_type: LogicalType,
    #[serde(default)]
    pub nullable: bool,
}
```

Example from `crates/aether-data-schema/src/lib.rs:110`:

```rust
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogicalType {
    TextId,
    Text,
    LongText,
    Bool,
    Int32,
    Int64,
    Float64,
    DecimalMoney,
    UnixSeconds,
    UnixMillis,
    Timestamp,
    Json,
    Bytes,
}
```

### Keep generation and check symmetrical

Any new generated artifact must be written in generate mode and compared in
check mode. `generate_sources_to_dir` and `check_generated_dir` currently list
the same drivers in the same order.

Example from `crates/aether-data-schema/src/lib.rs:423`:

```rust
pub fn generate_sources_to_dir(
    schema: &LogicalSchema,
    sources: &[SchemaSource],
    output_root: impl AsRef<Path>,
) -> Result<(), SchemaError> {
    let output_root = output_root.as_ref();
    write_generated_readme(output_root)?;
    write_driver_sources(output_root, "postgres", sources, |tables| {
        dialect::postgres::emit_named_schema(schema, tables)
    })?;
```

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

---

## Forbidden Patterns

### Do not use unordered maps for generated order

Bad:

```rust
// DON'T: HashMap order would make generated SQL unstable.
let mut tables = std::collections::HashMap::new();
```

Use `BTreeMap` or sort explicitly.

### Do not add runtime DB access

Bad:

```rust
// DON'T: this crate is a schema compiler, not a repository layer.
let rows = sqlx::query!("SELECT * FROM users").fetch_all(pool).await?;
```

Runtime migrations and repositories live in `crates/aether-data`, not
`crates/aether-data-schema`.

### Do not skip unknown TOML fields

Bad:

```rust
// DON'T: loose maps let misspelled schema fields pass silently.
type Table = std::collections::BTreeMap<String, toml::Value>;
```

Use the existing typed structs with `#[serde(deny_unknown_fields)]`.

### Do not make check mode best-effort

Bad:

```rust
// DON'T: stale generated files must fail the check.
if actual != expected {
    eprintln!("warning: stale schema");
}
```

Use `SchemaError::Validation` so CI and local scripts fail.

---

## Testing Requirements

Run at least this command for changes in this crate:

```bash
cargo test -p aether-data-schema
```

For changes touching logical schema or generated artifacts, also run:

```bash
bash crates/aether-data/schema/compose_schema.sh check
```

Existing tests live in `crates/aether-data-schema/src/lib.rs:809` and cover:

- Invalid index columns.
- Driver-specific type emission.
- Quoted table parsing across Postgres, MySQL, and SQLite.
- `CREATE TABLE` plus `ALTER TABLE ADD COLUMN` shape extraction.
- Required table and column coverage checks.
- Workspace generated artifact drift.
- Workspace logical schema coverage for required SQL tables.

Example from `crates/aether-data-schema/src/lib.rs:842`:

```rust
#[test]
fn validator_rejects_unknown_index_column() {
    let mut schema = announcements_schema();
    schema
        .tables
        .get_mut("announcements")
        .expect("fixture table exists")
        .indexes
        .push(Index {
            name: "ix_missing".to_string(),
            columns: vec!["missing".to_string()],
            unique: false,
        });

    let err = validate_schema(&schema).expect_err("invalid schema should fail");
    assert!(err.to_string().contains("unknown column missing"));
}
```

---

## Code Review Checklist

Reviewers should check:

- New logical fields are represented in typed structs and validated.
- Each new schema behavior has all three driver outputs updated.
- Generate mode and check mode stay symmetrical.
- Output order is deterministic.
- Errors include paths or table/column names.
- Tests cover at least one failure path and one output path.
- Generated files were regenerated only through the schema tool.
- No runtime DB dependency, HTTP dependency, or background service code entered
  this crate.

---

## Local Quality Commands

Use these commands from the workspace root:

```bash
cargo fmt --check -p aether-data-schema
cargo test -p aether-data-schema
bash crates/aether-data/schema/compose_schema.sh check
```

If `cargo fmt --check -p` is not supported by the local Cargo version, run:

```bash
cargo fmt --check
```

The check should be scoped in reporting even if the command formats the full
workspace.
