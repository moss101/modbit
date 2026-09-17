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

As built (PX-023): `apps/desktop/src/renderer/screens.ts` derives one `ScreenState` per screen — `kind` in `empty | loading | populated | error | degraded | recovery`, a label, and on every error, degraded and recovery state the `cause`, the `nextAction` and the `evidence` reference — from what the Core said and nothing else: the supervisor's Core status (`restarting` with its reason and retry, `failed`), the recovery report (`bootGeneration`, events and aggregates verified, sessions and tasks recovered, notes still to reconcile), the reducer's cards (the typed `TaskNeedsAttention` diagnostic with class, code, `user_action`, `recovery_path` and `evidence_refs`; the open approval's tool, effect class and intent hash from `ApprovalRequested`; the open question from `UserQuestionAsked`; `RoutingPlanAdmitted.feasibility`; `AcceptanceGateEvaluated`'s verdict, missing evidence, reject reasons and gate ref), the attention view, the Core's command rejections (`REPOSITORY_MISSING`, `NO_PROVIDER`, `STALE_REVISION`, …) and the pull request's ack states. Every screen renders it as a state line (`data-testid="fleet-state" | "composer-state" | "task-screen-state" | "review-state" | "settings-state"`, `data-kind`, and `-label` / `-cause` / `-next` / `-evidence` children). A snapshot carries states, not diagnostics, so a fresh load reads `GetTaskStatus` for every waiting or failed task; a reconnect after a Core restart keeps the cards and resubscribes from the renderer's own cursor (the run-aggregate and approval-aggregate events the Core stamps with the task id reach the cards, as task events do). The Task workspace decides on the card: the approval names the intent hash the Core showed (`ResolveApproval.intent_hash`, refused `INTENT_MISMATCH` for any other) and the question is answered by option or free text (`RespondToQuestion`), then resumed; the Review opens or updates the pull request (PX-007) through `APPROVAL_PENDING` → the decision on the exact intent → `OPENED | UPDATED`, or `DENIED` with the branch left local. Forced against the real Core in `apps/desktop/e2e/states.spec.ts` (provider refusing connections → `PROVIDER / CONNECT_FAILED` on the task and "provider down" on the Fleet, with the Core's user action and offset; a Core aborting before committing a write's outcome → "Core restarting" with the cards reconnecting by cursor, then "Core recovered" with the recovery report, then the task's "unknown tool outcome under reconciliation" from `RESTART_RECONCILING`; the agent's typed question → "awaiting your answer"; a declared destructive effect → "awaiting approval" with the intent, approved on the card; the acceptance gate's verdict on the Review; a pull request denied then opened against a wire-faithful GitHub fake and a real bare remote), `onboarding.spec.ts` (provider unavailable, `NO_PROVIDER`, repository missing) and `user-patch.spec.ts` (stale candidate revision). Rows whose substrate is later — stale policy bundle (`REGISTRY_STALE`, M9), offline with cloud features (M8), the Browser screen (M7), the daily budget and organization overrides — are mapped in `screens.ts` where the Core already names them and otherwise deferred by `DR-M6-003`; "quality floor infeasible" and "policy forbids the requested execution mode" are derived from `RoutingPlanAdmitted.feasibility` and the Core's rejection codes and proven at the reducer level (`screens.test.ts`), not forced end to end.

As built (M7.7 / M7.8, on the same screens): the task card carries a `security` count (`data-testid="task-security"`: `n blocked, m marked`, from `SecurityEventRecorded` — an instruction-shaped passage marked in what a tool returned, a call refused for carrying a credential; the shapes and the tool, never a secret), and Settings has a "Login credentials" section (`credentials`: the handles the desktop's broker holds with label, origin, account name and whether the OS keychain persists them; `credential-form` binds a new secret to an origin — the value crosses main once into `safeStorage` and never comes back; `credential-remove` forgets one). The Browser screen itself (M7.1–M7.6) is the `browser` panel: the hosted view over its placeholder, the state line, the lease (`browser-control` with take / return), the isolation probe.

## Notification model

- Notifications exist for `Needs Attention` reasons, task completion into `ReadyForReview`, and task failure. Routine progress never notifies.
- Delivery: in-app badge and list always; OS notifications through Electron main, off by default per task type until the user opts in; quiet hours respected.
- Coalescing: multiple reasons on one task collapse into one notification with the single next action; fleet-wide bursts collapse into a count.
- Every notification deep-links to the exact screen and state; acting on it is one keystroke away.
- CLI and IDE adapters receive the same reasons through events and render them in their idiom.

As built (PX-023): `apps/desktop/src/renderer/notifications.ts` derives the notifications from the cards and the Core's attention view — one per task, `attention` (every attention item's reason joined, the oldest item's action as the single next action, `coalesced` = the item count), `completion` (`ReadyForReview`, deep-linked to the Review) or `failure` (`Failed`, the typed diagnostic) — and collapses a burst above `BURST_THRESHOLD` tasks into one count deep-linked to the attention list; routine progress derives nothing. The in-app list (`data-testid="notifications"`, one `notification` with `data-kind`, `data-coalesced`, `data-count`, `data-delivered` and an `Open …` button that lands on the task card, the Review or the attention list) is always on. OS delivery is opt-in per kind in Settings (`notify-attention | notify-completion | notify-failure`), respects quiet hours (`notify-quiet`, start and end hours, wrapping midnight), considers only what is new since the last derivation (`newSince`, by id and reason), and goes through Electron main (`notify:deliver`: title and one line, never a path, a payload or a secret) which keeps a bounded delivery log (`notify:log`) — what the E2E reads instead of the notification centre (`MODBIT_SUPPRESS_OS_NOTIFICATIONS=1` keeps hosted runners quiet). Proven in `notifications.test.ts` (kinds, coalescing, burst, opt-in, quiet hours, newness) and forced in `states.spec.ts` (an attention notification inside quiet hours listed but not delivered; the completion delivered once quiet hours end; the restart's `RESTART` item delivered).

