# Task Card — IMP-EV-0046 Detached-agent permission ceiling

## Identity

- Task ID: IMP-EV-0046
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0046; owner label: Agent Runtime; subsystem: core-runtime (`services/modbit-core/src/agent_tools.rs::protected_effect`, `runtime.rs` dispatch), desktop renderer (`model.ts`)
- Qualification: QUAL-EV-0046 — Background child reaches protected effect and transitions to parent attention state.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; a real admitted child in its own worktree; the reducer pinned by a unit test)

## Goal

Background agents can consume grants but cannot create interactive privilege expansion.

## Existing-code audit

- classification: PARTIAL before: the child's lease was capped at `REVERSIBLE_WRITE` and its compiled surface omitted the tools its lease could not reach, so a protected effect was refused to the child (`TOOL_NOT_VISIBLE` / `LEASE_CEILING`) — but silently: no approval could open (correct) and nothing reached the parent or the board.
- production entry points: `agent_tools::protected_effect` (registry effect class vs the capsule's `effect_ceiling`; refuses `PROTECTED_EFFECT_REFUSED` before the visibility check, the harness rules and the kernel; appends `SubagentProtectedEffect` + `TaskNeedsAttention` on the parent with a `POLICY / PROTECTED_EFFECT_REFUSED` diagnostic); `runtime.rs` (the first dispatch arm for a capsule-bound task); `crates/domain` `TaskEvent::SubagentProtectedEffect`; the diagnostics corpus case `subagent_protected_effect_refused`; the Fleet reducer arm (evidence line; `TaskNeedsAttention` puts the running parent in Needs Attention).
- proof: the child asks for `git.worktree.close` (DESTRUCTIVE) inside its capsule: the parent's log gets `SubagentProtectedEffect` (child-a, the tool, `DESTRUCTIVE`, ceiling `REVERSIBLE_WRITE`) and a `TaskNeedsAttention` naming them with the diagnostic code, both before the child's result; the child's log has no `ToolCallProposed` for the tool and a `StepFailed PROTECTED_EFFECT_REFUSED`; no approval exists in the session; the child's transcript carries the refusal text; the child completes its scoped write and the parent collects a COMPLETED envelope with the artifact; the worktree is untouched.

## Limitations

- The parent's next action is a decision in words; a one-click "do it from the parent" does not exist yet (the parent agent or the user acts through the parent task's own tools and approvals).
- The child's node stays `BACKGROUND` while the parent decides; cancelling it is the parent's `agent.cancel`.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0046_a_background_child_reaching_a_protected_effect_moves_its_parent_to_attention`
- `REQ-EV-0046: a child's protected effect puts the running parent in Needs Attention with the evidence line (apps/desktop/src/renderer/model.test.ts)`
- `qual_ev_0245_fault_corpus_produces_stable_diagnostic_features`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
