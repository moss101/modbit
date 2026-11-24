# PX-071 task card — macOS actuator helper

Record: DR-PX-2026-10-03-008 (phase 5). Row, qualification and card: docs/62 REQ-PX-071, QUAL-PX-071, `#px-071`; docs/66 CUC-D05, CUC-E01, CUC-E02. Evidence tier: release-critical (a process holding Accessibility and Screen Recording).

## Audit (2026-11-23, against main 48d97a05)

Classification at start: IMPLEMENTED-PARTIAL. The helper (`services/modbit-actuator-macos`) was on main and proven through PX-069 (`evidence/m10/PX-069/real-mac-run-2026-11-09.txt`). The graph still said NOT_STARTED with no evidence. The clauses stood as follows.

**Proven on the real helper**
- (1) Driven in the background with the real cursor untouched: `ax_fixture.rs` `a_press_runs_in_the_background_and_the_real_cursor_does_not_move`.
- (9) Killing the helper mid-action sets the latch: `qual_px_069_real_helper_killed_after_delivery_latches_until_a_new_session_observes`.
- N3: no injection after Stop (`ax_fixture.rs`).
- N5: synthetic events are not counted as a person (`ax_fixture.rs`).
- (7), part of it: Stop by direct RPC and by a Core disconnect, both within 500 ms on the real helper.

**Proven partly, or only in unit tests**
- (2) Revocation is simulated with `MODBIT_ACTUATOR_DENY`, not a real permission change.
- (4) A forged token: the real helper with the test acting as the Core.
- (5) A broken signature: through the client with an ad-hoc signature. The real Core runs it with `require_signature: false`.
- (8) The helper exits when its stream ends; no test killed a real Core.
- N2: the Core refusing another pid's welcome is a unit test only.

**Not found**
- (3) Socket squatting. The helper speaks over the Core's own stdio, so there is no socket to squat. The spec rows still describe a socket.
- (6) A real person's keyboard input: `manual_human_input.rs` exists, but no run was retained.
- (7) The on-screen Stop overlay. Not built; the helper does not claim `stop_overlay`.
- N1: an unpinned helper accepted any 32-byte token.
- N4: no static dependency scan.
- N6: no test that a session ends with its turn.
- N7: no test that only the helper shows the permission prompt.

First missing link: N1, a helper launched without a pin welcomed any token (`main.rs` `vet_hello`).

## What was built (this slice)

- **N1, fail closed.** `services/modbit-actuator-macos/src/main.rs` `vet_hello` refuses every hello when `MODBIT_ACTUATOR_TOKEN_SHA256` is absent or empty. The Core pins every launch (`crates/computer/src/actuator.rs`). Raw test launches are now pinned.
  - Test: `handshake.rs` `an_unpinned_launch_welcomes_no_one`.
  - Unit test: `a_hello_is_accepted_only_with_the_right_major_token_length_parent_and_pin`.
- **N4, static scan.** `tools/architecture-lint/rules.toml` gains 22 forbid rules from `modbit-actuator-macos`: secrets, providers, tools, workspace, git, event store, policy, effects, core runtime, browser, sandbox, the Core-side computer crate, HTTP clients, Git libraries, keychains and databases. Credential environment reads were already confined by the `[[credentials]]` lint.
  - Test: `tools/architecture-lint/tests/real_workspace.rs` `qual_px_071_a_helper_holding_a_credential_or_file_path_authority_fails_the_scan`. The real helper is clean; a scratch helper that reaches `modbit-secrets`, `modbit-workspace` or `reqwest` through a middle crate fails the shipped rules.
- **N6, a session ends with its turn.** `px_computer.rs` `qual_px_071_a_control_session_does_not_survive_the_end_of_its_turn` (fixture actuator, real Core). A session the model never released closes `RUN_ENDED`, and the actuator holds nothing afterwards.
  - Mutation: removing the Core's `end_run` call fails this test.
- **(8) A killed Core.** `px_computer.rs` `qual_px_071_the_real_helper_releases_control_and_exits_when_the_real_core_is_killed` (real Core, real helper, real AppKit fixture). The Core is killed with SIGKILL while it holds a control session; the helper exited 15 ms later (bound: 2 s).

## Verification (macOS 27.0.1 arm64, rustc 1.97.1)

- `tests.log`:
  - helper: 26 unit tests, 15 handshake (including the new unpinned-launch test), 15 supply;
  - architecture-lint: 5 real-workspace tests, and the shipped rules pass the real workspace with 0 violations;
  - the N6 fixture test.
- `real-helper-ax-tests.log`: `ax-tests.sh` with `MODBIT_REQUIRE_AX_TESTS=1`, including the Core-kill test.
- `real-helper-ax-tests-run1-flake.log`: one earlier full-script run in which `qual_px_071_the_real_core_drives_a_real_application_through_the_real_helper` saw an empty first tree ("no element Name"). This flake predates this change: it occurred on this branch, which lacks PX-076, and once on the PX-076 branch. It appeared in 2 of about 8 full-script runs and never in 6 isolated runs. Cause not yet found; recorded, not hidden.
- clippy `-D warnings` clean (`clippy.log`).

## Remaining (status REAL_TESTING; owner-dependent unless noted)

- **(3)** Amend CUC-E02 and QUAL-PX-071 to the stdio design. This is a Decision Record against locked rows, so it is the owner's call. The alternative is to build a socket, which would be a regression.
- **(5)** A Developer ID signature with a pinned designated requirement, refused by the real Core with `require_signature: true` when broken. This needs the owner's Apple identity (shared with M10.2 and PX-072).
- **(2)** A real Accessibility revocation in System Settings. A person must change macOS privacy settings; the agent does not.
- **(6)** A real person pressing a key while the helper drives (`manual_human_input.rs`). It needs a person at the machine.
- **(7)** The on-screen Stop overlay is not built, so its design and placement are the owner's call. Stop-to-last-injected timing is measured for the desktop and emergency-stop sources on the fixture actuator only. Measuring them on the real Core and helper is agent work, still to do.
- **N7** Prove that only the helper shows the permission prompt. The desktop never calls `--request-permissions` yet, so this is agent work after the owner decides the onboarding flow.
- **(1)** Add the cursor assertion to the real-Core drive test. Agent work, after PR 248 (which edits that test) merges.
- The empty-first-tree flake above.
