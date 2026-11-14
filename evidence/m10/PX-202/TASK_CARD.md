# Task Card — PX-202 Pane dock: several panes open together in a grid, with Activity, Background tasks and Pull request panes

## Identity

- Task ID: PX-202 (REQ-PX-202; owner desktop); qualification QUAL-PX-202 / PX-E2E-202; Decision Record DR-PX-2026-11-11-021 (MWS-10, MWS-01, section 5.1, section 7; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (opening a pull request and stopping tasks move into panes, and the open panes are stored per task)
- Prerequisites: PX-201, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-pane-dock`.

## The rules implemented (quoted from the record)

- MWS-10: "The apps panel becomes a dock: any of the task's panes can be open together, tiled in a grid of at most two columns; each pane has a 45 px head with its name, its own actions, Minimize, Pop out and Close; open panes and their sizes are remembered per task. With panes open the rail gives way. Panes: Changes, Terminal, Browser, Files and Evidence (PX-048 owns their content), and three new ones: Activity (the task's events in order, searchable and filterable by kind), Background tasks, and Pull request (the request, its checks, people's comments)." Pop out is PX-203 and is not drawn.
- Section 5.1: "A pane is never under 280 by 100 px." MWS-01: "the top bar and every pane head 45 px, a 1 px stroke 8 % rule under each".
- Section 7: "panes tiled with heads of 45. Retained mutations show the checks failing on a surface pushed back to the superseded numbers (... one app at a time)"; "a stored panel tab becomes the one open pane".
- Section 6: "Where a board shows a control whose behaviour belongs to a row that is not built, the control is absent or disabled with its reason. Nothing is drawn that does nothing."

## Existing-code audit (before any edit)

- classification: IMPLEMENTED-PARTIAL. Traced from the production caller to the Core: `Shell` computes the panel's task (`panelTaskFor`), its artifacts (`artifactsOf`) and its remembered `{ open, tab }` (`prefs.panelByTask`), and mounts `AppsPanel`, a `Tabs` strip over five apps (`ChangesApp`, `TerminalApp`, `BrowserApp`, `FilesApp`, `EvidenceApp`), each a view over typed reads through the preload bridge. PX-200 put one toggle per pane in the title bar; they switched the one tab. `computeLayout` already gives the panel its width and makes the rail give way. The pull request is PRODUCTION-WORKING in the Review (`openPullRequest`, `resolveApproval`, `ingestCiResults`, `ingestReviewComments`, `reviewBundle`); stopping a child and a terminal is PRODUCTION-WORKING in the details rail (`cancelTask` behind the shell's confirmation, `terminalKill`). An activity list is NOT-FOUND: the renderer folds events into cards and keeps none; the transcript rows live in the open conversation's hook.
- first missing link: `AppsPanel` takes a single `tab: AppKind` and `PanelState` is `{ open, tab }`: there is no set of open panes, no grid and no size to remember.
- Found on the way:
  - With the tab strip gone, a dock shown while the centre is not its task (home after a reload: the panel shows the latest task with something to show) had no way to open another pane, because the title bar draws the toggles only for the task in the centre. The dock now carries the same toggles in a bar of its own in exactly that case.
  - The Core has no read of a pull request's state (open, merged, closed), and no read of the request a task opened: the number and URL exist only in the answer to `openPullRequest`. That is PX-211's. The pane shows the answer this window was given and nothing after a reload.
  - While the first test's task is open, `conversation:markRead` and a transcript re-read follow each other about every 0.4 s (the bridge log shows it in the quiet stretch before the Activity pane is opened, so it is not the pane's). Not investigated further and not verified against main; it belongs to the conversation (PX-047).

## What was built

- **State (`shell/prefs.ts`, `shell/dock.ts`).** `PanelState` is the task's dock: `panes` (open, in the order they tile), `minimized`, `last` (what the dock's own toggle brings back), `split` (the left column's share) and `weights` (a pane's share of its column's height). It stays under `panelByTask` in `modbit.ui.v1`; an entry from before (`{ open, tab }`) reads as that one pane, open or remembered. `dock.ts` is pure: open, close, minimize, toggle, the columns (one, or two where there are two panes or more and each can be 280), the left column's width and the rows' heights with the 280 and 100 minimums, and what a drag stores.
- **The dock (`shell/apps-panel.tsx`, `shell/pane.tsx`).** The panes of the grid in one or two columns; each pane has a head the height of the top bar (`--mb-top-bar`: 45, 40 in Compact) with its name, its own actions, Minimize and Close, over its body. A separator between two panes of a column and one between the columns: dragged, or moved with the arrow keys (Shift for a larger step, Home, End, Enter to reset), `Splitter` extended with a horizontal axis. Where the panes of a column cannot all be 100 tall the dock scrolls. The tab strip, "Widen the panel" and "Close the apps panel" are removed; the panel's width is still the frame's separator.
- **Own actions in the head.** `PaneActions` puts a pane's own controls in its head: Changes (summary, Refresh, Review, Stage all), Evidence (Refresh), Activity (the find field), Pull request (Refresh).
- **Title bar.** Eight toggles at most: Changes, Terminal (when the Core lists one), Browser, Files, Evidence, Activity, Background tasks, Pull request. Each says open, minimized or closed; an open pane's toggle closes it, a minimized one's brings it back, a closed one's opens it beside the others. The keyboard's "toggle apps panel" closes every pane and brings the same set back.
- **Minimize.** The pane leaves the grid and keeps its place and size; its toggle shows a mark and is named "… pane, minimized". A minimized terminal's view detaches; the process is the Core's and goes on, and the view attaches again by cursor to the same terminal when it comes back.
- **Activity (`apps/activity-app.tsx`, `apps/activity-model.ts`, `apps/delivered.ts`).** What the task did, oldest first, with a filter per kind that has entries (Messages, Edits, Commands, Reads, Approvals, Checks, Turns, Other, each with its count) and a find field (every word, any case). It lists what this window was already given and reads nothing: the transcript rows the open conversation read (handed over by `useConversation`) and the events main pushed (a memory-only buffer of at most 2,000 events for each of 64 tasks, never written anywhere and never decided from). A row is worded as the conversation words it; an event that a listed row already covers is not listed twice; an event with no row is its type in words with a few named fields. Pieces of a message being written, and the window's own housekeeping events (`ConversationRead`, `TerminalControlRecorded`), are not entries.
- **Background tasks (`apps/background-app.tsx`, `conversation/background-list.tsx`).** The list the rail's section draws, moved into one component used by both: each running child task (opens it, its Core state, Stop) and terminal (opens it in the Terminal pane, its running time, Stop), and the finished ones behind their count. Stop on a child asks with the confirmation Cancel task asks, then `cancelTask`; Stop on a terminal asks once more in place, then `terminalKill`.
- **Pull request (`apps/pull-request-app.tsx`, `apps/pull-request-model.ts`, `review/use-pull-request.ts`).** The Review's pull-request flow moved into one hook that the Review and the pane both use. Open pull request asks the Core; the Core answers with the approval; the pane then waits and decides nothing: the approval is answered on the docked approval card (which asks twice for an effect that cannot be undone), its answer reaches the window as an event of that approval, and the pane then asks the Core to go on, or says the attempt ended. Opened, it shows the number, the URL, the branch, the head and the receipts the Core answered with, and Update the request. Checks and Comments are what the Core recorded (`reviewBundle`): each check with the forge's result, the run and the commit; refused runs with the reason; each comment with its author and what became of it, an ignored comment without its words. Refresh asks the Core to read the forge's checks and the comments again. The renderer calls no forge.
- **Rail wording (open after PX-201).** The evidence line is said in a person's words with the Core's facts kept ("delegated child-a (BACKGROUND)" is "Started a child task in the background: child-a"; the Core's own text stays on the element as `data-core`). "Generation" is "Updates recorded". The economics paragraph is a list of facts: Checks, Model, Tokens, Cost, Tool calls, Time, Prompt reuse.
- **More over a docked approval card (open after PX-201).** More ends 8 px above a card it would run behind (never under 120 px) and scrolls inside; the card stays on top, so its answers can always be clicked.
- No preload, protocol, main-process, CSP or Rust change.

## Left out, and why

- **Pop out**: PX-203. No control is drawn for it.
- **The request's state, its title, "n files", Merge when ready, Auto-fix, Address** (board A4): PX-211, PX-212, PX-213, PX-214. The Core has no read for them.
- **The request after a reload**: the Core has no read of the request a task opened, so the pane shows no number then (the checks and comments are the Core's record and are shown).
- **"Clear finished"** (board C1): the Core has no command for it.
- **New terminal tab** (board A4): the Core has no client command that starts a terminal.
- **The dock's width per task**: the width is the frame's separator, one preference for the window, as before. What is per task is the set and the sizes inside the dock.
- **A link that opens the request in a browser**: the bridge has no call for it from this pane; the URL is text that can be selected.

## Verification

Unit: `@modbit/desktop` 405 of 405; `pnpm typecheck` clean. Real app, real Core, scripted model: `e2e/px-202-pane-dock.spec.ts`, 2 of 2; the gate `e2e/support/dock-gate.ts` runs in that spec and in `awl.spec.ts`. Whole desktop suite, local, macOS arm64, two runs: run 1, 204 passed, 3 failed, 2 skipped, 2 did not run (two failures were axe on the pane body, caused by this change and fixed; `secrets.spec.ts:66` passed alone and in run 2 and is not explained); run 2 on the final build, 209 passed, 0 failed, 2 skipped. Results by test in `desktop-e2e-full-results.txt`. `shell.spec.ts:25` needs `target/debug/modbit-core`: `target/debug` was linked to the prebuilt binaries for these runs.

- **Changes, Terminal and Evidence open together and tile**: the three panes are on screen at once in two columns; the gate holds in light and dark, Comfortable and Compact; axe on the shell.
- **The set and the sizes persist per task across a reload**: the separators are moved by keyboard, the boxes recorded; another task has no pane and opening one there leaves the first task's set and boxes as they were; after `page.reload()` the boxes are equal to the pixel.
- **A minimized terminal keeps its session**: a terminal that counts aloud is typed into, minimized (the Core lists it running and its output grows meanwhile), and brought back: the same terminal id, the same start time, the typed line on the screen, the count past where it was, and typing reaches it.
- **Activity**: every entry is found in the bridge's log (a row in a transcript answer for the task, or an event main pushed for it), in the log's order; a kind filter lists only that kind; find lists only entries with the words; opening the pane, filtering and finding call no channel the window was not already calling in the quiet stretch before.
- **Background tasks**: Stop on the terminal (the first click stops nothing, Keep keeps it, the second kills it: the Core's list says KILLED); Stop on the child (Escape cancels nothing, Enter cancels it: the Core's header says Cancelled).
- **Pull request**: before the approval nothing is written to the forge and nothing is pushed; the approval the pane waits on is one the Core lists as pending with the same intent; allowed on the docked card (asked twice), one request is created with the Core's token and the branch is on the remote; Refresh lists the two check runs the Core recorded and the refused one, and the two comments; every forge call carried the token only the Core holds.
- **Narrow window**: at 1000 wide the dock stacks behind Conversation | Apps and the gate holds; at 1000 by 640 with an approval card docked, More ends above the card, every section can be scrolled to and is not covered, and the card's answers are on top.

## Retained mutations (each asserted to fail)

- `dock-gate.ts` in `px-202-pane-dock.spec.ts`: one app at a time (every pane but the first hidden); a 40 px pane head; a pane under 280 wide; a pane under 100 tall; a third column; a details rail beside open panes. In `awl.spec.ts`: one app at a time; a pane under 100 tall; a 40 px pane head; pane heads pushed down.
- A minimized terminal that loses its session: the terminal is killed through the bridge while minimized, and the check fails on it.
- Activity: an entry for an event main never pushed; an entry for a row no transcript answer carried, out of order; a read made beyond what was delivered (`effectReceipts`).
- Pull request: "Merged" written into the pane; a value naming a read that does not exist.
- More: at the height it had before, it runs under the card.
- `measured-css.test.ts`: a pane head with a height of its own; a pane with a smaller minimum height. `dock.test.ts`: three panes are never one.

## Test ids

- Removed with the tab strip: `apps-tabs`, `tab-changes`, `tab-terminal`, `tab-browser`, `tab-files`, `tab-evidence` in the panel (the `Tabs` component and its ids remain in the gallery), `apps-panel-maximize`, `toggle-apps-panel`, `app-changes-empty`, `app-files-empty`, `app-evidence-empty`.
- New: `pane-<kind>`, `pane-head-<kind>`, `pane-name-<kind>`, `pane-body-<kind>`, `pane-minimize-<kind>`, `pane-close-<kind>`, `dock-grid`, `dock-col`, `dock-bar`, `dock-bar-task`, `splitter-columns`, `splitter-row-<kind>`, `pane-toggle-activity`, `pane-toggle-background`, `pane-toggle-pull-request`, `app-activity`, `activity-find`, `activity-kinds`, `activity-kind-<kind>`, `activity-list`, `activity-entry`, `activity-empty`, `activity-none`, `activity-shown`, `app-background`, `app-background-empty`, `background-running`, `app-pull-request`, `pr-pane-*`, `task-generation`, `task-economics-<fact>`.
- Kept: `apps-panel`, `region-apps`, `splitter-panel`, `pane-toggles`, `pane-toggle-<kind>`, every id inside the five apps, and the rail's ids (the Background tasks pane draws the rail's list, so `child-card`, `child-stop`, `rail-terminal` and the rest are in the pane too).

## Assertions deleted, replaced or changed

Deleted because the surface was removed on purpose (the tab strip and its tools), each with what stands in its place:

- `apps-panel.spec.ts` "tabs by arrows": Home selects Changes and ArrowRight selects Terminal in the tab list. In its place: a pane is opened from its toggle and closed from its head by keyboard, and the pane that was open stays beside the new one.
- `apps-panel-state.spec.ts` and `shell.spec.ts`: the Terminal tab of a task with no terminal shows `app-terminal-empty`. In their place: the task's second pane is Files (`app-files` visible). A task with no terminal has no Terminal toggle (`shell.spec.ts` asserts that already).
- `shell.spec.ts`: `toggle-apps-panel` is absent on home; focus on it and Enter closes the panel. In their place: `apps-panel` is absent on home; focus on `pane-close-files` and Enter closes the pane, and the stored dock has no pane.
- `awl.spec.ts` `workspaceDrift`: the tab list is in the top row at the bar's height, with its two mutations (a 40 px tab list, tabs pushed down). In their place: the first pane head is in the top row at the bar's height, the dock's gate, and the same two mutations on the pane head, plus one app at a time and a pane under 100.
- `measured-css.test.ts`: `.apps-panel .mb-tablist` reads its height from the token. In its place: `.pane-head` does, `.pane` is on the editor surface with a 100 px minimum, and two mutations.

Changed because the behaviour changed on purpose:

- `awl.spec.ts` `workspaceDrift`: "the commit bar is at the foot of the panel" is "at the foot of the Changes pane" (the panel itself where Changes is its only pane). Same fact; the pane is now the container.
- `checkpoints.spec.ts`: the hashing helper counts a file deleted between its existence check and its read as absent (a race seen on CI run 1, macOS). The assertions are unchanged.

- `px-200-task-view.spec.ts`: the toggles were four and are seven; opening Changes left Files not pressed (one at a time) and now leaves it pressed with both panes on screen; closing Changes left no panel and now leaves Files, which is then closed.
- `apps-panel-state.spec.ts`, `shell.spec.ts`: the stored state was `{ open, tab }` and is `{ panes: [...] }`; "its tab is selected" is "its pane is visible and the other task's is not".
- `shell.test.ts`: the preferences round-trip carries a dock; a hostile `{ open: true, tab: "nope" }` reads as the one pane Changes (it read as the tab Changes); `panesOf` lists the three new panes.
- `fleet-agents.spec.ts`: the evidence line is matched in its new words ("Started a child task in the background: child-a", "A child task finished: created src/a/a.txt", "Final check before review: passed | could not decide | did not pass").
- `states.spec.ts`: the gate's verdict is read from the Core's own text on the element (`data-core`), not from the words shown.
- `px-201-details-rail.spec.ts`: the label "Generation" is "Updates recorded" (the rail's own words, and the accessibility-tree check).

Moved with their fact (the assertion is unchanged, the way to the pane is its toggle): `tab-<kind>.click()` is `soloPane(page, kind)` in `agent-workspace`, `apps-panel`, `autonomy`, `wave4`, `terminal-history` and `terminal-renderer-restart`; "the tab is selected" is "the pane is visible" in `apps-browser`, `composer`, `apps-panel` and `awl`.

No assertion was weakened.

## Differences from the boards (my screenshots in `after/` beside the board sources `Task-Ready` and `Panes-More`)

- **Grid.** The boards draw a CSS grid with shared row heights. The app's columns each have their own rows (a separator moves two panes of one column only), the left column takes the odd pane, and two columns need 561 px.
- **Dock width.** The boards give the panes 59 % of the task area. The app keeps the frame's separator and its stored ratio (39 % by default); the screenshots were taken after widening it.
- **Heads.** No Pop out. Minimize is a chevron and stands on every pane (the boards draw it on Terminal and Browser only). The Changes head holds the summary, Refresh, Review and Stage all; the boards hold the branch names, search and a menu.
- **Minimized.** The boards draw a floating pill. The app marks the pane's toggle in the title bar.
- **Activity.** Oldest first with the time of day; the boards are newest first with "20s ago". The filters are the kinds that have entries, with counts; the boards draw five fixed ones and a Filter button. Entries are the Core's rows and event names in words, so "Turn finished" and "Task ready for review" appear; the boards' sentences are written by hand.
- **Background tasks.** A child's row ends in its Core state, not "step 2 of 3"; no "Clear finished"; "n finished" opens in place.
- **Pull request.** No title, no "into main · 5 files", no durations on checks, no Address, no Auto-fix switch, no Merge when ready, no Commit and push (that is the Changes pane's): none has a Core read or belongs to this row. The checks are the forge's runs by name with the run id and commit.
- **Title bar.** With eight toggles and a wide dock the task's title is cut short.
- Lucide has no icon here for a pull request or for activity in the generated set; Activity uses the clock, Background tasks the layers, Pull request the compare icon.

## Differences from the Decision Record

- MWS-10 "Pop out": not built (PX-203).
- MWS-10 "its sizes are remembered per task": the sizes inside the dock are; the dock's own width is one preference for the window.
- MWS-10 "Activity (the task's events in order)": the list is what this window was given, so after a reload it is the transcript's rows until the Core pushes again; events of the window's own housekeeping are left out.
- MWS-10 "Pull request (the request, its checks, people's comments)": the request is what the Core answered to this window; its state is never shown (PX-211).
- The record does not say where a pane is opened from when the title bar shows another page: the dock's own bar.
- Section 7 "a stored panel tab becomes the one open pane": a stored closed panel stays closed and its tab is what the dock's toggle brings back.

## Limitations

- **CI run 1** (43390459914 on a391764f, pull request 219): Rust, node, protocol, dossier, packaged desktop and supply chain passed on every system; desktop E2E passed on Windows and failed on macOS and Ubuntu with one test each; the cloud job failed.
  - Ubuntu, `awl.spec.ts:552`: "the commit bar is not at the foot of the panel" with Changes and Files open. Caused by this change: where the dock is one column the Changes pane is the upper pane, so its commit bar ends at the foot of that pane, not of the panel. The check now measures the foot of the Changes pane (the whole panel when Changes is its only pane); its fact is unchanged. Locally the same window gave two columns.
  - macOS, `checkpoints.spec.ts:46`: `ENOENT` reading `a.txt` inside the poll that waits for Undo all to delete it: the file was removed between the spec's existence check and its read. A race in the spec's helper, not in the product, and not caused by this change (Undo all's path is unchanged); the helper now counts a file removed in between as absent, so the poll still waits for the same final state.
  - cloud CI_COMPATIBLE: a Rust test timed out ("timed out waiting for worker b to hold the session" in `qual_m8_2_a_worker_claims_the_lease…`). This change has no Rust; the same job failed on another branch's run 43375533349 with a different timeout. Not caused by this change.
  - After the fixes, `awl.spec.ts`, `checkpoints.spec.ts` and `px-202-pane-dock.spec.ts` passed locally (8 of 8); the whole suite was not run again locally before the second push.
- The proof is local (macOS arm64); CI runs are recorded in `evidence.json`.
- Dragging a separator with the pointer is unit-proven through the model and was not driven end to end; the keyboard path was.
- The Browser pane in the grid, a pane head at exactly 280 px with every action, and a dock that scrolls were not driven end to end.
- A request denied on the card, an approval that expires, and Update the request were not driven end to end from the pane (the outcome of an approval is unit-proven).
- In the stacked view (a narrow window showing the panes) the approval card is in the conversation, so the pane sends the person there.
- The owner has not yet seen these screenshots beside the boards, which the record makes an exit criterion. The row is not COMPLETE and no graph status was set.
