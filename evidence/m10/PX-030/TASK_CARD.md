# Task Card — PX-030 Platform CI compatibility matrix, never release-grade by itself

## Identity

- Task ID: PX-030 (REQ-PX-030, ADOPT; related REQ-EV-0010; owner governance)
- Milestone: M10 / RELEASE_ZERO (rescheduled from M0 by DR-M0-005: the matrix runs from M0.1; the task completes when every named conformance suite exists)
- Qualification: QUAL-PX-030 / PX-E2E-030 (docs/62)
- Evidence tier: release-critical (evidence semantics of the platform label; a security boundary in secret custody)

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The three-OS matrix existed from M0.1 (`rust`, `node`, `desktop-e2e`, `fail-fast: false`), and a summary job printed a fixed "CI_COMPATIBLE only" note whatever the results. Nothing scanned documentation or client text for support claims; neither client stated the platform's state; the PTY session test and every symlink path-policy test were Unix-only; there was no secret-storage conformance suite, and on Linux Electron's `basic_text` fallback (a fixed key) would have counted as keychain custody; packaging did not exist.
- first missing link: no check that text never presents a CI-compatible platform as supported.

## Delivered in this change

- `tools/platforms.json` (recorded states) and `tools/support_claims.py` (scan + self-test; RELEASE_GRADE refused until PX-031 is COMPLETE), run in the dossier job.
- `tools/ci_label.py` and the `CI_COMPATIBLE summary` job (`if: always()`), labelling from the needed jobs' results and failing on any that did not succeed.
- `modbit-cli platform`; the desktop header's `platform-state` (via `localState`, no new bridge request).
- PTY and symlink path-policy tests run on Windows (ConPTY; real file and directory symlinks).
- `apps/desktop/e2e/secrets.spec.ts`; desktop main treats `basic_text` as no keychain (`keychainUsable`, `keychainBackend` in `providerStatus`).

## Finding (fixed here)

- Running the PTY session on Windows for the first time hung the suite, for two reasons, both product bugs of the terminal broker on Windows. First, after the child exited the broker dropped the pseudo-console's master but still held its stdin writer, so the ConPTY stayed open, the reader never reached end-of-file and the exit was never recorded: the broker now releases the writer with the master and bounds the final drain (5 s). Second, portable-pty creates the ConPTY with `PSEUDOCONSOLE_INHERIT_CURSOR`, so the pseudo-console asks its terminal for the cursor position (`ESC [ 6 n`) and holds the child's input until answered — nothing answered, so the child never read a line: the broker, acting as the terminal, now answers (`ESC [ 1 ; 1 R`). The test bounds the whole session (60 s) and reports the output and events it saw, so a regression fails with its evidence instead of hanging.

## Verification

- `python3 tools/support_claims.py --self-test` (six claims caught, six definitions passed) and the repository scan (122 files, none); planted claims in docs/76, `apps/cli/src/main.rs` and the desktop renderer each fail the scan; `windows: RELEASE_GRADE` without PX-031 fails it.
- `python3 tools/ci_label.py --self-test` (success labels; failed, skipped or missing results do not).
- `px_030_the_cli_states_the_platform_as_ci_compatible_and_not_as_support` (apps/cli), `platform.test.ts` (desktop main), `fleet.spec.ts` (the header states CI_COMPATIBLE, never support).
- `secrets.spec.ts` on macOS: the Keychain branch — ciphertext only, owner-only record, no key text or base64 anywhere in the profile, the Core has the provider again after an app restart.
- Windows and Linux results come from the CI run cited in `evidence.json`.

## Remaining before COMPLETE

- The packaging and updater suite: it needs the packaged build of M10.2.
- The required status checks on `main` are a repository setting the owner holds; the summary job fails whenever a platform suite fails, so requiring it makes a failing suite block the merge.

## Evidence

- `evidence.json` in this directory (after CI)
- docs/76 as built
