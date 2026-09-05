# Agent competence: planning, verification and repair

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 item 6 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`).  
> **Owners:** core-runtime (planning, repair loop control, escalation), context-engine (retrieval before edit), workspace-git (change strategy), verification (verification plan, self-review, completion), skills (role prompts that carry these contracts), eval-bench (measurement, `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`). No new canonical subsystem.

## Why this document exists

The rest of the dossier governs the agent: what it may touch, what counts as proof, how it recovers. This document specifies how the agent **works**: the contracts that turn a goal into a verified change. Governance without competence produces safe failures; competence without governance produces unsafe successes. Both are required, and results are what decide whether Modbit is better than the alternatives.

Every contract below is a Core-enforced behavior with events and evidence, not a prompt suggestion. Prompts and skills express them; the runtime checks them.

## The competence loop

```text
understand → plan → retrieve before edit → change strategy → verify → repair (bounded, evidence-driven) → self-review → complete
```

The loop runs inside the initial leg of a ConditionalExecutionPlan and inside any revision leg. Escalation to a stronger solver, independent review or a human uses the prevalidated continuation slots of `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`; competence contracts never invent a branch.

## 1. Understanding and planning (PX-014)

- **Clarification policy.** The agent asks the user only when the goal is ambiguous in a way that changes the change set, the verification, or a protected effect. It never asks to confirm what it can verify from the repository. Questions are typed `UserQuestion` steps with the concrete alternatives.
- **Plan artifact.** Before the first write the agent records a plan through `plan.update`: the observable outcome, the files and symbols expected to change, the verification it intends to run, the protected effects it foresees, and the budget it expects. The plan lives in the WorkGraph, not in the transcript, and is shown in the task timeline. Plan revisions are events.
- **Decomposition.** Subtasks follow `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`; a plan that proposes parallel builders states their write sets so the conflict detector can rule.

## 2. Retrieval before edit (PX-015)

No edit to a file the agent has not read at the current workspace revision. Before changing a symbol the agent has its definition and references from the Context Engine (`18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`), and the Context Ledger records what was retrieved and later whether it was used. Editing a file without a retrieval record for its revision is a Core-rejected `ToolCallPolicyDecision`, not a style violation.

## 3. Change strategy (PX-016)

- Small, reviewable, revision-bound diffs applied through the Change Engine; one concern per ChangeTransaction where the task allows.
- Tests first when the task adds or changes behavior with an existing test harness: write or extend the failing test, then make it pass. Where no harness exists the plan says so and the verification plan compensates with compile, diagnostics and a reproduction command.
- Never widen scope silently: files outside the plan's expected set trigger a plan revision event and, when policy requires, a question.
- Generated files, lockfiles and migrations are changed only through their generators or with an explicit plan entry.

## 4. Verification plan derivation (PX-017)

The Verification Engine derives the plan from task type, changed files, repository configuration and the agent's proposals (`33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md`): build, typecheck, lint, targeted tests, diagnostics delta, diff invariants, security rules. The agent may add checks; it cannot remove mandatory ones. The derived plan is recorded before the first verification run so that "what would have proven this" is auditable even when the task fails.

## 5. Bounded, evidence-driven repair (PX-018)

Repair is the part most systems leave to chance. Modbit makes it a record.

```text
RepairAttempt {
  attempt_ordinal
  failure_signature      # normalized: failing check, error class, primary location, stable message fingerprint
  hypothesis             # one sentence, normalized fingerprint for equivalence
  evidence_refs[]        # what the agent read or ran to form the hypothesis
  intended_fix           # files/symbols and the change intent
  change_ref             # ChangeTransaction that implemented it
  verification_result    # ref to the re-run of the derived plan
  outcome                # RESOLVED | PARTIAL | UNCHANGED | WORSENED
}
```

Rules enforced by Core:

- Every repair attempt records all fields before the next verification run; a change without a RepairAttempt after a failed verification is rejected.
- **Equivalent repeated hypotheses trigger escalation.** If a new attempt's hypothesis fingerprint equals a prior attempt's for the same failure signature, the loop does not run it. It escalates through the compiled continuation slot (stronger solver, independent review or human) or, when no slot exists in Alpha, moves the task to `Needs Attention` with the attempt history.
- Bounds: at most N attempts per failure signature and M per task, both from policy, both budgeted in the plan; exhaustion escalates the same way. Budget exhaustion never truncates silently (`14_AGENT_RUNTIME_AND_ORCHESTRATION.md`).
- A repair that worsens verification is recorded as `WORSENED` and reverted through the Change Engine before the next attempt unless the agent records why the regression is expected.
- Repair history is part of the Review screen and of the competence benchmarks: repair loops per task is a first-class metric.

## 6. Self-review and completion (PX-019)

Before proposing completion the agent runs a self-review against the plan: every expected file changed or explained, every verification in the derived plan executed with its result, every protected effect receipted, no scope beyond the plan, no debug leftovers, and the acceptance criteria restated with evidence. The self-review is a `SelfReview` step with structured findings; unresolved findings block the completion proposal. Completion itself is decided by the Acceptance Gate (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`), never by the agent's statement.

## Roles, prompts and skills

Solver, reviser, reviewer and escalation prompts compiled by the Prompt/Skill Compiler carry these contracts as instructions, and the runtime enforces them as behavior. Skills may specialize strategies per language or framework (the language and platform support matrix, doc 76, added in stage E) but cannot relax a contract. The Isolated Non-Committing Reviewer receives the plan, the RepairAttempt history and the self-review as evidence, never the solver's hidden reasoning.

## Events, persistence and measurement

Events `PlanRecorded`, `PlanRevised`, `RepairAttemptRecorded`, `RepairEscalated` and `SelfReviewRecorded` join the canonical envelope (`30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`); RepairAttempt rows persist in the existing core store (`31_DATABASE_AND_STORAGE_SCHEMA.md`); RunStep gains `Plan`, `RepairAttempt` and `SelfReview` types (`13_DOMAIN_MODEL_AND_STATE_MACHINES.md`). Measurement, baselines and targets are defined in doc 63; no number in this document is a target.
