# Task Card — M6.6 attention-first Fleet states/UI

## Identity

- Task ID: M6.6
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/10 "Home / Fleet"; docs/32 "Event consumption"; docs/43 M6.6; owner label: desktop; subsystem: desktop renderer (`apps/desktop/src/renderer/model.ts`, `index.tsx`), Core snapshot (`TaskView.origin` / `parent_task_id`)
- Qualification: M6.6 — the user supervises multiple tasks without raw-log polling: cards show phase, active agents, latest evidence and the next required action; a delegating parent shows its children nested; Needs Attention is derived from structured reasons.
- Evidence tier: real-system (the packaged Electron app driving the real modbit-core with a real admission of a child agent; the model reducer pinned by unit tests)

## Goal

Attention-first supervision as Modbit-native states — Needs Attention, Ready for Review, Running, Waiting, Completed, Failed — with cards that show goal, phase, duration, active agent count, latest evidence, risk and next required action, without interrupting the user for routine progress.

## Existing-code audit

- classification: PARTIAL before this task (M2/M4 UI): cards showed state, wait reason, generation and attachments; Needs Attention came from `TaskNeedsAttention` only; subagents, phases, agents, evidence and risk had no place on the board.
- production entry points: `apps/desktop/src/renderer/model.ts` (`AgentCounts`, `Phase`, `TaskCard.parentTaskId` / `origin` / `agents` / `phase` / `latestEvidence` / `risk` / `children`; the reducer arms for `AgentNodeCreated`, `AgentNodeTransitioned`, `SubagentAdmitted`, `SubagentAdmissionRefused`, `SubagentResultRecorded`, `CapacityDenied`, `CapacityTicketGranted`, `VerificationRunRecorded`, `AcceptanceGateEvaluated`, `ContinuationActivated`, `RealizedRiskDerived`; `childrenOf`; `columns` excluding subagent tasks from the board); `apps/desktop/src/renderer/index.tsx` (`Card` renders phase, agents, risk, evidence, next action and the nested children with test ids); `crates/protocol` `TaskView.origin` / `parent_task_id` and `services/modbit-core/src/server.rs` `GetSessionSnapshot` (the parent from the admission record); `apps/desktop/src/main/main.ts` (the snapshot mapping).
- proof: the packaged app against the real Core: a parent delegates a child through the real admission; its card shows the nested child with the child's goal, the evidence line `delegated child-a`, one top-level card only; when the child ends the parent's evidence reads `child completed: created src/a/a.txt`, agents settle at `0/2`, the child's nested state is `ReadyForReview`, the parent reaches Ready for Review; after a full app restart the board is rebuilt from the Core with the child still nested and never top-level. The reducer's unit test pins every arm: agent counts by status, phases from run-aggregate evidence, a failed child putting the parent in Needs Attention, a refused ticket as a typed capacity wait cleared by the grant, awaiting-human from `TaskWaiting`.

## Limitations

- Duration and execution location are not on the card yet (the wire snapshot carries `created_at` only); objective profile arrives with the routing view.
- Renderer-side steering of children (`agent.cancel` from the card) is not exposed; the CLI and the parent agent cancel.
- REQ-EV-0046's transition of a background child reaching a protected effect into the parent's attention state renders when the Core emits it (M6.7 follow-on); today a child's refusal by its lease shows as a failed child on the parent card.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `fleet: a delegating parent shows its phase, agents and the nested child; the child is never a top-level card (apps/desktop/e2e/fleet-agents.spec.ts)`
- `M6.6: agents, phases, capacity waits and children come from Core events; a child nests under its parent (apps/desktop/src/renderer/model.test.ts)`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
