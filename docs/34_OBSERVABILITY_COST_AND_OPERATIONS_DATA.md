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

As built (IMP-EV-0023, REQ-EV-0023): the cloud SLO event ladder. A `cloud_isolated` task's start is recorded on its log as `SloStageRecorded { stage, at_ms, run_id?, warm?, detail }` rung by rung — `REQUESTED` when the run is asked for, `PREWARM` with what a warm pool offered (`NONE`: no warm pool exists), `SANDBOX_REQUESTED` and `SANDBOX_READY` around the gateway's provisioning (`warm` when the task's own sandbox was reused rather than provisioned, the sandbox id as detail), and each run's `FIRST_TOKEN` (the model's first output) and `FIRST_TOOL` (its first tool dispatch). Local tasks record none. `modbit_observability::slo` derives each start's provisioning, request→ready, request→first token and request→first tool latencies and the cold and warm figures (count, median, largest); `GetSloLadder { task_id }` serves them, a rung never reached is -1, never 0. Today every start through the Cloud API is cold (a task's sandbox lives for the task; the API relays no second start); a warm start is a second run on the Core while the task holds its sandbox — after a person returns the work. Proven by `qual_ev_0023_a_cloud_tasks_starts_emit_every_slo_timestamp_and_cold_and_warm_latencies` (the hosted cloud job: Postgres, the Sandbox Gateway on the MicroVM backend).

As built (M10.1): the operations dashboard. `GetDashboard { session_id }` is the Core's own aggregation of one session's log at the moment it is asked, never a stored or sampled feed: task states; tool outcomes (succeeded, failed, unknown outcome, cancelled); the usage ledger's cost (registry minor units, currency, priced/unpriced/unreported calls) and tokens, by model and by task (with each task's tool calls and SLO starts); the cold and warm SLO figures across the session's cloud starts (the IMP-EV-0023 ladder); the last 20 failures by class and code; and each provider endpoint's health. The desktop shows it in its Operations pane (header "Dashboard", refreshable) and `modbit-cli dashboard --session <id>` prints it. Proven by `m10_1_the_dashboard_is_the_sessions_ledger_ladder_and_log_aggregated` (the cost is what the provider billed and the sum of the tasks' economics; models and tasks add up; tool counts are the log's; the refusal is a failure), the cloud ladder test (the dashboard's cold and warm figures are the ladder's) and the desktop E2E `e2e/dashboard.spec.ts` (the app shows exactly the numbers the CLI reads from the same Core, and a refresh follows the log).

## User-visible evidence

Task review shows human-scale data, not telemetry internals: model used when label policy permits, commands/tests, changed files, verification status, browser/external effects, approvals and receipts. Cost can be shown per task with estimated/actual provider usage.

## Logs

Structured JSON logs have severity, component, event ID and error code. Raw content requires debug opt-in and redaction. Security audit records and Effect Ledger are separate from normal application logs.

As built (IMP-EV-0017, REQ-EV-0017): one failure identity, two renderings. A failure is a `FailureDiagnostic` (class, code, retryability, user action, recovery path, evidence); `user_explanation()` renders it for a person and `model_repair()` for the model, and neither carries the source's own words — those stay in `detail` as evidence. One redactor (`modbit_secrets::Redactor`, owned by effects-security) applies one policy on every surface: a value in the Core's custody (provider keys, the forge token, external servers' credentials) is replaced wherever it appears, and error text — a message, a reason, a diagnosis, a rejection — also loses every credential-shaped token. It runs in the provider gateway (the endpoint's own key and the provider's words), the forge and external-server clients, at the Core's tool-result boundary before a result is stored or read by the model, on every rejection leaving the Core, on `TaskStatus` as it is read, and as the event store's payload filter before anything is hashed and persisted (sealed receipts excepted). Proven end to end by `qual_ev_0017_a_secret_bearing_internal_error_is_redacted_for_the_person_and_the_model` (a server failing with its credential in the error; a provider refusing with the key echoed; a rejection repeating a pasted path; neither Core's data directory holds either secret).

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

As built (EPR-010): the request is its task, and its accounting and outcome record (`request-accounting/1`, `services/modbit-core/src/accounting.rs`) is derived from the canonical log alone — the task's runs, the review tasks its reviewer legs ran on, and late invoices — never from a second store. Every attempt is `RoutingAttemptRecorded` with the total input, its cached and cache-written subsets, output, latency and its cost priced where it ended at the registry in force (plain input, cached input at the cached price, cache writes with their surcharge, output; `priced_under` names the generation, so a later registry never reprices history). The record holds: the executed path as legs (consecutive slots of one role and binding within a run; a revision or a return to work is a new leg; a reviewer activation is a leg on its review task) with every attempt, the failed, cancelled and cut-off ones included; one decision per admitted plan (`RoutingDecisionRecorded` beside every admission — compiled, `OPERATOR` or `DIRECT` — with each candidate's expected and worst-case cost, quality mean and lower bound in basis points, eligibility and reason, the choice probability and the compile latency); every version (each plan's provenance, profiler, skills, gate, risk, statistics, thresholds, economics, pricing); cost (priced inference by solver and reviewer, reservations held for unknown usage, unpriced attempts, verification runs, checks and process time as `LOCAL_PROCESS_UNPRICED`, request and reviewer tool time, wall-clock); reservations (activated, settled, held, and released once the request stops); request, leg and gate observations kept apart — `initial_leg_success` stays false when an escalation succeeds, escalations, reviews (validated against unsupported findings) and revisions carry their own success, and every gate evaluation carries its later correction (`ACCEPTED_LATER_SHOWN_WRONG` from a person's return at that revision, `REJECTED_LATER_SHOWN_VALID` from a later passing completion run at it) or `NONE_OBSERVED`; raw signals by event reference only (a person's decisions, corrections and reverts after the first proposal, every intervention, tests and build of the last completion run, merge from the forge), a payload that can no longer be read listed as unavailable, never as absent; a labelled counterfactual (`ESTIMATED` from the compiler's own direct-frontier candidate; `observed` is always `NOT_EXECUTED`; there is no saving field); and the signals it could not observe, by name (explicit feedback, merge, keep — no keep window is configured — composite reward — no reward version is active — unpriced attempts, unavailable payloads). Unknown usage, and an invocation a crash cut off before its attempt was recorded, hold their slot's reservation until `ReconcileUsage` (a late invoice) settles the attempt once at the registry's prices; a second delivery is a duplicate that charges nothing, and an attempt whose usage the provider reported refuses one (`ALREADY_KNOWN`). Attempts are keyed by run, plan, slot and ordinal; a replayed record is named and ignored. Only the Core's own tenant is read. The record rides the log as the task event `RequestOutcomeRecorded` (the record an object) in the same append as each run's end, and again when a review concludes, a person decides and an invoice settles; a later record supersedes an earlier one and records are never summed. `GetRequestOutcome` returns the record as of now beside the latest durable one.

As built (IMP-EV-0032, REQ-EV-0032): the per-call usage ledger (`services/modbit-core/src/usage.rs`) is one row per `ModelUsageRecorded` — the agent's own calls and the calls made for it (a context specialist) — with its run and turn from the event's lineage, the binding and the model the provider answered as, the provider's request id, input, cached input and output, and whether the provider reported it; each row is priced at its own binding in the active registry with the request accounting's rules (`accounting::price`), in minor units, and an unreported row is unknown, never zero. `GetTaskEconomics` now reads it: `cost_minor` with the registry's currency, scale and generation, `priced_calls`/`unpriced_calls`/`unreported_calls`, and `runs` — per run and per step (turn): calls, tokens, cost, whether every call was priced and reported, tool calls and tool time, verification time (the checks' durations) and the provider request ids; the catalog-list `cost_usd` is computed per call at each call's own model (it had priced the whole task at the last model) and is `pricing_known` only when every call was reported and catalogued. `ReconcileInvoice { task_id, invoice_json, tolerance_bp }` compares a provider invoice sample (`modbit.invoice-sample/1`: `{schema, provider, rows: [{request_id, model, input_tokens, cached_input_tokens, output_tokens, cost_minor?}]}`) with the ledger row by request id: `MATCHED` within the tolerance (the largest relative difference over input, cached, output and cost, in basis points, rounded up), `OUT_OF_TOLERANCE`, `MODEL_MISMATCH` (the invoice names neither the binding nor the answered model), `UNKNOWN_USAGE` (settle it with `ReconcileUsage`), `NOT_IN_LOG` (billed, never recorded), `NOT_ON_INVOICE` (recorded, not billed) and `OTHER_TASK` (not counted); it is read-only and `within_tolerance` only when every row of the task matched. A side question appends nothing to the log (QUAL-EV-0261), so its invoice rows are `NOT_IN_LOG` by design. Proven by `qual_ev_0032_a_provider_invoice_reconciles_against_the_canonical_usage_within_tolerance` against the provider's own record of what it served and charged.

