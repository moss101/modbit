# Task Card — IMP-EV-0150 Large-scale parallel execution

## Identity

- Task ID: IMP-EV-0150
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0150; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0150 — Overlapping writes are serialized/denied before execution.
- Evidence tier: real-system (the real Core with real admissions; the conflict rules over a repository fact table in unit tests)

## Goal

Parallelism only for separable tasks with conflicts/merge verification.

## Existing-code audit

- classification: PRESENT since M6.3/M6.4: admission step 4 refuses a write set that overlaps a live child's (explicit paths and globs) and the semantic detector adds symbol ownership, hot spots, migrations/shared config, generated files and lockfiles. No code change in this task.
- production entry points: `services/modbit-core/src/spawn.rs` step 4 (`WRITE_CONFLICT` naming the pairs; `modbit_core_runtime::conflict::detect` over `repo_facts`), `crates/core-runtime/src/conflict.rs`; the parent merges the children's branches deterministically through Git.
- proof: two disjoint builders (`src/a/`, `src/b/`) are admitted in parallel and both branches merge into main with both files present; a third whose scope overlaps `src/a/` is refused `WRITE_CONFLICT` at stage `WRITE_SET` with nothing taken; the unit rules block the same public symbol in two scopes, hot spots and shared generated/fixture files, and pass disjoint scopes.

## Limitations

- Scale is bounded by capacity tickets and the depth-1 fan-out; there is no cross-task (sibling primary) conflict check yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`
- `disjoint_scopes_pass_and_overlapping_ones_block`
- `the_same_public_symbol_in_two_scopes_is_an_ownership_conflict`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
