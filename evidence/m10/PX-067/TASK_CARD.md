# Task Card — PX-067 Hardened Git execution: repository hooks, fsmonitor, attributes and credential redaction

## Identity

- Task ID: PX-067 (REQ-PX-067; owner workspace-git); qualification QUAL-PX-067
- Evidence tier: release-critical (effect-bearing behavior; see the acceptance below)
- Closed from E2E_PROVEN to COMPLETE on 2026-10-20 after the re-run recorded in evidence.json.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified by the tests below through the real Core)

## Verification

- acceptance: Real Git against hostile fixture repositories: one with a pre-commit hook that writes a marker file, one with `core.fsmonitor` pointing at a script, one with a clean filter attribute, one with a remote URL containing a token; every Core operation (status, diff, worktree add, commit, merge, push to a local remote) leaves no marker, runs no script and writes no token to any log, event or error; the security event is recorded for each neutralised setting.
- tests: `services/modbit-core/tests/worktrees_px.rs::px_067_a_hostile_repository_runs_nothing_and_leaks_no_token_through_core_operations`, `crates/git/tests/px_067_remaining.rs`, `tools/architecture-lint/tests/spawns.rs`
- re-run: macOS local 2026-10-20 and CI run 39029466637 on main ac4aa804, all green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task.

## Evidence

- `evidence.json` and `ci-run-39029466637.json` in this directory
