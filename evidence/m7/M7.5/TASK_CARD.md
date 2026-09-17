# Task Card — M7.5 Targeted screenshot/vision fallback

## Identity

- Task ID: M7.5
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Action hierarchy" rung 4 (a targeted screenshot/vision action for a canvas, unlabeled or visual-only region; a full screenshot only as last-resort evidence), "V2 media interaction" (browser regions use the same MediaEnvelope/Artifact Store path; vision receives only targeted regions when semantic state is insufficient; any visual fallback records reason, source state version and post-action verification), REQ-EV-0282 (targeted visual escalation), REQ-EV-0223 (targeted media escalation).
- Qualification: docs/51 E2E-014 — a canvas/unlabeled visual control fixture: the semantic compiler marks the visual region, only the targeted region is captured, the model uses the vision fallback, the action succeeds and the evidence records the fallback reason.
- Evidence tier: real-system (the real Core with a fake host; the real app on a real canvas)

## Goal

When the accessibility tree cannot say what a region is, show the model exactly that region and nothing more, let it act on a point inside it under the same verification as any action, and keep the reason for the fallback on the record.

## Existing-code audit

- classification: MISSING before this task: no visual region, no capture tool, no point click; `HostRequest::Capture` existed as a host primitive without a consumer.
- production entry points: `crates/browser/src/compiler.rs` (`VisualRegion`, `VISUAL_ROLES`, `visual_reason`, `PageEntities.visual_regions`; `classify_action` for nameless actions: canvas page-only, other visual regions protected); `crates/browser/src/lib.rs` (`HostRequest::Act.at`); `crates/tools/src/browser.rs` (`browser.capture` through `crate::media::read` with the region's box and provenance; `browser.act` on a visual region with `at`, `VISUAL_POINT_REQUIRED` / `VISUAL_ACTION_UNSUPPORTED`, `visual_fallback` in the answer; `region_json`; visual regions remembered for the effect classification); `crates/domain/src/task.rs` (`BrowserRegionCaptured`, `BrowserActionPerformed.visual_fallback`); `services/modbit-core/src/tools.rs` (both journaled); `apps/desktop/src/main/browser.ts` (boxes for canvas/image/figure; the click at a point clamped to the box); `crates/tools/tool-matrix.json` (the `browser.capture` row).
- proof: `qual_m7_5_a_visual_region_is_captured_targeted_and_clicked_by_a_point_with_the_fallback_on_record` (real Core, fake host): the snapshot marks the canvas with the compiler's reason and its box and lists no entity for it; the capture asks the host for exactly the canvas's clip, comes back as `image/png` through the media pipeline with an egress reference and both reasons, and the next model request carries an image part; the click at (150, 20) reaches the host with the point, the fake picks green and the `text_contains` postcondition holds with `visual_fallback` in the answer; the log holds one `BrowserRegionCaptured` (box, role, reason, a 64-hex egress reference) and one `BrowserActionPerformed` with `visual_fallback.at`, `ReversibleWrite` and the verdict. `apps/desktop/e2e/browser.spec.ts` (real app, a real `<canvas>` painted red/green with an `<output>`): the snapshot's `visual_regions` carries the canvas with its reason, the capture is a 240×60 PNG of it with the stated reason, the click at three quarters of its width and half its height picks green — `picked: green` in the page text, the postcondition held.

## Limitations

- Regions are detected by role (canvas, unlabeled image/figure, graphics document); a visual-only control drawn inside a div with no such role is not marked (the model can still capture any entity's box by reference).
- The capture is normalized to the clip's CSS-pixel size (a HiDPI frame is scaled down); a very large canvas still costs its full pixels within the media budget.
- The host captures through `webContents.capturePage` and parks a view hidden with the panel back in the window for the capture (beside it, out of sight; in place only if that yields no frame), because off the window Chromium composites no frame on Linux and Windows and neither CDP's `Page.captureScreenshot` nor `capturePage` answers there (found on hosted CI runs 35171623072 and 35172860410); a capture over 7 s per attempt is `CAPTURE_TIMEOUT`.
- Clicks only; drags and typing on a visual region are not offered.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_5_a_visual_region_is_captured_targeted_and_clicked_by_a_point_with_the_fallback_on_record` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` (the M7.5 steps)
- `a_canvas_and_an_unlabeled_image_are_visual_regions_a_labeled_image_is_not` (crates/browser)
- Regression: `qual_m7_1_…` … `qual_m7_4_…`, `qual_ev_0217_…` / `qual_ev_0230_…`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
