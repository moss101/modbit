# Task Card — PX-126 Check-suite and comment webhooks into CI-result and review-comment ingestion

- Task ID: PX-126 (REQ-PX-126; owner sandbox-cloud; related REQ-EV-0147, REQ-EV-0148); qualification QUAL-PX-126 / PX-E2E-126; DR-PX-2026-10-05-013; release-critical (external events become task input and verification evidence; signature, replay and tenant boundaries).
- Branch `px-126-webhook-ingestion`, from `main` at 98180a4c.

## Audit (2026-11-23)

- Graph: NOT_STARTED; the adoption audit said DOCUMENTED-ONLY. That was stale: wave 2 (#64, d10d46d7) built the event mapping in `apps/cloud-api/src/forge.rs`, the `Ingest:ci` / `Ingest:review_comments` dispatch in `apps/cloud-worker/src/session.rs`, the tenant-scoped `task_for_pull_request` lookup in `crates/event-store/src/cloud/mod.rs`, and two qualifications (`px_126_webhooks.rs`, `px_126_ingestion.rs`) that the hosted cloud CI job runs on Postgres. The row never moved. Doc 24 still said pull-request events were ignored.
- Trace: `POST /v1/forge/github/webhook` → signature → delivery claim (once) → mapping and installation → event-time window → task in the mapped tenant → durable command (relayed or queued) → worker Core `IngestCiResults` / `IngestReviewComments` (PX-009 / PX-008) → `forge.ci.status` / comment read through the kernel with the broker-held token → `CiEvidenceRecorded` / untrusted `TaskInputQueued` → mirrored to the cloud log. Locally, the desktop (`use-pull-request.ts`, `pull-request-app.tsx`) and the CLI (`forge_verbs.rs`) send the same commands.
- Classification: PRODUCTION-WORKING on the wire-faithful GitHub fake (WIRED). First missing links against QUAL-PX-126: (1) "a passing one does not override a failing local check" was not proven through the webhook (only that green CI is not acceptance on a task whose check passed); (2) only the stale and wrong-tenant refusals were asserted on the audit, not replay, unsigned, mis-signed, unmapped or installation mismatch; (3) no mutation evidence (the QUAL requires the skipped-nonce mutation to fail the replay test); (4) no real-GitHub event.

## What was built

No product code changed; the behaviour was already there. Tests and the spec:

- `apps/cloud-api/tests/px_126_webhooks.rs`: every refusal is asserted on the denial audit under its delivery (replay, unsigned, signature, stale past and future, unmapped, installation mismatch, wrong tenant).
- `apps/cloud-worker/tests/px_126_ingestion.rs`: new `qual_px_126_a_green_delivery_never_passes_the_tasks_failing_own_check`. The task's own configured check fails in Modbit at completion (`VerificationRunRecorded` COMPLETION FAILED, not ReadyForReview); a signed green `check_suite` delivery through the real API and worker is recorded as `ci` evidence with conclusion success; `VerificationRunRecorded`, `AcceptanceGateEvaluated`, `RealizedRiskDerived` and `ReviewDecisionRecorded` counts are unchanged, the latest completion run is still FAILED, and the task state is unchanged.
- `apps/cloud-worker/tests/px_126_live.rs` (new, `#[ignore]`, fails without its variables): the real-GitHub event.
- `docs/24_*`: as-built PX-126 paragraph under "Forge webhook intake".

## Verification (macOS 27.0.1 arm64, rustc 1.97.1, Postgres 17 scratch cluster)

- `px_126_webhooks.rs` — 1 passed; `px_126_ingestion.rs` — 2 passed (`cloud-suite.log`).
- Full `modbit-cloud-api` and `modbit-cloud-worker` suites — all passed, the live test ignored (`cloud-suite.log`).
- Mutations (`mutation-checks-2026-11-23.txt`), 5 of 5 caught, sources restored and touched: M1 replay (nonce) check skipped → the replay assertion fails (the QUAL's named mutation); M2 signature check skipped; M3 event-time window skipped; M4 the store's task lookup ignores the tenant → the wrong-tenant step fails; M5 the worker drops `Ingest:ci` → both worker tests fail.
- Real GitHub (`px126-live.log`, 1 passed in 16 s) on the owner's scratch repository `moss101/modbit-forge-test` with PX-125's fine-grained token (`~/.config/modbit/forge-test-token`, through `MODBIT_GITHUB_TOKEN` only): the test opens a real pull request on a `modbit/pr-<task>` branch and posts a real `@modbit` comment as the token's owner. A delivery carrying GitHub's own issue, comment and repository objects, signed with the app's secret, goes to the real API (`202 Ingest:review_comments` for the task). The real worker's real Core reads the comment back from api.github.com and queues one untrusted steering input carrying the real comment's text; nothing is approved; a replay is `409 WEBHOOK_REPLAYED`. A `check_suite` delivery for the same pull request makes the Core read the real check runs: GitHub answers 403 to a fine-grained token (no Checks permission, PX-125's finding), the ingestion is `REJECTED FORGE_FORBIDDEN`, and no evidence is invented; the task stays ReadyForReview; the token is on no event and in no log. Afterwards the repository has no open pull request and only `main`.
- clippy `-D warnings` and fmt clean on the touched crates.

## Limitations

- The delivery's transport in the live run is the test, not GitHub's servers: no public endpoint reaches this machine, and creating a repository webhook needs a permission the scratch token does not have (and is a repository settings change for the owner). The payload is GitHub's real objects for a real event, signed the way GitHub signs.
- A real CI result through the webhook is not proven: the fine-grained token cannot read check runs. That needs a GitHub App or a classic token, as PX-125 recorded; PX-127 needs the same.
- The live test runs on this machine only; CI has no repository secrets for it.
- The "staging Cloud API" is the real API and worker over a real Postgres in-process, as in every cloud qualification here (and the hosted cloud CI job).
