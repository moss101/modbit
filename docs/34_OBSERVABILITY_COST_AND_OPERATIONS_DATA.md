# Observability, Cost, and Operations Data

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Principles

Observability must explain **what happened, why the runtime decided it, what it cost, and what evidence proves it** without turning logs into a second source of truth.

## Tracing

OpenTelemetry spans use IDs: tenant (hashed/pseudonymous where exported), session, task, run, turn, step, tool call and sandbox. Trace propagation crosses Core→provider, Core→Sandbox Gateway→guest and Core→browser bridge.

Do not put raw prompts, source code, secrets or terminal contents into metrics labels.

## Metrics

### Runtime
- active/queued/waiting tasks;
- admitted subagents and capacity utilization;
- turn duration;
- tool success/application-failure/infra-failure/unknown-outcome;
- approval wait time;
- recovery/resume count and duration.

### Context
- retrieval latency by level L0-L3;
- candidates/read tokens/injected tokens;
- Context Pack precision proxy;
- prompt cache hit rate;
- compaction latency/stale rejection/fallback count;
- index freshness lag.

### Execution
- terminal attach/replay latency;
- sandbox provisioning/recovery duration;
- browser semantic snapshot/delta size;
- screenshot fallback rate;
- OutputRef spill/read volume.

### Model/cost
- input/output/cached tokens;
- cost per task/turn/provider;
- model first-token/total latency;
- provider error/rate-limit/failover rate.

### Reliability/security
- stale lease write rejection;
- protected effect approvals/denials;
- secret-handle use;
- cross-tenant authorization denials;
- emergency stops.

## User-visible evidence

Task review shows human-scale data, not telemetry internals: model used when label policy permits, commands/tests, changed files, verification status, browser/external effects, approvals and receipts. Cost can be shown per task with estimated/actual provider usage.

## Logs

Structured JSON logs have severity, component, event ID and error code. Raw content requires debug opt-in and redaction. Security audit records and Effect Ledger are separate from normal application logs.

## SLOs

Initial targets:
- 99.9% cloud control API availability monthly excluding planned maintenance;
- no acknowledged event loss;
- 99.9% successful resume for crash tests from supported durable states;
- 100% protected external effects either have receipt or explicit `UnknownOutcome` reconciliation record;
- zero cross-tenant resource access in security suite.

## Verified workflow economics and routing observability

ADR-R-045/048 make total inference cost plus human intervention and wall-clock per verified successful task the primary economic view, subject to the required quality floor. Retain separate units and declare any utility conversion weights. Tokens remain diagnostic detail. Attribute every solver, reviewer, reviser, escalation, retry, cancelled and fallback attempt; include cache-read/write/refill costs and deterministic verification cost. Missing provider usage stays unknown/estimated, not zero; deduplicate and settle reservations through canonical accounting.

RoutingTrace records plan choice/exclusions/scores, profile and envelope refs, all policy/profiler/compiler/model/registry/Skill/gate versions, candidate revision, routing epoch, choice propensity, switch reasons, complete costs and final verified outcome. OutcomeRecord retains raw feedback, keep/revert/correction/repair/intervention/test/build/merge signals with source/time/window and missingness alongside reward_version and composite reward. Redact content and enforce tenant/retention policy; telemetry never grants authority.

Report calibration Brier/ECE/OOD and drift, workflow regret, harmful under/overroute, gate false accept/reject, relational reviewer recall/false positives/churn, escalation recovery, invalid plans, budget overruns, cancellation cleanliness, stale-result rejection, partial-apply escape and denied canonical/external reviewer effects. Separate estimated direct-frontier savings from observed isolated alternative runs. Alert on quality floor/security/control regression and invoke the evidence-tested policy rollback in docs 61/71.

## v1.1 exact policy replay and independent attribution

RoutingDecisionRecord pins profile/envelope refs/versions, model/Skill registry, outcome_statistics_version, compiler_version, gate_version and realized_risk_version. For every candidate retain quality mean/LCB, predicted complete cost/latency, eligibility/exclusion reasons; retain chosen plan and ordered slot/leg/role/outcome path, derived labels and QUALITY_FLOOR_INFEASIBLE when applicable. Never infer a past bound or sample count from current statistics.

Separate request final success/satisfaction/keep/revert/intervention/cost/time/repair from leg initial rejection/escalation success/reviewer genuine defect or false positive/revision success/tool reliability, and gate later false accept/reject/risk miss or overclassification. Successful frontier escalation does not credit an earlier failed economical leg. Include reviewer tools/processes in cost and latency. Independently publish acceptance false accept/reject, realized-risk false negative/positive and critical_surface_miss_rate; these block release even if routing improves. See EPR-015/016/019 and doc 61.
