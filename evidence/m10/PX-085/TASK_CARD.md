# Task Card — PX-085 Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source

## Identity

- Task ID: PX-085 (REQ-PX-085; owner sandbox-cloud); qualification QUAL-PX-085 / PX-E2E-085; Decision Record DR-PX-2026-10-03-010
- Evidence tier: release-critical (network-facing intake that creates tenant tasks)
- Prerequisites: PX-011, M8.1, PX-082, PX-083, DOC-PX-010, all COMPLETE

## Existing-code audit (2026-10-27)

- Graph said NOT_STARTED; the code exists and runs (graph drift).
- Production path: `POST /v1/hooks/{endpoint}` in `apps/cloud-api/src/automations.rs` -> `modbit_automation::webhook::verify` (HMAC-SHA256 over `v1.<t>.<nonce>.<body>`, constant-time compare before the window, window both ways) -> nonce as a primary key in the tenant store (replay = unique violation) -> endpoint decides the tenant -> `CloudStore::dispatch_firing` issues the canonical task with the (definition, version, event id) idempotency key; schedules run in the worker claim loop on the database clock (`apps/cloud-worker`); refusals audited under typed codes; endpoint secrets derived `HMAC(master, endpoint || rotation)`, shown once, never stored.
- Classification: PRODUCTION-WORKING. First missing link: none in code; graph row and evidence missing. No product code changed in this closing.

## Proof

- `apps/cloud-api/tests/px_085_automations.rs` (11 tests), `apps/cloud-worker/tests/px_085_cloud_triggers.rs` (6), `apps/cloud-worker/tests/px_085_egress.rs` (organisation allow-list at the gateway) — real Postgres, real S3-compatible store, real Core and worker. The CI job fails if any cloud test skips itself.
- CI run 40239702943 (head 40ef7536, landed as 84eecda2): cloud job green; all PX-085 tests ok (`ci-run-40239702943.json`).
- Local macOS, Postgres 17 (scratch cluster): px_085_automations 11 passed; `modbit-automation` webhook unit tests 5 passed.
- Mutation check (QUAL: "dropping the timestamp window lets an old delivery through"): `mutation-check.txt` — the Stale check removed from `verify`; KILLED by `qual_px_085_forged_tampered_stale_future_and_malformed_signatures_are_refused_and_audited_under_typed_codes` and the unit test `the_window_bounds_both_directions_and_the_timestamp_is_signed`.

## Limitations (not proven)

- "Real staging Cloud API" is the CI-hosted Cloud API over real Postgres and SeaweedFS S3, not a deployed staging environment.
- The cloud run uses the scripted model stand-in, not a live provider. No gate attested.
