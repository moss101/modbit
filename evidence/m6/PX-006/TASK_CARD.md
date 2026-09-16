# Task Card — PX-006 GitHub forge adapter behind the External Tool Hub

## Identity

- Task ID: PX-006
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-006; owner label: external-tools; subsystem: `crates/tools/src/forge.rs`, `services/modbit-core/src/forge.rs`, policy kernel default lease, protocol (`ConfigureForge`), tool matrix
- Qualification: QUAL-PX-006 — a real GitHub test repository through the forge adapter: read an issue, create and update a PR, read review comments and check-run status, each with capability lease, effect class, receipt and idempotency key; token supplied by the secret broker only; a call without lease or with a token in arguments is rejected; a retried create with the same idempotency key yields one PR; egress to any other host is denied.
- Evidence tier: real-system (the real Core and kernel; a wire-faithful GitHub REST fake standing for api.github.com — the real-repository run waits for a token, DR-M6-002)

## Goal

Implement the `forge.*` tool family for GitHub behind the External Tool Hub rules with capability leases, effect classes, receipts, idempotency keys and broker-supplied tokens (docs/17, docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: docs/17 admitted the family (DR-PX-2026-09-05); no `forge.*` tool, no forge configuration, no credential custody for a forge, `crates/secrets` empty, the External Tool Hub (M9.4) unbuilt.
- production entry points: `crates/tools/src/forge.rs` (`forge.issue.read`, `forge.pr.create`, `forge.pr.update`, `forge.pr.comments.read`, `forge.ci.status`; `ForgeConfig`, `ForgeLedger`, `read_issue`; token-in-arguments and egress refusals; duplicate-head reconciliation), `services/modbit-core/src/forge.rs` (`ForgeCustody` from `MODBIT_GITHUB_TOKEN` / `ConfigureForge`; `LogLedger` over `ForgePullRequestOpened` / `ForgePullRequestUpdated`), `services/modbit-core/src/tools.rs` (the family registered; `InvokeContext.forge` / `forge_ledger`), `crates/policy/src/kernel.rs` (`network.egress` + `secret.use` in the trusted profile's default lease, scoped to the forge host and token handle), `server.rs` (`ConfigureForge`), `surface.proto`, `crates/domain` (the two events), `crates/tools/tool-matrix.json` (five rows, the `forge` namespace).
- proof: against the GitHub fake: `forge.issue.read` returns provenance `forge_issue` data tagged `UNTRUSTED_EXTERNAL_CONTENT` with the bearer token on the wire and no token in the result and no receipt; a URL on another host is `EGRESS_DENIED` and a credential-looking argument `TOKEN_IN_ARGUMENTS`, both before anything is sent; `forge.pr.create` is `APPROVAL_PENDING` until approved (nothing sent), then opens PR #1 with one receipt; the same idempotency key again replays from the log with no create sent; a key the log does not know meets the forge's `422 already exists` and reconciles to PR #1 — one pull request throughout; `forge.pr.update` is approved, receipted and PATCHes; comments and check runs read as untrusted data; every request carried the Core's token, no event carries it; `ForgePullRequestOpened` × 2 (two keys, one PR) and `ForgePullRequestUpdated` × 1 on the log; a `review_isolated` task is refused by the kernel with nothing sent.

## Limitations

- the real GitHub test repository run waits for a token (DR-M6-002); the token custody is the Core's memory-only custody until the M8.6 secret broker; other forges are not implemented.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_006_the_forge_adapter_reads_and_writes_github_behind_leases_effects_receipts_and_idempotency_keys`
- `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-34761496840.json` (main at 299f8be; the change commit c2441aa, the DR landing note 76256dd and its manifest reseal 299f8be)
