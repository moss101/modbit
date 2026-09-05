# Skill Registry and Evolution Integration — Skill Evolution Without a Second Runtime

> **Decision:** **ADAPT**, with the evolution loop **EXPERIMENT-gated** until Modbit reproduces a useful engineering-task lift.  
> **Non-goal:** do not create a second general memory system, second agent harness, or external skill-packaging framework dependency.

## Why this belongs in Modbit

Modbit already needs versioned skills, skill selection, provenance, evaluation and a way to improve procedures over time. Skill-evolution research adds one valuable separation: **raw execution experience, accumulated evolution knowledge, and executable skills are different artifacts**. Skill-packaging research contributes useful packaging/validation discipline: a skill/tool is not complete because a Markdown file exists; registry wiring and real execution must be validated.

The result is **one Modbit Skill subsystem with an offline/eval Skill Evolution Lab**, not a evolution-lab service beside Engineering Memory.

## Canonical architecture

```text
Production runs
   │ immutable selected evidence/traces (policy filtered)
   ▼
Evolution Trace Archive  ───────────────┐
   │                                    │
   ▼                                    │
Skill Evolution Knowledge Store         │
(patterns, failures, hypotheses)         │
   │                                    │
   ▼                                    │
Candidate Skill Proposer                 │
   │ atomic diff + purpose + evidence    │
   ▼                                    │
Skill Qualification Harness ◄────────────┘
   │
   ├─ reject → retain audit artifact; active skill unchanged
   └─ promote → signed/versioned Skill Registry
                       │
                       ▼
                Prompt/Skill Compiler
                       │
                       ▼
                 Production agent
```

The **production agent does not receive the evolution wiki by default**. It receives only the approved skill projection and task-authorized resources. This prevents the optimization history from becoming prompt bloat or an accidental source of authority.

## Storage contracts

### `EvolutionTrace`

Required fields: `trace_id`, benchmark/task ID, repository revision, model/provider config, InstructionManifest hash, tool-capability snapshot hash, environment revision, events/evidence refs, final outcome, verification result, cost/latency metrics and redaction status. Traces are immutable after sealing.

### `EvolutionPattern`

Required fields: `pattern_id`, claim, supporting trace IDs, contradicting trace IDs, scope, confidence, created/updated revision, maintainer version, tags and supersession relation. A pattern is **evaluation knowledge**, not Engineering Memory and not runtime authority.

### `SkillCandidate`

Required fields: candidate ID, base skill version, atomic patch, PURPOSE statement, motivating pattern/trace IDs, expected behavior change, required tool names, maximum capability ceiling, target task classes, proposer model/config and creation time.

### `SkillQualification`

Records benchmark version, baseline skill, candidate skill, model matrix, repetitions/seeds, verified-completion delta, safety failures, token/tool/time deltas, regressions by task class and final promotion decision.

## Promotion transaction

A skill is promoted only when all required gates pass:

1. schema/package validation;
2. no capability widening beyond declared ceiling;
3. static security scan of scripts/resources;
4. direct skill test;
5. registry/discovery test;
6. real tool/integration tests where applicable;
7. fixed engineering benchmark comparison against active skill and no-skill baseline;
8. safety/policy regression suite;
9. cross-model transfer test when the skill is intended to be model-neutral;
10. signed immutable version write + atomic registry head update.

Any failure leaves the current production skill untouched.

## Wiki Maintainer behavior

The maintainer operates only on qualification-eligible traces. It must record success **and failure** patterns, preserve conflicting evidence and use bounded on-demand retrieval from the evolution knowledge index. It cannot edit production skills directly and cannot write Engineering Memory.

## Skill Proposer behavior

The proposer receives the skill's current version, outcome summary and compact pattern index first. It hydrates exact supporting patterns/traces as needed. It proposes **one bounded behavior change per candidate** unless the evaluator explicitly authorizes a multi-change experiment. This keeps attribution possible.

## Skill-packaging lessons retained

- portable `SKILL.md`-style package plus optional scripts/resources;
- exact registry/discovery validation;
- test the direct implementation and the end-to-end registered invocation path;
- use real integration/API examples for integrations, not fake examples;
- credentials are host-resolved SecretRefs, never model-supplied tool arguments;
- allow metadata such as `model-invocable: false` so a skill can be user/system-only;
- compact/selective loading rather than injecting full skill packages into every request.

The external scientific tool collection that accompanied skill-packaging research is explicitly **REJECTED** as a Modbit dependency because it is outside Modbit's software-engineering product scope.

## Failure modes

- **Benchmark overfit:** require hidden/held-out tasks and task-family regression report.
- **Skill poisoning:** only verified/redacted traces can enter the evolution corpus; prompt/tool outputs remain untrusted data.
- **Self-promotion:** proposer/maintainer have no registry-head write capability.
- **Wiki bloat:** compact index + lazy pattern/trace hydration; retention policy for low-value duplicates.
- **False causality:** atomic candidate diffs and repeated paired evaluation.
- **Model-specific trick:** report per-model results and never assume transfer.
- **Authority confusion:** evolution knowledge cannot be used for session recovery or policy.

## Acceptance gates

The evolution architecture may ship as an **EXPERIMENT** once rollback, isolation and qualification tests pass. It may become default only after Modbit's own engineering benchmark shows a repeatable practical lift over simpler manual/versioned skill refinement. Paper results are research evidence, not Modbit performance claims.

## Evaluated model × Skill conditional routing

EPR-013 qualifies model × Skill × effort × context/harness combinations in independently versioned Outcome Statistics with source digests, samples and confidence; Model Registry remains current configuration. Pin Skill/registry/statistics versions in each conditional decision and outcome. Intrinsic RequestProfile never includes catalog/economics/Skill binding authority.

Normal Prompt/Skill Compiler emits solver/reviewer/reviser instructions within canonical capabilities. Skills cannot grant reviewer canonical/persistent/external writes, secrets or self-promotion; permitted ephemeral review tools remain bounded by EPR-018. Default reviewer context excludes solver hidden reasoning. Changed/revoked or unqualified combinations require renewed holdout/gate/rollout evidence. Policy Lab and Skill Evolution use the existing Eval Harness without becoming another runtime or memory owner.
