# Task Card — PX-023 Screen flow and state completeness with notification model

## Identity

- Task ID: PX-023
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-023 (related: REQ-EV-0010); owner label: desktop; subsystem: `apps/desktop/src/renderer/screens.ts`, `apps/desktop/src/renderer/notifications.ts`, `apps/desktop/src/renderer/model.ts`, `apps/desktop/src/renderer/index.tsx`, `apps/desktop/src/main/main.ts` (task status, question, approval, pull request and notification IPC), `packages/ide-adapter-core/src/client.ts` (`openPullRequest`, `respondToQuestion`)
- Qualification: QUAL-PX-023 — each screen's empty, loading, populated, error, degraded and recovery states are forced against a real Core (Core restart, provider down, stale bundle, offline, unknown outcome, quality floor infeasible, human continuation) and each names cause, next action and evidence; notifications fire only for attention, completion and failure and coalesce per task. Failure proof: a screen missing a required state fails the matrix; routine progress producing a notification fails; a recovery banner claiming progress not in Core events fails.
- Evidence tier: real-system (the real Electron app against the real modbit-core; the rows whose substrate is M7–M9 are mapped and deferred by DR-M6-003)

## Goal

Every screen has its six states, each degraded or error state names its cause, next action and evidence from what the Core said, and notifications exist for attention, completion and failure only — coalesced per task, deep-linked, OS delivery opt-in with quiet hours (docs/39).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: docs/39 declared the matrix and the notification model; the desktop had the Core-restart, recovery and error banners (M0/M1) and the M6.6 board. Found while building: the renderer dropped every run-aggregate and approval-aggregate event (only `aggregateType == "task"` reached the cards), and the reducer compared wait reasons to `Approval` / `UserInput` while the wire spells them `APPROVAL` / `USER_INPUT` — so no card ever entered the awaiting-human phase from a live event. Both fixed here (`model.ts` `normalizeReason`, the event filter in `index.tsx`).
- production entry points: `screens.ts` (`fleetState`, `newTaskState`, `taskState`, `reviewState`, `settingsState` → `ScreenState {screen, kind, label, cause?, nextAction?, evidence?}`), `notifications.ts` (`deriveNotifications`, `newSince`, `osDeliveryDue`, `BURST_THRESHOLD`, preferences), `model.ts` (`diagnostic`, `approval`, `question`, `feasibility`, `gate`, `lastOffset` on the card from `TaskNeedsAttention`, `ApprovalRequested/Resolved`, `UserQuestionAsked/Answered`, `RoutingPlanAdmitted`, `AcceptanceGateEvaluated`), `index.tsx` (`StateLine` on every screen; the notifications list with deep links; Settings with credential custody and notification preferences; the card's approval and question panels; the Review's pull request flow; reconnect by cursor; `GetTaskStatus` seeding on a fresh load), `main.ts` (`task:status`, `question:respond`, `approval:resolve`, `review:pullRequest`, `notify:deliver` / `notify:log`), `preload.ts`, `client.ts`.
- proof: `states.spec.ts` (three tests) forces, on a real Core: a provider refusing connections → task `provider stopped the task` (`CONNECT_FAILED`, the Core's user action, the offset) and Fleet `provider down`; a Core aborting before committing a write's outcome (`MODBIT_FAULT_KILL_BEFORE_EVENT=ToolCallSucceeded:2`) → Fleet `Core restarting` (cards `reconnecting by cursor`), `Core recovered` with the recovery report of boot 2, the task `unknown tool outcome under reconciliation` (`RESTART_RECONCILING`, `ReconcileToolCall` as the next action); the agent's `user.ask` → `awaiting your answer` with the question id, answered on the card, resumed; a declared destructive effect → `awaiting approval` naming the intent hash and the approval id, approved on the card, the effect run once; the Review's state following the acceptance gate; a pull request `APPROVAL_PENDING` → denied (`DENIED`, the branch absent from the real bare remote, nothing POSTed) → tried again → approved → `OPENED #1` with the push on the remote and the POST carrying the Core's token; notifications: the attention item inside quiet hours listed, deep-linked and not delivered (main's log empty), the completion delivered once quiet hours end, the restart's `RESTART` item delivered; `onboarding.spec.ts` (composer `provider unavailable`, Fleet `Core refused` with `NO_PROVIDER`, composer `repository untrusted or missing` with `REPOSITORY_MISSING`); `user-patch.spec.ts` (Review `stale candidate revision`); `screens.test.ts` and `notifications.test.ts` (every row's mapping, kinds, coalescing, burst, opt-in, quiet hours, newness); `model.test.ts` (wire wait reasons).

## Limitations

- stale policy bundle, offline with cloud features, the daily budget, the Browser screen and organization overrides wait for their M7–M9 substrate (DR-M6-003); "quality floor infeasible" and "policy forbids the requested execution mode" are proven at the reducer level and in the surface suite, not forced through the packaged app; OS notification display itself is not asserted (main's delivery log is, with `MODBIT_SUPPRESS_OS_NOTIFICATIONS=1` on hosted runners).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/states.spec.ts` — "screen states: provider down, human continuation, review gate and a denied then opened pull request; notifications coalesce, deep-link and honour opt-in and quiet hours"; "screen states: a Core that dies before committing a tool outcome comes back reconciling — degraded, recovery, then the task's unknown outcome with its evidence; the restart notifies"; "task workspace: awaiting your answer (the agent's typed question) and awaiting approval with the exact intent, decided on the card; the reasons notify and coalesce"
- `apps/desktop/e2e/onboarding.spec.ts`, `apps/desktop/e2e/user-patch.spec.ts`, `apps/desktop/e2e/fleet-agents.spec.ts` (assertions added)
- `apps/desktop/src/renderer/screens.test.ts`, `apps/desktop/src/renderer/notifications.test.ts`, `apps/desktop/src/renderer/model.test.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
