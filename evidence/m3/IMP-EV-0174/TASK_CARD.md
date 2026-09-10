# Task Card — IMP-EV-0174 Fast Context subagent

## Identity

- Task ID: IMP-EV-0174
- Milestone: M3
- Requirement: REQ-EV-0174; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0174` — Specialist has no mutation tools and produces provenance-complete pack.
- Evidence tier: real-system or production-equivalent

## Goal

Bounded read-only retrieval specialist can build ContextPack.

## Existing-code audit

- classification: NOT-FOUND before this task. The agent could build a pack itself with `context.pack`, but every search it needed cost it a turn of its own context; nothing could take a context question off its hands.
- production entry point: `context.fast` in the agent's tool projection (`services/modbit-core/src/runtime.rs`), implemented by `services/modbit-core/src/subagent.rs`; the allowed toolset is `CONTEXT_SPECIALIST_TOOLS` in `crates/core-runtime/src/harness.rs`.
- proof: the specialist is the task's own model on the task's own endpoint, given a smaller world — the retrieval tools its profile and lease already allow, a few turns and one job. Its authority is decided at dispatch, not by the projection: a name outside the read-only set is refused before any tool runs, so a wrong projection could not authorize a write either. A real run proves both halves: the specialist is offered `context.pack` and the search tools and none of `change.apply`, `change.batch`, `shell.exec`, `git.commit`, `task.complete` or `user.ask`; it asks for `change.apply` anyway and is refused with the reason; the file on disk is unchanged; and what it hands back is a real Context Pack whose entries carry path, revision, content hash, reason and sources, reported afterwards by the Context Inspector. Its work is on the canonical log: a ContextCompile step per model call under the actor `context-specialist`, and its tokens recorded on the calling turn, so the task's economics include what the specialist spent.

## Limitations

The specialist runs synchronously inside the caller's tool call, so the agent waits for it. It is bounded by turns (at most 6) and by the pack's token budget, not by a wall clock. It has no memory between calls: each question starts fresh.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0174_a_read_only_specialist_builds_the_pack_and_cannot_mutate`
- `qual_ev_0173_task_economics_report_quality_and_cost_from_the_log`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
