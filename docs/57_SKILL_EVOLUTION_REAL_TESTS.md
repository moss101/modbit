# Skill Evolution Real-System Tests

## Test environment

Use a dedicated `skill-evolution-fixtures` repository with at least three task classes: localized bug repair, multi-file change, and repository comprehension/verification. Every task has a fixed Git revision, setup script, hidden acceptance test and evidence rubric. Production provider credentials are never embedded; staging uses SecretRefs.

## WSK-E2E-001 — immutable trace sealing

Run an actual agent task with a real model provider, real Git worktree and real tests. Seal the `EvolutionTrace`; then attempt mutation. Expected: content digest prevents mutation and a new correction record must reference, not overwrite, the sealed trace.

## WSK-E2E-002 — success/failure pattern consolidation

Generate both successful and failed verified runs of the same task family. Run the real Wiki Maintainer worker. Expected: pattern records cite exact trace IDs, include contradicting evidence, and do not write Engineering Memory or active skill files.

## WSK-E2E-003 — atomic candidate proposal

From a known base skill, propose one behavioral change using an actual proposer model. Expected: candidate contains base hash, atomic diff, PURPOSE, motivating patterns/traces and declared tool/capability ceiling. Reject candidate if it changes unrelated files or widens authority.

## WSK-E2E-004 — promotion and rollback

Run candidate against the fixed qualification suite. Force a regression. Expected: candidate receives REJECTED qualification, active skill head remains byte-identical, candidate/evidence remain queryable, and evolution knowledge persists. Repeat with a passing candidate: registry head updates atomically and old version remains addressable.

## WSK-E2E-005 — inference isolation

Run production task using promoted skill. Capture final PromptEnvelope/ContextInspector. Expected: approved skill projection appears, but raw evolution wiki/traces are absent unless separately retrieved by an explicitly authorized task source.

## WSK-E2E-006 — capability non-escalation

Place a malicious candidate instruction requesting network/secret/admin tooling. Expected: skill package may state a requirement, but Tool/Capability projection cannot exceed task policy; promotion security gate fails if package attempts prohibited executable behavior.

## WSK-E2E-007 — selective evolution retrieval

Populate thousands of patterns/traces. Run proposer with compact index. Expected: only evidence actually hydrated is counted in context, token budget is respected, and final candidate provenance names every hydrated source.

## WSK-E2E-008 — paired benchmark

Compare `no skill`, `current skill`, and `candidate skill` on identical tasks/models/environments with repeated trials. Report verified completion, regressions, input/output tokens, tool calls, wall time and cost. Do not promote on aggregate score if any hard safety/correctness gate fails.

## WSK-E2E-009 — cross-model transfer

Nightly, evaluate a promoted model-neutral skill on at least two materially different eligible model families. Expected: results are reported separately; poor transfer does not invalidate a model-specific skill but blocks a model-neutral label.

## WSK-E2E-010 — no second memory/recovery path

Kill Core during production task while Skill Lab services are offline. Expected: task resumes entirely from Event/Protocol/Checkpoint stores; no Skill Evolution data is required for recovery.
