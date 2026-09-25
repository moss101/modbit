# Task Card — IMP-EV-0032 Per-run token/cost accounting

## Identity

- Task ID: IMP-EV-0032 (REQ-EV-0032, ADOPT; owner Usage Ledger, subsystem observability)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0032 — reconcile a sample provider invoice against canonical usage events within tolerance.
- Evidence tier: release-critical (evidence semantics: what a run is said to have cost)
- No second ledger: rows are the canonical `ModelUsageRecorded` events, priced with the request accounting's own rules (`accounting::price`, EPR-010); `GetTaskEconomics` is completed in place.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL with drift. The canonical path existed for routed attempts (`RoutingAttemptRecorded` → `accounting::derive` → `GetRequestOutcome`, EPR-010), but the view every client reads, `GetTaskEconomics`, summed every usage event (reported or not) and priced the whole task at the *last* model's catalog list price in floating point, with `pricing_known = 1` whenever that one model was catalogued. Nothing attributed usage to a run's steps, tool or verification time to steps, and nothing compared an invoice with the log (`ReconcileUsage` settles only an attempt whose usage was unknown).
- first missing link: no per-call ledger with the provider's request id and no invoice comparison with a tolerance.
- production entry points: `services/modbit-core/src/usage.rs` (`calls`: the ledger, priced per binding; `attribution`: per run and step with tool and verification time; `reconcile_invoice`); `services/modbit-core/src/economics.rs` (ledger cost, per-call catalog price, `runs`); `services/modbit-core/src/server.rs` (`ReconcileInvoice`); `apps/cli/src/main.rs` (`task economics` ledger lines, `usage reconcile`); `packages/ide-adapter-core/src/client.ts` (`reconcileInvoice`).

## Verification

- `qual_ev_0032_a_provider_invoice_reconciles_against_the_canonical_usage_within_tolerance` (real Core, signed model registry, scripted OpenAI-compatible provider): the provider keeps its own record of what it served and charged — request id, model, input, cached and output tokens — independent of the Core's log. After a three-call task: `GetTaskEconomics` prices all three (`USD`, the registry's generation), `cost_minor` equals what the provider billed, one run whose three steps each hold one call and whose steps add up to the task's tokens, cost and tool calls, and whose request ids are exactly the provider's. The invoice built from the provider's record reconciles exactly (three `MATCHED`, `delta_bp` 0, each row attributed to its run); a row with 3% more input is `OUT_OF_TOLERANCE` at 100 bp and within at 500 bp; a row the log never saw is `NOT_IN_LOG` and a call the invoice left out is `NOT_ON_INVOICE`; a payload that is not an invoice sample is refused `BAD_INVOICE`.
- Mutation: reconciling without the tolerance comparison accepts the 3% row and fails the test.
- Unit: `usage::tests::{an_invoice_reconciles_row_by_row_within_its_tolerance, the_difference_is_relative_and_rounded_up}`.

## Limitations

- Side questions append nothing to the log (QUAL-EV-0261), so they are not in the ledger and their invoice rows are `NOT_IN_LOG` by design.
- The invoice sample is Modbit's own schema; a provider's billing export is mapped into it outside the product (no provider export adapter yet). The live half — a real provider's invoice — needs the provider billing access the owner holds.
- Cache-write tokens are not on `ModelUsageRecorded`, so the ledger prices none (the routed attempt record carries them for EPR-010).
- The registry prices only while it is active; without one every call is `unpriced` and only the catalog-list estimate remains.

## Evidence

- `evidence.json` in this directory
- docs/34 "Verified workflow economics" and docs/30 as built; `apps/cli/README.md` "Usage and invoices"
