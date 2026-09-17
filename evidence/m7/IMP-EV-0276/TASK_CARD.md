# Task Card — IMP-EV-0276 Chromium as execution engine, not a new browser engine

## Identity

- Task ID: IMP-EV-0276
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0276; disposition ADOPT
- Mandatory behavior: Build agent-native semantic runtime on Chromium/CDP rather than replacing web engine.
- Qualification: `QUAL-EV-0276` — Modern SaaS fixture executes with standard Chromium compatibility.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Build agent-native semantic runtime on Chromium/CDP rather than replacing web engine.

## Existing-code audit

- classification: IMPLEMENTED by M7.1 (a real Chromium `WebContentsView` driven through CDP; no engine of Modbit's own).
- production entry points: `apps/desktop/src/main/browser.ts` (`webContents.debugger`, CDP 1.3: Accessibility, DOM, Input, Page), `crates/browser` (the protocol over it).
- proof: `browser.spec.ts` (real app): a real page with a form, a canvas, a POST sign-in and a hostile page executes with standard Chromium behaviour; `qual_m7_1_…` on the real Core.

## Limitations

- The fixture is a local page, not a hosted SaaS.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/browser.spec.ts`
- `qual_m7_1_a_browser_session_is_hosted_by_the_desktop_and_driven_through_the_cdp_bridge`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
