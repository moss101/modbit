# Performance, Context Economics, and Benchmark Plan

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


All numbers are **engineering targets**, not claimed results.

## Desktop/runtime budgets

| Metric | Target |
|---|---:|
| Warm fleet snapshot from local Core | < 150 ms p95 |
| Local command dispatch overhead excluding process | < 25 ms p95 |
| L0 exact/symbol retrieval warm | < 100 ms p95 |
| L1 hybrid retrieval warm on medium repo | < 300 ms p95 |
| Context Pack compile excluding embedding backlog | < 500 ms p95 typical |
| UI event-to-render latency | < 100 ms p95 |
| Terminal replay attach | < 250 ms p95 local |
| Browser semantic delta after settled DOM change | < 250 ms p95 local |
| Core idle RSS | target < 250 MB excluding indexes/LSP/model assets |
| Renderer idle RSS | target < 300 MB |

## Repository scale classes

Benchmark at roughly:
- small: <10k files;
- medium: 10k–100k files;
- large: 100k+ files / multi-million LOC.

Measure cold index, incremental one-file edit, branch/worktree switch, symbol query, hybrid query, structural impact query and memory footprint.

## Retrieval comparison

Profiles:
- **A Baseline:** exact/ripgrep-like tools.
- **B Hybrid:** exact + BM25 + semantic ANN.
- **C Modbit Structural:** B + AST/LSP/dependency/Git/diagnostics/runtime evidence and planner.

Hold model, prompt, task, environment and limits constant. Agent is not forced to call retrieval treatment.

### Targets derived from project hybrid-retrieval benchmark comparison

On equivalent BrowseComp-Plus protocol, aim for:
- answer accuracy ≥ ~99%;
- input tokens reduction ≥ ~40% vs baseline;
- tool calls reduction ≥ ~45%;
- agent time reduction ≥ ~40%.

Do not publish/claim until independently reproduced. Record index build time separately and also report end-user cold-start + incremental-index cost.

## Engineering benchmarks

- SWE-QA-style repository multi-hop question suite.
- Relevant-file recall@1/@5/@10.
- Evidence precision among injected chunks.
- Changed-code impact accuracy.
- Diagnostic/test linkage precision.
- Cross-file relation answer correctness.
- SWE-bench Verified or equivalent coding suite using frozen model/environment for regression; establish baseline before setting score target.

## Agent reliability metrics

- task success across N independent trials;
- median/p95 tokens, tool calls and wall time;
- wrong-effect attempts blocked;
- number of repair loops;
- completion evidence coverage;
- restart/resume success.

## Browser metrics

- percentage actions solved structurally without screenshot;
- semantic state bytes vs full AX snapshot bytes;
- delta bytes/action;
- stable entity ID survival across benign DOM rerenders;
- targeted visual fallback success;
- takeover latency.

## Performance regression policy

A PR fails if deterministic benchmark worsens >10% on a protected metric without approved decision record. Agent/model benchmarks run nightly because stochasticity/cost requires repeated trials; release uses frozen baseline and confidence intervals.


## V2 benchmark profiles

Retrieval benchmark must include A baseline exact search, B hybrid retrieval and C structural Modbit context. Keep task/model/revision/environment fixed and report verified outcome, input tokens, normalized tool calls, agent time, cold index time, incremental update time, recall@K and evidence precision. Skill evolution reports no-skill/current/candidate paired results and modality tests report bytes transformed/provider latency/vision fallback rate. External benchmark numbers remain targets until reproduced.

## Execution policy and complete-task economics

The primary economic view is total inference cost plus human intervention and wall-clock per verified success subject to a required quality floor (ADR-R-045). Preserve separate units and raw signals; report token/cache/retry/escalation/critic/revision/verification detail and estimated versus observed direct-frontier alternatives. Capture EPR-000's direct baseline before EPR-005 non-regression measurements.

Measure profiler extraction-inclusive p50 <25 ms where practical and p95 <50 ms on named hardware; publish Brier/ECE, OOD/floor-success calibration by task/domain/modifier/risk, registry generation and product version. Evaluate workflow regret, gate false accepts/rejects, relational critic recall/false positives, escalation recovery, cache switch/refill economics, final quality and intervention. Profiler is bounded and never requires a general LLM call or repository-wide scan.

Use immutable training/validation/untouched holdout splits, sample-size/power and interval methodology, actual exploration propensities, safe counterfactual replay, shadow/canary/A-B and reversible policy versions. Required thresholds and rollback triggers are approved and versioned before evaluation; missing or regressing quality-floor evidence blocks promotion regardless of savings. Full gate definitions are in doc 61; no benchmark numbers in this dossier are claimed results.

## v1.1 independent confidence and gate benchmarks

Use separately versioned Outcome Statistics, representative sample counts and conservative LCB/posterior quality feasibility; high benchmark means remain weak priors until product evidence earns cheap routing. Modes pin quality floor/confidence requirement, and cost ranking cannot buy a quality violation. No-feasible fallback records QUALITY_FLOOR_INFEASIBLE. Price every reviewer tool/process and switch hysteresis along with ordinary inference/verification.

EPR-019 independently measures acceptance_false_accept_rate, acceptance_false_reject_rate, realized_risk_false_negative_rate, realized_risk_false_positive_rate and critical_surface_miss_rate on separate oracle-labeled held-out candidates. Router gains cannot excuse unsafe gate rates. Track request, leg and gate outcomes separately; successful escalation cannot credit failed initial solver. Required thresholds/method/samples, exact statistics/compiler/gate/risk versions and rollout rules are in doc 61.
