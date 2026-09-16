# Desktop Frontend Implementation

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Stack

- Electron shell with hardened main/preload/renderer split.
- React + TypeScript renderer.
- TanStack Query for command/query cache where useful; authoritative live state comes from event reducer, not optimistic UI assumptions.
- A small state machine/reducer layer for local UI state only.
- Syntax highlighting and diff rendering in Trusted Code Surface; **no Monaco/Code-OSS/editor buffer architecture**.

All dependency versions are pinned exactly in lockfiles and updated through automated compatibility/security PRs.

## Security settings

For every renderer/web view: `nodeIntegration=false`, `contextIsolation=true`, Electron sandbox enabled, no remote module, restrictive CSP, no arbitrary navigation, no direct shell/file APIs. Preload exposes only generated SurfaceProtocol functions.

Untrusted browser `WebContentsView` lives in a dedicated partition and cannot call Modbit preload APIs.

## Renderer modules

```text
src/
├─ app-shell/
├─ fleet/
├─ task/
├─ timeline/
├─ approvals/
├─ execution-policy/
├─ code-review/
├─ terminal/
├─ browser/
├─ artifacts/
├─ evidence/
├─ workspaces/
├─ settings/
├─ protocol-client/
└─ shared with apps/cli and packages/ide-adapter-core: one thin-client contract (docs/29)
```

As built (PX-001): the protocol client and the Core supervisor live in `packages/ide-adapter-core` (`CoreClient`, `CoreSupervisor`); `apps/desktop/src/main/main.ts` imports them and adds only Electron hosting (windows, IPC, safeStorage). The desktop passes the thin-client conformance suite as "shared protocol client" (docs/29 "As built (PX-001)").

## Event consumption

On window load:
1. request Session/Fleet snapshot;
2. subscribe from snapshot cursor;
3. reduce ordered events into view models;
4. acknowledge cursor periodically;
5. on gap, fetch replay;
6. on incompatible/expired cursor, fetch fresh projection.

Renderer never fabricates task completion. A “completed” card only renders from Core `TaskCompleted` event.

As built (M6.6): the Fleet reducer (`apps/desktop/src/renderer/model.ts`) derives, from Core events only, what the PRD "Home / Fleet" card shows beyond its state — the phase (`drafting`, `verifying` from `VerificationRunRecorded`, `reviewing` / `awaiting human` from `AcceptanceGateEvaluated` and `TaskWaiting(Approval | UserInput)`, `escalating` from `ContinuationActivated`, `delegating` from `AgentNodeCreated` / `SubagentAdmitted`, `waiting for capacity` from `CapacityDenied`, `done`), the active agent count over the AgentGraph (`AgentNodeCreated` / `AgentNodeTransitioned`, by status), the realized risk level (`RealizedRiskDerived`), the latest evidence line (verification, gate verdict, continuation, delegation, a child's result) and the next required action — a child that ended `FAILED` or `WAITING` or a refused capacity ticket puts the parent in Needs Attention with the reason. A subagent's task is never a top-level card: it nests under its parent (`childrenOf`), linked by `SubagentAdmitted` / `AgentNodeCreated.child_task_id` live and by `TaskView.parent_task_id` / `origin` from the snapshot after a restart. The renderer applies run-aggregate events stamped with the task id as well as task events; nothing is inferred from logs. Event payloads name tasks as dashed UUID text while envelopes and snapshots carry bytes rendered as 32-hex; the reducer keys every card by the hex form (`hexId`), so a child named in `AgentNodeCreated` / `SubagentAdmitted` links to the card its own `TaskCreated` envelope created.

As built (REQ-EV-0151, REQ-EV-0275): the board's attention strip (`data-testid="attention"`, one `attention-item` per entry with `data-kind` and `data-task-id`) renders the Core's `GetAttention` view for the session — items derived in `services/modbit-core/src/attention.rs` from canonical unresolved state only: an approval still `REQUESTED` (`APPROVAL`), a question still unanswered (`QUESTION`), a start refused for capacity (`CAPACITY`), a waiting task's latest attention line (`STALL` for no progress, `RESTART` after a Core restart — also for a task that awaited an approval when the Core restarted, since resolving that approval does not re-enter the suspended run and a `StartTask` does (found by the VS Code adapter's restart test, PX-002) — `BLOCKER` for a harness/budget/policy stop, `FAILURE` otherwise), and, for a running parent, a child's write conflict not superseded by a later admission (`CONFLICT`) or a protected effect it reached and has not ended since (`PROTECTED_EFFECT`); each carries the command or tool that clears it. The renderer re-reads the view after every task event (debounced) and never adds an item of its own; an item is gone the moment its resolving event lands — `ApprovalResolved`, `UserQuestionAnswered`, `TaskStarted` / `TaskResumed` / `TaskCancelled`, `CapacityTicketGranted`, `SubagentAdmitted`, `SubagentResultRecorded`. There is no reminder store and no second scheduler; the CLI reads the same view with `attention list --session <id>`. Tests: `qual_ev_0151_0275_attention_items_are_derived_from_canonical_state_and_clear_with_it`, `apps/desktop/e2e/attention.spec.ts`.

