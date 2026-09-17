# Task Card — IMP-EV-0086 Verified fallback + evidence

## Identity

- Task ID: IMP-EV-0086
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0086; disposition ADOPT
- Mandatory behavior: Fallback action records modality/reason/target and verifies post-state.
- Qualification: `QUAL-EV-0086` — Raw input fallback without post-check is rejected by completion verifier.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Fallback action records modality/reason/target and verifies post-state.

## Existing-code audit

- classification: PARTIAL before: the visual fallback recorded modality, reason and target (M7.5) and verified a declared postcondition; nothing rejected a fallback that declared none.
- production entry points: `services/modbit-core/src/runtime.rs` (`unverified_visual_fallbacks`: the completion verifier refuses `task.complete` with `VISUAL_FALLBACK_UNVERIFIED` while a point click has no postcondition or a failed one and no later verified redo on the same reference), `BrowserActionPerformed.visual_fallback` / `postcondition_held`.
- proof: `qual_ev_0086_…` (real Core, scripted model): a point click without `expect` succeeds and is on the log unverified; completion is refused naming it; the click redone with a postcondition that holds lets completion through; both records name their call.

## Limitations

- Verification is the declared postcondition (URL, text, value, change); the verifier does not judge whether the postcondition was the right one.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0086_a_visual_fallback_without_a_verified_postcondition_blocks_completion`
- `qual_m7_5_a_visual_region_is_captured_targeted_and_clicked_by_a_point_with_the_fallback_on_record`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
