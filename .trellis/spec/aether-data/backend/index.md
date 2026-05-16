# aether-data Backend Guidelines

These guidelines describe the `crates/aether-data` runtime data-access crate.
They are based on the current source, the local ABCoder AST for
`repo_name="aether-data"`, and GitNexus resources for `repo="Aether"`.

## Scope

`aether-data` is not a generic data model crate. It owns concrete SQL drivers,
repository implementations, backend composition, migration/backfill/export
workflows, and schema maintenance glue.

The public DTOs and shared repository contracts that other crates compile
against mostly live in `aether-data-contracts`; `aether-data` re-exports only
the runtime-facing handles and implementation-specific modules needed by the
application.

Real source example:

```rust
// crates/aether-data/src/lib.rs:10
pub mod backend;
mod config;
mod database;
pub mod driver;
mod error;
pub mod lifecycle;
pub mod maintenance;
pub mod repository;
```

The crate root keeps `config`, `database`, and `error` private while exposing
typed handles through `pub use` lines such as `DataBackends`,
`DataLayerConfig`, `DatabaseDriver`, and `DataLayerError`
(`crates/aether-data/src/lib.rs:19`).

## Guide Index

| Guide | Use It For |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Where code belongs, how modules are layered, and which public paths are allowed. |
| [Database Guidelines](./database-guidelines.md) | SQL driver policy, pools, repositories, migrations, schema fragments, transactions, and leases. |
| [Error Handling](./error-handling.md) | `DataLayerError`, sqlx conversion helpers, validation errors, migration errors, and rollback behavior. |
| [Quality Guidelines](./quality-guidelines.md) | Visibility, contract boundaries, naming, tests, type conversions, and anti-patterns. |
| [Logging Guidelines](./logging-guidelines.md) | Current `tracing` usage, structured fields, log levels, and sensitive-data boundaries. |

## Pre-Development Checklist

1. Identify whether the change is a contract change or an implementation change.
   Contract types used outside this crate usually belong in
   `crates/aether-data-contracts`, not in `crates/aether-data`.
2. Pick the correct layer before editing: `driver/*` for pool primitives,
   `repository/<domain>/*` for request-path domain SQL, `backend/*` for
   composition and maintenance workflows, `lifecycle/*` for migrations,
   backfills, exports, and bootstrap.
3. For a new repository domain, implement the selected SQL drivers before
   wiring it through `DataReadRepositories` or `DataWriteRepositories`.
4. For table-shape changes, start with `crates/aether-data/schema/logical/*.toml`
   or the schema fragment workflow described in `schema/README.md`; do not edit
   executable baseline SQL independently.
5. Confirm whether Postgres-only behavior is intentional. Transactions and
   leases are currently Postgres-specific; normal read/write repositories are
   multi-driver.
6. Avoid restoring old top-level module paths such as `aether_data::postgres`
   or `aether_data::redis`; `tests/public_entrypoints.rs` bans them.

## Quality Check

Before calling data-layer work complete, verify the relevant slice:

1. Run focused tests for changed modules, for example
   `cargo test -p aether-data <test_name>` when touching repository or driver
   behavior.
2. Run schema checks when changing schema files:
   `bash crates/aether-data/schema/compose_schema.sh check`.
3. Run migration tests when touching migrations or schema fragments. At minimum,
   cover the migration test that matches the changed driver.
4. Check that MySQL and SQLite SQL do not gain Postgres-only syntax such as
   `jsonb`; this is enforced in `crates/aether-data/src/lifecycle/migrate/tests.rs:484`.
5. Check that exported crate paths remain grouped under `driver`, `lifecycle`,
   `backend`, `repository`, and root re-exports. The public entrypoint scanner
   treats old top-level paths as regressions (`crates/aether-data/tests/public_entrypoints.rs:5`).
6. Keep logs structured and low-noise. Lifecycle operations log progress;
   request-path repositories generally do not log per query.

## Current Architecture Facts

- GitNexus reports the Aether repo is indexed and exposes 3,140 files, 83,229
  symbols, and 300 execution flows for repo `Aether`.
- The local ABCoder AST for `aether-data` lists the source modules under
  `src/backend`, `src/driver`, `src/repository`, `src/lifecycle`, and the root
  modules `config.rs`, `database.rs`, `error.rs`, and `lib.rs`.
- ABCoder shows `DataBackends` depends on `DataLayerConfig`, `PostgresBackend`,
  `MysqlBackend`, `SqliteBackend`, `DataLeaseBackends`,
  `DataReadRepositories`, `DataTransactionBackends`, and
  `DataWriteRepositories`.
- ABCoder shows `PostgresTransactionRunner` is referenced by leases,
  settlement, usage, wallet, candidate repositories, and backend transaction
  wiring, so transaction behavior is a shared Postgres primitive.

## Non-Goals

- Do not turn this crate into a SeaORM layer; the current implementation is
  `sqlx`-based, with raw SQL constants and `QueryBuilder`.
- Do not move shared DTOs into this crate just because a repository uses them.
- Do not add a generic SQL abstraction to hide driver differences. The current
  policy is shared Rust behavior with driver-specific physical SQL.
- Do not introduce Redis APIs here. `DataLayerError` has a Redis variant in the
  shared contract, but `aether-data` currently has no Redis implementation files.
