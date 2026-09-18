# Task Card — IMP-EV-0081 Dedicated browser/viewer/computer workers

## Identity

- Task ID: IMP-EV-0081
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0081 (specialized surfaces use the same canonical run/evidence contracts); QUAL-EV-0081 (a browser worker crash does not corrupt the Core and a restart reattaches the session); docs/22 "Local browser", docs/32 "Security settings".
- Qualification: the real Electron app against the real Core and a real page; the browser view's process killed between two of the agent's observations.
- Evidence tier: real-system.

## Goal

The browser view is a worker on the canonical contracts — its session the Core's, its requests and answers the browser protocol's, its evidence the task's log — and its process dying is the host's to mend: the view is restarted at its page and attached again to the same session, the Core untouched, the agent's next observation served.

## Existing-code audit

- classification: PARTIAL before this task: the view was a dedicated sandboxed worker on the canonical contracts (M7.1: `AttachBrowserHost`, `BrowserHostRequest`/`Response`, the session journaled on the task; the cloud browser on the same contracts, M8.8), and a Core restart re-attached every live view (`reattachAll`); but the view's own process dying only notified the renderer (`gone`) — the session kept a dead view, every request after it failed, and nothing proved the Core survived it intact.
- production entry points: `apps/desktop/src/main/browser.ts` (`restartView` on `render-process-gone`: `view-gone` logged and notified; the CDP session detached; the view loaded again at its http(s) page or `about:blank`; `attach` again to the same session — `BrowserHostAttached` journaled by the Core; `view-restarted` logged; `browser:state` with `restarted`; `VIEW_RESTARTING` answered to a request arriving meanwhile; `HostedSession.restarting`), the Core's `AttachBrowserHost` (a repeated attach from the same connection replaces the link — unchanged).
- proof: `apps/desktop/e2e/workers.spec.ts::browser view killed (REQ-EV-0081)` — the agent navigates to a real page and snapshots it (`Greeter` named); the view's process is killed from main by its web contents id; the host's log shows `view-gone` (not ok) then `view-restarted` (ok); the view describes as attached with the same web contents id, a new OS process, the same URL; the Core reports the session open with the `electron-main` host attached; the model's held fourth request is released and the agent's second snapshot is a delta against the first: the same page unchanged (`entity_count` 3, `unchanged`), its state version moved on; the task reaches review on the same Core pid; the page was fetched exactly twice (the agent's navigation and the restart).

## Limitations

- A request in flight at the instant of the crash fails as a CDP error (the model retries); the restart serves the next one.
- The restarted page is a fresh load of the same URL: form state and in-page JavaScript state of the dead process are gone (the person's takeover, if any, keeps its lease).
- The cloud browser's process (in the sandbox guest) is the guest's to restart; its loss is the sandbox-loss path (M8.9), not this one.

## Verification

- `apps/desktop/e2e/workers.spec.ts` — "browser view killed (REQ-EV-0081)"

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
