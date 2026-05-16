# Backend Development Guidelines

> Project-specific guidelines for the `aether-data-schema` Rust crate.

---

## Scope

This directory documents backend conventions for:

```text
crates/aether-data-schema/
```

The crate is a logical schema compiler for Aether's data storage layer. It reads
typed TOML definitions, validates them, emits driver-specific SQL for Postgres,
MySQL, and SQLite, and checks generated artifacts for drift.

It is not:

- A SeaORM entity crate.
- A runtime migration executor.
- A repository or query layer.
- An HTTP or admin service.
- A logging or observability crate.

---

## Source Evidence

These guidelines were filled from the current source, GitNexus graph data, and
ABCoder UniAST output for `repo-id=aether-data-schema`.

Key source files:

- `crates/aether-data-schema/src/lib.rs`
- `crates/aether-data-schema/src/bin/aether-schema.rs`
- `crates/aether-data-schema/src/dialect/mod.rs`
- `crates/aether-data-schema/src/dialect/postgres.rs`
- `crates/aether-data-schema/src/dialect/mysql.rs`
- `crates/aether-data-schema/src/dialect/sqlite.rs`
- `crates/aether-data/schema/logical/*.toml`
- `crates/aether-data/schema/compose_schema.sh`

GitNexus showed the crate has no runtime request execution flow attached to the
indexed process graph. Its main symbols are local schema/compiler symbols:
`SchemaError`, `LogicalSchema`, `load_schema_sources`,
`generate_sources_to_dir`, `check_generated_dir`, `check_required_tables`, and
the three driver `emit_named_schema` functions.

ABCoder confirmed the same symbol surface from the Rust AST and showed the CLI
binary depends on the library workflow functions rather than owning schema
logic itself.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Actual module layout, ownership, data flow, naming, and where new code belongs | Filled |
| [Database Guidelines](./database-guidelines.md) | Logical schema format, driver DDL emission, migration boundary, naming, and drift checks | Filled |
| [Error Handling](./error-handling.md) | `SchemaError`, fail-loud validation, CLI propagation, and non-API error surface | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Deterministic output, typed schema model, forbidden patterns, testing, and review checklist | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Current no-logging policy, CLI stdout rules, and future logging boundaries | Filled |

---

## Core Rules

1. Keep this crate deterministic.
2. Keep logical schema definitions typed.
3. Keep library code silent and return errors.
4. Keep driver emitters symmetrical across generate and check mode.
5. Keep runtime database work outside this crate.
6. Keep generated SQL as compiler output, not as hand-edited source.
7. Keep validation fail-loud.

---

## Normal Development Loop

For schema compiler changes:

```bash
cargo test -p aether-data-schema
```

For logical schema or generated output changes:

```bash
bash crates/aether-data/schema/compose_schema.sh generate
bash crates/aether-data/schema/compose_schema.sh check
```

For review, inspect both logical source and generated output:

```text
crates/aether-data/schema/logical/*.toml
crates/aether-data/schema/generated/{postgres,mysql,sqlite}/baseline/
```

---

## When To Edit Which File

Edit `src/lib.rs` when:

- Adding or changing a logical schema type.
- Adding validation.
- Changing generated file writing or checking behavior.
- Extending required table/column coverage checks.

Edit `src/dialect/mod.rs` when:

- Adding a shared helper used by multiple drivers.
- Adding or changing logical type mappings.
- Changing default or nullability override behavior.

Edit `src/dialect/postgres.rs` when:

- Changing Postgres DDL layout.
- Changing `public.` qualification.
- Changing Postgres constraint, index, or foreign key emission.

Edit `src/dialect/mysql.rs` when:

- Changing MySQL type mapping behavior.
- Changing backtick quoting behavior.
- Changing inline `CREATE TABLE` definition generation.

Edit `src/dialect/sqlite.rs` when:

- Changing SQLite type mapping behavior.
- Changing inline primary key behavior.
- Changing SQLite index emission or double-quote escaping.

Edit `src/bin/aether-schema.rs` when:

- Adding CLI flags or subcommands.
- Wiring an existing library function into a command.

Do not put core schema compiler logic in the binary.

---

## Project-Specific Anti-Patterns

Do not add SeaORM entities here:

```rust
// DON'T: this crate does not own runtime entity models.
#[derive(DeriveEntityModel)]
pub struct Model {
    pub id: String,
}
```

Do not connect to a database here:

```rust
// DON'T: runtime query code belongs in crates/aether-data.
let pool = sqlx::PgPool::connect(database_url).await?;
```

Do not print progress from library code:

```rust
// DON'T: generated output and checks must stay clean.
println!("generated schema");
```

Do not hand-edit generated files:

```text
crates/aether-data/schema/generated/**/*
```

---

## Review Expectations

Before accepting a change in this crate, verify:

- `cargo test -p aether-data-schema` passes.
- `compose_schema.sh check` passes if schema output is involved.
- Every new logical field has validation.
- Every driver emits consistent behavior or intentionally documented divergence.
- Errors identify the relevant path, table, column, or generated file.
- Generated file set and manifests remain deterministic.
- No placeholder documentation remains in this spec directory.

---

## Language

All documentation in this spec directory is written in English.
