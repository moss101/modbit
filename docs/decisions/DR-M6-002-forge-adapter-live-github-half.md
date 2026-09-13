---
id: DR-M6-002
title: The forge adapter, pull requests and issue intake seal on a wire-faithful GitHub fake; the real-repository run waits for a token
status: accepted
date: 2026-09-13
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; evidence-scope decision applying DR-M2-001, DR-M3-002, DR-M3-003 and DR-M6-001 (live proof pending credentials) to PX-006, PX-007 and PX-010, under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M6-002 — PX-006/007/010 seal on the wire-faithful fake; the real GitHub run waits

## Trigger / evidence

QUAL-PX-006 names "a real GitHub test repository through the forge
adapter" with "token supplied by the secret broker only"; PX-E2E-006 names
"real GitHub test repository, broker-held token"; QUAL-PX-010 names "a real
issue URL". The repository holds no forge credential and no test
repository is provisioned (DR-M2-001's condition, confirmed on 2026-09-13
for provider keys and holding for a GitHub token as well).

Everything else the three qualifications name is provable here, and is:

- the `forge.*` family behind the kernel — capability leases, effect
  classes, receipts, idempotency keys, the token in the Core's custody and
  in the `Authorization` header only, egress pinned to one host, refusal of
  a token in arguments and of any other host, one pull request per key
  through a retry and through a forge that already has the head, a profile
  without the capability refused — against a wire-faithful GitHub REST
  fake that answers with GitHub's shapes, statuses and duplicate refusal and
  records every request with whether it carried the bearer token
  (`qual_px_006_…`);
- pull requests from a reviewed result: the dedicated branch pushed
  through the typed Git operation to a real bare remote reached through
  git's own `insteadOf` rewrite of the forge URL, only after approval;
  the evidence summary in the body; a denied approval leaving no branch;
  a stale candidate refused; the same revision replayed; a later reviewed
  revision updating the same pull request with a new receipt
  (`qual_px_007_…`);
- issue intake from the desktop's New Task screen and the CLI on the same
  Core: the issue read first, an unreadable one refused with no task, the
  task named after the issue, its text attached as untrusted context with
  provenance `forge_issue`, the ordinary loop run, the lease unchanged
  whatever the issue says (`qual_px_010_…`, `apps/desktop/e2e/issue-intake.spec.ts`).

What only a real forge can show is that GitHub's production API accepts
what the fake accepts; the fake reproduces the documented request and
response shapes the adapter uses.

## Current behavior

No `forge.*` tool existed; `crates/secrets` and the External Tool Hub
(M9.4) are unbuilt. The token custody here is the Core's in-memory custody
established for provider keys (PX-022); the secret broker of M8.6 will take
it over behind the same `secret.use` capability.

## Proposed replacement

1. PX-006, PX-007 and PX-010 ship as above, run by `.github/workflows/ci.yml`
   on macOS, Linux and Windows.
2. The real-repository run — the same tests with `MODBIT_GITHUB_API_BASE_URL`
   unset, a `MODBIT_GITHUB_TOKEN` repository secret and a dedicated test
   repository named by `MODBIT_GITHUB_TEST_REPO` — runs in
   `.github/workflows/live-providers.yml` once the owner adds the secret,
   exactly as DR-M2-001, DR-M3-002, DR-M3-003 and DR-M6-001 defer theirs.
   Its first green run is appended to each task's `evidence.json`.
3. The three tasks seal on item 1 with their task cards stating the open item.

## Migration

None: new tools, new commands, additive fields (`CreateTask.issue_url`,
`TaskCreated.goal_text`), new record-only events, and `network.egress` +
`secret.use` added to the trusted profile's default lease (scoped to the
forge host and token handle; the isolated and autonomous profiles are
unchanged and still denied).

## Compatibility

No interface changes meaning. Leases minted before this record lack the
two operations and refuse `forge.*` until the task is recreated.

## Security impact

The forge token joins the provider key in the Core's memory-only custody
(docs/23): never journaled, logged, stored, echoed or returned; refused in
arguments; used in one header for one host. Writes remain approval-bound
external effects with receipts.

## Test impact

`qual_px_006_…`, `qual_px_007_…`, `qual_px_010_…` in the surface suite,
`issue-intake.spec.ts` in the desktop E2E, the tool matrix rows and
namespace justification, the policy kernel tests. The live half is the one
deferred to item 2.

## Rollback

Revert the commits that introduce the tools, commands and the lease
operations. Nothing on the log changes shape.
