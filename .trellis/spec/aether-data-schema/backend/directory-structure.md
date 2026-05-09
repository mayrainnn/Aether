# Directory Structure

> How `crates/aether-data-schema` is organized and where new schema compiler code belongs.

---

## Overview

`aether-data-schema` is a small Rust foundation crate. It is not a SeaORM entity
crate and it does not execute runtime queries. Its job is to compile Aether's
logical TOML schema definitions into driver-specific SQL fragments, then fail
loudly when generated artifacts drift from the logical source of truth.

GitNexus maps the crate as six Rust files plus the package metadata, with no
runtime execution process attached to request handling. ABCoder's UniAST graph
confirms the public graph is centered on `SchemaError`, `LogicalSchema`,
`load_schema_sources`, `generate_sources_to_dir`, `check_generated_dir`, and the
three `dialect::*::emit_named_schema` functions.

The crate owns:

- Logical schema data types and validation.
- TOML loading from `crates/aether-data/schema/logical/*.toml`.
- SQL emission for Postgres, MySQL, and SQLite.
- Generated schema drift checks.
- A thin CLI binary called `aether-schema`.

It does not own:

- Runtime migrations or repository implementations.
- Database connections, transactions, or pools.
- HTTP handlers, service state, or request logging.
- SeaORM entities.

---

## Directory Layout

```text
crates/aether-data-schema/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs
    ├── bin/
    │   └── aether-schema.rs
    └── dialect/
        ├── mod.rs
        ├── mysql.rs
        ├── postgres.rs
        └── sqlite.rs
```

The source tree is intentionally flat. Keep it that way unless a new concern
cannot fit cleanly into one of the existing files.

---

## Module Responsibilities

### `src/lib.rs`

`lib.rs` is the canonical logical schema model and workflow module. It defines
the public schema types, validates them, loads TOML sources, writes generated
schema fragments, checks drift, and contains the crate's tests.

