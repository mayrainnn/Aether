# Live export/import verification results

Date: 2026-05-10

Scope: real Python latest export, Rust pre-fix import reproduction, current Rust import/model-test verification.

## Python export

The Python system was started locally from the latest Python image and connected to the VPS Python database/Redis through SSH tunnels. Browser automation logged into the admin UI, opened `/admin/system`, clicked the real `导出配置` action, and captured the export API response.

Artifacts:

- Raw export: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/python-config-export-raw.json`
- Browser evidence: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/python-export-browser-evidence.json`
- Screenshot: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/python-export-system-page.png`

Export summary:

| Field | Value |
| --- | --- |
| version | `2.2` |
| providers | 17 |
| provider API keys | 24 |
| provider endpoints | 24 |
| provider models | 65 |
| global models | 14 |
| oauth providers | 1 |
| system configs | 37 |
| provider types | `codex`, `custom` |
| exported endpoint formats | `claude:cli`, `openai:cli`, `openai:compact` |

Important compatibility observation: Python exports some numeric fields as JSON strings, for example monetary quota and timeout values.

## Pre-fix Rust behavior

Pre-fix Rust was verified with the `ghcr.1ms.run/fawney19/aether:pre` image against the isolated verification database.

Artifact:

- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/prefix-rust-import-evidence.json`

Result:

- Login succeeded.
- Importing the real Python export failed with HTTP `400`.
- Exact failure detail: `配置文件格式无效: providers[4].monthly_quota_usd: expected a finite number`.

This reproduces the migration break before model-test execution: the Python exporter emitted decimal-like numeric fields as strings, while Rust import DTO deserialization only accepted JSON numbers.

## Current Rust behavior before the final fix

The current local Rust gateway was started against the same verification database after applying migrations and backfills.

Observed failures before the final patch:

- Immediate import after invoking the standalone purge endpoint raced with the asynchronous purge task and failed while updating a global model.
- Retrying after purge completion advanced further, then failed with `无效的 api_format: claude:cli`.

The purge/import race is an operational sequencing issue in the manual verification flow. The import-format failure is a Rust compatibility bug because Python exports legacy CLI aliases while Rust import only accepted canonical storage formats.

## Current Rust behavior after the final fix

Artifact:

- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/current-rust-import-final-evidence.json`

Result:

- Login succeeded.
- Importing the same real Python export succeeded with HTTP `200`.
- Import stats:
  - providers: 16 created, 1 updated.
  - endpoints: 24 created.
  - keys: 24 created.
  - models: 65 created.
  - global models: 14 updated.
  - oauth providers: 1 created.
  - system configs: 37 created.
- Post-import verification database counts:
  - providers: 17.
  - provider API keys: 24.
  - provider endpoints: 24.
  - models: 65.
  - global models: 14.

Format normalization verified in imported rows:

- Python `openai:cli` became Rust `openai:responses`.
- Python `openai:compact` became Rust `openai:responses:compact`.
- Python `claude:cli` became Rust `claude:messages`.

## Model-test verification

Artifacts:

- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/current-rust-model-test-evidence.json`
- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/current-rust-model-test-custom-evidence.json`
- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/current-rust-model-test-candidates-evidence.json`
- `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/current-rust-model-test-active-custom-evidence.json`

Results:

| Provider | Type | Format | Result | Interpretation |
| --- | --- | --- | --- | --- |
| Codex | `codex` | `openai:responses` | HTTP 200, `success=false`, `Provider auth is unavailable for openai:responses` | Import succeeded, but provider-query model test does not support this OAuth/Codex auth path. |
| Mayrain-Codex | `custom` | `openai:responses` | HTTP 200, `success=false`, `Service temporarily unavailable` | Import and routing reached provider execution; upstream was unavailable for this check. |
| Chintao / Foxnio-Free | `custom` | provider-specific | HTTP 404, no active endpoint/key | Expected from imported data because candidate keys were inactive. |
| GlmCodingPlan | `custom` | `claude:messages` | HTTP 200, `success=true` | Positive live model-test after importing the real Python export into current Rust. |

The successful GlmCodingPlan test confirms the import fix is sufficient for at least one real active custom provider/key/model path. The remaining negative model-test results are provider auth/runtime availability or inactive-data states, not Python-export import compatibility failures.

