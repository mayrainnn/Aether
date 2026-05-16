# Database Guidelines

`apps/aether-gateway` does not own raw SQL query construction for domain data.
It configures and consumes persistence through `aether_data`, `GatewayDataState`,
and `AppState` facade methods. It also uses `aether_runtime_state::RuntimeState`
for Redis/memory runtime state and lightweight key-value/cache operations.

Keep this file: database and runtime persistence are applicable to this crate.

## Configuration

Database configuration is assembled in `main.rs` from CLI/env args. SQL database
driver and URL are inferred when possible.

```rust
// apps/aether-gateway/src/main.rs:301
impl GatewayDataArgs {
    fn effective_database_driver(&self) -> Option<DatabaseDriver> {
        self.database_driver.map(Into::into).or_else(|| {
            self.database_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(DatabaseDriver::from_database_url)
        })
    }
```

Pool settings are explicit and passed into `SqlDatabaseConfig`.

```rust
// apps/aether-gateway/src/main.rs:329
fn effective_sql_database_config(&self) -> Option<SqlDatabaseConfig> {
    let url = self.effective_database_url()?;
    let driver = self
        .effective_database_driver()
        .or_else(|| DatabaseDriver::from_database_url(&url))
        .unwrap_or(DatabaseDriver::Postgres);

    Some(SqlDatabaseConfig {
        driver,
        url,
        pool: SqlPoolConfig {
            min_connections: self.postgres_min_connections,
            max_connections: self.postgres_max_connections,
            acquire_timeout_ms: self.postgres_acquire_timeout_ms,
            idle_timeout_ms: self.postgres_idle_timeout_ms,
            max_lifetime_ms: self.postgres_max_lifetime_ms,
            statement_cache_capacity: self.postgres_statement_cache_capacity,
            require_ssl: driver != DatabaseDriver::Sqlite && self.postgres_require_ssl,
        },
    })
}
```

Startup validation should fail loudly for incompatible topology/runtime
configuration, but tolerate explicitly local-only single-node mode.

```rust
// apps/aether-gateway/src/main.rs:917
fn validate_deployment_topology(
    args: &Args,
    database: Option<&SqlDatabaseConfig>,
    data_redis_url: Option<&str>,
    runtime_backend: RuntimeBackendArg,
) -> Result<(), std::io::Error> {
    if matches!(args.deployment_topology, DeploymentTopologyArg::SingleNode) {
        if database.is_none() && data_redis_url.is_none() {
            warn!(
                "single-node deployment is starting without SQL database or Redis; local-only mode is allowed, but admin/auth/billing persistence will be limited"
            );
        }
```

## GatewayDataConfig

`GatewayDataConfig` is the gateway-level configuration object. It may be
disabled, constructed from a generic SQL config, or constructed from a Postgres
URL.

```rust
// apps/aether-gateway/src/data/config.rs:5
#[derive(Clone, Default)]
pub struct GatewayDataConfig {
    database: Option<SqlDatabaseConfig>,
    postgres: Option<PostgresPoolConfig>,
    encryption_key: Option<String>,
}
```

Debug output must not print the encryption key.

```rust
// apps/aether-gateway/src/data/config.rs:12
impl fmt::Debug for GatewayDataConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayDataConfig")
            .field("database", &self.database)
            .field("postgres", &self.postgres)
            .field("has_encryption_key", &self.encryption_key.is_some())
            .finish()
    }
}
```

## Repository Facade

`GatewayDataState` stores optional repository traits from `aether_data` and
`aether_data_contracts`. Do not add direct SQL to gateway handlers.

```rust
// apps/aether-gateway/src/data/state/mod.rs:131
#[derive(Clone, Default)]
pub(crate) struct GatewayDataState {
    config: GatewayDataConfig,
    backends: Option<DataBackends>,
    auth_api_key_reader: Option<Arc<dyn AuthApiKeyReadRepository>>,
    auth_api_key_writer: Option<Arc<dyn AuthApiKeyWriteRepository>>,
    global_model_reader: Option<Arc<dyn GlobalModelReadRepository>>,
    global_model_writer: Option<Arc<dyn GlobalModelWriteRepository>>,
    provider_catalog_reader: Option<Arc<dyn ProviderCatalogReadRepository>>,
    provider_catalog_writer: Option<Arc<dyn ProviderCatalogWriteRepository>>,
```

When persistence is disabled, `from_config` builds a facade with no backends and
no repositories.

```rust
// apps/aether-gateway/src/data/state/core.rs:19
pub(crate) fn from_config(config: GatewayDataConfig) -> Result<Self, DataLayerError> {
    if !config.is_enabled() {
        return Ok(Self {
            config,
            backends: None,
            auth_api_key_reader: None,
            auth_api_key_writer: None,
            auth_module_reader: None,
            auth_module_writer: None,
```

When persistence is enabled, repository handles come from `DataBackends`.

```rust
// apps/aether-gateway/src/data/state/core.rs:64
let backends = DataBackends::from_config(config.to_data_layer_config())?;
let auth_api_key_reader = backends.read().auth_api_keys();
let auth_api_key_writer = backends.write().auth_api_keys();
let provider_catalog_reader = backends.read().provider_catalog();
let provider_catalog_writer = backends.write().provider_catalog();
let usage_reader = backends.read().usage();
let usage_writer = backends.write().usage();
```

## Query And Mutation Pattern

Handlers should call `AppState` methods. `AppState` methods should delegate to
`GatewayDataState` and convert data-layer errors into `GatewayError`.