Report calibration Brier/ECE/OOD and drift, workflow regret, harmful under/overroute, gate false accept/reject, relational reviewer recall/false positives/churn, escalation recovery, invalid plans, budget overruns, cancellation cleanliness, stale-result rejection, partial-apply escape and denied canonical/external reviewer effects. Separate estimated direct-frontier savings from observed isolated alternative runs. Alert on quality floor/security/control regression and invoke the evidence-tested policy rollback in docs 61/71.

## v1.1 exact policy replay and independent attribution

RoutingDecisionRecord pins profile/envelope refs/versions, model/Skill registry, outcome_statistics_version, compiler_version, gate_version and realized_risk_version. For every candidate retain quality mean/LCB, predicted complete cost/latency, eligibility/exclusion reasons; retain chosen plan and ordered slot/leg/role/outcome path, derived labels and QUALITY_FLOOR_INFEASIBLE when applicable. Never infer a past bound or sample count from current statistics.

Separate request final success/satisfaction/keep/revert/intervention/cost/time/repair from leg initial rejection/escalation success/reviewer genuine defect or false positive/revision success/tool reliability, and gate later false accept/reject/risk miss or overclassification. Successful frontier escalation does not credit an earlier failed economical leg. Include reviewer tools/processes in cost and latency. Independently publish acceptance false accept/reject, realized-risk false negative/positive and critical_surface_miss_rate; these block release even if routing improves. See EPR-015/016/019 and doc 61.
