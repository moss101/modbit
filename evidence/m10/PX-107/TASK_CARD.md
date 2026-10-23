# Task Card — PX-107 AGENTS.md and CLAUDE.md as a trust-gated native rules layer

## Identity

- Task ID: PX-107 (REQ-PX-107; owner skills); qualification QUAL-PX-107
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Core and the scripted provider capturing the provider request: Modbit's own AGENTS.md rules appear in a run's request in a trusted checkout of this repository and are absent in the same checkout marked untrusted (the Inspector lists them as not loaded with the reason); a nearer-directory file takes precedence over the root file for a touched path; a file above the cap is truncated with the marker and the request stays under the aggregate cap; a planted line in AGENTS.md asking to approve everything or to call a tool changes no policy and no tool projection; editing the file between turns changes the next request and the recorded hash.
- tests: `services/modbit-core/tests/context_rules_pack.rs::px_107_agents_md_reaches_the_model_only_in_a_trusted_repository_with_provenance`, `services/modbit-core/tests/context_rules_pack.rs::px_107_editing_agents_md_between_turns_changes_the_next_request_and_its_hash`, `services/modbit-core/tests/context_rules_pack.rs::px_107_oversize_files_are_cut_with_a_marker_and_the_aggregate_stays_bounded`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
