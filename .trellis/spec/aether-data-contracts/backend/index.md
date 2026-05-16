# Backend Development Guidelines

> Project-specific backend guidance for the `aether-data-contracts` crate.

---

## Scope

These guidelines apply to `crates/aether-data-contracts/`, the Rust crate that
defines shared data-layer contracts for Aether services. The crate owns
repository traits, stored read models, write inputs, query DTOs, conversion
helpers, and the stable `DataLayerError` vocabulary.

This directory is intentionally specific to the contract crate. Concrete SQL,
Redis, transaction, migration, connection pool, and runtime behavior belongs in
`aether-data` or higher-layer service crates.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Actual crate layout, facade pattern, domain responsibilities, and naming rules | Filled |
| [Repository Contracts](./repository-contracts.md) | Read/write trait shape, default methods, stored/write/query DTO semantics, and implementation boundaries | Filled |
| [Error Handling](./error-handling.md) | `DataLayerError`, validation, stored-value conversion, and error surfacing rules | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Naming, visibility, type safety, serde compatibility, tests, and forbidden patterns | Filled |
| [Logging Guidelines](./logging-guidelines.md) | No-logging policy, structured observability outputs, and sensitive data rules | Filled |

`database-guidelines.md` was removed for this crate. `aether-data-contracts`
does not interact with databases directly: it contains database-facing type
contracts, but no queries, migrations, connections, transactions, SeaORM,
sqlx, Redis, or pool management code.

---

## Evidence Sources Used

The specs were derived from the actual source under
`crates/aether-data-contracts/` and the available GitNexus MCP resources for
repo `Aether`. GitNexus reports Aether as an indexed repository with 3,140
files, 83,229 symbols, and 300 execution flows. The crate's source and
dependency facts are the primary evidence for these guidelines.

Important source anchors:

- `crates/aether-data-contracts/Cargo.toml:1` declares the crate as
  `aether-data-contracts`.
- `crates/aether-data-contracts/Cargo.toml:9` shows the dependency boundary.
- `crates/aether-data-contracts/src/lib.rs:1` defines the crate facade.
- `crates/aether-data-contracts/src/error.rs:1` defines `DataLayerError`.
- `crates/aether-data-contracts/src/repository/mod.rs:1` lists the public
  repository domains.
- `crates/aether-data-contracts/src/repository/usage/types.rs:1343` begins the
  largest read repository trait in this crate.

---

## High-Level Rules

Keep this crate pure. It should define contracts and validation behavior, not
perform storage operations.

Keep public imports stable. Domain `mod.rs` files re-export types from private
`types.rs` modules; consumers should import from
`aether_data_contracts::repository::<domain>::...`.

Keep fallibility explicit. Public constructors, parsers, validation methods,
and repository traits return `Result<_, crate::DataLayerError>`.

Keep not-found semantics distinct. Optional lookups return
`Result<Option<T>, DataLayerError>`, not `Result<T>` with a special error and
not bare `Option<T>`.

Keep read and write traits separate. Compose them with a blanket `*Repository`
trait only when both sides exist for the domain.

Keep logging out of this crate. Return typed errors and structured summaries;
let callers choose tracing spans, redaction, retry policy, and log levels.

---

## Removed Or Non-Applicable Templates

`database-guidelines.md` is intentionally absent. This crate mentions database
values in parser names such as `from_database` because it defines storage
contracts, but it does not own database I/O. Query construction and transaction
guidance should be documented in the concrete `aether-data` spec.

No frontend, route-handler, UI, or API-response guidelines apply to this spec
directory. The crate is a backend Rust contract crate and does not expose HTTP
routes.

---

## Review Checklist

Before accepting a change to this crate, verify:

- New public types are re-exported through the correct domain `mod.rs`.
- New fallible code returns `crate::DataLayerError`.
- Stored-row constructors reject corrupt database values rather than defaulting
  silently.
- Write inputs provide `validate` methods when they carry user or caller data.
- New repository methods are added to the read or write trait according to
  behavior, not convenience.
- Concrete database dependencies are not introduced.
- Sensitive body/header/key data is not logged.
- Unit tests live next to the invariant they protect.

Run at least `cargo test -p aether-data-contracts` after contract changes.
