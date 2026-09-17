# Task Card — IMP-EV-0082 Deterministic→accessibility→visual ladder

## Identity

- Task ID: IMP-EV-0082
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0082; disposition ADOPT
- Mandatory behavior: Use deterministic integration, semantic/AX state, then targeted pixels/raw input fallback.
- Qualification: `QUAL-EV-0082` — Accessible form completes with zero screenshot dependency; canvas case escalates explicitly.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Use deterministic integration, semantic/AX state, then targeted pixels/raw input fallback.

## Existing-code audit

- classification: IMPLEMENTED by M7.2–M7.5 (this card ladders the requirement and adds the ladder proof in one run).
- production entry points: `crates/browser/src/compiler.rs` (semantic entities first; `VisualRegion` only where the AX tree cannot represent the control), `crates/tools/src/browser.rs` (`browser.act` on refs; `browser.capture` region-only; a point click only on a visual region), `apps/desktop/src/main/browser.ts` (CDP structural actions; `capturePage` for the region).
- proof: `qual_ev_0082_0234_0277_0282_…` (real Core, fake host): a sign-up form is filled, selected, checked and submitted (under approval) from semantic state alone — the host is never asked for a pixel before the drawn picker; the canvas escalates explicitly to one region capture with the reason on record, then a point click verified by postcondition. `browser.spec.ts` (real app): the login form completes with no capture; the canvas picker is captured region-only.

## Limitations

- No OS-level (non-browser) deterministic integration exists in this build: the ladder's first rung is the browser's semantic state; raw input is the point click inside a captured region.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0082_0234_0277_0282_an_accessible_form_needs_no_pixels_and_a_canvas_escalates_explicitly`
- `apps/desktop/e2e/browser.spec.ts`
- `qual_m7_5_a_visual_region_is_captured_targeted_and_clicked_by_a_point_with_the_fallback_on_record`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
