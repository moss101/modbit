# Task Card — IMP-EV-0277 Accessibility/DOM/CDP semantic state

## Identity

- Task ID: IMP-EV-0277
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0277; disposition ADOPT
- Mandatory behavior: Fuse AX/DOM/layout/network state into compact model-facing page representation.
- Qualification: `QUAL-EV-0277` — Accessible workflow completes without screenshot/OCR dependency.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Fuse AX/DOM/layout/network state into compact model-facing page representation.

## Existing-code audit

- classification: IMPLEMENTED by M7.2 (AX + DOM node links + layout boxes compiled into entities).
- production entry points: `crates/browser/src/compiler.rs` (`compile`: role, name, value, landmark path, box, DOM node id), `apps/desktop/src/main/browser.ts` (`snapshot`: `Accessibility.getFullAXTree` + `DOM.getBoxModel`).
- proof: `qual_ev_0082_0234_0277_0282_…`: the accessible workflow completes without screenshot or OCR; `qual_m7_2_…`: entities with stable references and boxes.

## Limitations

- Network state is not fused into the representation (URL, title, entities, text and boxes are).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0082_0234_0277_0282_an_accessible_form_needs_no_pixels_and_a_canvas_escalates_explicitly`
- `qual_m7_2_entities_carry_stable_references_and_a_changed_element_resolves_stale`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
