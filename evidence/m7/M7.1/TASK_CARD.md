# Task Card — M7.1 Local sandboxed WebContents session + CDP bridge

## Identity

- Task ID: M7.1
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Local browser" (a sandboxed `WebContentsView` in a dedicated partition with Node disabled and strict context isolation; a Browser Bridge controlling that same `webContents` through CDP; the renderer only hosts the view; page content never gets a privileged API), docs/22 "Prompt-injection isolation" (page text is untrusted evidence), REQ-EV-0110 (browser behind an authenticated peer boundary), REQ-EV-0276 (Chromium as the engine, not a new browser), docs/44 row "Live same-session browser/takeover" (browser/main).
- Qualification: docs/51 E2E-013's session half — a real page in the live embedded Chromium, the browser view visibly the same session the agent drives; the semantic-ID action and postcondition halves belong to M7.2–M7.4.
- Evidence tier: real-system (the real Core with a fake host over the real protocol; the real Electron app hosting a real Chromium view against a real HTTP page)

## Goal

One live Chromium session per task that the person sees and the agent drives — hosted by Electron main in isolation, owned by the Core (identity, control lease, record), reached by the agent only through typed requests over the authenticated socket, its content never trusted.

## Existing-code audit

- classification: MISSING before this task: `crates/browser` was the M0 stub; no browser session, host, bridge, tool, event or protocol message existed; the tool matrix deferred the `web` family.
- production entry points: `crates/browser/src/lib.rs` (`BrowserSessionId`, `ControlLease` / `Controller` with fencing generations, `HostRequest` / `HostResponse` / `Clip` / `AxNode`, `PageState::fingerprint`, `PortError`, `BrowserPort`, `navigable`); `crates/protocol/proto/modbit/v1/surface.proto` (`OpenBrowserSession` / `BrowserSessionOpened`, `AttachBrowserHost` / `BrowserHostAttached`, the `BrowserHostRequest` frame, `BrowserHostResponse` / `BrowserHostResponded`, `GetBrowserSession` / `BrowserSessionView`, `CloseBrowserSession` / `BrowserSessionClosed`); `crates/protocol/src/client.rs` (`next_browser_request`, requests buffered across acks and events); `crates/domain/src/task.rs` (`BrowserSessionOpened`, `BrowserHostAttached`, `BrowserNavigated`, `BrowserSessionClosed`); `services/modbit-core/src/browser.rs` (`BrowserSessions`: open / attach / deliver / close, `connection_closed` failing owed requests, `from_events` rebuild, the `BrowserPort` with per-request deadlines and the lease check); `services/modbit-core/src/server.rs` (the `browser.host` client capability for the desktop kind; the host request channel selected in the connection loop; `attach_browser_host` refusing `HOST_NOT_ISOLATED` / `PARTITION_MISMATCH`; the four commands; `browser_session_record`); `services/modbit-core/src/tools.rs` (`browser.*` registered; `BrowserNavigated` journaled beside a successful navigation); `services/modbit-core/src/runtime.rs` (a navigation counts as progress); `crates/tools/src/browser.rs` (`browser.navigate`, `browser.snapshot`; `browser.control`; `NAVIGATION_BLOCKED`; `UNTRUSTED_WEB_CONTENT`); `crates/policy/src/kernel.rs` (`browser.control` in the trusted and autonomous default leases); `crates/tools/tool-matrix.json` (two rows, the `browser` namespace); `packages/ide-adapter-core/src/client.ts` (`onBrowserRequest`, `openBrowserSession`, `attachBrowserHost`, `respondBrowserHost`, `browserSession`, `closeBrowserSession`); `apps/desktop/src/main/browser.ts` (`BrowserHost`: the hardened view, attach and re-attach after a Core restart, the CDP handlers, show/hide/close, the delivery log, the isolation probe); `apps/desktop/src/main/main.ts` (`browser:*` IPC); `apps/desktop/src/renderer/index.tsx` (the Browser panel), `screens.ts` (`browserState`).
- proof: `qual_m7_1_a_browser_session_is_hosted_by_the_desktop_and_driven_through_the_cdp_bridge` (real Core, a fake host on a desktop-kind connection): opening journals the session with its partition and an agent lease at generation 1, and opening again answers the same session; a CLI-kind client is refused `CLIENT_CAPABILITY`; a view without the sandbox or with Node is refused `HOST_NOT_ISOLATED`, another partition `PARTITION_MISMATCH`; the agent's `file:///etc/passwd` is `NAVIGATION_BLOCKED` and never reaches the host; the http(s) navigation and the snapshot reach the host with the lease generation and come back to the model tagged `UNTRUSTED_WEB_CONTENT` with the page's instruction-shaped title as data; the host disconnecting fails the next navigation `NO_BROWSER_HOST` / `BROWSER_HOST_GONE` (an infrastructure failure, no page invented); the task's log holds one `BrowserSessionOpened`, one `BrowserHostAttached` with the isolation stated, one `BrowserNavigated` (URL, version 1, generation 1, a 64-hex fingerprint) — none for the refused URL or the lost host; `GetBrowserSession` shows the record; after a Core restart the session is rebuilt from the events (URL, version, partition) and a new host attaches to the same partition. `apps/desktop/e2e/browser.spec.ts` (the real app, a real HTTP page): the card opens the session, the panel shows the host's state, main's view is in the session's partition, shown over the placeholder and attached, the isolation probe finds nothing privileged from inside the page; with the panel closed the agent navigates the same view (the fixture is fetched exactly once, the `file:` URL never reaches the host, the host's log reads navigate ok / snapshot ok), the model sees the refusal, the page state and the tree with the form's names and the title tagged untrusted, the Core's record names the URL, version and fingerprint with the host attached; reopening the panel shows the same page at the same URL and title as main's view, and the probe still finds nothing privileged.

## Limitations

- Semantic entities and stable references (M7.2), state deltas (M7.3), semantic actions and postconditions (M7.4), the targeted visual fallback (M7.5) and the takeover surface (M7.6; the control lease and its fencing exist and refuse agent input under user control) follow; `browser.snapshot` returns the raw bounded accessibility tree until M7.2 compiles entities.
- One host per session on one connection; a second attach replaces the first.
- The cloud browser (remote CDP, streamed view) is M8 substrate.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_1_a_browser_session_is_hosted_by_the_desktop_and_driven_through_the_cdp_bridge` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` — "browser session: a sandboxed WebContentsView in the app, attached to the Core, driven by the agent through the CDP bridge; the page is untrusted, file: is refused, nothing privileged is reachable"
- `crates/browser` unit tests (`control_lease_fences_stale_agent_input`, `only_http_and_https_are_navigable`, `fingerprint_changes_with_url_title_or_version`, `requests_and_responses_round_trip_as_tagged_json`)
- Regression: `qual_ev_0217_…` / `qual_ev_0230_…` (the matrix rows and namespace), `qual_m5_1_…` (projection unchanged for ReadOnly tools), `m6_3_…` (its script reordered so the overlapping spawn meets a live scope — a hosted Windows runner had let child-a finish first), the whole surface-protocol suite

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35143904650.json` (main at cc5e95e; the change commit 8b825dc and the keyboard E2E forward-fix cc5e95e)
