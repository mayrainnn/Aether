# Error Handling

`aether-data` uses one shared data-layer error type and keeps driver-specific
conversion at the boundary where `sqlx` returns errors.

## Canonical Error Type

The error enum is defined in `aether-data-contracts` and re-exported by this
crate:

```rust
// crates/aether-data-contracts/src/error.rs:1
#[derive(Debug, thiserror::Error)]
pub enum DataLayerError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("postgres error: {0}")]
    Postgres(String),
```

`aether-data` exposes it with:

```rust
// crates/aether-data/src/error.rs:1
pub use aether_data_contracts::DataLayerError;
```

Do not create a second crate-local error enum for repositories or drivers. Use
`DataLayerError` for runtime data access failures and `MigrateError` for
`sqlx::migrate` lifecycle calls.

## sqlx Error Conversion

Postgres-specific paths use `map_postgres_err`; generic MySQL/SQLite paths use
`map_sql_err`.

```rust
// crates/aether-data/src/error.rs:11
pub(crate) trait SqlxResultExt<T> {
    fn map_postgres_err(self) -> Result<T, DataLayerError>;
}

impl<T> SqlxResultExt<T> for Result<T, sqlx::Error> {
    fn map_postgres_err(self) -> Result<T, DataLayerError> {
        self.map_err(postgres_error)
    }
}
```

Use these helpers at every `sqlx` callsite where the surrounding function
returns `Result<_, DataLayerError>`.

Example repository mapping:

```rust
// crates/aether-data/src/repository/quota/postgres.rs:71
let row = sqlx::query(FIND_BY_PROVIDER_ID_SQL)
    .bind(provider_id)
    .fetch_optional(&self.pool)
    .await
    .map_postgres_err()?;
row.as_ref().map(map_row).transpose()
```

MySQL and SQLite repositories should use `map_sql_err()`:

```rust
// crates/aether-data/src/repository/quota/mysql.rs:66
let rows = builder.build().fetch_all(&self.pool).await.map_sql_err()?;
rows.iter().map(map_row).collect()
```

## Configuration Errors

Configuration validation returns `InvalidConfiguration`. Keep these checks
early, before constructing pools or runners.

```rust
// crates/aether-data/src/database.rs:86
pub fn validate(&self, driver: DatabaseDriver) -> Result<(), DataLayerError> {
    if self.min_connections > self.max_connections {
        return Err(DataLayerError::InvalidConfiguration(format!(
            "{driver} min_connections cannot exceed max_connections"
        )));
    }
```

Driver-specific factory checks should reject mismatched driver configs:

```rust
// crates/aether-data/src/driver/mysql/pool.rs:18
pub fn new(config: MysqlPoolConfig) -> Result<Self, DataLayerError> {
    if config.driver != DatabaseDriver::Mysql {
        return Err(DataLayerError::InvalidConfiguration(format!(
            "mysql pool requires mysql driver, got {}",
            config.driver
        )));
    }
```

Use `InvalidConfiguration` for invalid URLs, impossible pool settings,
unsupported SSL flags, non-positive timeouts, and empty maintenance identifiers.

## Input Errors

Use `InvalidInput` when caller-provided data is out of range or malformed after
configuration has already been accepted.

```rust
// crates/aether-data/src/driver/postgres/lease.rs:98
let lease_ms = i64::try_from(options.lease_ms).map_err(|_| {
    DataLayerError::InvalidInput("postgres lease lease_ms exceeds i64 range".to_string())
})?;
```

Another example is converting a user timestamp for quota reset:

```rust
// crates/aether-data/src/repository/quota/postgres.rs:101
let result = sqlx::query(RESET_DUE_SQL)
    .bind(i64::try_from(now_unix_secs).map_err(|_| {
        DataLayerError::InvalidInput("provider quota reset timestamp overflow".to_string())
    })?)
```

Avoid silent casts from `u64` to `i64`; name the field in the error message.

## Unexpected Database Values

Use `UnexpectedValue` when the database returns a shape that violates the Rust
contract.

```rust
// crates/aether-data/src/repository/usage/mysql.rs:900
fn row_i32(row: &MySqlRow, field: &str) -> Result<i32, DataLayerError> {
    let value: i64 = row.try_get(field).map_sql_err()?;
    i32::try_from(value).map_err(|_| DataLayerError::UnexpectedValue(format!("{field} overflow")))
}
```

Parsing JSON from a row also maps parse failures to `UnexpectedValue`:

```rust
// crates/aether-data/src/repository/usage/mysql.rs:847
audit.request_metadata = row
    .try_get::<Option<String>, _>("request_metadata")
    .map_sql_err()?
    .map(|raw| serde_json::from_str(&raw))
    .transpose()
    .map_err(|err| DataLayerError::UnexpectedValue(err.to_string()))?;
```

## Migration Errors Stay Separate

Migration and backfill entrypoints return `sqlx::migrate::MigrateError`, not
`DataLayerError`.

```rust
// crates/aether-data/src/lifecycle/migrate.rs:31
pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    let mut conn = pool.acquire().await?;
```

Backend maintenance methods preserve that separation:
`run_database_migrations`, `pending_database_migrations`, and
`prepare_database_for_startup` return `MigrateError` through the backend
dispatch layer.

## Transaction Rollback

Postgres transaction helpers commit on success and attempt rollback on error:

```rust
// crates/aether-data/src/driver/postgres/tx.rs:94
let mut tx = self.begin(options).await?;
match f(&mut tx).await {
    Ok(value) => {
        tx.commit().await.map_err(postgres_error)?;
        Ok(value)
    }
    Err(err) => {
        let _ = tx.rollback().await;
        Err(err)
    }
}
```

The rollback error is intentionally ignored so the original domain error
surfaces to the caller.

## Anti-Patterns

DON'T use `unwrap()` or lossy casts in repository row mapping. Convert with
`try_from` and return `InvalidInput` or `UnexpectedValue`.

DON'T map all SQL errors to `Postgres`. Only Postgres-specific modules should
use `map_postgres_err`.

DON'T convert `MigrateError` into `DataLayerError` in lifecycle APIs. Callers
need dirty-version and migration-source details.

DON'T swallow repository errors except for deliberately best-effort maintenance
steps that log a warning, such as expired API key cleanup in
`repository/usage/postgres/cleanup.rs:456`.
