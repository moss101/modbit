# Task Card — IMP-EV-0075 Out-of-process privileged execution

## Identity

- Task ID: IMP-EV-0075
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0075 (native/browser/computer privileged effects execute outside the renderer and the model, behind typed RPC); QUAL-EV-0075 (a renderer-compromise test cannot invoke a privileged effect without the capability); docs/32 "Security settings".
- Qualification: the real Electron app against the real Core, arbitrary code run inside the app's renderer (the compromise), and a second window holding the same preload bridge.
- Evidence tier: real-system.

## Goal

Nothing privileged runs in the renderer: the Core socket and its boot secret, the shell and the filesystem, credential values and input into the browser views live in Electron main and the Core, reachable only through typed requests the app's own top frame may make, each validated and each vouched for by the Core.

## Existing-code audit

- classification: PARTIAL before this task: the renderer was sandboxed without Node (M0/M4), the bridge typed, every argument validated (REQ-EV-0103), the browser views isolated from the bridge (M7.1), the credential broker in main (M7.8); but any web contents in the app holding the preload — a frame, a window an attacker opened — could ask main for anything the bridge names, and nothing proved what a compromised renderer could and could not reach.
- production entry points: `apps/desktop/src/main/main.ts` (`handle`: every IPC handler answers this window's web contents and its top frame only — `SENDER_REFUSED` otherwise, audited on `ipcRefusals`, exposed as `debug:ipcRefusals`; the CSP's `connect-src 'none'`; `debug:coreInfo` answers pid and endpoint only), `apps/desktop/src/preload/preload.ts` (the bridge: named functions only), the Core's own checks (an approval it never issued, a browser session it never opened, credentials as handles).
- proof: `apps/desktop/e2e/workers.spec.ts::renderer compromise (REQ-EV-0075)` — `require`, `process`, `window.electron`, `ipcRenderer` absent; the bridge's members equal the allow-list and are all functions; `debug:coreInfo` has `pid` and `endpoint` only, nothing named secret; `fetch` and `WebSocket` to the Core's address refused by the page's CSP; a forged approval id refused by the Core, a forged browser session refused, a credential added from the renderer listed as a handle without its value; a `srcdoc` frame inside the page has no bridge; a second `BrowserWindow` with the same preload holds `window.modbit` yet every request it makes is `SENDER_REFUSED` and audited (`session:create`, reason "not the app window") while the app's window is answered.

## Limitations

- The guard identifies the app's renderer by its web contents and top frame; a compromise inside that renderer can still ask for the person's own decisions (create a task, approve what the Core is asking the person to approve) — those are the person's, and the Core journals them as such.
- The socket's unreachability from the renderer rests on the CSP and the sandbox (no raw sockets in a web page); the boot secret is what the Core requires, held by main only.
- The Core's refusals are its own (`UNKNOWN_APPROVAL`, `NO_SUCH_SESSION`); this task adds no new Core check.

## Verification

- `apps/desktop/e2e/workers.spec.ts` — "renderer compromise (REQ-EV-0075)"

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
