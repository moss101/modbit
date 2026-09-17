# Task Card — IMP-EV-0147 Browser/E2E evidence

## Identity

- Task ID: IMP-EV-0147
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0147; disposition ADOPT
- Mandatory behavior: Normalize browser/runtime/test evidence to exact revision/run step.
- Qualification: `QUAL-EV-0147` — Visual/browser test artifact links to revision and verification claim.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Normalize browser/runtime/test evidence to exact revision/run step.

## Existing-code audit

- classification: PARTIAL before: browser evidence events carried fingerprints and egress references but no link to their call.
- production entry points: `crates/domain/src/task.rs` (`tool_call_id` on `BrowserNavigated`, `BrowserPageObserved`, `BrowserActionPerformed`, `BrowserRegionCaptured`, `BrowserCredentialFilled`), `services/modbit-core/src/tools.rs` (set from the call) — the chain is artifact → browser record → tool call (run, turn, model call id, arguments hash) → the run's steps.
- proof: `qual_ev_0082_0234_0277_0282_…` and `qual_ev_0086_…`: every `BrowserRegionCaptured` / `BrowserActionPerformed` names its call; the capture's egress reference is a 64-hex object; the click's verification claim (`postcondition_held`) sits on the same record.

## Limitations

- Browser evidence links to the run through the call, not to a workspace revision (the browser is not the workspace).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0082_0234_0277_0282_an_accessible_form_needs_no_pixels_and_a_canvas_escalates_explicitly`
- `qual_ev_0086_a_visual_fallback_without_a_verified_postcondition_blocks_completion`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
