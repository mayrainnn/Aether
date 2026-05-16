# Quality Guidelines

Quality in `aether-data` is mostly about keeping the layer boundaries stable,
preserving typed contracts, and proving multi-driver behavior with focused
tests.

## Visibility and Public Surface

Prefer private modules plus root re-exports for public types. The crate root
exposes `backend`, `driver`, `lifecycle`, `maintenance`, and `repository`, but
keeps `config`, `database`, and `error` private:

```rust
// crates/aether-data/src/lib.rs:19
pub use backend::{
    DataBackends, DataLeaseBackends, DataReadRepositories, DataTransactionBackends,
    DataWriteRepositories, PostgresBackend,
};
pub use config::DataLayerConfig;
pub use database::{DatabaseDriver, SqlDatabaseConfig, SqlPoolConfig, DEFAULT_SQLITE_DATABASE_URL};
pub use error::DataLayerError;
```

Use `pub(crate)` for helpers consumed across internal modules but not meant for
callers. `DataReadRepositories::from_backends` is a good example because only
backend composition should construct the registry:

```rust
// crates/aether-data/src/backend/read.rs:81
pub(crate) fn from_backends(
    postgres: Option<&PostgresBackend>,
    mysql: Option<&MysqlBackend>,
    sqlite: Option<&SqliteBackend>,
) -> Self {
```

## Naming Conventions

Use driver prefixes for concrete implementations:

- `Sqlx*` means Postgres implementation backed by `sqlx::PgPool`.
- `Mysql*` means MySQL implementation.
- `Sqlite*` means SQLite implementation.
- `InMemory*` means tests/dev memory implementation.

Real example:

```rust
// crates/aether-data/src/repository/auth/mod.rs:7
pub use memory::InMemoryAuthApiKeySnapshotRepository;
pub use mysql::MysqlAuthApiKeyReadRepository;
pub use postgres::SqlxAuthApiKeySnapshotReadRepository;
pub use sqlite::SqliteAuthApiKeyReadRepository;
```

Do not introduce ambiguous names such as `SqlAuthApiKeyRepository` when the
implementation is actually driver-specific.

## Contracts and DTO Placement

Shared DTOs, repository traits, and cross-crate error contracts belong in
`aether-data-contracts` when other crates compile against them. Implementation
helpers and SQL row mapping stay in `aether-data`.

When a type is implementation-local, put it beside the repository. Example:

```rust
// crates/aether-data/src/repository/auth/mysql.rs:168
struct CreateApiKeyInsertRecord {
    user_id: String,
    api_key_id: String,
    key_hash: String,
    key_encrypted: Option<String>,
```

Keep such structs private unless another module actually needs them.

## Backend Wiring

Wire repositories through backend methods that clone pools into trait objects:

```rust
// crates/aether-data/src/backend/postgres.rs:190
pub fn provider_catalog_read_repository(&self) -> Arc<dyn ProviderCatalogReadRepository> {
    Arc::new(SqlxProviderCatalogReadRepository::new(self.pool_clone()))
}
```

The registry then chooses the configured backend in a deterministic order:

```rust
// crates/aether-data/src/backend/read.rs:139
provider_catalog: postgres
    .map(PostgresBackend::provider_catalog_read_repository)
    .or_else(|| mysql.map(MysqlBackend::provider_catalog_read_repository))
    .or_else(|| sqlite.map(SqliteBackend::provider_catalog_read_repository)),
```

Do not create repositories directly in application crates when `DataBackends`
already exposes the handle.

## Type Safety

Prefer explicit conversion helpers over unchecked casts. Row mappers name the
field that overflowed or had a negative value:

```rust
// crates/aether-data/src/repository/usage/mysql.rs:915
fn row_u64(row: &MySqlRow, field: &str) -> Result<u64, DataLayerError> {
    let value: i64 = row.try_get(field).map_sql_err()?;
    u64::try_from(value).map_err(|_| DataLayerError::UnexpectedValue(format!("{field} negative")))
}
```

For driver config, parse from typed enums rather than stringly branching in
callers:

```rust
// crates/aether-data/src/database.rs:45
impl FromStr for DatabaseDriver {
    type Err = DataLayerError;
```

## Testing Patterns

Use lazy pool tests for constructors because they validate config without
requiring a running database:

```rust
// crates/aether-data/src/driver/postgres/pool.rs:119
let factory = PostgresPoolFactory::new(config).expect("factory should build");
let _pool = factory.connect_lazy().expect("lazy pool should build");
```

Use unit tests for pure SQL builders and option validation. The transaction
module verifies setup SQL without connecting to Postgres:

```rust
// crates/aether-data/src/driver/postgres/tx.rs:126
pub(crate) fn build_transaction_setup_statements(
    options: PostgresTransactionOptions,
) -> Vec<String> {
```

Use integration tests only where needed. Migration tests can start a local
Postgres if `initdb` and `postgres` exist, and skip cleanly if local startup is
unavailable (`crates/aether-data/src/lifecycle/migrate/tests.rs:35`).

## Import and Entry Point Hygiene

The public entrypoint scanner rejects old direct modules and grouped imports:

```rust
// crates/aether-data/tests/public_entrypoints.rs:5
const OLD_ENTRYPOINTS: &[&str] = &[
    "backends", "backfill", "export", "migrate", "mysql", "postgres", "redis", "sqlite",
];
```

Do not reintroduce those paths in `src/lib.rs` or workspace imports.

## Anti-Patterns

DON'T add a new dependency for simple SQL composition. Existing code uses
`sqlx::query`, `sqlx::query_scalar`, and `sqlx::QueryBuilder`.

DON'T duplicate trait definitions in this crate when `aether-data-contracts`
already owns the caller-facing contract.

DON'T use driver-specific SQL syntax in a shared helper unless the helper name
and module clearly say it is driver-specific.

DON'T build read/write registries with borrowed repositories. The established
shape is `Arc<dyn Trait>` returned from backend constructor methods.

DON'T add tests that require a developer database for constructor-level logic.
Use `connect_lazy` for pool factory coverage and reserve live database tests for
migration/runtime behavior.
