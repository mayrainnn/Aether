# Database Guidelines

Keep this guide. `aether-testkit` directly manages local Postgres and Redis
processes for integration baselines, prepares the Aether Postgres schema through
`aether-data`, and uses `sqlx` for baseline fixture tables. It is not a
repository layer and should not own production schema definitions.

## Scope

Database-related code belongs in this crate only when it supports tests or
baseline experiments:

- starting/stopping temporary local Postgres;
- starting/stopping temporary local Redis;
- preparing the Aether schema in a test database;
- creating disposable baseline fixture tables;
- driving Redis/Postgres fault recovery probes.

Do not add production repository methods, domain queries, or migrations here.
Those belong in `aether-data` / `aether-data-contracts`.

## Local Postgres Process

`ManagedPostgresServer` owns a temporary `initdb` directory, a child process, and
a loopback database URL.

```rust
// crates/aether-testkit/src/postgres.rs:10
#[derive(Debug)]
pub struct ManagedPostgresServer {
    child: Option<Child>,
    postgres_bin: String,
    port: u16,
    workdir: PathBuf,
    data_dir: PathBuf,
    database_url: String,
}
```

Startup reserves a local port, creates a temp workdir, honors binary override
environment variables, and runs `initdb`.

```rust
// crates/aether-testkit/src/postgres.rs:21
pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
    let port = reserve_local_port()?;
    let workdir = std::env::temp_dir().join(format!(
        "aether-postgres-baseline-{}-{}",
        std::process::id(),
        port
    ));
```

```rust
// crates/aether-testkit/src/postgres.rs:31
let initdb_bin = std::env::var("AETHER_INITDB_BIN")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "initdb".to_string());
let postgres_bin = std::env::var("AETHER_POSTGRES_BIN")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "postgres".to_string());
```

Guideline: keep local Postgres opt-in through this helper. Do not assume CI has
Postgres already running on a fixed port.

## Postgres Readiness

Readiness is proven by opening and closing a real `PgConnection`, not by a sleep.

```rust
// crates/aether-testkit/src/postgres.rs:110
let database_url = self.database_url.clone();
let ready = wait_until(
    std::time::Duration::from_secs(10),
    std::time::Duration::from_millis(50),
    || {
        let database_url = database_url.clone();
        async move {
            match PgConnection::connect(&database_url).await {
                Ok(connection) => connection.close().await.is_ok(),
                Err(_) => false,
            }
        }
    },
)
```

Guideline: new dependency readiness checks should use `wait_until` and a real
operation against the dependency.

## Schema Preparation

Use the production data layer to prepare and run migrations. Testkit should not
duplicate migration SQL.

```rust
// crates/aether-testkit/src/postgres.rs:145
pub async fn prepare_aether_postgres_schema(
    database_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = PostgresPoolConfig {
        database_url: database_url.to_string(),
        ..Default::default()
    };
```

```rust
// crates/aether-testkit/src/postgres.rs:153
let backends = DataBackends::from_config(DataLayerConfig::from_postgres(config))?;
let pending_migrations = backends
    .prepare_database_for_startup()
    .await?
    .unwrap_or_default();
if !pending_migrations.is_empty() {
    backends.run_database_migrations().await?;
}
```

Guideline: if a baseline needs the Aether schema, call
`prepare_aether_postgres_schema`; do not paste migration statements into the
binary.

## Disposable Fixture Tables

