# Task Card — PX-019 Self-review and completion contract

## Identity

- Task ID: PX-019
- Milestone: M2 (backlog batch 2e: qualification of the M2.7/M2.8 substrate)
- Requirement: REQ-PX-019 (ADOPT); owner label: verification; subsystem: verification
- Qualification: QUAL-PX-019
- Evidence tier: real-system or production-equivalent

## Goal

Real fixture: the SelfReview step lists plan coverage, executed verifications, receipts, scope and leftovers; unresolved findings block the completion proposal; the Acceptance Gate, not the agent, decides completion

## Existing-code audit

- classification: FOUND (M2.7/M2.8) — qualification
- production entry point: task.complete carries the SelfReview (findings with resolved flags, verification evidence); SelfReviewRecorded on the log; unresolved findings refuse the proposal; the COMPLETION run and attribution (the Acceptance Gate) decide, never the agent's text
- proof: two SelfReviewRecorded events in the M2.8 test: the first proposal is refused by the gate on a REGRESSION although the agent claimed done, the second accepted after the fix; a proposal without a plan is refused (plan gate)

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-core/surface_protocol` :: `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review`
- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
