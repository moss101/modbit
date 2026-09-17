# Task Card — IMP-EV-0085 Watchdog / emergency stop

## Identity

- Task ID: IMP-EV-0085
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0085; disposition ADOPT
- Mandatory behavior: Host-owned timeout/kill independent of model loop.
- Qualification: `QUAL-EV-0085` — Emergency stop halts input within safety bound and records reason.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Host-owned timeout/kill independent of model loop.

## Existing-code audit

- classification: PARTIAL before: `EmergencyStop` blocked effects at the kernel; a navigation was a read, and the host had no stop of its own.
- production entry points: `crates/tools/src/browser.rs` (`browser.navigate` is a reversible write of the session — halted by the stop like any effect; reads continue), `crates/policy/src/kernel.rs` (`EMERGENCY_STOP` denies effects), `apps/desktop/src/main/browser.ts` (`emergencyStop(sessionId, reason)`: the host's own fence, `EMERGENCY_STOPPED`, for the session's life; per-request bounds — settle ≤ 8 s, capture ≤ 7 s per attempt, click dispatch ≤ 1.5 s — independent of the model loop), `apps/desktop/src/main/main.ts` (`browser:emergencyStop`; an `EmergencyStopActivated` event from anyone fences the host too), the Browser panel's `browser-emergency-stop`.
- proof: `qual_ev_0085_…` (real Core): after the stop the next navigation and action are refused before the host (nothing more reaches it), observation continued before, and `EmergencyStopActivated` carries the reason. `browser.spec.ts` (real app): the button on the panel fences the host (`emergency-stop` in its log, no `act` after it) and the Core refuses the agent's next input.

## Limitations

- A stop is for the session's life (the Core never lifts it); an action already in flight completes within the host's bound.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0085_an_emergency_stop_halts_browser_input_before_the_host_with_the_reason_on_record`
- `apps/desktop/e2e/browser.spec.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
