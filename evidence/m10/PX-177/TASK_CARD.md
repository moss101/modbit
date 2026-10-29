# PX-177 task card — typed human handover for steps only the person can do

Decision Record: DR-PX-2026-10-26-018 (D1, D2). Evidence tier: release-critical (a new approval-bound effect-bearing tool and a computer-control session reset).

## Audit (before)
Classification: DOCUMENTED-ONLY. `user.ask` existed but is a free question: no typed reason, no approval expiry, no effect on the control session. Nothing told a computer-use or browser run how to give a CAPTCHA, a payment confirmation, a second factor or a secure field to the person without trying it itself. First missing link: no tool and no state.

## Change
- `crates/tools/src/handover.rs` (new): `handover.request {kind: captcha | payment_confirmation | second_factor | secure_field | other, target: browser | computer, instruction}`; an external side effect under `browser.control`, so it is an approval the person sees. `prepare` binds `{handover: {kind, target, instruction}}` into the approval scope (the person is shown what to do) and refuses an invalid handover before anyone is asked. `invoke` runs only after the person approved ("I did it") and returns `handed_back` with the instruction to observe again; it touches nothing of the target, so the agent cannot complete the step.
- Parking, expiry, restart: the existing approval machinery (the run parks on the approval, `MODBIT_APPROVAL_TTL_MS` expiry closes an unanswered one with `ApprovalExpired` and acts on nothing, the approval is an event on the log).
- `crates/computer/src/runtime/mod.rs::handed_back` + `ComputerHost::handed_back` (called by the runtime after a successful handover): the task's control-session snapshot and frame are dropped, so an element or coordinate from before the handover is refused until the task observes again; the observation record's frame/snapshot are cleared.
- `agent_profiles.rs`: the computer-use child is offered the tool and told when to use it; `services/modbit-core/src/tools.rs` registers it.

## Proof
`services/modbit-core/tests/px_computer.rs` (real Core process, real fixture actuator, real approvals, scripted model):
1. `qual_px_177_a_handover_parks_for_the_person_and_the_run_resumes_from_a_fresh_observation`: the approval's scope carries the kind and instruction; no `ComputerActionPerformed` falls between `ApprovalRequested` and its resolution; after the person approves, the press on the pre-handover snapshot is refused, a fresh `computer.state` follows and the same press then performs (exactly one input in the whole run).
2. `qual_px_177_a_declined_or_unanswered_handover_acts_on_nothing`: a denial returns APPROVAL_DENIED with no `handed_back`; an ignored approval expires (`ApprovalExpired`) and nothing is performed.
3. `qual_px_177_a_restart_while_a_handover_is_parked_keeps_it_parked`: SIGKILL the Core while the handover waits; the new Core lists the same approval (id, intent hash) still REQUESTED, unexpired, with nothing performed.
Mutation: `evidence/m10/PX-177/mutation-check.txt` (the session reset removed → test 1 fails on the stale press).
Full `px_computer` (35 tests), `modbit-tools`, `modbit-computer` and the core unit tests pass; clippy `--workspace -D warnings` and lint are clean.

## Known limits
- The browser stack is not exercised: the tool, its approval and its expiry are target-independent, but a stale-handle reset exists only for computer control; a browser run is told to take a fresh `browser.snapshot`. A browser fixture page with a challenge needs Chromium and is not in this suite.
- The "live view" is the existing desktop approval and attention surfaces showing the bound instruction; no new live-view surface was built.
- After a restart the run itself is not resumed by the Core; the approval stays pending and the person resumes the task as for any parked approval.
- In an unattended profile the tool is refused like any approval-bound call; it is a tool for attended runs.
