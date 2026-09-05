# Agent competence: planning, verification and repair

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 item 6 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`); scope bounds, repair policy and execution mechanics hardened by DR-PX-2026-09-05-006 (`97_DOSSIER_MAINTENANCE_LOG.md`).  
> **Owners:** core-runtime (planning, repair loop control, escalation), context-engine (retrieval before edit), workspace-git (change strategy), verification (verification plan, self-review, completion), skills (role prompts that carry these contracts), eval-bench (measurement, `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`). No new canonical subsystem.

## Why this document exists

The rest of the dossier governs the agent: what it may touch, what counts as proof, how it recovers. This document specifies how the agent **works**: the contracts that turn a goal into a verified change. Governance without competence produces safe failures; competence without governance produces unsafe successes. Both are required, and results are what decide whether Modbit is better than the alternatives.

Every contract below is a Core-enforced behavior with events and evidence, not a prompt suggestion. Prompts and skills express them; the runtime checks them. How the verification plan is executed (baseline, staged targeting, normalized test reports, flake handling, diff invariants) is specified in `64_VERIFICATION_EXECUTION_CONTRACTS.md`; how the runtime sequences turns, budgets and failure evidence is the harness contract in `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`.

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

## 3. Change strategy (PX-016) and scope policy (PX-038)

- Small, reviewable, revision-bound diffs applied through the Change Engine; one concern per ChangeTransaction where the task allows.
- Tests first when the task adds or changes behavior with an existing test harness: write or extend the failing test, then make it pass. Where no harness exists the plan says so and the verification plan compensates with compile, diagnostics and a reproduction command.
- Never widen scope silently: files outside the plan's expected set trigger a plan revision event and, when policy requires, a question.
- Generated files, lockfiles and migrations are changed only through their generators or with an explicit plan entry.
- Tests are not a lever: a test named by the task's acceptance, or failing at baseline, may not be deleted, skipped, weakened or rewritten to pass; other test-file changes require a plan entry. The Change Engine enforces this as diff invariant DI-3 (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §4).

**Scope policy.** The first `PlanRecorded` freezes the **original plan** (`plan_v1`): its expected files and symbols are the original write set. Scope is bounded, not merely transparent:

```text
ScopePolicy {
  max_out_of_plan_files_without_question     # Alpha default 2
  max_plan_revisions_without_question        # Alpha default 2
  always_ask_paths[]                         # migrations, CI configuration, lockfiles and dependency manifests, security/policy files, tests named by the acceptance
  headless_resolution                        # FAIL_CLOSED (Needs Attention) | policy-configured auto-allow
}
```

- Every `PlanRevised` carries a `scope_delta` (paths added, paths removed, reason). Core counts files changed outside the original write set and plan revisions per task.
- Beyond either bound, or for any `always_ask_paths` match, the next out-of-scope write is refused until a typed `UserQuestion` offering the concrete alternatives (continue with the expansion, split it into a follow-up task, stop) is answered; a `ScopeExpansionRecorded` event carries the counters and the answer. With no user present (CLI, CI), `headless_resolution` decides, and the default is fail-closed.
- Subagents keep the hard write-set denial of `REQ-EV-0144`; the bounds above govern the primary agent.
- The scope metric of doc 63 is measured against the original plan, so revising the plan cannot make the metric look clean.

## 4. Verification plan derivation (PX-017)

The Verification Engine derives the plan from task type, changed files, repository configuration and the agent's proposals (`33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md`): build, typecheck, lint, targeted tests, diagnostics delta, diff invariants, security rules. The agent may add checks; it cannot remove mandatory ones. The derived plan is recorded before the first verification run so that "what would have proven this" is auditable even when the task fails. Execution follows the stages of `64_VERIFICATION_EXECUTION_CONTRACTS.md`: a BASELINE run before the first write, bounded TARGETED runs inside the repair loop and a COMPLETION run at the final candidate revision with regression attribution against the baseline. "Targeted" means the selection rule of that document, heuristic in M2 and impact-based from M3, never the agent's unrecorded choice.

## 5. Bounded, evidence-driven repair (PX-018) and repair policy (PX-039)

Repair is the part most systems leave to chance. Modbit makes it a record.

```text
RepairAttempt {
  attempt_ordinal
  failure_signature      # normalized from CheckResults (doc 64 §2): failing check, error class, primary location, stable message fingerprint
  hypothesis             # one sentence, normalized fingerprint for equivalence
  evidence_refs[]        # what the agent read or ran to form the hypothesis
  intended_fix           # files/symbols and the change intent
  change_ref             # ChangeTransaction that implemented it
  change_fingerprint     # normalized diff hash of the change
  verification_result    # ref to the re-run of the derived plan
  outcome                # RESOLVED | PARTIAL | UNCHANGED | WORSENED
}
```

Rules enforced by Core:

- Every repair attempt records all fields before the next verification run; a change without a RepairAttempt after a failed verification is rejected.
- **Equivalent repeated hypotheses trigger escalation.** If a new attempt's hypothesis fingerprint equals a prior attempt's for the same failure signature, the loop does not run it. It escalates through the compiled continuation slot (stronger solver, independent review or human) or, when no slot exists in Alpha, moves the task to `Needs Attention` with the attempt history.
- Bounds: at most N attempts per failure signature and M per task, both from policy, both budgeted in the plan; exhaustion escalates the same way. Budget exhaustion never truncates silently (`14_AGENT_RUNTIME_AND_ORCHESTRATION.md`).
- A repair that worsens verification is recorded as `WORSENED` and reverted through the Change Engine before the next attempt unless the agent records why the regression is expected.
- A `FLAKY` or `UNKNOWN` check never forms a failure signature (doc 64 §2–3); a repair whose only passing evidence is a check that went `FLAKY` is `UNCHANGED`, not `RESOLVED`.
- Repair history is part of the Review screen and of the competence benchmarks: repair loops per task is a first-class metric.

**Repair policy.** The bounds and the loop's discipline are one versioned policy object:

```text
RepairPolicy {
  max_attempts_per_signature        # Alpha default 2
  max_attempts_per_task             # Alpha default 6
  max_worsened_before_escalation    # Alpha default 1
  reproduction_required             # Alpha default true when the goal reports a failure or defect
  max_consecutive_no_progress_turns # Alpha default 3
}
```

- **Reproduction first.** When the goal reports a failure, the first verification run must reproduce it: a `KNOWN_FAILING` check at baseline or a reproduction command whose output matches the reported symptom, recorded as evidence. A ChangeTransaction whose intent is a fix, recorded before a reproduction, is rejected while `reproduction_required` holds. If the failure cannot be reproduced within budget, the attempt is recorded as `UNREPRODUCED` and the agent asks a typed question or proceeds with the limitation stated in the plan.
- **No-progress detection.** Core rejects an attempt whose `change_fingerprint` equals a prior attempt's in the same task and counts it as an equivalent attempt; an attempt that reverts a prior attempt's change (oscillation) escalates; and `max_consecutive_no_progress_turns` turns without a new ChangeTransaction, verification run, retrieval record, plan revision or question emit `NoProgressDetected` and escalate or move the task to `Needs Attention`. This is the competence face of the runtime's stall detection.
- Defaults are Alpha configuration, policy-overridable and revisited by Decision Record after the PX-020 baseline; they are not targets.

## 6. Self-review and completion (PX-019)

Before proposing completion the agent runs a self-review against the plan: every expected file changed or explained, every verification in the derived plan executed with its result, every protected effect receipted, no scope beyond the plan without a recorded expansion, no unresolved diff-invariant finding, no quarantined mandatory check, no debug leftovers, and the acceptance criteria restated with evidence. The self-review is a `SelfReview` step with structured findings; unresolved findings block the completion proposal. After the SelfReview the COMPLETION run of `64_VERIFICATION_EXECUTION_CONTRACTS.md` executes at the final candidate revision; a `REGRESSION` against the baseline or a `DENY`-class invariant violation blocks the proposal. Completion itself is decided by the Acceptance Gate (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`), never by the agent's statement.

## Roles, prompts and skills

Solver, reviser, reviewer and escalation prompts compiled by the Prompt/Skill Compiler carry these contracts as instructions, and the runtime enforces them as behavior. Skills may specialize strategies per language or framework (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`) but cannot relax a contract. The Isolated Non-Committing Reviewer receives the plan, the RepairAttempt history and the self-review as evidence, never the solver's hidden reasoning.

## Events, persistence and measurement

Events `PlanRecorded`, `PlanRevised`, `RepairAttemptRecorded`, `RepairEscalated`, `SelfReviewRecorded`, `ScopeExpansionRecorded` and `NoProgressDetected`, together with the verification events of doc 64, join the canonical envelope (`30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`); RepairAttempt rows persist in the existing core store (`31_DATABASE_AND_STORAGE_SCHEMA.md`); RunStep gains `Plan`, `RepairAttempt` and `SelfReview` types (`13_DOMAIN_MODEL_AND_STATE_MACHINES.md`). Measurement, baselines and targets are defined in doc 63; no number in this document is a target.
