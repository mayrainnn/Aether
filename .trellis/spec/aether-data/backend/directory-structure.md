# Directory Structure

`crates/aether-data` is organized by runtime responsibility rather than by
database table. Keep new code in the layer that owns the concern.

## Top-Level Source Layout

```text
crates/aether-data/
  src/lib.rs
  src/config.rs
  src/database.rs
  src/error.rs
  src/driver/{postgres,mysql,sqlite}/
  src/repository/<domain>/
  src/backend/
  src/lifecycle/
  src/maintenance.rs
  migrations/{postgres,mysql,sqlite}/
  backfills/{postgres,mysql,sqlite}/
  schema/
```

The crate root exposes a small public surface and keeps implementation modules
private where possible:

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

`config`, `database`, and `error` are private modules because callers use the
re-exported types (`DataLayerConfig`, `DatabaseDriver`, `DataLayerError`) rather
than module paths.

## Driver Layer

`src/driver/*` contains low-level infrastructure primitives only:

- `driver/postgres/pool.rs` owns `PostgresPoolConfig`, `PostgresPoolFactory`,
  and `PostgresPool`.
- `driver/mysql/pool.rs` owns `MysqlPoolFactory` over `SqlDatabaseConfig`.
- `driver/sqlite/pool.rs` owns `SqlitePoolFactory`, parent directory creation,
  `foreign_keys(true)`, and WAL setup for file databases.
- `driver/postgres/tx.rs` and `driver/postgres/lease.rs` provide
  Postgres-specific transaction and lease primitives.

Real source example:

```rust
// crates/aether-data/src/driver/postgres/pool.rs:75
#[derive(Debug, Clone)]
pub struct PostgresPoolFactory {
    config: PostgresPoolConfig,
}
```

Do not put domain queries in driver pool files. Pool files validate config and
create lazy pools; repository files execute table-specific SQL.

## Repository Layer

`src/repository` is organized by domain. Most domains follow:

```text
src/repository/<domain>/
  mod.rs
  types.rs
  memory.rs
  postgres.rs
  mysql.rs
  sqlite.rs
```

`mod.rs` exports the driver implementations and contract-facing traits/types:

```rust
// crates/aether-data/src/repository/auth/mod.rs:1
mod memory;
mod mysql;
mod postgres;
mod sqlite;
mod types;

pub use memory::InMemoryAuthApiKeySnapshotRepository;
pub use mysql::MysqlAuthApiKeyReadRepository;
pub use postgres::SqlxAuthApiKeySnapshotReadRepository;
pub use sqlite::SqliteAuthApiKeyReadRepository;
```

Use explicit driver filenames (`postgres.rs`, `mysql.rs`, `sqlite.rs`). Avoid a
new generic `sql.rs` when the SQL syntax, bind style, JSON handling, timestamp
handling, upsert semantics, or locking behavior differs by driver.

## Backend Composition Layer

`src/backend` chooses one configured SQL driver and wires the concrete
repositories into app-facing handles:

```rust
// crates/aether-data/src/backend/mod.rs:42
#[derive(Debug, Clone, Default)]
pub struct DataBackends {
    config: DataLayerConfig,
    postgres: Option<PostgresBackend>,
    mysql: Option<MysqlBackend>,
    sqlite: Option<SqliteBackend>,
    leases: DataLeaseBackends,
    read: DataReadRepositories,
    transactions: DataTransactionBackends,
    write: DataWriteRepositories,
}
```

`DataBackends::from_config` validates config, builds at most one SQL backend,
then derives leases, read repositories, transaction runners, and write
repositories from that backend (`crates/aether-data/src/backend/mod.rs:83`).

The read/write handles are trait-object registries. Add new repositories to
`backend/read.rs` or `backend/write.rs` only after every selected backend has a
constructor method. The current read registry prioritizes Postgres, then MySQL,
then SQLite:

```rust
// crates/aether-data/src/backend/read.rs:95
auth_api_keys: postgres
    .map(PostgresBackend::auth_api_key_read_repository)
    .or_else(|| mysql.map(MysqlBackend::auth_api_key_read_repository))
    .or_else(|| sqlite.map(SqliteBackend::auth_api_key_read_repository)),
```

## Lifecycle and Schema Layer

`src/lifecycle` owns migration, backfill, export/import, and empty-database
bootstrap flows. These files are not normal request-path repositories.

`schema/` is the schema maintenance workspace. The executable migration paths
stay under `migrations/{postgres,mysql,sqlite}` because `sqlx::migrate!` embeds
those directories:

```rust
// crates/aether-data/src/lifecycle/migrate.rs:21
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");
```

Schema source policy is documented in `crates/aether-data/schema/README.md`.
Portable table-shape changes start in `logical/*.toml`; generated SQL is review
output, not a fourth hand-edited source of truth.

## Public API Boundary

The crate bans old top-level public paths. The scanner lists deprecated
entrypoints such as `postgres`, `mysql`, `sqlite`, `redis`, `migrate`,
`backfill`, and `export`:

```rust
// crates/aether-data/tests/public_entrypoints.rs:5
const OLD_ENTRYPOINTS: &[&str] = &[
    "backends", "backfill", "export", "migrate", "mysql", "postgres", "redis", "sqlite",
];
```

Keep public imports under the current grouped modules or root re-exports.

## Anti-Patterns

DON'T add domain SQL to `driver/*`; put it in `repository/<domain>/<driver>.rs`.

DON'T wire a repository in `DataReadRepositories` or `DataWriteRepositories`
before the backend has a concrete constructor for each supported driver.

DON'T add a crate-root module for a single old entrypoint. Use existing grouped
modules and re-exports.

DON'T treat `schema/generated/**` as source. It is generated audit output.

DON'T add Redis implementation files to `aether-data` without a broader design
change. Current Redis references are contract/test compatibility only.