Example from `crates/aether-data-schema/src/lib.rs:7`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("failed to read schema file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("schema validation failed: {0}")]
    Validation(String),
}
```

Example from `crates/aether-data-schema/src/lib.rs:73`:

```rust
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Table {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub columns: Vec<Column>,
}
```

Keep public logical schema structs in `lib.rs` so callers and tests can see one
type surface. Keep helper parsing functions private unless another crate has a
real need to call them.

### `src/dialect/mod.rs`

`dialect/mod.rs` owns shared SQL helper functions and logical type mapping. It
also declares the three driver modules. Shared helpers belong here only when at
least two driver emitters use the same behavior.

Example from `crates/aether-data-schema/src/dialect/mod.rs:50`:

```rust
fn column_default<'a>(
    column: &'a Column,
    override_: Option<&'a DriverColumnOverride>,
) -> Option<&'a DefaultValue> {
    override_
        .and_then(|override_| override_.default.as_ref())
        .or(column.default.as_ref())
}
```

### `src/dialect/postgres.rs`

Postgres emission keeps table creation, constraints, indexes, and foreign keys
as separate DDL statements under the `public` schema.

Example from `crates/aether-data-schema/src/dialect/postgres.rs:11`:

```rust
pub fn emit_named_schema(schema: &LogicalSchema, table_names: &[String]) -> String {
    let mut out = String::new();
    for table_name in table_names {
        let table = schema
            .tables
            .get(table_name)
            .expect("named schema table should exist");
```

### `src/dialect/mysql.rs`

MySQL emission builds a single `CREATE TABLE` definition list. It quotes
reserved table names such as `date` and `usage` with backticks.

Example from `crates/aether-data-schema/src/dialect/mysql.rs:111`:

```rust
fn needs_quoting(identifier: &str) -> bool {
    matches!(identifier, "date" | "usage")
}
```

### `src/dialect/sqlite.rs`

SQLite emission keeps indexes outside `CREATE TABLE`, handles single-column
primary keys inline, and quotes reserved table names with double quotes.

Example from `crates/aether-data-schema/src/dialect/sqlite.rs:20`:

```rust
if table.primary_key.len() == 1 && table.primary_key[0] == column.name {
    definition.push_str(" PRIMARY KEY");
    if column.auto_increment {
        definition.push_str(" AUTOINCREMENT");
    }
}
```

### `src/bin/aether-schema.rs`

The binary is a thin clap entrypoint. It should parse CLI arguments, call the
library workflow functions, and print generated SQL for the `print` command. Do
not move validation or SQL emission logic into the binary.

Example from `crates/aether-data-schema/src/bin/aether-schema.rs:46`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate {
            schema_dir,
            output_dir,
        } => {
            let loaded = load_schema_sources(schema_dir)?;
            generate_loaded_to_dir(&loaded, output_dir)?;
        }
```

---

## Data Flow

The normal workflow is:

1. Read sorted `*.toml` files from a schema directory.
2. Deserialize each file into `SchemaFile`.
3. Merge all tables into one `LogicalSchema`.
4. Validate identifiers, duplicate names, references, and required columns.
5. Emit one generated SQL file per logical source for each driver.
6. Write per-driver `manifest.txt` files and a generated README.
7. In check mode, compare every generated file byte-for-byte.

Example from `crates/aether-data-schema/src/lib.rs:200`:

```rust
pub fn load_schema_sources(root: impl AsRef<Path>) -> Result<LoadedSchema, SchemaError> {
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
    paths.sort();
    paths.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"));
```

---

## Organization Rules

- Put schema model types in `src/lib.rs`.
- Put driver-specific DDL syntax in `src/dialect/{postgres,mysql,sqlite}.rs`.
- Put shared type/default/nullability helpers in `src/dialect/mod.rs`.
- Keep CLI code in `src/bin/aether-schema.rs` as orchestration only.
- Keep generated output under `crates/aether-data/schema/generated/**`, not in
  this crate's `src/` tree.
- Keep executable migrations under `crates/aether-data/migrations/**`.
- Keep logical schema source under `crates/aether-data/schema/logical/*.toml`.

---

## Naming Conventions

- Rust files and modules use lowercase snake case.
- Public types use PascalCase: `LogicalSchema`, `SchemaSource`,
  `DriverColumnOverride`.
- Public workflow functions are verbs: `load_schema_sources`,
  `generate_loaded_to_dir`, `check_generated_dir`.
- Logical TOML files use ordered numeric prefixes:
  `001_identity.toml`, `002_provider_catalog.toml`, and so on.
- Logical identifiers must be lowercase snake case, enforced by
  `validate_identifier` in `src/lib.rs:391`.

Example from `crates/aether-data-schema/src/lib.rs:391`:

```rust
fn validate_identifier(value: &str, kind: &str, table_name: &str) -> Result<(), SchemaError> {
    let mut chars = value.chars();
    let valid = matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch == '_')
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
```

---

## Adding New Code

When adding a new logical schema feature:

1. Add the shape to the typed model in `src/lib.rs`.
2. Add validation in `validate_schema` or a private helper.
3. Add driver-specific emission in all three driver files.
4. Add tests in the existing `#[cfg(test)] mod tests` block.
5. Run `cargo test -p aether-data-schema`.
6. Run `bash crates/aether-data/schema/compose_schema.sh check`.

When adding a new driver:

1. Add a new `src/dialect/<driver>.rs`.
2. Add `pub mod <driver>;` to `src/dialect/mod.rs`.
3. Extend `generate_sources_to_dir` and `check_generated_dir`.
4. Extend the `Driver` enum in `src/bin/aether-schema.rs`.
5. Add driver output directory expectations in generated root checks.

Do not add a partially wired driver. Generated output and check mode must stay
symmetrical.

---

## Anti-Patterns

Do not put runtime data access in this crate:

```rust
// DON'T: aether-data-schema must not open runtime database connections.
let pool = sqlx::PgPool::connect(database_url).await?;
```

Do not put driver-specific syntax in `src/lib.rs` when it belongs in a dialect
module:

```rust
// DON'T: lib.rs should validate logical data, not branch on every SQL driver.
if driver == "postgres" {
    out.push_str("jsonb");
}
```

Do not hand edit generated SQL:

```text
crates/aether-data/schema/generated/postgres/baseline/*.sql
```

Those files are compiler output and drift fixtures. Edit
`crates/aether-data/schema/logical/*.toml`, then regenerate.
