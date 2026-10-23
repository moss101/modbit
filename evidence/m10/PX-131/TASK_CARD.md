# Task Card — PX-131 AuthorizationEpoch and CapabilitySnapshot: the capability view frozen per model round

## Identity

- Task ID: PX-131 (REQ-PX-131; owner effects-security); qualification QUAL-PX-131
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: the last QUAL clause: a skill revoked mid-round stays in that round's snapshot and is absent from the next

## Verification

- tests: `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_a_policy_tightened_mid_round_applies_from_the_next_round_with_both_epochs_recorded`, `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_an_emergency_stop_takes_effect_inside_the_round`, `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_a_core_killed_mid_round_resumes_the_same_snapshot_and_epoch`, `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_a_core_killed_between_epoch_changes_continues_the_sequence`, `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_receipts_carry_the_decision_epoch_and_a_forged_epoch_fails_the_chain`, `services/modbit-core/tests/px_131_epoch.rs::qual_px_131_a_skill_revoked_mid_round_is_in_that_round_and_absent_from_the_next`
- re-run: CI run 39960289486.

## Limitations

- Unix-only suite (shell calls): the Windows leg is not run; the revocation of the approving principal is covered by the emergency-stop path; no gate attested.

## Evidence

- `evidence.json` and `ci-run-39960289486.json` in this directory
