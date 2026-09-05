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
├─ code-review/
├─ terminal/
├─ browser/
├─ artifacts/
├─ evidence/
├─ workspaces/
├─ settings/
└─ protocol-client/
```

## Event consumption

On window load:
1. request Session/Fleet snapshot;
2. subscribe from snapshot cursor;
3. reduce ordered events into view models;
4. acknowledge cursor periodically;
5. on gap, fetch replay;
6. on incompatible/expired cursor, fetch fresh projection.

Renderer never fabricates task completion. A “completed” card only renders from Core `TaskCompleted` event.

## Task composer behavior

Submit button first calls `CreateSession` if needed, then `CreateTask`. UI renders queued task from returned durable IDs. If the window crashes after response, the task is recoverable from Core.

Advanced controls map directly to typed policy/execution options; no hidden checkbox that bypasses capability rules.

## Attention UX

`Needs Attention` is derived from structured reasons: approval pending, user question, policy conflict, capacity/quota, secret required, ambiguous effect, merge conflict or unrecoverable runtime fault. The card shows the **single next action**, not raw agent logs.

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

## Frontend completion gate

A screen is not complete until Playwright/Electron E2E drives the real app against real local Core and verifies state through process restart. Storybook/static mock screens may be used for visual development but never count toward feature completion.

## Execution policy projections

Implement objective preference controls and allowed manual pins using `SetExecutionPreference` in SurfaceProtocol. Render plan/leg verification and escalation progress, complete task usage and exact failure/attention reasons from Core events. Expose `GetRoutingDiagnostics` only when authorized; labels follow organization policy. Do not ship routing classifier, model economics, weights, registry authority or credentials in renderer/plugin business logic. Reconnect by event cursor with no repeated provider dispatch. Validate these flows through EPR-005/010 in doc 61.
