# Task Card — IMP-EV-0280 Page/action/state graph

## Identity

- Task ID: IMP-EV-0280
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0280; disposition ADOPT
- Mandatory behavior: Record observed transitions/compound actions as evidence/cache, not universal authority.
- Qualification: `QUAL-EV-0280` — Known flow can reuse verified transition; changed page invalidates fingerprint.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Record observed transitions/compound actions as evidence/cache, not universal authority.

## Existing-code audit

- classification: MISSING before: transitions were on the log (M7.4) but never offered back.
- production entry points: `crates/browser/src/lib.rs` (`KnownTransition`; `BrowserPort::remember_transition` / `transitions_from`), `services/modbit-core/src/browser.rs` (per-session bounded store; rebuilt from `BrowserActionPerformed` after a restart), `crates/tools/src/browser.rs` (`browser.act` records; `browser.snapshot` offers `known_transitions` for the page's fingerprint).
- proof: `qual_ev_0280_…` (real Core): after a verified click, a fresh read of the same page (same fingerprint) offers the transition with its verification and destination; a changed page (another fingerprint) offers nothing; the record is rebuilt after a Core restart.

## Limitations

- Transitions are per session (not shared across tasks or sites); the model may reuse one, the runtime never replays one on its own.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0280_a_known_transition_is_offered_for_the_same_fingerprint_and_not_for_a_changed_page`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