Raw SQL is acceptable for baseline-local tables that are not part of the Aether
schema. Keep names prefixed with `baseline_` and drop/create them explicitly.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:416
async fn bootstrap_failure_recovery_lease_table(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("DROP TABLE IF EXISTS baseline_failure_lease_jobs")
        .execute(&pool)
        .await?;
```

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:422
sqlx::query(
    "CREATE TABLE baseline_failure_lease_jobs (
         id TEXT PRIMARY KEY,
         status TEXT NOT NULL,
         updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
         lease_owner TEXT,
         lease_expires_at TIMESTAMPTZ
     )",
)
```

Use `QueryBuilder` for bulk inserts rather than string-building values.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:434
let mut builder =
    sqlx::QueryBuilder::new("INSERT INTO baseline_failure_lease_jobs (id, status) ");
builder.push_values(0..32, |mut row, index| {
    row.push_bind(format!("recovery-job-{index:03}"))
        .push_bind("ready");
});
builder.build().execute(&pool).await?;
```

DON'T use `format!` to interpolate row values into SQL. `push_bind` keeps
fixtures safe and mirrors production query discipline.

## Transaction and Lease Testing

Failure recovery baselines exercise production data-layer abstractions rather
than talking directly to Postgres everywhere.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:345
let backend = PostgresBackend::from_config(PostgresPoolConfig {
    database_url: postgres_url.to_string(),
    min_connections: 1,
    max_connections: 8,
    acquire_timeout_ms: config.timeout.as_millis() as u64,
    idle_timeout_ms: 60_000,
    max_lifetime_ms: 10 * 60_000,
    statement_cache_capacity: 64,
    require_ssl: false,
})?;
```

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:363
let slow_query_timed_out = transaction_runner
    .run(
        PostgresTransactionOptions {
            statement_timeout_ms: Some(config.postgres_statement_timeout.as_millis() as u64),
            ..PostgresTransactionOptions::read_write()
        },
```

Guideline: when testing Aether data-layer behavior, instantiate
`PostgresBackend` and its runners. Use raw `sqlx` only to seed disposable test
state.

## Local Redis Process

`ManagedRedisServer` mirrors the Postgres helper: temp workdir, child process,
loopback URL, stop/restart, and drop cleanup.

```rust
// crates/aether-testkit/src/redis.rs:17
pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
    let port = reserve_local_port()?;
    let workdir = std::env::temp_dir().join(format!(
        "aether-redis-baseline-{}-{}",
        std::process::id(),
        port
    ));
```

```rust
// crates/aether-testkit/src/redis.rs:26
let binary = std::env::var("AETHER_REDIS_SERVER_BIN")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "redis-server".to_string());
let redis_url = format!("redis://127.0.0.1:{port}/0");
```

Redis starts without persistence for repeatable local baselines.

```rust
// crates/aether-testkit/src/redis.rs:60
let child = Command::new(&self.binary)
    .arg("--save")
    .arg("")
    .arg("--appendonly")
    .arg("no")
    .arg("--port")
    .arg(self.port.to_string())
```

## Redis Readiness

Readiness is a protocol-level PING over `tokio::net::TcpStream`.

```rust
// crates/aether-testkit/src/redis.rs:95
async fn redis_ping(addr: (&str, u16)) -> Result<bool, std::io::Error> {
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    stream.write_all(b"*1\r\n$4\r\nPING\r\n").await?;
    let mut buffer = [0_u8; 16];
    let len = stream.read(&mut buffer).await?;
    Ok(buffer[..len].starts_with(b"+PONG"))
}
```

Guideline: avoid sleeps after process start; poll until the dependency answers a
real command or the timeout expires.

## Redis Runtime-State Usage

When testing Aether Redis behavior, use `aether_runtime_state` clients and
runners.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:239
let redis_url = redis_server.lock().await.redis_url().to_string();
let factory = RedisClientFactory::new(RedisClientConfig {
    url: redis_url,
    key_prefix: Some(format!("aether-failure-recovery-{}", std::process::id())),
})?;
let client = factory.connect_lazy()?;
let keyspace = factory.config().keyspace();
```

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:246
let runner = RedisLockRunner::new(
    client,
    keyspace.clone(),
    RedisLockRunnerConfig {
        command_timeout_ms: Some(250),
        default_ttl_ms: 1_000,
    },
)?;
```

Guideline: prefix Redis keys with process-specific namespaces in baselines.
Never let local test data collide with a shared Redis instance.

## Configuration From Existing Services

Some baselines accept `--redis-url` or `--postgres-url`, but the current
failure-recovery baseline still requires managed services for restart tests.

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:185
let managed_redis = if config.redis_url.is_none() {
    Some(ManagedRedisServer::start().await?)
} else {
    None
};
```

```rust
// crates/aether-testkit/src/bin/failure_recovery_baseline.rs:214
let redis_server = Arc::new(Mutex::new(
    managed_redis.ok_or("failure recovery baseline requires managed redis")?,
));
let postgres_server =
    managed_postgres.ok_or("failure recovery baseline requires managed postgres")?;
```

Guideline: make external URLs explicit CLI options. Do not infer production
connections from ambient environment variables in testkit binaries.

## Cleanup

Both managed services remove temp directories on drop.

```rust
// crates/aether-testkit/src/postgres.rs:138
impl Drop for ManagedPostgresServer {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}
```

```rust
// crates/aether-testkit/src/redis.rs:103
impl Drop for ManagedRedisServer {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}
```

## DON'T

```rust
// DON'T: fixed ports make parallel test runs flaky.
let database_url = "postgres://aether@127.0.0.1:5432/postgres";
```

```rust
// DON'T: SQL value interpolation for fixture inserts.
sqlx::query(&format!("INSERT INTO jobs VALUES ('{id}')"));
```

```rust
// DON'T: duplicate production migrations in testkit.
sqlx::query("CREATE TABLE providers (...)").execute(&pool).await?;
```

Use reserved loopback ports, `QueryBuilder::push_bind`, and
`prepare_aether_postgres_schema` instead.
