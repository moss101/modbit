# Task Card — PX-117 More hook points, kernel-gated hook context injection and prompt-type hooks

## Identity

- Task ID: PX-117 (REQ-PX-117; owner extensions-hooks); qualification QUAL-PX-117
- Evidence tier: release-critical
- Prior state: REAL_TESTING since 2026-10-17 (commit 5774ec68, run 38990216920), held only because "the QUAL Core suites run on macOS and Linux only (no Core-level Windows run)".

## Closing (2026-10-27)

- The hold is resolved: #145 (24926b6b, "hooks on Windows") made the hook handlers cross-platform, and CI run 40239702943 (PR #147 head 40ef7536, landed on main as 84eecda2) passes all ten `qual_px_117_*` tests of `services/modbit-core/tests/px_117_hooks.rs` on macOS, Linux and Windows (`ci-run-40239702943.json`; counted from each rust job's log).
- Mutation check retained from the Windows change: `mutation-check.txt` (a hook not sent its request fails the lifecycle-payload suite).
- No product code changed in this closing.

## Limitations

- Prompt-type hooks route through the gateway to the scripted model stand-in, not a live provider. No gate attested.
