# Database Guidelines

> Database schema conventions for the `aether-data-schema` logical schema compiler.

---

## Overview

This crate is database-related, but it does not connect to a database. It is a
logical schema compiler that reads typed TOML definitions and emits
driver-specific DDL for Postgres, MySQL, and SQLite.

The project-specific correction is important: despite the generic PRD text,
`aether-data-schema` does not use SeaORM and does not define ORM entities. It is
closer to a small schema compiler and drift checker for `crates/aether-data`.

Current source of truth:

- Input logical schema: `crates/aether-data/schema/logical/*.toml`
- Generated audit output: `crates/aether-data/schema/generated/**`
- Executable migrations: `crates/aether-data/migrations/{postgres,mysql,sqlite}`
- Orchestration script: `crates/aether-data/schema/compose_schema.sh`

---

## Logical Schema Format

Each logical TOML file declares one or more `[table.<name>]` sections. Tables
use explicit columns, primary keys, unique constraints, indexes, and foreign
keys.

Example from `crates/aether-data/schema/logical/001_identity.toml:1`:

```toml
[table.users]
domain = "identity"
order = 10
primary_key = ["id"]

[[table.users.columns]]
name = "id"
type = "text_id"
length = 64
```

Logical tables should include:

- `domain` for grouping.
- `order` for deterministic generated output order.
- `primary_key` when the table has one.
- `columns` with logical `type`, `nullable`, `default`, and `length`.
- `uniques`, `indexes`, and `foreign_keys` when needed.

Do not encode driver-specific SQL directly unless the typed logical model cannot
represent it. Use `driver` overrides for narrow differences.

---

## Query Patterns

There are no query patterns in this crate. It does not use `sqlx`, SeaORM,
Diesel, Redis, or connection pools.

The only "database" operations are string generation and SQL shape parsing.

Example from `crates/aether-data-schema/src/dialect/postgres.rs:11`:

```rust
pub fn emit_named_schema(schema: &LogicalSchema, table_names: &[String]) -> String {
    let mut out = String::new();
    for table_name in table_names {
        let table = schema
            .tables
            .get(table_name)
            .expect("named schema table should exist");
        out.push_str(&format!(
            "CREATE TABLE IF NOT EXISTS public.{table_name} (\n"
        ));
```

Example from `crates/aether-data-schema/src/lib.rs:514`:

```rust
pub fn extract_table_shapes(sql: &str) -> BTreeMap<String, BTreeSet<String>> {
    const PREFIX: &str = "CREATE TABLE IF NOT EXISTS ";
    let mut tables = BTreeMap::<String, BTreeSet<String>>::new();
```

If you need real runtime queries, edit `crates/aether-data`, not this crate.

---

## Driver Type Mapping

Logical types are defined once in `LogicalType` and mapped per driver.

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

Driver mappings live in `src/dialect/mod.rs`:

- Postgres maps `Json` to `jsonb`, `Bytes` to `bytea`, and timestamp to
  `timestamp with time zone`.
- MySQL maps `Json` to `JSON`, `Bytes` to `LONGBLOB`, and timestamp to `BIGINT`.
- SQLite maps JSON to `TEXT`, bytes to `BLOB`, and most integer-like values to
  `INTEGER`.

Example from `crates/aether-data-schema/src/dialect/mod.rs:69`:

```rust
fn postgres_type(column: &Column) -> String {
    let override_ = column.driver.postgres.as_ref();
    if let Some(sql_type) = override_type(override_) {
        return sql_type.to_string();
    }
    if column.auto_increment {
        return "bigserial".to_string();
    }
```

When adding a logical type, update all three mapping functions and add tests
that prove each driver emits the intended SQL.

---

## Migrations

Generated schema files are not runtime migrations. The README in this crate says
runtime migrations remain under `crates/aether-data/migrations/**`.

Use the schema tool through the wrapper script:

```bash
bash crates/aether-data/schema/compose_schema.sh generate
bash crates/aether-data/schema/compose_schema.sh check
```

`compose_schema.sh` wires generated logical schema checks to the existing
migration directories.

Example from `crates/aether-data/schema/compose_schema.sh:24`:

```bash
check_logical_generated() {
  local args=()
  local path
  args+=(--require-tables-from "${root}/migrations/postgres/20260403000000_baseline.sql")
  for path in "${root}/migrations/mysql/"*.sql "${root}/migrations/sqlite/"*.sql; do
    [[ -f "${path}" ]] || continue
    args+=(--require-tables-from "${path}")
  done
  (cd "${root}/../.." && cargo run -q -p aether-data-schema --bin aether-schema -- check "${args[@]}")
  printf 'ok generated logical schema\n'
}
```

Do not promote generated SQL into executable migrations without an explicit
design decision in `crates/aether-data`.

---

## Naming Conventions

Logical identifiers must be lowercase snake case. `validate_identifier` enforces
this for columns, indexes, constraints, and references.

Example from `crates/aether-data-schema/src/lib.rs:391`:

```rust
fn validate_identifier(value: &str, kind: &str, table_name: &str) -> Result<(), SchemaError> {
    let mut chars = value.chars();
    let valid = matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch == '_')
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
```

Use these conventions:

- Table names: plural snake case, for example `users`, `api_keys`,
  `stats_daily`.
- Column names: snake case, for example `external_id`, `created_at`.
- Unique constraints: `<table>_<columns>_key` when matching existing style.
- Indexes: `<table>_<column_or_purpose>_idx`.
- Foreign keys: descriptive names that remain unique per table.

The validator currently enforces duplicate constraint/index names within a
table, not global uniqueness. Avoid duplicate names globally anyway because
drivers can have different namespace rules.

---

## Driver-Specific Quoting

Only quote identifiers where the driver requires it. MySQL and SQLite currently
quote reserved table names `date` and `usage`. Postgres output qualifies tables
with `public.` but does not quote identifiers.

Example from `crates/aether-data-schema/src/dialect/mysql.rs:111`:

```rust
fn needs_quoting(identifier: &str) -> bool {
    matches!(identifier, "date" | "usage")
}
```

Example from `crates/aether-data-schema/src/dialect/sqlite.rs:90`:

```rust
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
```

Do not blanket-quote every identifier unless all generated fixtures and
migration comparisons are updated intentionally.

---

## Transactions And Connections

Not applicable. There are no transactions or database connections in this
crate. If a future task asks for connection handling, it belongs in
`crates/aether-data` or another runtime layer.

---

## Common Mistakes

### Treating generated SQL as the source of truth

Bad:

```text
Edit crates/aether-data/schema/generated/mysql/baseline/001_identity.sql by hand.
```

Preferred:

```text
Edit crates/aether-data/schema/logical/001_identity.toml, then run generate.
```

### Adding a column to migrations but not logical schema

`check_required_tables` compares required executable SQL shape against the
logical schema. If a table or column exists in required SQL but not logical TOML,
the check must fail.

Example from `crates/aether-data-schema/src/lib.rs:461`:

```rust
pub fn check_required_tables(
    schema: &LogicalSchema,
    sql_paths: &[PathBuf],
) -> Result<(), SchemaError> {
    for path in sql_paths {
        let text = fs::read_to_string(path).map_err(|source| SchemaError::Read {
            path: path.clone(),
            source,
        })?;
```

### Adding a driver override without tests

Driver overrides affect generated DDL. Add tests around the expected emitted
type/default/nullability behavior before relying on them.

### Assuming Postgres, MySQL, and SQLite share DDL shape

They do not. Postgres emits constraints and indexes after table creation. MySQL
puts most definitions inside `CREATE TABLE`. SQLite emits indexes after table
creation and handles single-column primary keys inline.
