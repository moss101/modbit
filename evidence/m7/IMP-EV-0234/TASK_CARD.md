# Task Card — IMP-EV-0234 Browser snapshot + vision tools

## Identity

- Task ID: IMP-EV-0234
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0234; disposition ADOPT
- Mandatory behavior: Structural semantic browser remains primary; targeted vision only when required.
- Qualification: `QUAL-EV-0234` — Accessible and canvas fixtures validate escalation hierarchy.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Structural semantic browser remains primary; targeted vision only when required.

## Existing-code audit

- classification: IMPLEMENTED by M7.2 (structural snapshot) and M7.5 (targeted capture).
- production entry points: `crates/tools/src/browser.rs` (`browser.snapshot` / `browser.inspect` primary; `browser.capture` region-only), `crates/browser/src/compiler.rs` (visual regions).
- proof: `qual_ev_0082_0234_0277_0282_…`: the accessible fixture completes with zero captures; the canvas fixture escalates once, region-only, with the reason.

## Limitations

- A full-page screenshot as last-resort diagnostic evidence is not offered as a tool.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0082_0234_0277_0282_an_accessible_form_needs_no_pixels_and_a_canvas_escalates_explicitly`
- `qual_m7_5_a_visual_region_is_captured_targeted_and_clicked_by_a_point_with_the_fallback_on_record`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
