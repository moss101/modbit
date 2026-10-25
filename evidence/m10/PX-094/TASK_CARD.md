# Task Card — PX-094 Window chrome

## Identity

- Task ID: PX-094 (REQ-PX-094; owner desktop); qualification QUAL-PX-094
- Evidence tier: release-critical (main-process window options, adjacent to the security boundary)

## Existing-code audit

- classification at start: DOCUMENTED-ONLY for chrome and window state; the 8 percent zoom model already existed in the renderer (PX-045, per-viewer localStorage)
- first missing link: `new BrowserWindow` took only size options; no title bar, drag region, state store or translucency

## Verification

- `apps/desktop/src/main/window-chrome.test.ts`, `hardening-guard.test.ts` (pnpm test: 305 of 305 pass, exit 0)
- `apps/desktop/e2e/window-chrome.spec.ts` on the real app and real Core: 6 passed, 1 skipped (exit 0); regression shell/lifecycle/agents/keyboard/gallery: 23 passed
- mutation checks: `mutation-check.txt`

## Limitations

- Not proven: each supported macOS version (control positions measured on Darwin 27 only), double-click-to-zoom, a real drag, an OS Reduce transparency toggle (simulated with MODBIT_REDUCE_TRANSPARENCY=1), multi-display hardware, the packaged (not dist) app. Translucency opt-in has no settings control. COMPLETE is gated on DOC-PX-012 (REAL_TESTING).
