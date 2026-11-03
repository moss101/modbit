# Task Card — PX-192 Agent workspace language tokens

## Identity

- Task ID: PX-192 (REQ-PX-192; owner desktop); qualification QUAL-PX-192; Decision Record DR-PX-2026-11-03-019
- Evidence tier: iteration (presentation only)
- Prerequisites: PX-143, DOC-PX-018

## Existing-code audit (before any edit)

- classification: PRODUCTION-WORKING token pipeline (one typed function, contrast test) carrying ZCode values; first missing link: no Modbit-own neutral input set.

## What was built

- `workspace-language.ts` holds the light and dark inputs (chrome, sidebar, editor, base, accent, status and mode hues) and derives every `--color-*` variable from the AFW-A11 ladder; `palette.ts` maps the `--mb-color-*` roles from the same inputs; `--ui-font-size` 13 px, body 14/22; GEOMETRY 260/40. The ZCode parity unit tests, `fidelity.ts`, `fidelity-exceptions.ts`, `zdl-tokens.spec.ts`, `fidelity.spec.ts`, `zdl-coverage.ts` and the fidelity gallery are removed (record section 10). Light accent moved from the record's first value to `#145fb4` and dark to `#64a8f0` while building, because the contrast test found accent text on the selected fill at 4.40:1 and the dark primary label under the existing 7:1 headroom assertion; the record carries the final values.

## Verification

- tests: `apps/desktop/e2e/awl.spec.ts`, `packages/design-tokens/src/index.test.ts`; the whole desktop e2e suite against a real Core (evidence.json).

## Limitations

- macOS arm64 only so far; Windows and Linux are CI legs not yet run. Native window chrome and translucency stay with PX-094. Colours are Modbit's own (doc 65 section 2): they are near the reference product, not identical.
