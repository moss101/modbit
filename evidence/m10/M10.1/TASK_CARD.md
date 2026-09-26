# Task Card — M10.1 telemetry/cost/SLO dashboards

## Identity

- Task ID: M10.1 (milestone task, docs/43; docs/77: "dashboards over the real usage ledger, SLO events and cost records, never a mock feed")
- Milestone: M10 (wave 1); after IMP-EV-0032 (usage ledger) and IMP-EV-0023 (SLO ladder)
- Evidence tier: release-critical (evidence semantics: what the product tells a person the work cost and how the cloud path performed)
- No second store or metrics system: the view is computed by the Core from its canonical log when asked (`services/modbit-core/src/dashboard.rs`), reusing the usage ledger (`usage::calls`) and the ladder derivation (`modbit_observability::slo`).

## Existing-code audit

- classification: DOCUMENTED-ONLY. The desktop showed one task's economics in a banner (the drifted catalog estimate, since corrected by IMP-EV-0032); nothing showed a session's cost by model or task, tool outcomes, SLO figures, failures or provider health, and no client had a cross-task view.
- first missing link: no aggregate over a session's log.
- entry points: `dashboard.rs` (`view`); `server.rs` (`GetDashboard`); `packages/ide-adapter-core/src/client.ts` (`dashboard`); `apps/desktop/src/main/main.ts` (`dashboard:get`), `preload.ts` (`DashboardSummary`), `renderer/index.tsx` (`Dashboard` pane, header button); `apps/cli/src/main.rs` (`dashboard --session`).

## Verification

- `m10_1_the_dashboard_is_the_sessions_ledger_ladder_and_log_aggregated` (real Core, signed registry, scripted provider): one task to review priced under the registry, one refused by the provider; the dashboard's `cost_minor` is what the provider billed and equals the sum of the two tasks' `GetTaskEconomics`; each task row's cost, model calls and tool calls equal its economics; the model rows add up to the cost and the calls; states `ReadyForReview 1`, `Waiting 1`; tool successes equal the log's `ToolCallSucceeded`; the refusal is among the failures; no cloud task, no SLO starts; the provider's health is listed.
- `qual_ev_0023_…` (apps/cloud-worker, real Cloud API, worker, Sandbox Gateway) now also reads the worker Core's dashboard: one cold and one warm start, the cold figures identical to `GetSloLadder`'s, the task row with two starts.
- `e2e/dashboard.spec.ts` (Playwright, the real Electron app and Core, scripted provider): a task runs to review in the app; the Operations pane's tokens, cost, task row (model calls, tool calls, state), tool successes, model row and SLO starts are exactly what `modbit-cli dashboard` reads from the same Core; after a second task a refresh shows two rows and the states.

## Limitations

- Per session: a fleet-wide (all sessions, all tenants) dashboard and exported time series (OpenTelemetry) are not built; docs/34's full metric list (retrieval latency by level, terminal attach latency, …) is not all on the view.
- Cost is shown in the registry's currency only when a registry priced the calls; otherwise the view says unpriced.
- Recent failures are the last 20.

## Evidence

- `evidence.json` in this directory
- docs/34 and docs/30 as built; `apps/cli/README.md` "Dashboard"
