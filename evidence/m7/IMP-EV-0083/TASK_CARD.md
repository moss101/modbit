# Task Card — IMP-EV-0083 Exact application identity

## Identity

- Task ID: IMP-EV-0083
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0083; disposition ADOPT
- Mandatory behavior: Resolve approved app/window/process identity before actions.
- Qualification: `QUAL-EV-0083` — Window title spoof cannot substitute a different process identity.
- Evidence tier: real-system (the real app on a real page; the real Core with a fake host)

## Goal

Resolve approved app/window/process identity before actions.

## Existing-code audit

- classification: PARTIAL before: a session was bound to its partition (M7.1) and a host to its connection; the answering view was not re-verified per request.
- production entry points: `apps/desktop/src/main/browser.ts` (`HostedSession.webContentsId`: the exact web contents attached, verified before every request — `WINDOW_UNVERIFIABLE` otherwise; `describe` reports the web contents id and the OS process id), `services/modbit-core/src/server.rs` (a host attaches to a session only in the session's partition — `PARTITION_MISMATCH` — and only while no other host holds it), `crates/tools/src/browser.rs` (actions resolve by entity identity inside the attached view; a title is untrusted text).
- proof: `browser.spec.ts` prompt-injection test (real app): a page whose title claims to be another is driven and reported under the identity of the exact web contents attached — the host's `webContentsId` and `osProcessId` equal the real view's, the partition is the session's; the title changes nothing. `qual_ev_0089_…`: a host that cannot verify its view answers `WINDOW_UNVERIFIABLE`, an application failure with its recovery, never a click elsewhere.

## Limitations

- Identity is the browser's own web contents; there is no approved-application registry for OS windows outside the browser in this build.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/browser.spec.ts`
- `qual_ev_0089_every_failure_code_of_the_taxonomy_is_emitted_with_its_recovery`
- `qual_ev_0110_0084_a_forged_peer_cannot_attach_and_a_second_host_gets_the_conflict`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
