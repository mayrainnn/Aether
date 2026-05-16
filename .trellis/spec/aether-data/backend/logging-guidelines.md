# Logging Guidelines

`aether-data` uses `tracing` sparingly. Current logs are concentrated in
lifecycle operations and unusual maintenance cleanup paths, not in normal
repository reads/writes.

## Imports and Macros

Lifecycle migration and backfill modules import only the levels they use:

```rust
// crates/aether-data/src/lifecycle/migrate.rs:14
use tracing::{error, info, warn};
```

The Postgres empty-database bootstrap imports `info` only:

```rust
// crates/aether-data/src/lifecycle/bootstrap/postgres.rs:5
use tracing::info;
```

Do not add blanket tracing imports to repository modules unless the module has a
real operational event to report.

## Info Logs for Lifecycle Progress

Use `info!` for lifecycle progress that operators need during startup or
maintenance.

```rust
// crates/aether-data/src/lifecycle/migrate.rs:162
if pending_migrations.is_empty() {
    info!(
        total_migrations,
        applied_migrations = applied_count,
        pending_migrations = 0,
        "database migrations already up to date"
    );
    return Ok(());
}
```

When applying migrations or backfills, include stable structured fields such as
`current`, `total`, `version`, `description`, and `elapsed_ms`:

```rust
// crates/aether-data/src/lifecycle/migrate.rs:183
info!(
    current,
    total,
    version = migration.version,
    description = %migration.description,
    "applying database migration"
);
```

Backfills follow the same style:

```rust
// crates/aether-data/src/lifecycle/backfill.rs:171
info!(
    current,
    total,
    version = backfill.version,
    description = %backfill.description,
    elapsed_ms,
    "applied database backfill"
);
```

## Warn Logs for Recoverable Operational Problems

Use `warn!` when an operation can continue but the event should be visible.

Migration lock release failures after an already failing migration are warnings:

```rust
// crates/aether-data/src/lifecycle/migrate.rs:45
Err(unlock_error) => {
    warn!(
        error = %unlock_error,
        "database migration lock release failed after migration error"
    );
}
```

Usage cleanup logs invalid cleanup windows and exits with `Ok(0)`:

```rust
// crates/aether-data/src/repository/usage/postgres/cleanup.rs:588
if matches!(newer_than, Some(value) if value >= cutoff_time) {
    warn!(
        cutoff_time = %cutoff_time,
        newer_than = ?newer_than,
        "usage cleanup header sweep skipped due to invalid window"
    );
    return Ok(0);
}
```

Best-effort cleanup sub-steps may warn and continue, as with expired API key
sweeps:

```rust
// crates/aether-data/src/repository/usage/postgres/cleanup.rs:456
let keys_cleaned =
    match cleanup_expired_api_keys(&self.pool, auto_delete_expired_keys).await {
        Ok(count) => count,
        Err(err) => {
            warn!(error = %err, "usage cleanup expired api key sweep failed");
            0
        }
    };
```

## Error Logs for Dirty or Unsafe State

Use `error!` when the database state is unsafe and the operation returns an
error immediately.

```rust
// crates/aether-data/src/lifecycle/migrate.rs:135
if let Some(version) = conn.dirty_version().await? {
    error!(version, "database migration state is dirty");
    return Err(MigrateError::Dirty(version));
}
```

Keep `error!` rare. Most repository failures should propagate as
`DataLayerError` without logging at the data layer; callers can decide whether
to log request context.

## Sensitive Data Boundaries

Do not log:

- API keys, encrypted keys, key hashes, OAuth secrets, LDAP bind passwords, or
  management token values.
- Raw request/response bodies from usage audit rows.
- Full SQL strings built from table names unless the names have already passed
  strict identifier validation.
- User-provided JSON payloads from export/import records.

Prefer counts, versions, IDs that are already operational identifiers, and
elapsed timings.

## No Per-Query Logging by Default

Normal repositories such as auth, provider catalog, quota, wallet, and usage
execute queries and return errors without logging. Example:

```rust
// crates/aether-data/src/repository/quota/postgres.rs:87
sqlx::query(FIND_BY_PROVIDER_IDS_SQL)
    .bind(provider_ids)
    .fetch_all(&self.pool)
    .await
    .map_postgres_err()?
```

Adding logs around every query would duplicate caller logs and risk exposing
data. Instrument request handlers or service-level workflows instead.

## Bootstrap Logs

The empty-database bootstrap logs only when it actually applies the snapshot or
allows snapshot bootstrap despite unrelated public tables:

```rust
// crates/aether-data/src/lifecycle/bootstrap/postgres.rs:61
info!(
    cutoff_version = EMPTY_DATABASE_SNAPSHOT_CUTOFF_VERSION,
    stamped_migrations = migrations.len(),
    "bootstrapping empty database from empty_database_snapshot"
);
```

This keeps normal startup quiet when the database already has applied
migrations.

## Anti-Patterns

DON'T add `debug!` or `trace!` statements to compensate for missing tests.

DON'T log and then return the same repository error at every layer. Pick the
operational boundary that has enough context.

DON'T log secrets or request bodies even at debug level.

DON'T use unstructured string interpolation when fields matter. Prefer
`version = migration.version` and `error = %err` style fields.

DON'T turn expected empty states into warnings. Empty pending backfill and
pending migration lists are `info!` events or silent `Ok` values.
