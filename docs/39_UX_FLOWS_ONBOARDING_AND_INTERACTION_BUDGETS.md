# UX flows, onboarding and interaction budgets

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 item 7 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`). Owner: desktop; the CLI and IDE adapters inherit the same states and copy rules through the thin-client contract (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`).  
> **Rule:** every screen in `10_PRODUCT_PRD_AND_UX.md` has the states, flows and budgets below before it counts as implemented; a screen with a happy path only is `DECLARED`, not `IMPLEMENTED`.

## Principles

1. **Attention is the scarce resource.** The product interrupts only for decisions the user must make; everything else is visible on demand.
2. **Every state names the next action.** Empty, error, degraded and recovery states each say what happened, why, and the single thing the user can do, with a link to evidence.
3. **Truth comes from Core.** No client shows a state the Core has not persisted; optimistic UI is limited to acknowledging a command, never to showing an outcome.
4. **Measured, not admired.** Onboarding time, interaction latencies and state coverage are metrics with E2E scenarios, not design intentions.

## Onboarding: first useful task within five minutes (PX-022)

**Definition.** From first launch of a fresh install to the first task reaching `ReadyForReview` with real evidence (a diff, a test run, receipts) on a small real repository, using a representative starter task, on the reference hardware named in `53_PERFORMANCE_AND_BENCHMARK_PLAN.md`. The clock includes provider setup and repository trust; it excludes only the user's own reading time as measured by the scripted E2E. Median over fresh profiles must be within five minutes; the p90 is reported.

**Flow.**

1. Launch: the app starts Core, shows a single-screen welcome with three steps and no marketing.
2. Provider: paste or sign in for one model provider; the key goes to the OS keychain through Electron main; a live test call confirms it; failure explains the cause (invalid key, network, quota) and offers retry. Organization bundles pre-fill policy where present.
3. Repository: open a local repository or clone one; trust is explicit and scoped; the workspace snapshot and index start in the background with visible progress and a usable state before indexing completes.
4. First task: a template gallery of small tasks appropriate to the detected stack (add a test, fix a lint failure, explain a module) plus a free-text goal; the task starts immediately with the direct baseline configuration.
5. First result: the Review screen opens on the diff with tests and evidence; the user merges, exports or discards; the next-step hint points to the Fleet view and the CLI.

**Empty states.** No provider: the welcome persists with step 2 highlighted. No repository: step 3. No tasks: New Task with templates. Every empty state is a working control, not an illustration.

## Screen flows and states (PX-023)

Each screen defines five states: **empty**, **loading**, **populated**, **error**, **degraded**, plus **recovery** where the Core reports a restart or reconciliation.

| Screen | Degraded and recovery states that must exist |
|---|---|
| Home / Fleet | Core restarting (banner with the last persisted state and a countdown), provider down (tasks Waiting with cause), stale policy bundle (routing on last-good bundle, `REGISTRY_STALE`), offline (local tasks continue, cloud features disabled with reason) |
| New Task | provider unavailable, repository untrusted or missing, budget exhausted for the day, policy forbids the requested execution mode |
| Task workspace | event stream reconnecting by cursor, unknown tool outcome under reconciliation, awaiting approval with the exact intent, quality floor infeasible under policy, awaiting human continuation |
| Review | verification incomplete (verdict INCONCLUSIVE with missing evidence listed), REJECT with reasons, stale candidate revision, PR push denied or failed with receipt state |
| Browser | takeover active, session lost with recovery choice, visual fallback in use with reason |
| Settings | keychain unavailable, organization policy overrides a user preference (shown, not hidden) |

Error copy names the cause, the next action and the evidence reference. Recovery states show what the Core recovered and what still needs reconciliation; they never invent progress.

## Notification model

- Notifications exist for `Needs Attention` reasons, task completion into `ReadyForReview`, and task failure. Routine progress never notifies.
- Delivery: in-app badge and list always; OS notifications through Electron main, off by default per task type until the user opts in; quiet hours respected.
- Coalescing: multiple reasons on one task collapse into one notification with the single next action; fleet-wide bursts collapse into a count.
- Every notification deep-links to the exact screen and state; acting on it is one keystroke away.
- CLI and IDE adapters receive the same reasons through events and render them in their idiom.

## Keyboard model (PX-024)

Global: new task, search, jump to attention list, jump to running, approve or deny focused approval (with confirmation for irreversible effects), take or return browser control, pause and cancel focused task. Lists: arrow navigation, enter to open, escape to return, type-ahead filter. Review: next and previous change, expand hunk, toggle unified and split, jump to failing test. All controls reachable by keyboard; focus never lost on state change; live regions announce attention changes; status is never color-only. The accessibility conformance suite runs in the packaged E2E.

## Interaction budgets (PX-025)

Budgets are p95 on the reference hardware unless stated; they extend `53_PERFORMANCE_AND_BENCHMARK_PLAN.md` and are enforced in packaged E2E.

| Interaction | Budget |
|---|---:|
| Cold start to interactive Fleet (Core already installed) | < 2 s |
| Command acknowledgement (create, steer, approve) | < 100 ms |
| Event-to-render | < 100 ms |
| Open Review diff for a 500-line change | < 300 ms |
| Attention list update after a Core event | < 150 ms |
| Browser takeover switch | < 250 ms |
| Reconnect and replay after renderer restart | < 1 s to last state |
| Onboarding to first useful task (median) | < 5 min |

A budget miss fails the packaged E2E for the release-critical tier and is reported for the iteration tier.

## Measurement and evidence

PX-E2E-022 measures onboarding on fresh profiles with Playwright driving the packaged app; PX-E2E-023 checks the state matrix by forcing each degraded condition against a real Core; PX-E2E-024 traverses every screen by keyboard and runs the accessibility suite; PX-E2E-025 asserts the budgets. Screenshots are supplemental; state and timing come from Core events and Playwright traces.
