# Task Card — M1.4 Electron shell + real Fleet/New Task UI against Core

## Identity

- Task ID: M1.4
- Milestone: M1
- Canonical owner: Surface team (desktop owner); `apps/desktop` only. Core access exclusively through the M1.3 SurfaceProtocol.
- Requirement IDs: docs/32 (hardened main/preload/renderer split, event consumption, task composer, attention UX, error states, frontend completion gate), docs/10 screens 1 (Home / Fleet) and 2 (New Task), docs/39 PX-023 screen states (Core restarting degraded state and recovery), docs/30 (idempotent commands by `command_id`), docs/29 (thin client). Capabilities registered against REQ-EV-0103, REQ-EV-0108, REQ-EV-0192, REQ-EV-0010 via main's validated IPC and the Node client.
- Risk class: high (security boundary: renderer isolation)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

An Electron app whose main process spawns and supervises the real `modbit-core`, holds the boot secret, and exposes a narrow validated IPC to a sandboxed React renderer; a Fleet screen with the six PRD columns reduced from Core snapshot + events; a New Task composer that creates a durable Session + Task before anything else and renders the card from the returned Core id; degraded and recovery states when the Core dies; state that survives an app restart.

## Non-goals

- Task workspace, Review, Terminal, Browser, Settings screens (their milestones); OS notifications; keyboard model beyond focusable cards (PX-024); packaging/updater (M10).

## Required reading

`AGENTS.md`, `docs/10` §Core screens 1–2, `docs/29`, `docs/30`, `docs/32`, `docs/39` §Screen flows, §Notification model.

## Existing-code audit

- classification: NOT-FOUND (apps/desktop was a shell)
- production entry point: `dist/main/main.cjs` (Electron main) → `CoreSupervisor` → `CoreClient` (Node) → real socket; renderer `dist/renderer/index.js` via `window.modbit` (preload bridge)
- real effector/storage boundary: the real `modbit-core` process and its `core.db`
- tests found: none
- first missing/broken link: no Electron shell
- duplicate/drift risks: renderer keeping its own truth; prevented by rendering only from Core snapshot/events and re-reading after recovery

## Invariants

- `nodeIntegration=false`, `contextIsolation=true`, `sandbox=true`, restrictive CSP (`connect-src 'none'`), navigation and window.open denied; preload exposes only the bridge functions; every IPC handler validates its arguments and rejects malformed ones (`BAD_ARGUMENT`) without forwarding.
- The boot secret never reaches the renderer (the debug hook exposes pid/endpoint only).
- A "Completed" card renders only from a Core `TaskCompleted` event; Needs Attention derives from structured reasons.
- New Task uses a renderer-minted stable `command_id`, so a retry replays instead of duplicating.
- On Core exit main respawns with backoff; the renderer shows the degraded banner with the last persisted state, then the recovery banner and re-reads the projection.
- Quitting stops the Core child.

## Implementation slice

- domain/API: wire `EventEnvelope` gained `aggregate_type`, `aggregate_id`, `task_id` (additive fields 11–13; fixtures and TS bindings regenerated) so clients reduce events per aggregate
- service/effector: `src/main/protocol-client.ts` (framing + client), `core-supervisor.ts`, `main.ts` (IPC, CSP, window), `preload/preload.ts`
- UI/model projection: `renderer/model.ts` (ordered idempotent reducer, six columns), `renderer/index.tsx` (app shell, Fleet, New Task, banners, a11y live region), `index.html`
- failure/recovery: supervisor restart with backoff and status events; `CORE_UNAVAILABLE` rejections while restarting; error banner with retry
- build: `build.mjs` (esbuild; main/preload as `.cjs`, renderer as classic script)

## Verification

- unit: renderer model tests (Completed only via event, idempotent offsets, attention derivation, snapshot normalization)
- E2E (Playwright driving the real Electron app against the real Core): empty state → New Task → card with 32-hex Core id in Waiting, none in Running/Completed → Core SIGKILLed → restarting banner → reconnected → recovery banner → same task id → second task after recovery → app closed and relaunched on the same profile → both tasks back from the Core projection; invalid renderer arguments rejected by main
- E2E on hosted CI: macOS, Linux (xvfb), Windows

## Completion evidence

See `evidence.json`.