As built (M7.1): the Browser panel (`data-testid="browser"`) opens a task's browser session from its card (`task-browser`): main opens it on the Core and hosts the sandboxed `WebContentsView` (`apps/desktop/src/main/browser.ts`); the renderer reports the placeholder's rectangle (`browser:show`) and keeps the view there through resizes, hides it when the panel closes, and renders only what main and the Core report (`browser:describe`, `browser:session`, `browser:probe`, the `browser:state` pushes) — the page's content never enters the renderer. The panel's state (`browserState` in `screens.ts`): opening, empty (about:blank), populated, `session lost` (the page's renderer process gone, with Reopen), `host not attached`, `takeover active` (M7.6).

## Task composer behavior

Submit button first calls `CreateSession` if needed, then `CreateTask`. UI renders queued task from returned durable IDs. If the window crashes after response, the task is recoverable from Core.

Advanced controls map directly to typed policy/execution options; no hidden checkbox that bypasses capability rules.

## Attention UX

`Needs Attention` is derived from structured reasons: approval pending, user question, policy conflict, capacity/quota, secret required, ambiguous effect, merge conflict, human-required plan continuation, quality floor infeasible, budget exhausted with partial evidence, or unrecoverable runtime fault. The card shows the **single next action**, not raw agent logs.

## Task surface

Timeline groups low-level events into expandable RunSteps while preserving raw evidence access. Live streaming uses bounded UI buffers; old terminal/model deltas collapse to OutputRefs/event summaries to avoid renderer memory growth.

## Trusted Code Review

- Fetches content by CodeReference/revision.
- Shows stale banner when current revision differs.
- Diff actions are review operations: accept/merge/export/open externally/discard.
- Diagnostics and tests are linked to exact revision.
- No editable shadow buffer.

## Terminal

Terminal component connects to a TerminalSession stream using cursor. Scrolling back beyond replay window requests OutputRef ranges. User input is only enabled when policy says the terminal is user-controlled; agent and user input ownership is explicit.

## Browser

Renderer hosts a local `WebContentsView` controlled by main, or a remote viewer for cloud. Control lease badge is always visible. “Take control” is a command to Core/main, not a UI-only toggle.

## Error states

Every recoverable infrastructure error exposes: affected task, last durable state, retry/reconnect action and evidence ID. Generic toast-only handling is forbidden for task-affecting errors.

## States, notifications and budgets

Renderer modules implement the per-screen state matrix, notification coalescing and keyboard model of `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`; a screen without its degraded and recovery states is `DECLARED`, not `IMPLEMENTED`. Interaction budgets are asserted in packaged E2E from Playwright traces and Core event timestamps (PX-022..025).

## Frontend completion gate

A screen is not complete until Playwright/Electron E2E drives the real app against real local Core and verifies state through process restart. Storybook/static mock screens may be used for visual development but never count toward feature completion.

## Execution policy projections

Implement objective preference controls and allowed manual pins using `SetExecutionPreference` in SurfaceProtocol. Render plan/leg verification and escalation progress, complete task usage and exact failure/attention reasons from Core events. Expose `GetRoutingDiagnostics` only when authorized; labels follow organization policy. Do not ship routing classifier, model economics, weights, registry authority or credentials in renderer/plugin business logic. Reconnect by event cursor with no repeated provider dispatch. The `execution-policy/` module owns the preference controls, the executed-path timeline (initial leg, escalation, independent review, human), the acceptance verdict with its required assurance, reviewer findings and per-leg cost; the Review surface embeds these projections and cannot render a done state while the verdict is REJECT or INCONCLUSIVE. Path labels and model labels follow the policy bundle's visibility rule. Validate these flows through EPR-005/007/010 in doc 61.
