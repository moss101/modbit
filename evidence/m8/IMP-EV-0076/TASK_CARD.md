# Task Card — IMP-EV-0076 RPC/session-state separation

## Identity

- Task ID: IMP-EV-0076
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0076 (UI/worker messages are typed; the Core remains the canonical session owner); QUAL-EV-0076 (kill the renderer and reconnect; the session's truth is unchanged); docs/32 "Security settings", "Event consumption".
- Qualification: the real Electron app against the real Core; the renderer's process killed after a task reached review; the renderer brought back.
- Evidence tier: real-system.

## Goal

The renderer is a view of the Core's session, not its owner: its process dying loses nothing — main keeps the Core connection, the session lease and the browser views, and the renderer comes back from the Core's snapshot and cursor with the session exactly as it was.

## Existing-code audit

- classification: PARTIAL before this task: the messages were typed (the SurfaceProtocol over the bridge, M0/M4; the cloud's typed API, M8.1) and the renderer rebuilt from the snapshot and cursor at every start (docs/32 "Event consumption", PX-023), but a dead renderer process stayed dead — a blank window until the person restarted the app — and nothing proved the session's truth survived it unchanged.
- production entry points: `apps/desktop/src/main/main.ts` (`createWindow`: `render-process-gone` → `rendererLog` (reason, exit code, brought back or not) → the browser views leave the old window (`BrowserHost.windowReplaced`) → a fresh window at the old bounds → the old one destroyed; a clean exit is not brought back; `debug:rendererLog`), `apps/desktop/src/renderer/index.tsx` (`load`: snapshot from the Core, subscribe from its offset — unchanged), `apps/desktop/src/main/browser.ts` (`windowReplaced`).
- proof: `apps/desktop/e2e/workers.spec.ts::renderer killed (REQ-EV-0076)` — a task run to review; the session snapshot and the Core's pid taken; the renderer's process killed from main (`forcefullyCrashRenderer`); Playwright sees the crash and a new window; one window remains; `debug:rendererLog` has one entry (crashed/killed, brought back); the same Core pid; the session snapshot's `lastOffset`, `generation` and tasks equal before and after; the card is in the same column with the same task id; `taskStatus` and `reviewBundle` answered for it; the model was asked nothing by the restart (requests `[0, 1, 2]` only).

## Limitations

- A renderer that exits cleanly (`clean-exit`) is not brought back; that is the app closing.
- The new window takes the old bounds; the renderer's transient UI state (a typed goal not yet run, an open panel) is the renderer's and does not survive — by design, nothing of the session does live there.
- A crash storm is not rate-limited (each crash brings the window back once).

## Verification

- `apps/desktop/e2e/workers.spec.ts` — "renderer killed (REQ-EV-0076)"

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
