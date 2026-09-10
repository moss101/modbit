# Task Card — PX-034 Staged test targeting and mandatory completion run

## Identity

- Task ID: PX-034
- Milestone: M2 (backlog batch 2e: qualification of the M2.7/M2.8 substrate)
- Requirement: REQ-PX-034 (ADOPT); owner label: verification; subsystem: verification
- Qualification: QUAL-PX-034
- Evidence tier: real-system or production-equivalent

## Goal

Real fixture task: TARGETED runs execute failed-first, task-named and changed-file-mapped checks within budget and never support completion; the COMPLETION run executes the full configured suite plus every mandatory check at the final candidate revision before the Acceptance Gate; a ChangeTransaction after the COMPLETION run invalidates it and the completion proposal is refused until it reruns

## Existing-code audit

- classification: FOUND (M2.8) — qualification
- production entry point: TARGETED runs (verify.run: failed-first, task-named, changed-file-mapped) never satisfy completion; task.complete runs the COMPLETION stage at the final candidate revision; a later write moves the candidate revision so acceptance needs a fresh COMPLETION run
- proof: the TARGETED run reports failing checks first and does not complete the task; COMPLETION runs twice in the M2.8 test: first refused on a REGRESSION, then clean at the final revision after the fix

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-core/surface_protocol` :: `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34439622680.json`, `ci-run-34439622680-tests.log`: hosted CI run 34439622680 on fbc7657, green on macOS, Linux and Windows
