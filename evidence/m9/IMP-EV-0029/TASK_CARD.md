# Task Card — IMP-EV-0029 Task fingerprint + capability-based routing

## Identity

- Task ID: IMP-EV-0029 (REQ-EV-0029, ADAPT; owner Model Router)
- Milestone: M9
- Qualification: QUAL-EV-0029 — replay benchmark corpus and verify deterministic hard exclusions plus auditable scores.
- Evidence tier: release-critical (routing, policy and execution behaviour; event and protocol schema)
- Built on the existing compiler, ConditionalExecutionPlan and configuration layers (docs/27/38, doc 77): no second router, no second policy engine.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The compiler already filtered bindings on revocation, role, residency, governance profile, currency, tools, vision, structured output and context, ranked the rest by confidence-feasible quality and then expected cost, and digested its inputs. The Request Profiler (EPR-003) already recorded each request's intrinsic fingerprint (`RequestProfiled`, shadow). But the Core compiled every run with `Needs { tools: true }` alone — no context demand — and the organization's `models_allow` reached nothing: no compile, operator plan, pin or direct run was held to it, and a policy tightened mid-run never touched the model in force. The durable decision (`RoutingDecisionRecorded`) carried candidates but neither the hard exclusions nor the demands nor the input digest, so the decision could not be audited or replayed from the log.
- first missing link: the model policy was not an input to routing at all.
- production entry points: `crates/providers/src/compiler.rs` (`CompileInput.allowed_models`, `model_allowed`, `demands`, `Compiled.hard_exclusions/demands/demands_digest`); `services/modbit-core/src/routing.rs` (`model_policy` from the task's pinned configuration, `refused_by_model_policy`, compile demands, decision fields, operator admission check, replay on an ended run); `runtime.rs` (direct-path check, route decided before `TaskStarted`/`TaskResumed` and the capacity ticket, per-round check with `POLICY` re-route, `RunFailed { MODEL_NOT_ALLOWED }`); `crates/core-runtime/src/diagnostics.rs` (`MODEL_NOT_ALLOWED` is a `Policy` diagnosis).

## Verification

- `qual_ev_0029_hard_filters_decide_first_and_the_corpus_replays_exactly` (crates/providers): a corpus of six routing cases over an eight-binding signed registry — a plain request, a US-only request, an organization allowing two models, a sandboxed US request, a small request and one no binding can hold. Each is compiled twice and must be identical (plan, candidates, exclusions, digest); exactly the expected bindings are excluded, each for its stated reason, including the two best-measured, cheapest bindings; nothing excluded becomes a candidate; every candidate's expected cost recomputes from the registry prices and its lower bound from the evidence; the chosen plan clears the floor and no feasible plan is cheaper; the unservable request fails closed with every reason. A policy allowing every binding chooses what an unrestricted compile chooses under its own digest; a policy allowing nothing admits nothing. Mutation: disabling the policy filter fails it.
- `qual_ev_0029_hard_filters_decide_first_and_the_decision_replays` (services/modbit-core, real Core, signed registry, scripted OpenAI-compatible provider): (1) under an admin policy allowing four models, the cheapest three bindings (no tools; a 32k context; not allowed) are excluded and only `gpt-5` is ever asked; the run edits the file and is verified; `RequestProfiled` carries the fingerprint; `RoutingDecisionRecorded` carries the demands, their digest, each exclusion with its exact reason, the input digest and auditable candidates. (2) `CompileRoutingPlan` on the finished task reproduces the input digest, demands, exclusions and every candidate's costs and lower bound. (3) An operator plan naming `gpt-5-mini` is refused `MODEL_NOT_ALLOWED`; a pin on it is refused `PIN_NOT_ELIGIBLE` with the policy's reason and the task stays `Queued` with no `TaskStarted`, run or ticket. (4) Unrestricted, a run opens on `gpt-5-mini`; the admin policy is tightened while a slow `shell.exec` is in flight; the next round records `PolicyGenerationChanged`, re-routes at `POLICY` (`SWITCH` mini → gpt-5) with a new decision excluding mini by policy, and finishes on `gpt-5`; mini is never asked again. (5) On a Core with no registry (the direct path) the same tightening ends the run `RunFailed { MODEL_NOT_ALLOWED }` with a `TaskNeedsAttention` diagnosis, a restart on the refused model is refused `MODEL_NOT_ALLOWED`, and a restart on an allowed model routes a new run that completes. Mutation: disabling the per-round check fails it (no `POLICY` decision).
- Regression: the compiler golden corpus (unchanged: an unrestricted policy leaves digests as they were), `qual_ev_0030_…`, `qual_ev_0041_…`, `qual_epr_009_…`, the full workspace and modbit-core suites, clippy `-D warnings`.

## Limitations

- The intrinsic fingerprint (the Request Profiler's features) is recorded beside every routing decision but is not an input to it: EPR-003 keeps the profiler in shadow until its own qualification and rollout gates (doc 61) promote it. The capability demands the router does act on are the request's hard needs (tools; the context it is priced at), the execution profile, residency and the model policy.
- The expected input a request is priced at is a fixed 40 000 tokens; it is not estimated per request.
- Vision is not a hard demand: images reach a text-only binding described by the vision bridge or refused by name (docs/25).
- A run resumed from suspension keeps its route; the per-round check catches a refused model before its first dispatch.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/15 and docs/30 carry the as-built paragraphs