## Chrome plugin rerun with local Rust database

After the user corrected the Rust database source, the verification was rerun without using the remote Rust database. The local Docker image digest `sha256:78481659c47e862334611ccdaf7c369c986b3046da9857112f3b309114a65fb4` resolves to `postgres:18`; the running `aether-local-postgres` container uses that image and stores data in the local bind-mounted PostgreSQL data directory.

Local Rust database handling:

- Source database: local `aether` database in `aether-local-postgres`.
- Source counts before cloning: 19 providers, 48 provider API keys, 29 provider endpoints, 62 provider models, 19 global models, 1 oauth provider, 39 system configs, 0 proxy nodes.
- Verification target: local temporary database `aether_rust_chrome_local_verify_20260510`, cloned from local `aether`.
- Provider/config backup before clearing: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-local-rust-provider-backup.dump`.
- After clearing the verification target: providers, keys, endpoints, models, global models, oauth providers, system configs, and proxy nodes were all 0 before Rust startup. Rust startup then inserted one system config row.

Chrome plugin observations:

- The Codex Chrome Extension and native host checks passed.
- Chrome plugin tab listing confirmed Python admin tabs at `http://127.0.0.1:18084/admin/system`.
- Chrome plugin opened a Rust tab for the local Rust gateway on port `28087`; tab listing showed `http://127.0.0.1:28087/` with title `Aether`.
- Full Chrome navigation/DOM capture calls were intermittently slow and timed out, so the durable verification evidence for export/import/model-test is the API and database evidence below rather than a screenshot.

Artifacts from this rerun:

- Python raw export: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-python-config-export-raw.json`
- Python export evidence: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-python-export-evidence.json`
- Local Rust provider backup: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-local-rust-provider-backup.dump`
- Local Rust import evidence: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-local-rust-import-evidence.json`
- Local Rust model-test evidence: `.trellis/tasks/05-10-live-python-export-rust-import-model-test/artifacts/chrome-rerun-local-rust-model-test-evidence.json`

Python export summary from the rerun:

| Field | Value |
| --- | --- |
| version | `2.2` |
| providers | 17 |
| provider API keys | 24 |
| provider endpoints | 24 |
| provider models | 65 |
| global models | 14 |
| oauth providers | 1 |
| system configs | 37 |
| proxy nodes | 1 |
| exported endpoint formats | `claude:cli` x14, `openai:cli` x8, `openai:compact` x2 |

Import into current Rust against the local temporary database:

- Rust gateway was started on `http://127.0.0.1:28087`.
- Import status: HTTP `200`.
- Import stats: 17 providers created, 24 endpoints created, 24 keys created, 65 models created, 14 global models created, 1 oauth provider created, 37 system configs created.
- Proxy node import was skipped with the current Rust limitation message.
- Post-import database counts: 17 providers, 24 provider API keys, 24 provider endpoints, 65 provider models, 14 global models, 1 oauth provider, 38 system configs, 0 proxy nodes.

Format normalization in the local Rust database after import:

| Exported Python format | Imported Rust format | Count |
| --- | --- | --- |
| `claude:cli` | `claude:messages` | 14 |
| `openai:cli` | `openai:responses` | 8 |
| `openai:compact` | `openai:responses:compact` | 2 |

Positive model-test result from the local Rust database rerun:

| Provider | Format | Model | Result |
| --- | --- | --- | --- |
| GlmCodingPlan | `claude:messages` | `claude-sonnet-4-6` | HTTP `200`, `success=true`, response returned from upstream model `glm-4.7` |

This rerun confirms the fix using the local Rust PostgreSQL database requested by the user, not the remote Rust database.

## Root causes confirmed

1. Python export compatibility drift: Python serializes some decimal/numeric fields as strings; Rust import only accepted finite JSON numbers.
2. API format vocabulary drift: Python exports retired CLI aliases (`claude:cli`, `openai:cli`, `openai:compact`); Rust import accepted only canonical storage signatures.
3. Model-test behavior was being conflated with import correctness: Codex OAuth, inactive keys, and upstream availability can make model-test fail even when import succeeded.
4. The standalone purge endpoint is asynchronous, so a manual purge followed immediately by import can race. Verification should wait for purge completion, or rely on import overwrite logic rather than issuing a separate purge.