## Keyboard model (PX-024)

Global: new task, search, jump to attention list, jump to running, approve or deny focused approval (with confirmation for irreversible effects), take or return browser control, pause and cancel focused task. Lists: arrow navigation, enter to open, escape to return, type-ahead filter. Review: next and previous change, expand hunk, toggle unified and split, jump to failing test. All controls reachable by keyboard; focus never lost on state change; live regions announce attention changes; status is never color-only. The accessibility conformance suite runs in the packaged E2E.

As built (PX-024): `apps/desktop/src/renderer/keyboard.ts` is the pure mapping (`commandFor(key, {editable, screen, confirming, isMac})` → command; `SHORTCUTS` is the reference card the `?` panel renders): global `⌘/Ctrl+N` new task, `/` or `⌘/Ctrl+K` search (a type-ahead filter of the fleet), `a` attention list, `r` running column, `y` / `x` approve / deny the focused task's effect, `c` cancel, `s` steer (one line of `QueueInput STEER` on the card), `?` help; lists `↑ ↓` / `j k` within a column, `← →` across columns (from outside the board: the first or last column with cards), `Enter` opens (the Review for a result, the first control otherwise), `Escape` returns (from the Review to its card, from a field, from a confirmation); Review `↑ ↓` / `j k` previous / next change (every hunk is focusable), `Enter` / `e` collapse / expand the focused hunk, `u` unified / split (`diff.ts` pairs each run of removals with the additions that follow), `f` the failing check (or the verification list when none fails). Single letters never command while a field has focus; `Enter` and `Space` on a button or checkbox stay the control's; the platform modifier is ⌘ on macOS and Ctrl elsewhere. Irreversible effects confirm: `y` on a `Destructive` or `ExternalSideEffect` approval and `c` open an `alertdialog` naming the tool, the effect class, the intent hash and the task; only `Enter` confirms, `Escape` declines and nothing is sent. The approval, once confirmed, names the intent the Core showed (`INTENT_MISMATCH` otherwise). Focus is retained across the Core's state changes: a card re-mounted in another column on a `TaskWaiting` / `TaskStarted` / `TaskReadyForReview` / `TaskCancelled` event gets its focus back; the Review takes the focus when it opens and returns it to its card when it closes. Live regions: `data-testid="live-attention"` (`role=status`, polite) announces each new attention item in words — kind, task, reason — and every state line is `role=status` (`alert` on an error) with its kind in text (a mark and the word), never colour alone; hunk lines keep their `+` / `-` prefixes. The accessibility suite is axe-core 4.10 (WCAG 2.x A/AA and best practices), evaluated in the renderer by the E2E on the empty fleet and New Task, the populated fleet, the attention state, the help panel, the confirmation, the Review unified and split, and the final board — no violation of any impact — which found and fixed: content outside landmarks, the hidden fleet still displayed under the Review (`main[hidden]`), a second main and a nested complementary in the Review, scrollable plan / self-review / evidence regions without keyboard access. `apps/desktop/e2e/keyboard.spec.ts` drives all of it without one click, against the real Core (a task run to its declared destructive effect, approved by keyboard with confirmation, reviewed, a second task cancelled with confirmation). "Take or return browser control" waits for the Browser screen (M7, DR-M6-003); "pause" has no Core command — the keyboard offers cancel (confirmed) and steer.

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
