# Task Card — IMP-EV-0030 Specialist model chains

## Identity

- Task ID: IMP-EV-0030 (REQ-EV-0030, ADAPT; owner Model Router)
- Milestone: M9
- Qualification: QUAL-EV-0030 — primary outage triggers approved fallback and records RouterDecisionRecord.
- Evidence tier: release-critical (execution, routing and recovery behaviour)
- Built on the existing ConditionalExecutionPlan (doc 77, docs/27/38): no second router.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The plan schema had `Trigger::LegFailed` ("the predecessor's leg failed outright") and the gateway retried a failing provider a bounded number of times, but no binding could declare a fallback, the compiler never emitted a `LegFailed` slot, and a provider failure always ended the run (`ProviderFailed` → Waiting). The routing view also listed executed legs in stored-row order, which mislabels any path whose later slot id sorts first.
- first missing link: nothing declared an approved fallback, so nothing could be prevalidated or activated.
- production entry points: `crates/providers/src/registry.rs` (`RegistryEntry.fallbacks`, `MAX_FALLBACKS`, activation refuses `REGISTRY_INVALID_FALLBACK`); `compiler.rs` (the opener's chain as `fallback-N` `LegFailed` slots in every shape, in the worst case, out of the expected cost, exclusions recorded); `services/modbit-core/src/escalation.rs::at_provider_boundary` (admission, `SlotActivated`, `RouteReevaluated` PROVIDER, `ContinuationActivated` LEG_FAILED, agent rebinding, note); `runtime.rs` (both provider-failure exits call the boundary and continue on activation); `routing.rs` (`FALLBACK` label; executed legs ordered by first attempt); `accounting.rs` (a leg continued by a fallback is `FAILED`).

## Verification

- `qual_ev_0030_a_primary_outage_continues_on_the_approved_fallback_and_records_the_decision` (services/modbit-core, real Core, scripted OpenAI-compatible provider answering the primary model with 503 on every request): the signed registry names `gpt-5` as `gpt-5-mini`'s fallback; the compiled plan carries `fallback-1` (`gpt-5`, after `initial`) before anything runs, with its decision record; the primary's bounded retries are spent; the same run continues on `gpt-5` with one `SlotActivated`, a `PROVIDER` `SWITCH` from `openai/gpt-5-mini` to `openai/gpt-5` and a `LEG_FAILED` continuation (`PROVIDER_FAILED:…`); the fallback reads, edits and completes, verified by the repository's own check; the route reads `FALLBACK`; the request record keeps the initial leg failed and the continuation successful. In a fresh session under a registry with no chain, the same outage stops the run (`Waiting`) with a `PROVIDER` `STAY` naming the missing fallback and no continuation.
- `crates/providers`: `an_approved_fallback_chain_is_compiled_as_prevalidated_leg_failed_slots` (order, predecessors, worst case up, expected cost unchanged, an unusable fallback excluded with its reason); `a_fallback_chain_is_bounded_and_names_only_this_documents_bindings`.
- Regression: `qual_epr_006_…` (CASCADE), `qual_epr_007_…` (CRITIQUE), the compiler golden corpus (unchanged plan ids), the full modbit-core suite.

## Limitations

- The chain is declared on the signed registry entry — the operator's approved profile of that binding; agent profiles keep naming the preferred model and do not add fallbacks of their own.
- The chain follows the opener; a stronger continuation slot carries no fallback of its own.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/15 and docs/30 carry the as-built paragraphs
