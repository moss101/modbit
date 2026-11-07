# Task Card — PX-134 Model registry sign, verify and activate verbs on the CLI

## Identity

- Task ID: PX-134 (REQ-PX-134; owner model-gateway); qualification QUAL-PX-134
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: the older-than-active refusal (REGISTRY_OLDER_THAN_ACTIVE) and the fsync before the history is published, with the real-process SIGKILL test (#117)

## Verification

- tests: `apps/cli/tests/px_134_registry.rs::registry_verbs_sign_verify_activate_status_revoke_and_roll_back_through_the_core`, `services/modbit-core/tests/px_134_activation.rs::an_older_bundle_is_refused_until_it_is_rolled_back_to`, `services/modbit-core/tests/px_134_activation.rs::sigkill_during_activation_leaves_exactly_one_whole_active_bundle`
- re-run: CI run 39960289486.

## Limitations

- Verify, sign, activate, rollback, tamper, unknown-key, revoked-binding and key-never-printed are proven through the real CLI and Core; the SIGKILL proof kills the Core at twelve points of an activation rather than at a named instruction; the dev-key verb is not for production; no gate attested.

## Evidence

- `evidence.json` and `ci-run-39960289486.json` in this directory
