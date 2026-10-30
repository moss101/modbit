# Task Card — PX-182 Scenario adapter: per-tool skill guidance compiled for the tools actually exposed, as a versioned configuration

## Identity

- Task ID: PX-182 (REQ-PX-182; owner skills); qualification QUAL-PX-182; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: release-critical (what instructions reach the model; a new task event and wire field)

## Existing-code audit (main 5b17f473)

- Classification: NOT-FOUND for guidance blocks and the configuration record; PRODUCTION-WORKING for what it builds on. `modbit_skills::compile` already intersected a skill's `required_tools` with the task's tool surface and noted the missing ones, but the body was injected whole, so advice about a tool the task cannot call reached the model; nothing recorded the compiled set as one versioned artifact.
- Owners kept: `crates/skills` (`compile`), `services/modbit-core/src/skills.rs` (the per-round skill segment), the inspector's accounting. Trust, budget and loading rules unchanged.

## Built

- `crates/skills/src/lib.rs`: guidance blocks fenced `<!-- modbit:tool NAME -->` … `<!-- /modbit:tool -->`; `bind_guidance(body, projection)` keeps a block whose tool the surface offers (the tool or one under it) without its fences and drops any other whole; `CompiledSkill.guidance` lists each block as `(tool, kept)`; unit test.
- `services/modbit-core/src/skills.rs`: each round compiles the selected skills against the run's tool surface into a canonical JSON configuration (`modbit.skill-configuration/1`: skills with content and instruction hashes and their blocks, the sorted tools); its sha256 is the id; the object is stored and `SkillConfigurationCompiled {configuration_id, configuration_ref, skills, guidance_kept, guidance_dropped}` recorded on the run's lineage when it changes.
- `crates/domain`: the event. `run_control.proto` `ContextAccountingView.skill_configuration_id = 23` (additive, TS regenerated, protocol fixtures updated on both sides); the inspector fills it from the log.

## Proof (local, macOS arm64)

- `services/modbit-core/tests/surface_protocol.rs::qual_px_182_guidance_blocks_compile_for_the_tools_the_task_is_offered` (real Core, real skill files, owner trust): under the read-only `plan` profile the shell block is dropped and the read block kept; under `local_trusted` both are kept; a block for an unoffered tool never reaches a request and no fence leaks; the stored object's sha256 equals the recorded id and the accounting reports it; a second task with the same inputs compiles to the same id, a different tool set to a different one; an untrusted skill named explicitly compiles nothing.
- Existing skill tests (9/9), the skills crate tests, protocol fixtures both ways, workspace `cargo check`; clippy.
- Mutation check: `mutation-check.txt` — every block kept, a time stamp in the configuration, no record; all KILLED.

## Limitations

- The tool surface is the run's visible tool set at start (profile and lease), not each round's narrower projection after policy decisions. Graph status cannot move until PR #148 (which registers PX-182) lands on main.
