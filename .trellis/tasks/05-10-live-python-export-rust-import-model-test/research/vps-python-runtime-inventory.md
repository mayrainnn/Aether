# VPS Python runtime inventory

Date: 2026-05-10

Scope: live verification of the Python export to Rust import migration path. This file intentionally redacts credentials, API keys, cookies, tokens, and full connection URLs.

## Runtime layout

- VPS host: `113.44.139.22`.
- Running app containers observed:
  - `aether-app-rust`: image `ghcr.1ms.run/fawney19/aether:pre`, label `0.7.0-rc8`, public port `8085`, connected to the Rust database.
  - `postgresql`: shared PostgreSQL service.
  - `redis`: shared Redis service.
  - `openresty` and `frps`: networking support containers.
- Images observed:
  - Python latest image: `ghcr.1ms.run/fawney19/aether:latest`, label `0.6.3`, entrypoint `/entrypoint.sh`, command `supervisord`, exposed `80/tcp`.
  - Rust pre-fix image: `ghcr.1ms.run/fawney19/aether:pre`, label `0.7.0-rc8`, entrypoint `aether-gateway`.

## Databases

- Python database: `aether`.
  - Contains Alembic metadata.
  - Does not contain Rust `_sqlx_migrations`.
- Rust database: `aether-rust`.
  - Contains Alembic metadata and Rust `_sqlx_migrations`.
- Verification database created for this run: `aether_rust_liveverify_20260510`.
  - Cloned from `aether-rust` for import/model-test mutation.
  - The live `aether-rust` database was not purged during the import test.

Initial row counts:

| Database | providers | provider_api_keys | provider_endpoints | models | global_models | oauth_providers | system_configs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Python `aether` | 17 | 24 | 24 | 65 | 14 | 1 | 37 |
| Rust `aether-rust` | 17 | 29 | 24 | 65 | 15 | 1 | 37 |
| Verification clone | 17 | 29 | 24 | 65 | 15 | 1 | 37 |

## Local runtime setup

- Local Python export container used image `ghcr.1ms.run/fawney19/aether:latest`.
- Local Python container mapped `127.0.0.1:18084 -> 80`.
- Local Python connected to VPS PostgreSQL and Redis through SSH tunnels:
  - local PostgreSQL tunnel: `127.0.0.1:25432`.
  - local Redis tunnel: `127.0.0.1:26379`.
- Python user/auth-related environment values were copied from the deployment environment, but not recorded here.
- Current Rust verification ran locally on `127.0.0.1:28087` against the verification database and Redis DB isolated from the live app.
- Pre-fix Rust verification used the remote `ghcr.1ms.run/fawney19/aether:pre` image in a temporary container mapped through `127.0.0.1:18087`.

## Backup and restore path

Before mutating provider-related data, a provider-data backup was created from the Rust source database.

- Local artifact: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/aether-rust-provider-backup-20260510T091745+0800.dump`
- SHA-256: `30d56ddee9ffb4fa79daa34da6caaceee1fd094734d977efe24d9c1c4496bbe5`
- The backup artifact is sensitive because provider configuration can include encrypted credentials and operational metadata.

Restore shape, if needed:

```bash
# Run from a machine that can reach the target PostgreSQL container.
# Replace the database name with the intended restore target.
docker exec -i postgresql pg_restore -U mayrain -d <target_database> --clean --if-exists < aether-rust-provider-backup-20260510T091745+0800.dump
```

Do not run the restore against production without first confirming the target database and restore scope.
