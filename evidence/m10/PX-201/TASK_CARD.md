# Task Card — PX-201 Details rail: environment, approvals, context, tool servers, changes, evidence, background tasks and cost beside the conversation

## Identity

- Task ID: PX-201 (REQ-PX-201; related REQ-EV-0037, REQ-EV-0143; owner desktop); qualification QUAL-PX-201 / PX-E2E-201; Decision Record DR-PX-2026-11-11-021 (MWS-09, section 5.1; AFW-A12, A13, A14 unchanged); DR-PX-2026-11-04-020 sections 5.3, 5.6, 5.8 and 5.9 stay in force
- Evidence tier: release-critical (cancel, restore, stop and review are reached from the rail and the popover that held them is removed)
- Prerequisites: PX-200, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-details-rail`.

## Existing-code audit (before any edit)

- classification: IMPLEMENTED-PARTIAL. Every fact the rail needs already crossed the bridge; what was missing is the surface. Traced from the production caller to the Core: `Shell` mounts `TopBar` and `Conversation`; the conversation portalled the state pill and the branch into the top bar and More (`task-details-toggle`) opened `TaskDetails`, a popover that read `contextInspector` and `taskEconomics` and drew the card's facts with the Core's folded facts over them (PX-196, `withCoreFacts`); the composer's foot said where the task runs; `RunModeControl` read `runMode` and `allowRules`; `ContextRingView` owned the breakdown tray; `useCheckpointSurface` read `checkpoints` and owned the restore dialog; the composer's tray killed terminals with `terminalKill`; Cancel task set the shell's confirmation, which calls `cancelTask`.
- first missing link: no surface beside the conversation and no place in the frame's layout for one (`computeLayout` knew the list, the centre and the panel).
- Found on the way, and fixed because the rail depends on them:
  - The shell kept the headers the task list last read after the list left the screen (list collapsed to its rail), so "where the task runs" could be stale and, with no header, the composer's foot fell back to this machine's name without a read. The list now clears what it handed up when it unmounts, and the rail then reads the header itself; with no header the location row is absent.
  - The branch reached the top bar only once the Changes app had been opened (`changesState` was read only there). PX-048 (#170) made that read free of side effects, so the rail reads it itself once the task has started.
- The Core has no per-task tool-server list and no liveness probe. What it reports is the session's extensions with the servers each declares, `CONFIGURED` or `INERT` (`customizeHooks`, the read Customize uses). The Core's budget is in minor currency units with no currency named; the economics' cost is in dollars at catalog list prices. They are not one number.

## What was built

- **Layout (section 5.1).** `computeLayout` takes `detailsOpen` and answers `detailsPx` (320 where the task area is wider than 800 and no pane is open, else 0) and `detailsFits`; the numbers are `DETAILS_RAIL` in `shell/layout.ts`. `ShellFrame` draws `aside.details-rail` (320 wide, minimum 280, the editor surface, a 1 px edge at 8 %) beside the content and under the top bar, inside the task area, and hands its element to the shell; the open conversation fills it through a portal, as it fills the top bar's slot. The choice is `detailsOpen` in the layout preferences (`modbit.ui.v1`); a profile from before reads as shown.
- **Title bar.** A Details toggle first among the toggles (`details-toggle`, pressed while the rail is shown). It is absent where the task area is 800 or narrower. While a pane is open the rail gives way and the toggle stands unpressed; asking for the rail closes the pane. The state pill and the branch left the bar.
- **The rail (`conversation/details-rail.tsx`, words in `rail-model.ts`).** A head with the state pill and Details; then, each with a hairline above it:
  - Environment: where the task runs (the header's `executionLocation` in the person's words), the folder (the checkout a worktree came from, else the workspace root), the branch (`changesState`) with "own worktree" where the header names a checkout, and the run mode, which opens the run-mode panel.
  - Approvals: how many wait for this task and the kind of the first (`pendingApprovals`), and the saved rules in force (`runMode.rulesInForce`), which opens the same panel, where the rules are managed.
  - Context: the percentage of the window used with a bar (`contextAccounting`); it toggles the breakdown tray the ring toggles, and both say whether it is open.
  - Tool servers: how many the session's extensions declare, as configured or inert (`customizeHooks`); it opens Customize. **Left out when the Core lists none** (nothing is drawn that does nothing).
  - Changes: the header's added and removed lines and file count, and Review, which opens the Review.
  - Evidence: the checks that ran and passed and whether the Core counts the task verified (`taskEconomics`), the latest evidence line (the Core's folded facts), and the checkpoints: the committed turn boundaries, each with Restore, which opens the existing restore dialog (preview, then the question). Restore is off with its reason while the agent works, as on a turn.
  - Background tasks: each running child task (its goal opens it, its Core state, Stop) and each running terminal (its title opens it in the Terminal pane, how long it has run, Stop), and "n finished", which opens the list of ended children and terminals. Stop on a child asks with the confirmation Cancel task asks and then calls `cancelTask`; Stop on a terminal asks once more in place and then calls `terminalKill`.
  - Cost: the spend at catalog list prices or "not priced", the model calls and the model (`taskEconomics`), and each cap that is set with what is used of it in the Core's units (`contextAccounting.budgets`).
- **Details in the rail's head** opens state, phase, agents, generation, attachments, risk, security and next action. Closed, they are clipped from sight and stay in the accessibility tree. Open, the context inspector and the economics in full (which the popover held) stand under them and are read then.
- **More (`task-details-toggle`)** opens a card with what the popover held and the rail does not show: the conversation's density, Undo all, Browser and Cancel task. Where the rail is not beside the conversation (a task area of 800 or narrower, a pane open, or hidden by the person) the same card holds the rail's body under them (the controls stand first, so none moves as the body fills). Escape closes it and nothing else. It never covers a docked approval card: the card stays on top, as it did over the popover.
- **One read, two places.** `useRunMode` (the composer's control and the rail), `ContextTrayHost` with `useContextTray` (the ring and the rail), `useCheckpointSurface().list`.
- **Removed in the same change:** `conversation/task-details.tsx`; the top bar's slot with the state pill and the branch (`.topbar-slot`, `.topbar-pill`, `.topbar-branch`); the composer's `foot` prop and "This Mac" under the box; the shell's `branchByTask` and the `onBranch` chain through `AppsPanel` and `ChangesApp`; `.task-details` and `.task-subtasks`.
- No preload, protocol, main-process, CSP or Rust change: `dist/preload/preload.cjs` and `dist/main/main.cjs` have the digests PX-200 recorded.

## Left out, and why

- **Tool servers per task and their connection state.** The Core reports neither without a new read; the section shows the session's declared servers as configured or inert and is absent when there are none. In the e2e run the Core lists none, so the section is asserted absent there; its words are unit-tested.
- **"Move to cloud"** beside the location: PX-128.
- **Spend against budget as one line** ("$0.02 of $0.50"): the Core's cap has no currency. The spend is in the section's head and each cap is a row of its own.

## Verification

- Unit: `@modbit/desktop` 383 of 383; `pnpm typecheck` clean. New: `rail-model.test.ts` (the location is never assumed, folders, the change, verification, spend and caps, tool servers never "connected", what can be stopped, the restorable checkpoints); `shell.test.ts` (the rail's layout at 801, 800, the window's minimum, with a pane open, stacked; the preference); `measured-css.test.ts` (the rail's surface and its rows from the tokens, with a mutation).
- Real app, real Core, scripted model (`e2e/px-201-details-rail.spec.ts`). One task edits a file, starts a background terminal, delegates a child task that is held, has five checkpoints and a budget set through the bridge, and then asks for a protected command.
  - **The bridge's log.** The test taps, in the main process, the handlers the preload calls and the events main pushes (the product has no such tap). Every element the rail draws a value in names its read in `data-src`; the check finds each read answered for this task or session, and fails on any text in the rail that is neither a value nor one of the rail's own words. A second check computes each section's words from the logged answers and compares them with the screen. Run while running, while waiting on approval, after a reload and under More.
  - Stop on the terminal: the first click stops nothing, Keep keeps it, the second kills it (the Core's list says so) and it is counted finished. Stop on the child: the confirmation names it, Escape cancels nothing, Enter cancels it (the Core's header says Cancelled).
  - Restore from the rail opens the dialog with the Core's preview naming the changed file; Cancel leaves the file's hash as it was. Review opens the Review. The run mode and the rules open the run-mode panel; Context opens and closes the tray, and the ring agrees.
  - After a reload the phase, the agent counts and the attachments equal the Core's (`transcript().facts`), and they are not the defaults.
  - Hide, reload (still hidden, its body under More), show; a pane open hides it and asking for it closes the pane; 801 shown, 800 absent with no toggle; at 1000 wide More holds every section and control with the controls first, stands inside the window and leaves the approval card's answers on top; at the centre's minimum nothing is over the composer.
  - Axe: the rail in light, dark and high contrast; the shell with a docked approval; Details open; More beside the rail; More holding the details in the three themes.
- The gate (`e2e/awl.spec.ts`, `railDrift`): 320 wide, minimum 280, the editor surface, the 1 px edge, beside the conversation and not over the composer or the title bar, the sections in order with hairlines, the state and Details in its head, in both densities and both themes; a rail row is the density's row height with the chrome's text size; absent at 800 with no toggle; absent with a pane open; no popover.
- Whole desktop suite, local, macOS arm64, real Core, scripted models: two runs. Run 1 on bfa9e852: 206 passed, 1 failed, 2 skipped (the live-provider specs) in 19.3 m; the failure, `composer.spec.ts:204` (PX-054), measured the composer's width at 1280, where the conversation beside the rail is 672 wide: caused by this change, and the spec now runs at 1440 with its assertions unchanged. Run 2 on the final build: 207 passed, 0 failed, 2 skipped in 19.2 m. Results by test in `desktop-e2e-full-results.txt`. `shell.spec.ts:25` starts the app with no override and needs `target/debug/modbit-core`: the worktree has no cargo build, so `target/debug` was linked to the prebuilt binaries for these runs.
- Screenshots of the running app against a real Core at 1440 by 900 (`after/`, written by the spec with `MODBIT_SHOTS_DIR` set; captured at 2x): the running task with a child task, a background terminal, checkpoints and a budget (light, dark, both densities); the task waiting on approval (light, dark, both densities); Details open (light, dark); the window at 1000 wide with the rail gone and More open (light, dark).

## Retained mutations (each asserted to fail)

- `px-201-details-rail.spec.ts`: a value that names no read; a value naming a read the bridge never answered; words on the rail that are no value ("3 connected"); a made-up spend that names a real read; a location written in place that is not the header's; the window's default facts ("drafting", 0/0) against the Core's after a reload.
- `awl.spec.ts`: a rail drawn at a task area of 800; the popover back; a 260 px rail; a rail with no minimum; a rail on the sidebar surface; a rail with no edge; a rail floating over the composer; sections with no hairline.
- `measured-css.test.ts`: a rail row with a height of its own.

## Test ids

- Removed: none. `task-details` is no longer a dialog: it is the group of facts behind Details in the rail's head (`data-open`, `data-phase`, `data-state` as before).
- Moved, same id: `top-bar-state` and `top-bar-branch` (title bar to the rail's head and Environment; the names are kept because the controls survive); `conv-location` (under the composer to Environment); `task-state`, `task-phase`, `task-agents`, `task-attachments`, `task-risk`, `task-security`, `context-inspector`, `context-compaction`, `context-entry`, `context-omitted`, `task-economics` (popover to Details in the rail); `task-evidence` (to Evidence); `task-children`, `child-card`, `child-goal`, `child-state` (to Background tasks; an ended child is listed once "n finished" is opened); `conv-density`, `undo-all-wrap`, `undo-all`, `undo-all-armed`, `undo-all-cancel`, `task-browser`, `task-cancel` (popover to More); `task-details-toggle` (still More; its name is now "More").
- New: `details-rail`, `details-toggle`, `rail-body`, `rail-details-toggle`, `task-more`, `task-next`, `rail-environment`, `rail-folder`, `rail-worktree`, `rail-runmode`, `rail-runmode-current`, `rail-approvals`, `rail-approvals-waiting`, `rail-approvals-waiting-row`, `rail-rules`, `rail-rules-count`, `rail-context`, `rail-context-toggle`, `rail-context-use`, `rail-tool-servers`, `rail-tool-servers-open`, `rail-tool-servers-count`, `rail-changes`, `rail-changes-size`, `rail-review`, `rail-evidence`, `rail-verification`, `rail-checkpoints`, `rail-checkpoints-count`, `rail-checkpoint-list`, `rail-checkpoint`, `rail-restore`, `rail-background`, `child-stop`, `rail-terminal`, `rail-terminal-title`, `rail-terminal-elapsed`, `rail-terminal-stop`, `rail-terminal-stop-confirm`, `rail-terminal-stop-cancel`, `rail-finished`, `rail-finished-count`, `rail-finished-list`, `rail-terminal-ended`, `rail-cost`, `rail-spend`, `rail-model-calls`, `rail-model`, `rail-budget-cost`, `rail-budget-time`.

## Assertions deleted or moved

- Deleted, surface removed on purpose (two):
  - `awl.spec.ts` `workspaceDrift`: "`top-bar-state` is in the top bar". In its place: with More closed the bar holds neither the state nor the branch, and `railDrift` has the state in the rail's head.
  - `awl.spec.ts` `composerDrift`: "a control under the box is not before where the task runs" (`conv-location` under the box). In its place: where the task runs is not under the box. The controls ending at the box's right edge is unchanged, and its mutation (the controls at the left) still fails.
- Moved with their fact:
  - `awl.spec.ts` `columnDrift` measures the conversation, not the whole centre, and runs at 1440 wide: beside a 320 px rail the conversation at 1280 is 672 wide, under the column. Its numbers (768, 736, inset 16) and its three mutations are unchanged.
  - `composer.spec.ts` (PX-054) runs at 1440 wide for the same reason; its width and height assertions are unchanged.
  - `supervision.spec.ts`: "the details are not open" is "More is not open and Details is closed"; Escape closing the details is Escape closing More.
  - `fleet-agents.spec.ts`: the ended child is read after "1 finished" (asserted) is opened.
  - `shell.test.ts`: the pinned key list of the layout preferences gains `detailsOpen`.
  - `e2e/support/new-task.ts`: `openTaskDetails` opens Details in the rail's head (and More); `openTaskMore` is new; `conversation.spec.ts`, `checkpoints.spec.ts`, `workers.spec.ts` and `awl.spec.ts` take the density, Undo all and Browser from More.
- No assertion was weakened.

## Differences from the boards (my screenshots in `after/` beside the board sources `Task-Working` and `Task-NeedsYou`, read as HTML and CSS, classes `rail2`, `rsec`, `rrow`)

- **A head.** The boards' rail starts with Environment. The app's starts with a row holding the state pill and the Details control, which the record asks for.
- **Hairlines.** The app draws one above every section, the first included (it separates the head); the boards draw them between sections.
- **Environment.** No "Move to cloud" button. The folder's parent is the absolute path, cut with an ellipsis ("modbit-px201-repo… · /var/folde…"); the boards abbreviate the home folder ("~/projects"). "own worktree" stands only for a task in a worktree. The run mode's row ends in "change", not in what the mode does ("asks before writes"): the Core's sentence for a mode is long and is the row's tooltip.
- **Approvals.** A question mark in a circle and the count in the warning colour; the boards have a filled warning mark. The rules row has a list icon (Lucide has no "rule").
- **Tool servers.** "n configured" or "n inert", not "2 connected", and no section when there is none.
- **Evidence.** "Verification" with "1 of 1 passed", "verified" or "not run yet"; the boards say "Targeted checks" with "running". A line with the Core's latest evidence text stands under it; the boards have none. "restore" opens a list of turns in place, each with its file count and Restore; the boards do not draw it open.
- **Background tasks.** A child's row ends in its Core state ("Running"), not in a time; a terminal's ends in its running time. The terminal's name is its command line in the monospace face, cut with an ellipsis. "n finished" has "show" at its end and opens a list.
- **Cost.** The head holds the spend alone ("$0.0003", four decimals under a dollar); the boards have "$0.02 of $0.50". The caps are rows: "0 of 1,000 minor units 0%" and "8 s of 60 min 0 s 0%". The model is the Core's name for it ("gpt-5"), not "Auto balance".
- **Sizes.** Rows are the density's row height (32, 30 in Compact; the boards 32 and 28) with 14 px text (13 in Compact; the boards 13.5 and 13). Section heads are 13 px medium (12 in Compact; the boards 13 in both). Padding is the boards' 6, 18, 18; icons are Lucide at 16.
- **Stop** is a 9 px square in a 22 px button, as on the boards.
- **Title bar.** Details is first among the toggles, as on the boards, with Lucide's panel icon. The panes' order is PX-200's.
- **When it hides.** At a task area of 800 or narrower (the record); the boards' sheet hides it at a window of 1180.
- **With a pane open** there is no rail; the boards do not show the two together either (their `panes-on` layout has no rail).
- Type is the system face.

## Differences from the Decision Record

- MWS-09 "Tool servers": the session's declared servers, configured or inert, and nothing when there are none (the audit).
- MWS-09 "spend against budget": two facts in two units, not one ratio (the audit).
- MWS-09 "behind Details in the rail's head": Details also holds the context inspector and the economics in full, which the popover held and no section shows.
- Section 5.5 names no place for the conversation's density, Undo all, Browser and Cancel task. They are under More.
- Section 7 "the details popover's ids move to the rail": four of them (density, undo all, browser, cancel) moved to More.
- The record does not say what the toggle does while a pane is open: here it closes the pane and shows the rail.
- `top-bar-state` and `top-bar-branch` keep names that no longer say where they are.

## Limitations

- More opens from the right end of the title bar, so with the rail shown it lies over the rail's head while it is open.
- While an approval card is docked the card is on top of More: in a short window the lower sections of More are behind the card until it is decided or the window is made taller. The popover had the same order.
- A child's Stop and Cancel task share one confirmation banner; it names the task it will cancel.
- The header's change size follows the task list's read of the headers; where the list is collapsed the rail reads the headers itself, 400 ms after the task's last event.
- The tool-server section with servers present was not driven end to end (no extension with a server is loaded in the run).
- A task in a worktree of its own, a cloud location, an unpriced model, a time cap near its limit and a failed verification are unit-proven only.
- **CI run 1** (43231429943 on 8e56e9eb): the desktop E2E jobs failed on the three systems. `browser.spec.ts:147` and `:574` on macOS and Windows: the runners' windows are narrower than the local one, so More held the rail's whole body, the helper left it open, and this change had raised More above the docked approval card, whose Allow could then not be clicked. More is under a docked card again and `openTaskDetails` opens More only where the rail is absent. `checkpoints.spec.ts:46` on Ubuntu: Undo all stood under the rail's body in More and moved as it filled, so the click was lost; the controls now stand first. `terminal-history.spec.ts:94` on macOS (its setup saw the task Waiting, not Running) is not explained; it passed on the other systems and locally.
- After those fixes the 13 touched specs passed locally (47 of 47); the whole suite was not run again locally before the second push.
- The proof is local (macOS arm64); CI runs are recorded in evidence.json.
- The owner has not yet seen these screenshots beside the boards, which the record makes an exit criterion. The row is not COMPLETE and no graph status was set.
