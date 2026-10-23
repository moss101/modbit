# Task Card — PX-044 Design tokens, UI primitives, tray host and a model-free state gallery

## Identity

- Task ID: PX-044 (REQ-PX-044; owner desktop); qualification QUAL-PX-044
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: check_dossier G11: a COMPLETE work item or an evidence bundle whose test evidence is only gallery specs fails (QUAL-PX-044 negative proof)

## Verification

- tests: `apps/desktop/e2e/gallery.spec.ts`, `packages/design-tokens/src/index.test.ts`, `tools/test_dossier.py::test_gallery_state_counted_as_proof_is_rejected`
- re-run: CI run 39960289486.

## Limitations

- The gallery spec is design-review evidence and is listed beside non-gallery tests; G11 refuses it alone. No gate attested.

## Evidence

- `evidence.json` and `ci-run-39960289486.json` in this directory
