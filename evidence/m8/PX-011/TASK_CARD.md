# Task Card — PX-011 Forge webhook intake through the Cloud API

## Identity

- Task ID: PX-011
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-PX-011 (a forge webhook to the Cloud API creates the same canonical task for the tenant after signature verification and policy checks); related REQ-EV-0010; QUAL-PX-011 / PX-E2E-011; docs/24 "Forge webhook intake", docs/29 "Issue-to-task intake", docs/30.
- Prerequisites: PX-010 (issue-to-task intake), M8.1 (the Cloud API, Postgres, object storage) — both COMPLETE.
- Qualification: the real Cloud API over real Postgres and real MinIO, deliveries signed exactly as GitHub signs them; the API-append path and the worker-relay path both proven.
- Evidence tier: real-system.

## Goal

A GitHub App's webhook to the Cloud API makes the same canonical task the desktop makes from an issue — for the tenant whose mapping names the repository, once per delivery, after the signature verifies and the mapping's policy admits it — with no second task model and no raw secret ever logged or stored.

## Existing-code audit

- classification: MISSING before this task: the Cloud API had no webhook endpoint and no repository→tenant mapping; issue-to-task existed only on the local path (PX-010, the Core reading an issue through the forge adapter).
- production entry points: `apps/cloud-api/src/forge.rs` (`POST /v1/forge/repositories` map + `GET` list; `POST /v1/forge/github/webhook`: signature under the app's secret, delivery-id-once, repository→tenant mapping, installation and event policy, the canonical task appended or relayed), `crates/event-store/src/cloud/schema.rs` (schema v6: `forge_repositories`, `webhook_deliveries`) and `crates/event-store/src/cloud/mod.rs` (`map_forge_repository` — one tenant per repository, `forge_repository`, `forge_repositories`, `claim_webhook_delivery` — once per id, `finish_webhook_delivery`, `webhook_deliveries`, `denials_for_resource`), `crates/domain/src/task.rs` (`ForgeIssueIntake` — the one rendering both intake paths share: `goal()`, `document_text()`; `TaskCreatedFromIssue.provenance` now `forge_issue` or `forge_webhook`), `crates/protocol/proto/modbit/v1/surface.proto` (`CreateTask.issue_json`), `services/modbit-core/src/server.rs` (origin `forge_webhook` makes the canonical intake from the inline issue), `apps/cloud-worker/src/session.rs` (the relayed `CreateTask` carries origin and the issue to the worker's Core), `apps/cloud-api/src/lib.rs` (the webhook secret in memory only).
- proof: `apps/cloud-api/tests/cloud_api.rs::qual_px_011_a_signed_forge_webhook_makes_the_canonical_task_for_its_tenant_once_and_the_rest_is_refused_and_audited` (the API-append path: a signed opened-issue delivery makes `TaskCreated`(origin `forge_webhook`)/`TaskQueued`/`ContextDocumentAttached`(untrusted)/`TaskCreatedFromIssue`(provenance `forge_webhook`) in the mapping's session, seen by cursor; the issue text kept as data; unsigned, mis-signed, replayed with a changed body, mis-installed, unmapped and cross-tenant deliveries each refused and on the audit; the delivery ledger records each outcome; a label-gated mapping waits for the label; the secret never appears in any response), and `apps/cloud-worker/tests/cloud_worker.rs::qual_px_011_a_webhook_for_a_held_session_is_relayed_and_the_worker_makes_the_canonical_task` (the relay path: a delivery for a session a worker holds is relayed and the worker's Core makes the same four canonical events).

## Limitations

- `pull_request` events are accepted and ignored (`202`); feeding PX-008/PX-009 when configured is later work.
- One provider (`github`) and one signature scheme (`X-Hub-Signature-256`); other forges are not wired.
- Intake is `issues.opened` or `issues.labeled` with the mapping's label; other issue actions are ignored by design.
- The webhook secret is one per API process; per-installation secrets are not modelled (the installation id gates instead).

## Verification

- `apps/cloud-api/tests/cloud_api.rs::qual_px_011_a_signed_forge_webhook_makes_the_canonical_task_for_its_tenant_once_and_the_rest_is_refused_and_audited`
- `apps/cloud-worker/tests/cloud_worker.rs::qual_px_011_a_webhook_for_a_held_session_is_relayed_and_the_worker_makes_the_canonical_task`
- `apps/cloud-api/src/forge.rs` unit tests (signature verifies only under its secret and exact body; the delivery id names the command)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
