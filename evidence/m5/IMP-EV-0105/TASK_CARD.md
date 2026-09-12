# Task Card — IMP-EV-0105 Skills + workspace rules injected explicitly

## Identity

- Task ID: IMP-EV-0105
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0105; owner label: Instruction Compiler; subsystem: prompt-compiler / skills
- Qualification: `QUAL-EV-0105` — Prompt trace proves rules/skills present only when selected and survive compaction.
- Evidence tier: real-system (the real Core with a compaction epoch; every request the model server received)

## Goal

Rules and skills reach the prompt only by explicit selection, with an instruction manifest that records source, version and reason, and they survive compaction because they are a stable segment, not transcript.

## Existing-code audit

- classification: PARTIAL before: the prompt compiler had a rules segment the runtime left empty; no manifest. With M5.5, `SkillSelected` is the manifest record and `PromptInput.skills` the segment.
- production entry points: `crates/prompt-compiler` (skills inside the rules segment; in the segment hash and the cache key), `services/modbit-core/src/runtime.rs` (selected once per run, injected every turn), `TaskEvent::SkillSelected` (name, version, content hash, source, reason, instructions hash).
- proof: `qual_ev_0105_0114_skill_instructions_survive_compaction_and_the_registry_follows_the_disk`: with a 1500-token compaction budget the run opens a `ContextEpochOpened` and every request before and after it carries `# Skill: reader v1.0.0`; a run with no selected skill sends no skill text at all; the selection record carries the package hash.

## Limitations

- Workspace rules files (repository instructions) are not read yet — the rules segment carries skills; IMP-EV-0059/0129 add path-scoped and layered rules.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0105_0114_skill_instructions_survive_compaction_and_the_registry_follows_the_disk`
- `qual_m5_5_signed_skills_are_selected_compiled_and_recorded_and_unsigned_ones_are_not`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
