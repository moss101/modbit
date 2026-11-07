# Task Card — PX-049 Window lifecycle: size, quit with running work, session-end hook and recovery presentation

## Identity

- Task ID: PX-049 (REQ-PX-049; owner desktop); qualification QUAL-PX-049
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-24 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: #134 added the Core session_end hook point and EndSession command, the three-way quit prompt (keep open / quit and resume / quit and leave suspended), the recovery re-send after page load, and the hardening hash guard

## Verification

- tests: `apps/desktop/e2e/lifecycle.spec.ts (opens at 1280 x 800 and cannot go below 900 x 600; quitting or closing with work running asks first; does not ask when idle)`, `apps/desktop/e2e/session-end.spec.ts (resume chosen: recovered run with the Core's counts and the run continues; leaving suspended: nothing continues; a 2 s session-end hook completes before exit with its sentinel; a failing hook is shown and journaled; killing the Core while the hook runs leaves the task recoverable)`, `services/modbit-core/tests/px_049_session_end.rs (5 tests: resume survives a real kill and is spent by the resume, without resume, 2 s hook, failed/timed-out/malformed hooks)`, `apps/desktop/src/main/hardening-guard.test.ts (preload, CSP and webPreferences pins over the LF form)`, `apps/desktop/src/main/lifecycle.test.ts (window constants, quitChoice)`
- re-run: CI run 40076778858.

## Limitations

- The QUAL asks that the main.ts diff show only window options and quit handling; the #134 diff also holds the recovery re-send and the resume call, which is recovery presentation under PX-049 and needs a reviewer to accept it. Cited run 40076778858 is the green PR run on head f3081826; the squash commit cc41fce2 has an identical tree (git diff empty), and main's own run on cc41fce2 failed on hosted-macOS actuator tests unrelated to PX-049 (a rerun was requested). Hook output journaled is the hook's answer reason, not its stdout/stderr; a session with no tasks runs no hooks; the Core never resumes tasks at boot on its own, a headless client calls StartTask. No gate attested.

## Evidence

- `evidence.json` and `ci-run-40076778858.json` in this directory
