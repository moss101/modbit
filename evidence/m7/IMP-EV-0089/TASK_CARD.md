# Task Card — IMP-EV-0089 Typed computer-use failure taxonomy

## Identity

- Task ID: IMP-EV-0089
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0089; disposition ADOPT
- Mandatory behavior: Emit stable failures: TARGET_STALE, TARGET_OCCLUDED, WINDOW_UNVERIFIABLE, ACCESSIBILITY_UNAVAILABLE, HUMAN_ACTIVE, MODAL_BLOCKING, TARGET_NOT_EDITABLE, ACTION_UNSAFE, PERMISSION_REQUIRED.
- Qualification: `QUAL-EV-0089` — Fault fixtures trigger each code and verify recovery guidance.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Emit stable failures: TARGET_STALE, TARGET_OCCLUDED, WINDOW_UNVERIFIABLE, ACCESSIBILITY_UNAVAILABLE, HUMAN_ACTIVE, MODAL_BLOCKING, TARGET_NOT_EDITABLE, ACTION_UNSAFE, PERMISSION_REQUIRED.

## Existing-code audit

- classification: PARTIAL before: `TARGET_STALE`, `TARGET_DISABLED`, `USER_HAS_CONTROL`, `EFFECT_RECLASSIFIED` existed under their own names; no occlusion, modal, window, accessibility or permission code.
- production entry points: `crates/tools/src/browser.rs` (`FAILURE_TAXONOMY` with recovery guidance, `recovery_for`; `host_error` passes a taxonomy code through as an application failure with its recovery; `ACCESSIBILITY_UNAVAILABLE` for a loaded page with no tree; `TARGET_NOT_EDITABLE` for entry on a disabled field; `HUMAN_ACTIVE` and `ACTION_UNSAFE` renamed from M7.6/M7.4), `apps/desktop/src/main/browser.ts` (`TARGET_OCCLUDED` by hit test at the click point, `MODAL_BLOCKING` from CDP dialog events, `WINDOW_UNVERIFIABLE` by web-contents identity, `TARGET_NOT_EDITABLE` by the editable check, `PERMISSION_REQUIRED` from the session's permission handler).
- proof: `qual_ev_0089_…` (real Core, fake host): each code is the call's outcome with its recovery text — from the Core (stale, disabled, empty tree, unsafe, human active) and from the host (occluded, modal, window, permission, not editable) — and none is an infrastructure failure. `browser.spec.ts` faults test (real page): an overlay, a div, a geolocation request and an alert produce the codes on a real Chromium.

## Limitations

- `HUMAN_ACTIVE` and `ACTION_UNSAFE` were `USER_HAS_CONTROL` and `EFFECT_RECLASSIFIED` in the M7.4/M7.6 cards' text.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0089_every_failure_code_of_the_taxonomy_is_emitted_with_its_recovery`
- `apps/desktop/e2e/browser.spec.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
