# Task Card — PX-203 Pane pop-out: a pane in a window of its own under the same preload and content security policy

## Identity

- Task ID: PX-203 (REQ-PX-203; owner desktop); qualification QUAL-PX-203 / PX-E2E-203; Decision Record DR-PX-2026-11-11-021 (MWS-10, section 8 "PX-203 adds a second window", section 9; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (the main process gains a second kind of window; the IPC sender boundary, the preload and terminal attachment ownership change)
- Prerequisites: PX-202, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-pop-out`, cut from origin/main ff0b1ac9 (PX-202's dock).

## The rules implemented (quoted from the record)

- MWS-10: "each pane has a 45 px head with its name, its own actions, Minimize, Pop out and Close ... Pop out moves a pane to a window of its own with the same preload and content security policy; closing that window returns the pane to the dock."
- Section 9: "A popped-out pane runs under the same preload and content security policy." "every effect still goes through the Capability Kernel and its approvals".
- Card PX-203: "Pop out moves a pane to its own window; closing that window returns the pane to the dock; quitting closes every pane window; a pane is in one place at a time; no approval can be decided from a pane window."
- QUAL-PX-203: Terminal and Changes each pop out and keep working; closing returns each to the dock with its state; the hardening guard passes for the new window (no node integration, the same CSP, navigation and new windows refused); the quit prompt counts work in pane windows. Fails if a window is created with other web preferences, a pane is shown in both places, or an approval is decided from a pane window.

## Existing-code audit (before any edit)

- classification: NOT-FOUND for pop-out itself (PX-202 drew no control on purpose); the window boundary it needs was PRODUCTION-WORKING for exactly one window. Traced: `createWindow` in `apps/desktop/src/main/main.ts` (one `BrowserWindow`, `webPreferences` contextIsolation/sandbox/no Node/webSecurity, preload `preload.cjs`, `will-navigate` prevented, `setWindowOpenHandler` deny, renderer-gone replacement and liveness watch), the CSP set once on `session.defaultSession` by `onHeadersReceived`, `handle()` refusing any sender that is not the app window's top frame (REQ-EV-0075, audited in `ipcRefusals`), `send()` pushing every event to that one window, `TerminalHost` keeping "which attachments this window opened" with no notion of which window, `did-start-loading` detaching every attachment, and `activeWork`/`quitPrompt` counting running tasks and terminals. The static guard is `src/main/hardening-guard.test.ts` (pins on the preload bytes, the CSP, the webPreferences block and the constructor's keys).
- first missing link: `handle()` accepts the app window only and `send()`/`TerminalHost` know one window, so a second window could not ask anything, and admitting it would have admitted it to every channel, approvals included. There was also no way to say which pane a window shows.
- Found on the way:
  - The existing key pin in `hardening-guard.test.ts` reads only keys at four spaces of indentation: a spread (`...(cond ? { ... } : {})`) in the constructor could carry any option past it. The new factory guard pins the spreads by text.
  - `TerminalHost.detachAll()` ran on any `did-start-loading` and would have ended a pane window's terminal attachment when the app window reloads. Attachments now carry their owner window. The first whole-suite run then showed the other side of it: a crashed app window's attachments had been ended only by its replacement's `did-start-loading` (which detached everything), so with ownership the dead window kept the terminal's input lease and `terminal-renderer-restart.spec.ts:18` failed (LEASE_HELD). A window's attachments now end when it closes and, for a crashed app window, before its replacement is made.
  - The Changes pane reset its chosen file to the first one on every mount; it now keeps the file per task in memory (the same view state is what travels with a pane).

## What was built

- **One window factory (`src/main/main.ts` `createAppWindow(role, bounds)`).** The single `new BrowserWindow` in main, with the single, unchanged `webPreferences` block (its pin did not move), the same preload and the same page (`index.html`, the pane window at `#pane`). `will-navigate` and the window-open handler are registered for every window before the roles are told apart. The app window keeps everything it had (close asks the quit question, recovery resend, renderer replacement and the liveness watch). A pane window: its own title ("Terminal: <task>"), a 320 x 240 minimum, opened at least 720 x 560 or the pane's size in the dock; closing it hands its last view state to the app window (`pane:returned`) and detaches its own terminal attachments; a renderer that dies closes it (the pane comes back).
- **Pane-scoped IPC (`src/main/pane-windows.ts`, `handle()` in main.ts).** A pane window may ask only an allow list of channels (`PANE_CHANNELS`): its own `pane:*`, `core:status`, the Terminal's `terminal:*`, the Changes pane's `review:bundle`, `review:codeView`, `changes:*`, `checkpoints:restore` (the undo of a revert), Files' `files:*`, Evidence's `evidence:receipts`, `context:inspector`, `review:bundle`; and each only for the pane kind that needs it and only naming the window's own task and session (checked at the argument's position). Everything else, including `approval:resolve`, `approval:allowAlways`, `modeproposals:decide`, `question:respond`, `task:*`, `review:decide`, `review:pullRequest`, `runmode:set`, `rules:*`, `events:subscribe` and every `pane:` channel of the app window, is refused with `SENDER_REFUSED` and recorded in `ipcRefusals` as `pane window: <why>`. A channel added to main later is closed to pane windows until it is listed.
- **Events and terminal frames per window.** `core:event` goes to a pane window only for its own task; `core:status` to every window; `terminal:frame` only to the window that opened the attachment. `TerminalHost` attachments carry an `owner` (web contents id): another window's handle is refused as unknown, `detachAll(owner)` ends only that window's attachments, a lost connection tells each window about its own.
- **Bridge (`src/preload/preload.ts`, pin updated with its review note).** App window: `popOutPane`, `paneWindows`, `focusPaneWindow`, `onPaneWindows`, `onPaneReturned`, `onPaneShow`. Pane window: `paneContext` (main says which pane of which task; the address cannot choose), `reportPaneState`, `paneShow` (the task's review, a call in its conversation, or the waiting approval: shown in the app window, decided nowhere else), `returnPane`.
- **The dock (`shell/apps-panel.tsx`, `shell/pane.tsx`, `shell/dock.ts`, `shell/shell.tsx`).** Pop out (Lucide `maximize-2`) in the head of Terminal, Changes, Files and Evidence, before Minimize. A popped-out pane stays in the dock's stored set (so it is back in its place after the window closes or after a quit) but is not tiled; its title-bar toggle and Panes-menu entry say "in its own window" (`data-state="popped"`, a dot under the toggle) and bring that window forward. The list of pane windows is main's, read on start and on every change, so a reloaded or replaced app window still leaves them out. A returned pane is opened in the dock with the terminal it showed and the Changes file and scroll.
- **The pane window (`renderer/pane-window.tsx`, `PaneWindowFrame` in `shell/pane.tsx`).** The same 45 px head (the pane's name, the task's title, the pane's own actions, Back to the dock), the same app component over the same reads: Terminal (the task's terminals, read on open, after the task's events and on a poll while one runs), Changes, Files, Evidence. Theme, density and zoom follow the person's settings (and their changes, through storage events).
- **Changes in a pane window (`apps/changes-app.tsx`).** Commit and push asks the Core as before; the card shows the exact intent but no Allow or Deny: "Answer it on the approval card in the Modbit window", with Show the approval (brings the app window and the task's docked card forward). When the approval's event arrives (allowed, denied, expired) the commit goes on in the window, or ends, as it would after an answer on the card. The file chosen and the list's scroll are view state that travels.
- **The quit question (`src/main/lifecycle.ts`, `activeWork`).** It counts the running tasks a pane window shows and the running terminals a pane window is attached to ("1 task is running (shown in a pane window) and 1 background terminal is running (shown in a pane window)"), and says the pane windows close with Modbit and their panes are in the dock on the next start. Pane windows that show nothing running are not work: nothing is asked for them alone.

No protocol, Core, CSP or Rust change. No locked architecture file changed.

## Left out, and why

- **Pop out on Browser, Activity, Background tasks and Pull request.** Browser's view is a `WebContentsView` that `BrowserHost` attaches to the app window (moving it is the browser host's change, PX-073's surface). Activity lists "what this window was given" (PX-202), so a new window would start empty and mislead. Background tasks stops a child through the shell's confirmation and `task:cancel`, which a pane window may not ask. Pull request waits on approvals and asks `review:pullRequest`, which a pane window may not ask. None of the four draws Pop out ("Nothing is drawn that does nothing").
- **A screenshot of the quit question.** It is the main process's native dialog; this machine refuses screen capture to this session (`screencapture` reports "could not create image from rect", inside and outside the sandbox). `after/quit-question.txt` is the text main asked the dialog to show in the run, recorded by the spec; the spec asserts every word of it.
- **Remembering pane windows across a quit.** The record says closing returns the pane to the dock; on the next start the pane is in the dock, not in a window.
- **An icon made for pop-out.** The generated Lucide set has no `external-link`; adding one means regenerating `icons.generated.ts` from lucide-react 1.17.0, which is not on this machine. `maximize-2` (already in the set) stands in.

## Verification

Unit: `@modbit/desktop` 416 of 416; `pnpm typecheck` clean. New: `pane-windows.test.ts` (3: every deciding channel refused to every pane kind, the allow list is channels main answers and holds none that decides, a channel not listed is closed; own task and session only, pane kind only; view state and show requests cleaned), `lifecycle.test.ts` (+1: the quit question counts pane-window work), `terminal-host.test.ts` (+1: attachments are their window's), `hardening-guard.test.ts` (+2: the one-factory guard, and its retained mutations).

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a…), scripted model: `e2e/px-203-pop-out.spec.ts`, 1 of 1, about 35 s, run several times locally on the final build. What it proves, in order:

- **Terminal pops out and keeps working**: a second window appears, the dock no longer has the pane (`data-panes` loses it), its toggle says "Terminal pane, in its own window"; the window shows the same terminal id, the line typed in the dock, a count past where it was, the Core's process not restarted; a line typed in the window reaches the process (`got:in-its-window`).
- **Changes pops out and keeps working**: the same file chosen and the list scrolled within 40 px of where it was; choosing the other file reads its diff in the window.
- **One place at a time**: every pane window's pane is absent from the dock of the same task, and no pane has two windows.
- **The hardening guard (runtime, `e2e/support/window-guard.ts`)**, for the app window and each pane window: Electron's applied web preferences equal the app window's (contextIsolation true, nodeIntegration false, sandbox true, webSecurity true, no Node in subframes or workers, no webview tag); the default session; the Content-Security-Policy in force, read from the browser's own violation report of an injected inline script, equal to the app window's and the pinned one; no `require`, `process`, `module`, `Buffer`, `global` in the page; the same bridge (the same preload); the window's `will-navigate` handlers refuse a navigation; `window.open` returns null and no window is added.
- **No approval from a pane window**: with a real `shell.exec` approval pending, both pane windows draw no approval card, and from each `resolveApproval`, `allowAlways`, `respondToQuestion`, `decideModeProposal`, `cancelTask` and `changesState` of another task are all refused by main (`SENDER_REFUSED`); main recorded each refusal; the Core still lists the approval as pending; the card is still in the app window.
- **The quit question** (asked while the task runs): exactly "1 task is running (shown in a pane window) and 1 background terminal is running (shown in a pane window). Quit anyway?", the pane-window line in the detail, the three buttons; the default answer keeps both pane windows.
- **Axe** on each pane window, light and dark: no violation.
- **The app window reloads**: both toggles still say popped, the dock still leaves the panes out, and the Terminal window goes on receiving output (its attachment is its own).
- **The approval is answered on the app window's card**: the remote is really added; the task is ReadyForReview.
- **Closing returns each pane with its state**: Terminal by Back to the dock (same terminal, same running session, typing reaches it from the dock), Changes by closing the window as the OS does (list.txt chosen, scroll within 40 px).
- **Commit and push from a pane window**: Changes popped out again shows the file the dock showed; Stage all, message, Commit and push: the card in the window has no Allow or Deny, `resolveApproval` from the window is refused, nothing is pushed; Show the approval opens the task in the app window with its card; Allow there, and the window shows the commit COMPLETED and the bare remote has the branch. Back in the dock, Changes reads the committed state.
- **Failure: a pane window's renderer dies** (Evidence popped out, `forcefullyCrashRenderer`): the window closes, Evidence is back in the dock, the Core is still connected.
- **Quitting closes every pane window**: with a Terminal window open, Quit and stop: the process exits 0.

Whole desktop suite, local, macOS arm64, two runs; see `desktop-e2e-full-results.txt`. Run 1: 210 passed, 2 failed, 3 skipped; both failures were caused by this change (`workers.spec.ts:145`, the bridge's pinned names; `terminal-renderer-restart.spec.ts:18`, a crashed window's input lease) and are fixed. Run 2, on the final build: 212 passed, 0 failed, 3 skipped (the two live-model specs and the Windows-only one).

## Retained mutations (each asserted to fail)

- `hardening-guard.test.ts` `windowFactoryDrift`: a second constructor for pane windows with Node in the page; an option for pane windows carried by a spread (`sandbox: false`); navigation refused for the app window only; a pane window that loads another page; a window with its own partition.
- `window-guard.ts` in `px-203-pop-out.spec.ts`: a window built with `nodeIntegration: true, contextIsolation: false, sandbox: false` (fails on web preferences and Node in the page); a window in a partition where the policy header is not set (an inline script runs; not the default session); a window with the same preferences and neither refusal (navigation or new window not refused). Their bridges were refused by main ("not the app window").
- Both places: a Terminal pane drawn in the dock while its window is open.
- Approval: after the approval is decided (on the app window's card) the same check reports it is no longer pending.
- `pane-windows.test.ts`: an allow list holding an approval channel is caught by the first test's filter.

## Test ids

- New: `pane-popout-<kind>`, `pane-window`, `pane-window-task`, `pane-window-error`, `pane-window-loading`, `pane-window-notice`, `pane-return`, `changes-scroll`, `changes-approval-elsewhere`, `changes-show-approval`; `data-state="popped"` on `pane-toggle-<kind>`.
- In a pane window the existing ids of the head (`pane-head-<kind>`, `pane-name-<kind>`, `pane-body-<kind>`) and of the apps are reused.
- None removed.

## Assertions deleted, replaced or changed

- None deleted.
- `hardening-guard.test.ts`: the preload pin moved to the new bridge (76e83c38…), with the review note beside it. The CSP, webPreferences and key pins are unchanged and pass.
- `e2e/workers.spec.ts` (REQ-EV-0075): the pinned list of bridge names gains the ten pane-window methods, with a note beside them; the check (the bridge is exactly the list) is unchanged.
- `terminal-host.test.ts`: every call gains the owner window (`owner: 1` on attach, `, 1` on ack, detach, write, input, resize, detachAll), because ownership is now part of the call. The fake now numbers its attachments, so in "a window that reloads detaches everything it opened" the second attachment is `att2` and the assertion reads `detach:att2` (it read `detach:att1`, the same attachment under the fake's old fixed id). Same fact.

## Differences from the boards and the record

- The boards A4 and C1 draw the dock and do not draw a pane window; the window's head is the dock's head with the task's title and Back to the dock in place of Minimize and Close.
- Pop out uses `maximize-2`; a popped pane's toggle has a dot (a minimized one has a bar).
- MWS-10 says each pane has Pop out; four of eight do (see Left out).
- The record says closing returns the pane; Back to the dock does the same from inside the window.
- In the dock at narrow widths the Changes head's own actions are cut at the left ("ew" of Review in `returned-*.png`); that is PX-202's head at that width, not changed here.

## Limitations

- The proof is local (macOS arm64); CI is recorded in `evidence.json`.
- Navigation refusal is proven through the window's own `will-navigate` handlers (a navigation the page starts and the window refuses leaves Playwright waiting forever on that frame); the static guard proves the handler is registered for every window, and the mutation proves a window without it fails.
- Files popped out is not driven end to end (its channels are unit-proven; Evidence is, through the crash test).
- A commit denied or expired on the app window's card while the Changes window waits is not driven end to end (allowed is); the outcome is read by `approvalOutcome`, unit-proven for the Pull request pane.
- The pane window shows the task's title as main recorded it at pop-out; a rename after that is not followed.
- The owner has not seen the screenshots. No graph status was set.
