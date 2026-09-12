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

## As built (M5.7, EXPERIMENT)

The lab is `modbit_skills::evolution`, offline, under its own directory (`<profile>/skill-lab/` with `traces/`, `corrections/`, `patterns/`, `candidates/`, `impact/`, `versions/`), and the qualification harness is `benchmarks/skill-evolution`. Three stores, never one: an `EvolutionTrace` is sealed under the digest of its canonical JSON — sealing the same content again is the same trace, writing other content under a sealed id is refused, and a `TraceCorrection` is a new record that names the sealed trace; `EvolutionPattern`s are written by the `Maintainer` (one claim per task class and observation, supporting and contradicting trace ids side by side, confidence in basis points, a later revision superseding rather than overwriting, the compact `index.json` carrying heads only); `SkillCandidate`s come from the `Proposer` — one `AtomicPatch` to `SKILL.md` or a procedure, bound to the base content hash, with PURPOSE, motivating pattern and trace ids, the declared tools and ceiling, the proposer configuration — and `Proposer::validate` refuses an unrelated file, a path that leaves the package, a widened `required_tools` or `capability_ceiling` (in the candidate or its manifest), a stale base, or prohibited executable behaviour (`require(`, `import(`, `eval(`, `fetch(`, `process.env`, the raw isolate binding, …). The `ProposerModel` is a trait; the lab's stand-in is the deterministic `TemplateProposer`; a model-backed proposer implements the same trait through the gateway and holds no registry-head write (REQ-EV-0237: nothing in the proposer can promote).

`KnowledgeStore::hydrate` brings patterns in by id under a token budget (bytes / 4): only what is hydrated counts, the rest is refused by name, and a candidate's provenance names exactly the hydrated sources (WSK-E2E-007). `Qualifier::qualify` decides over paired `no_skill` / `current` / `candidate` trials with the hard gates first — package and authority validity, safety failures, correctness by task class, correctness overall, per-model transfer — and economics (tokens, tool calls, time, cost) only after them: a cheaper but less correct candidate is REJECTED (REQ-EV-0248); the reasons are recorded gate by gate. `Promotion::promote` runs only under a PROMOTE qualification of that candidate: the base is copied to an immutable `versions/<name>@<version>/`, the patch applied, an `EVALUATION.json` of the new content written, a `SIGNATURE.json` signed under the lab's key, and the registry head swapped in atomically (staged copy, rename); a rejected candidate leaves the head byte-identical and stays queryable with its qualification; every decision is an `ImpactRecord` (candidate, patch, source patterns, qualification, disposition, proposer, environment, version) and `Promotion::rollback` moves the head to any archived version, the wiki untouched (REQ-EV-0197, 0200, 0201, 0247). The harness (`arms_report`, `transfer_report`, `ablation`) publishes the paired reports with the shared statistics, the per-model lines (a regressing family withholds the model-neutral label and blocks promotion; one family promotes a model-specific skill), and the REQ-EV-0207 ablation against manual refinement, each with its method.

Proofs: `crates/skills/tests/evolution.rs` (WSK-E2E-001, 002, 003/006, 004, 007; REQ-EV-0202), `benchmarks/skill-evolution/tests/harness.rs` (WSK-E2E-008, 009; REQ-EV-0207), and on the real Core `wsk_e2e_005_010_a_promoted_skill_reaches_the_model_without_the_wiki_and_recovery_needs_no_lab` (a promoted version 1.0.1 reaches the model as its projection and nothing of the wiki does; the Core hard-killed mid-task with the lab deleted resumes from the event, protocol and checkpoint stores alone). What the EXPERIMENT does not establish: the fixture trials are deterministic, so the intervals have no width and the ablation's finding on the fixture is that the lab's lift over manual refinement is not meaningful there; a live proposer model, the dedicated `skill-evolution-fixtures` repository and nightly cross-model runs are the conditions under which the mechanism could earn an ADR.

## Acceptance gates

The evolution architecture may ship as an **EXPERIMENT** once rollback, isolation and qualification tests pass. It may become default only after Modbit's own engineering benchmark shows a repeatable practical lift over simpler manual/versioned skill refinement. Paper results are research evidence, not Modbit performance claims.

## Evaluated model × Skill conditional routing

EPR-013 qualifies model × Skill × effort × context/harness combinations in independently versioned Outcome Statistics with source digests, samples and confidence; Model Registry remains current configuration. Pin Skill/registry/statistics versions in each conditional decision and outcome. Intrinsic RequestProfile never includes catalog/economics/Skill binding authority.

Normal Prompt/Skill Compiler emits solver/reviewer/reviser instructions within canonical capabilities. Skills cannot grant reviewer canonical/persistent/external writes, secrets or self-promotion; permitted ephemeral review tools remain bounded by EPR-018. Default reviewer context excludes solver hidden reasoning. Changed/revoked or unqualified combinations require renewed holdout/gate/rollout evidence. Policy Lab and Skill Evolution use the existing Eval Harness without becoming another runtime or memory owner.
