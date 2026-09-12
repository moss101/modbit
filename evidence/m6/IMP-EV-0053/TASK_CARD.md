# Task Card — IMP-EV-0053 Stall/no-progress detection

## Identity

- Task ID: IMP-EV-0053
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0053; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0053 — Seed repeated read/search loop; watchdog moves run to STALLED with evidence.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server seeding a repeated search loop)

## Goal

Detect repeated low-novelty cycles and surface blocker rather than loop indefinitely (ADAPT: Modbit's stalled state is `Waiting` + `TaskNeedsAttention` with `NoProgressDetected` as the evidence, not a separate run state).

## Existing-code audit

- classification: PRESENT since M2.7 / PX-039 (docs/28 §5 repair policy: `max_consecutive_no_progress_turns`, Alpha default 3). No code change in this task: the audit traces the production path and names the qualification that already runs on every CI push.
- production entry points: `crates/core-runtime/src/harness.rs` (`no_progress_turns`, `check_turn_budget`), `services/modbit-core/src/runtime.rs` (a turn with no transaction, verification, retrieval, plan revision or question counts; `NoProgressDetected { turns }` then `TaskNeedsAttention` "… without progress" and the task waits on the user).
- proof: the scripted model plans, then repeats `search.exact` three turns: `NoProgressDetected` lands once with `turns: 3`, the task is `Waiting(UserInput)` with a `TaskNeedsAttention` reason naming the turns without progress, and `task.complete` never ran — the loop did not continue indefinitely.

## Limitations

- Novelty is measured by the absence of progress-bearing actions per turn, not by comparing tool outputs; a loop that alternates a cheap plan revision with reads is bounded by `max_turns`.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_039_reproduction_first_is_enforced_and_no_progress_turns_escalate`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
