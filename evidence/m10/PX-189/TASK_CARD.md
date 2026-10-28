# Task Card — PX-189 A trace schema for execution evidence and a Core producer that feeds the Skill Evolution Lab

## Identity

- Task ID: PX-189 (REQ-PX-189; owner skills); qualification QUAL-PX-189; prerequisites DOC-PX-017 (#156) and, for the configuration link, PX-182 (#155): this branch is stacked on #155.
- Evidence tier: release-critical (data leaving a run; must be redacted and read-only)

## Existing-code audit (main 5b17f473)

- Classification: IMPLEMENTED-PARTIAL. `crates/skills/src/evolution.rs` (M5.7) had sealed traces, patterns, proposer and promotion, fed only by benchmark harnesses and hand-built traces; no Core producer existed, and the trace held no tool steps, memory or skill configuration.
- Owners kept: the lab (`crates/skills::evolution`), the Core's skills module for the producer; no write to the log, the registry, memory or policy.

## Built

- `evolution.rs`: `TraceStep {tool, arguments, response, error}`; `EvolutionTrace` gains `skill_configuration_id`, `memory`, `steps` (serde-default and skipped when empty, so the ids of traces sealed before are unchanged).
- `services/modbit-core/src/skills.rs` `spawn_trace_producer` (started in `server.rs`): off unless `MODBIT_SKILL_LAB_DIR`; on each `RunCompleted | RunFailed | RunCancelled` it reads the run's records and seals one trace: the compiled configuration (PX-182), injected memory ids, each tool call with its arguments and response passed through the Core's redactor and cut to 2,000 characters, the failure code, the capability snapshot hash, the verification status, observations (`read-before-edit`, `tests-before-complete`, `uses:<tool>`), tokens, tool calls and wall time.

## Proof (local, macOS arm64)

- `services/modbit-core/tests/surface_protocol.rs::qual_px_189_a_finished_run_becomes_a_sealed_redacted_trace_the_lab_proposes_from`: a real Core with a lab directory, a trusted skill and a scripted model that reads a file holding a planted provider key; the run (waiting on the acceptance gate's review, then cancelled) yields exactly one sealed trace naming the run's `SkillConfigurationCompiled` id, with both reads as steps and the key's step response `[redacted]`; the key is absent from the trace file; the log did not move after the producer ran; the real `Maintainer` and `Proposer` turn it into a candidate and the skill file is byte-identical; a Core given no lab writes none.
- Re-run: the skill-lab tests of `surface_protocol` (`wsk_*`), `qual_px_182`, the skills crate and the skill-evolution benchmark tests; workspace `cargo check`; clippy.
- Mutation check: `mutation-check.txt`.

## Limitations

- Harness steps (`plan.update`, `task.complete`) are not tool calls and are not trace steps. A run that suspends is not traced until it ends. Graph status cannot move until #156 lands on main (and #155 first).
