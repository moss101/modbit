# Task Card — IMP-EV-0282 Targeted visual escalation

## Identity

- Task ID: IMP-EV-0282
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0282; disposition ADOPT
- Mandatory behavior: Capture only visual regions that structural state cannot represent.
- Qualification: `QUAL-EV-0282` — Canvas/image button fixture escalates locally; standard form does not.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Capture only visual regions that structural state cannot represent.

## Existing-code audit

- classification: IMPLEMENTED by M7.5.
- production entry points: `crates/browser/src/compiler.rs` (visual regions: canvas, unlabeled image/figure, graphics document), `crates/tools/src/browser.rs` (`browser.capture` region-only).
- proof: `qual_ev_0082_0234_0277_0282_…`: the form does not escalate; the canvas does, to its own box only.

## Limitations

- Regions are detected by role.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0082_0234_0277_0282_an_accessible_form_needs_no_pixels_and_a_canvas_escalates_explicitly`
- `a_canvas_and_an_unlabeled_image_are_visual_regions_a_labeled_image_is_not`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
