# Task Card — EPR-010 Capture request, leg and gate accounting/outcomes

## Identity

- Task ID: EPR-010
- Milestone: M9 (phase 5); prerequisites EPR-007, EPR-009 (both COMPLETE)
- Requirement: REQ-EPR-010 (related REQ-EV-0112); owner label: observability; code in `services/modbit-core` (`accounting.rs`), `crates/domain`, `crates/providers`, `crates/protocol`
- Qualification: QUAL-EPR-010 / EPR-E2E-010 — reconcile provider/process usage and retained events for an initial-leg failure followed by successful escalation; request success true, initial success false, gate corrections independent, all versions/mean/LCB and executed slots reconstructable, missing signals explicit. EPR-FI-010 — interrupt accounting, replay duplicate usage, deliver a late invoice, apply retention deletion: no double charge, lost reservation, fabricated savings or cross-tenant telemetry.
- Evidence tier: release-critical (canonical persistence, protocol and schema, evidence semantics)

## Goal

Account all inference, reviewer tool/process, verification and revision/escalation/retry attempts and cache costs; log the exact versions, candidate quality mean/LCB and executed slot path; attribute request success, the failed initial leg, reviewer findings/revision and gate errors separately; keep raw corrections, reverts and interventions under the privacy policy.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The log already held the plans, admissions (with the chosen plan's lower bound), slot activations, one `RoutingAttemptRecorded` per attempt with input/output tokens and unknown usage kept unknown, continuations, reviewer legs and results, revisions, gate evaluations and risk derivations; `GetTaskEconomics` summed tokens at list price for one model.
- first missing link: nothing assembled those records into a request's accounting and outcome. Attempts carried no cached subset, latency or price (the only per-attempt pricing, EPR-006's spend-so-far, ignored the cache discount); the compiler's candidates with their quality mean and lower bound were computed and dropped; a reviewer leg's usage sat on its review task, unlinked from the request's cost; an invocation a crash cut off left no trace in any total; there was no settlement of reservations, no late-invoice path, no attribution of request against leg against gate, and no raw-signal record.
- found on the way: the Messages (Anthropic) adapter reported `input_tokens` without the cache reads that wire reports beside it, while Chat Completions' total includes them — the same field meant two things. Fixed in `crates/providers/src/anthropic.rs` (plain + read + write is the total; reads and writes are its subsets; a repeated cumulative report replaces the held one) with `Usage.cache_write_input_tokens`.
- production entry points:
  - `services/modbit-core/src/accounting.rs` — `price` (per-subset pricing on the EPR-009 basis), `derive` (the record from the log: tenant-filtered, review tasks followed, attempts keyed and deduplicated, cut-off invocations held, late invoices applied once, legs, versions, decisions, cost, reservations, request/leg/gate observations, signals by reference, labelled counterfactual, missing signals), `record_event`/`append_record`, `view` (`GetRequestOutcome`), `reconcile` (`ReconcileUsage`).
  - `services/modbit-core/src/runtime.rs` — every attempt priced where it ends (`routing_attempt_event`: cached/write subsets, latency, `cost_minor`, `priced_under`); the record appended in the same batch as every run end (`RUN_END`, with the end's task state and stop reason); a decision record for the uncompiled direct baseline.
  - `services/modbit-core/src/routing.rs` — `RoutingDecisionRecorded` beside every compiled and operator admission (candidates with mean/LCB in basis points, choice probability, compile latency).
  - `services/modbit-core/src/critique.rs` (record at `REVIEW_CONCLUDED`), `review.rs` (at `REVIEW_DECIDED`), `server.rs` (`GetRequestOutcome`, `ReconcileUsage`).
  - `crates/domain` — `RunEvent::RoutingDecisionRecorded`, `RoutingAttemptRecorded` extended (serde defaults: older attempts read as unknown, never zero), `TaskEvent::UsageReconciled` and `TaskEvent::RequestOutcomeRecorded` (valid in every task state), `routing::RoutingCandidate`.
  - `crates/protocol/proto/modbit/v1/surface.proto` — the two commands and their views; TypeScript bindings regenerated.

## Verification

- `qual_epr_010_the_request_record_reconciles_to_provider_usage_and_keeps_legs_and_gates_apart` (services/modbit-core, real Core, scripted OpenAI-compatible provider reporting cached prompts): the initial leg on gpt-5-mini fails (NO_PROGRESS) — request `fail`, initial leg false, the compiler's decision with every candidate's mean/LCB and every plan version on record, the durable `RUN_END` record holding every attempt; an operator plan prevalidates gpt-5 and the resumed run escalates and succeeds — request `pass`, verified, not first-pass, initial leg still false, escalation success true, path `CASCADE`, the gate's REJECT kept with `NONE_OBSERVED`; every attempt reconciles token for token (input, cached subset, output, provider request id) with what the provider reported, and the cost equals those tokens at the registry's prices; the operator plan's versions and decision (`OPERATOR`, mean/LCB) read back; missing signals named; no saving field; one `RUN_END` record per run end; the person's ACCEPT is an explicit signal by reference with a `REVIEW_DECIDED` record; a killed and restarted Core rebuilds the same record.
- `epr_fi_010_unknown_work_holds_its_reservation_across_a_kill_and_a_late_invoice_settles_it_once`: a provider answer without usage and an invocation cut off by a Core kill each hold their slot's reservation before and after the resumed run (cost incomplete, total = priced + held, the run end's record says so); a late invoice with the wrong request id, for a missing attempt, with subsets larger than the total, or for a reported attempt is refused (`REQUEST_ID_MISMATCH`, `NO_SUCH_ATTEMPT`, `BAD_PAYLOAD`, `ALREADY_KNOWN`); the right one settles the attempt once at the registry's prices (held falls by exactly one reservation, inference rises by exactly the invoice) with a `USAGE_RECONCILED` record; a second delivery is a duplicate that charges nothing and leaves one invoice on the log; an unknown request has no record.
- Unit, on the real SQLite store and object directory (`accounting::tests`): per-subset pricing and clamping; a replayed attempt record counted once and named; another tenant's envelopes in the same session and run excluded; a steering payload removed by retention listed as unavailable and missing, the intervention still counted from its envelope, money and outcome unchanged.
- `usage_means_the_same_on_both_wires_and_a_repeated_report_is_not_added` (crates/providers conformance).
- Regression: the full modbit-core suite and the providers, domain and event-store suites (see evidence.json).

## Failure and negative proof

- Before the change, the same escalated request had no record at all; the Messages adapter's `input_tokens` for the conformance stream (50 plain, 40 read) read 50 where the bill is 90.
- The FI test's refusals and the duplicate delivery are the negative cases of the late-invoice path; the unit tests are the negative cases of tenant filtering, deduplication and retention.

## Limitations

- Retention deletion is exercised at the accounting boundary (a payload object removed under the store); no product operation deletes payloads yet — docs/31 "Retention" keeps event data until an account/enterprise retention policy deletes it, and that operation belongs to the retention owner. The record copies no content, so such an operation governs the signals it references.
- Deterministic verification is local process time with no price; it is reported as time, not money.
- The keep signal needs a configured keep window and the composite reward a reward version; neither exists, and both are named missing rather than assumed.
- Model calls outside routing attempts (compaction workers, the vision bridge) are not in the request's attempts; their usage is on the log (`MediaBridged` carries tokens) and joins the record when those calls become attempts.
- The live-provider half: the proof runs the real Core against a scripted provider over HTTP; any live run (the live-competence workflow) now produces the same records.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/34, docs/30, docs/15 and docs/27 carry the as-built paragraphs