```rust
// apps/aether-gateway/src/state/runtime/gemini_files.rs:17
pub(crate) async fn list_gemini_file_mappings(
    &self,
    query: &aether_data::repository::gemini_file_mappings::GeminiFileMappingListQuery,
) -> Result<
    aether_data::repository::gemini_file_mappings::StoredGeminiFileMappingListPage,
    GatewayError,
> {
    self.data
        .list_gemini_file_mappings(query)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))
}
```

If a repository is optional, data-layer methods should return empty/default
results, `None`, or `false` according to the method semantics. Do not make every
disabled-backend path a server error.

```rust
// apps/aether-gateway/src/data/state/runtime.rs:53
pub(crate) async fn run_database_migrations(
    &self,
) -> Result<bool, sqlx::migrate::MigrateError> {
    match &self.backends {
        Some(backends) => backends.run_database_migrations().await,
        None => Ok(false),
    }
}
```

## Migrations And Backfills

The gateway exposes migration/backfill methods through `AppState`; actual SQL
migration implementation lives below `aether_data`.

```rust
// apps/aether-gateway/src/state/core.rs:311
pub async fn run_database_migrations(&self) -> Result<bool, sqlx::migrate::MigrateError> {
    self.data.run_database_migrations().await
}

pub async fn run_database_backfills(&self) -> Result<bool, sqlx::migrate::MigrateError> {
    self.data.run_database_backfills().await
}
```

Startup and operational code should use these wrappers so disabled/local-only
mode remains supported.

## Maintenance Jobs

Maintenance workers only spawn when the relevant repository/backend is present.

```rust
// apps/aether-gateway/src/maintenance/runtime/workers.rs:69
pub(crate) fn spawn_db_maintenance_worker(
    data: Arc<GatewayDataState>,
) -> Option<tokio::task::JoinHandle<()>> {
    if !data.has_database_maintenance_backend() {
        return None;
    }

    let timezone = maintenance_timezone();
    Some(tokio::spawn(async move {
        loop {
            tokio::time::sleep(duration_until_next_db_maintenance_run(Utc::now(), timezone)).await;
            if let Err(err) = run_db_maintenance_once(&data).await {
                log_maintenance_worker_failure("db_maintenance", "tick", &err);
            }
        }
    }))
}
```

Follow this pattern for new persistence-backed background jobs: feature-detect
the backend, spawn conditionally, and log failures without killing the worker
loop.

## Runtime State And Redis

`RuntimeState` is separate from SQL persistence. It can be memory-backed or
Redis-backed and is used for semaphores, queues, kv operations, and lightweight
distributed state.

Scheduler affinity writes update the local cache and, when the runtime state is
not memory, asynchronously mirror to runtime kv.

```rust
// apps/aether-gateway/src/state/core.rs:73
fn spawn_scheduler_affinity_redis_write(
    &self,
    cache_key: &str,
    target: &SchedulerAffinityTarget,
    ttl: Duration,
) {
    if self.runtime_state.is_memory() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
```

Use runtime kv wrappers on `AppState` when the behavior is cache-like.

```rust
// apps/aether-gateway/src/state/runtime/gemini_files.rs:74
pub(crate) async fn cache_set_string_with_ttl(
    &self,
    key: &str,
    value: &str,
    ttl_seconds: u64,
) -> Result<(), GatewayError> {
    self.runtime_state
        .kv_set(
            key,
            value.to_string(),
            Some(std::time::Duration::from_secs(ttl_seconds)),
        )
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))
}
```

## Local In-Process Caches

Local caches use `aether_cache::ExpiringMap` and typed cache wrappers.

```rust
// apps/aether-gateway/src/cache/auth_context.rs:7
#[derive(Debug, Default)]
pub(crate) struct AuthContextCache {
    entries: ExpiringMap<String, GatewayControlAuthContext>,
}

impl AuthContextCache {
    pub(crate) fn get_fresh(
        &self,
        cache_key: &str,
        ttl: Duration,
    ) -> Option<GatewayControlAuthContext> {
        self.entries.get_fresh(&cache_key.to_string(), ttl)
    }
```

Add a typed cache wrapper instead of storing arbitrary `serde_json::Value` maps
in unrelated state.

## Test Stores

Many `AppState` methods first check `#[cfg(test)]` in-memory stores, then fall
back to the repository facade. This keeps handler tests fast and deterministic.

```rust
// apps/aether-gateway/src/state/runtime/wallet/balance_mutations.rs:20
#[cfg(test)]
if let Some(store) = self.auth_wallet_store.as_ref() {
    let mut guard = store.lock().expect("auth wallet store should lock");
    let Some(wallet) = guard.get_mut(wallet_id) else {
        return Ok(None);
    };
```

Only add test stores for gateway-facing behavior that would otherwise require a
large external fixture. Repository semantics should still be tested in
`aether_data`.

## DON'T

Do not use raw `sqlx::query!` or SQL strings inside `handlers/`, `control/`, or
`executor/`. Put repository behavior in `aether_data` and expose it through
`GatewayDataState`.

Do not treat disabled persistence as an error unless the route explicitly
requires a backend. Follow existing default/none behavior at the data facade.

Do not print `encryption_key`, database URLs with credentials, Redis URLs, or
OAuth/provider tokens in logs or debug output.

Do not add Redis-only behavior without a memory-mode fallback or startup
validation. The gateway supports local-only/single-node operation.
