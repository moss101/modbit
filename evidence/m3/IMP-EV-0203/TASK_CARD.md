# Task Card — IMP-EV-0203 Inference agent isolated from evolution wiki

## Identity

- Task ID: IMP-EV-0203
- Milestone: M3
- Requirement: REQ-EV-0203; owner label: Context Policy; subsystem: context-engine
- Qualification: `QUAL-EV-0203` — Prompt audit confirms evolution store is absent during normal run.
- Evidence tier: real-system or production-equivalent

## Goal

Production agent receives approved skill, not raw optimization wiki/traces unless task separately retrieves authorized evidence.

## Existing-code audit

- classification: NOT-FOUND before this task, because there was no generated store to keep out of the prompt. IMP-EV-0060 created one, and this task holds the line: it stays out unless the task asks for it.
- production entry point: the prompt envelope in `crates/prompt-compiler/src/lib.rs` (the segments a run is compiled from) and the `knowledge.map` tool, which is the only way the map reaches a run.
- proof: the prompt is compiled from named segments — system rules, workspace rules, the compaction epoch, the task's Context Pack, the harness state, the transcript — and the repository map is none of them. It reaches a run only as the result of a tool call the task made, in the transcript, where it is labelled and checked. The audit runs on a real task: every non-tool message of every request is checked for the map's object ref and for its own text, and the first request of the run, before any tool call, carries no part of it. The map's payload says what it is, so even when a task does retrieve it, it arrives as a discovery aid rather than as instructions.

## Limitations

There is no optimization/evolution trace store yet (EPR-010 to EPR-012 own that), so what this audit keeps out of the prompt is the repository knowledge map, the only generated store that exists. The audit checks the requests the provider received, which is what the model actually saw.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0060_0203_the_repository_map_flags_stale_claims_and_never_enters_the_prompt_by_itself`
- `qual_ev_0169_context_pack_reaches_the_prompt_with_provenance_or_not_at_all`
- `stable_segments_share_cache_keys_and_projection_changes_are_hashed`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
