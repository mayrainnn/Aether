# Live Python Export to Rust Import Model-Test Verification

## Goal

Run an end-to-end, live-environment verification of the migration path that a
real Python-version Aether system exports provider configuration and the Rust
version imports it, then prove whether model testing failed before the current
fix and whether it succeeds after the fix.

## What I Already Know

- The user wants a real Python export, not only synthetic fixtures.
- Python should run locally through Docker where possible.
- The Python database and Redis likely exist on VPS `113.44.139.22`; access is
  through SSH as root. Do not print credentials in logs or final reports.
- Local Python Docker should connect to the VPS database and Redis through SSH
  tunnels.
- Environment variables related to users/auth should match the current Python
  deployment so browser login/export behavior reflects the real system.
- The Rust verification must use the current database, but provider-related
  data must be backed up before deletion/import.
- Required comparison:
  - code state before the recent fix: import Python-exported config and run
    model test to reproduce the issue if it exists;
  - current optimized code: repeat import and model test to confirm fix.
- Browser automation is required for the Python system export flow.

## Requirements

- Inventory the VPS safely before mutating anything:
  - running Docker containers;
  - Postgres/MySQL/Redis containers and exposed ports;
  - Python Aether container/image/version;
  - database names, table availability, and provider-related row counts.
- Identify or reconstruct the Python runtime environment for local Docker:
  - image/tag or Dockerfile;
  - env vars needed for DB, Redis, auth, encryption, admin session/login;
  - frontend/admin port and login route.
- Establish SSH tunnels from local machine to VPS DB/Redis.
- Start the Python version locally against the VPS DB/Redis.
- Use browser automation to log into the Python system and export supplier/provider configuration.
- Preserve the exported JSON as a task artifact under this task directory, with secrets redacted in any human-readable notes.
- Start Rust against the target verification database.
- Before Rust import, back up provider-related data:
  - provider catalog providers;
  - endpoints;
  - keys;
  - global/provider models;
  - OAuth/provider ops/proxy data if linked to provider testing.
- Delete/clear provider-related data only after backup succeeds and is verified.
- On a pre-fix Rust code state, import the Python export and run model test.
- Capture exact response payloads, logs, and browser/API evidence for the failure or non-failure.
- On the current Rust code state, repeat import and model test from the same export.
- Compare old/new behavior and decide whether code changes are sufficient.
- If current code still fails, diagnose root cause and implement the minimal fix with tests.

## Acceptance Criteria

- [x] VPS inventory is written to `research/vps-python-runtime-inventory.md`.
- [x] Python export artifact is saved under `artifacts/` and referenced in notes.
- [x] Rust DB provider backup is saved under `artifacts/` before any deletion.
- [x] Pre-fix Rust import/model-test result is captured with exact failure or success evidence.
- [x] Current Rust import/model-test result is captured with exact evidence.
- [x] If the issue reproduces pre-fix and passes current code, the comparison explains why.
- [x] If the issue does not reproduce, the report explains what real data differed from the suspected failure.
- [x] If current code fails, a focused fix is implemented and verified.
- [x] No credentials, raw API keys, tokens, or decrypted provider secrets are printed in final output.
- [x] Any DB mutation has a documented restore path from the backup artifact.

## Execution Design

### Phase A: Environment Inventory

1. Check local Docker availability.
2. SSH into VPS through a non-echoing method.
3. Inspect Docker containers/images/networks on VPS.
4. Identify database and Redis endpoints.
5. Locate Python app env/config and running image.
6. Record only redacted inventory.

### Phase B: Local Python Export

1. Create SSH tunnels to DB and Redis.
2. Start local Python app container with copied/redacted environment values.
3. Use browser automation against local Python admin UI.
4. Export provider/supplier configuration from the UI.
5. Save raw export securely as task artifact; write redacted summary.

### Phase C: Rust Pre-Fix Reproduction

1. Resolve pre-fix commit as the parent of `1b217d54` unless code history shows a better baseline.
2. Build/start Rust from that commit in an isolated worktree or temporary checkout.
3. Connect Rust to verification DB/Redis.
4. Back up provider-related tables.
5. Clear provider-related data.
6. Import Python export.
7. Run model test through browser/API and capture evidence.

### Phase D: Current Rust Verification

1. Return to current branch/code.
2. Restore or re-clear provider-related data from the same backup strategy.
3. Import the same Python export.
4. Run the same model test.
5. Capture and compare evidence.

### Phase E: Fix If Needed

1. If current Rust still fails, isolate whether the cause is import version,
   auth/encryption, endpoint/key normalization, transport support, or frontend
   test-intent routing.
2. Run GitNexus impact before editing symbols.
3. Add deterministic regression coverage where possible.
4. Run targeted Rust/frontend checks and GitNexus detect changes.

## Safety Constraints

- Do not overwrite or drop VPS databases.
- Do not delete provider-related data before a backup file is created and sanity-checked.
- Do not commit raw export files if they contain credentials.
- Do not echo the VPS password, provider API keys, DB URLs with passwords, cookies, or tokens.
- Prefer local tunnels and local Docker for Python app execution over changing the VPS runtime.
- Keep unrelated untracked `.trellis/spec/*/database-guidelines.md` files untouched.

## Definition of Done

- PRD and research artifacts capture enough detail to reproduce the test.
- Live export/import evidence is captured for both pre-fix and current Rust states.
- Data backup and restore instructions are recorded.
- Any code changes are committed only after tests/checks pass.
- Trellis task is archived and journaled when complete.

## Out of Scope

- Replacing the Python production deployment.
- Modifying VPS schema or production data beyond the explicit provider-data backup/delete/import cycle.
- Requiring live provider credentials in automated CI tests.
- Broad redesign of provider import/export unless live evidence proves it is needed.

## Technical Notes

- Related previous work commit: `1b217d54 fix: 防止迁移导入后的模型测试失真`.
- Recent task archive: `.trellis/tasks/archive/2026-05/05-10-model-test-stability-decoupling/`.
- Prior deterministic runbook: `docs/operations/model-test-verification.md`.
- Memory note: prior Aether pool scheduling audits found pool scheduling is a plan-construction ordering stage, so pool model tests must not be treated as direct provider mapping tests.
