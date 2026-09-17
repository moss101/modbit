# Task Card — IMP-EV-0087 Human activity preemption

## Identity

- Task ID: IMP-EV-0087
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0087; disposition ADOPT
- Mandatory behavior: Recent human activity revokes/parks controller and enforces cooldown.
- Qualification: `QUAL-EV-0087` — Inject real mouse/keyboard event; automation stops and requires reacquisition.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Recent human activity revokes/parks controller and enforces cooldown.

## Existing-code audit

- classification: PARTIAL before: the takeover was explicit (a button, M7.6); the person's activity in the view did not move the lease.
- production entry points: `apps/desktop/src/main/browser.ts` (`before-input-event` → `preempt`: a real key in the view moves control to the person on the Core and at the host at once; the host's own CDP input is excluded by the in-flight counter and a 250 ms window; `humanInputAt` on `describe`), the Core's `HUMAN_ACTIVE` refusal, the panel's return-control button (reacquisition).
- proof: `browser.spec.ts` preemption test (real app): with the agent's run live, a key from the person moves the lease to generation 2 and the panel to USER; the agent's fill is refused `HUMAN_ACTIVE` and never reaches the host; control returned (generation 3) lets the fill land with its postcondition.

## Limitations

- Mouse activity in the view is not observed by Electron before the page (no `before-input-event` for pointer input); keyboard is; a cooldown beyond the explicit return of control is not enforced.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/browser.spec.ts`
- `qual_m7_6_taking_control_blocks_agent_input_at_once_and_returning_it_moves_the_lease`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
