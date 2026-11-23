# Task Card — PX-204 Split view: two tasks side by side, each with its own title bar, transcript, composer and approval card

## Identity

- Task ID: PX-204 (REQ-PX-204; owner desktop); qualification QUAL-PX-204 / PX-E2E-204; Decision Record DR-PX-2026-11-11-021 (MWS-11, section 5.1, section 5.6; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (two approval surfaces can be on screen and the keys must act on the focused task only)
- Prerequisites: PX-202, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-split-view`, cut from origin/main ddeda3bc (PX-200..203 sealed).

## The rules implemented (quoted from the record)

- MWS-11: "Two tasks can sit side by side in the centre, each with its own title bar, transcript, composer and approval card; one has focus and shows it; the keyboard acts on the focused one. Alt+click on a row, the preview card and the palette open a task in split view."
- Card PX-204: "Two tasks in the centre with a visible focus; the keyboard acts on the focused one; each composer queues to its own task; closing one returns to a single task; a third is refused; below the width two columns need, split view is refused with its reason."
- QUAL-PX-204: with one task waiting on approval and one running, Enter and the approve and deny keys decide only in the focused task; each composer queues to its own task; Alt+click on a row and the preview card open split view; a reload restores it. Fails if a key decides the unfocused task's approval, a queued message reaches the other task, or a third task is accepted.
- Section 5.1: "The centre's minimum, the single-pane threshold and the window's minimum size keep their numbers." Section 5.11: "Nothing is drawn that does nothing."

## Existing-code audit (before any edit)

- classification: NOT-FOUND for split view (the shell has list | centre | dock; "split" in the renderer meant only the Review's split diff, `keyboard.ts` `toggleSplit`). What a second conversation needs was PRODUCTION-WORKING for exactly one conversation:
  - `computeLayout` (`shell/layout.ts`) gives one centre; `ShellFrame` draws one top bar, into whose end slot the open conversation portals More.
  - `Conversation` (`conversation/conversation.tsx`) is self-contained per task (its own `useConversation`, composer, approval dock, mode-proposal dock, checkpoint bar, context tray host), keyed by task id: it can be mounted twice. Its own DOM queries (`.mb-tray[data-tray-id="approvals"]`, `.conv > .mb-tray-host`) read the whole document.
  - Trays: `ApprovalDock`, `ModeProposalDock`, `Composer` (queue, policy) and `ContextTrayHost` all present into the one module-level `trays` store under fixed ids ("approvals", "mode-proposal", "composer-queue", "context"); `TrayHost` with no prop reads that store. The context ring docks its tray through one module-level `dock` function (the last mounted host's).
  - Keys: `ApprovalDeck` pushes an "approval-card" key scope (Enter run, Shift+Enter always, Escape skip) whenever it has cards; `KeyScopes.dispatch` runs the innermost scope first. `use-app.ts` `runKeyCommand` (y approve, x deny, c cancel, s steer) acts on `activeElement.closest("[data-task-id]")`, else on the first `[data-testid="conversation"]` in the page; steer focuses the first `composer-input` in the page; the composer's Escape focuses the first `conv-scroll` in the page.
  - `useDockApprovals` lists every pending approval of the session, the open task's first and other agents' after them, marked foreign.
- first missing link: the tray store is one per window with fixed ids, so a second conversation's approval card, queue and context would replace the first's (and both hosts would draw the same tray); and the deck's key scope and `runKeyCommand` choose a task by stacking order and page order, not by which conversation has the keyboard, so with two cards on screen Enter or Escape would decide whichever deck mounted last, and y/x with nothing focused the first conversation in the page.

## What was built

- **Split state (`shell/split.ts`, `shell/prefs.ts`).** `SplitState { tasks: [left, right], ratio, focus }` under `split` in `modbit.ui.v1`. `parseSplit` reads two distinct ids or nothing (a stored list of three or more reads as none, never trimmed). `requestSplit` accepts a task beside the one the centre shows (the new one on the right, with the keyboard) and refuses, in words: nothing in the centre, the task already there, a third task ("Split view holds two tasks. Close one of them first: Close in its title bar."). `openInSplit`: a plain open while two are side by side gives a task that is there the keyboard and puts another task in the focused column (split view stays at two). `remainingAfterClose`, `ratioForColumns`, `splitRefusal`.
- **Geometry (`shell/layout.ts`).** `SPLIT_LAYOUT`: a column keeps the centre's minimum, `GEOMETRY.centerMin` (424); beside an open dock it keeps the conversation's 560 of PX-202. Two columns need 848 of task area (`SPLIT_NEED`). `computeLayout` takes `split` and returns `split: { left, right }` and `splitFits`. With the dock open the dock sits beside the two only where each keeps 560 (2 x 560 + 280), else it stacks behind Conversation | Apps exactly as for one task. The details rail never stands beside two columns (`detailsPx` 0): each task's facts are under its own More, as when a task area is too narrow for the rail. No token changed.
- **Split view (`shell/split-view.tsx`, `shell/shell.tsx`, `shell/shell-frame.tsx`).** Two columns, each with the shell's own `TopBar` (title with the task's menu, that task's pane toggles and Panes menu, More, and Close: "Close: leave split view"), and `Conversation` mounted once per column: one component, not a copy (the shell's JSX for it moved into `conversationFor`, used by both views). The column with the keyboard has a 2 px accent rule along its top (`box-shadow: inset 0 2px 0 var(--mb-color-accent)`) and its group is named "<title>, has the keyboard". Focus moving into a column, or a press in it, gives it the keyboard; `conversation` (the shell's open task) is always the focused column's task, so the dock, the keyboard's commands and the app's trays follow it. A separator between the columns sizes them (arrows, Shift, Home, End, Enter resets; `splitter-split`). The shell frame's own title bar is not drawn while two columns are; `ShellFrame` gains `split`, `layoutRef` and children that can read the layout; `data-split` on the shell says `none`, `shown` or `paused`.
- **Trays per task (`packages/ui/src/tray.tsx`, approvals, composer, context ring).** `TrayStoreContext` and `useTrayStore()`: a subtree presents into the store its provider gives, the app's one store by default. `ApprovalDock`, `ModeProposalDock`, `Composer` and the context ring use it; the ring's docking function is kept per store. With one task the conversation's store is the app's store, so nothing changes; in split view each task has a store of its own and its own `TrayHost` above its composer, and the focused column also hosts the app's own trays (limits, archive undo). `ApprovalDock` gains `shownBeside`: the other column's approvals are on their own card there and are not docked here as another agent's.
- **Keyboard routing (`renderer/key-focus.ts`).** The conversation that has the keyboard carries `data-key-focus="true"`, the other "false" (one task: "true"). `keyTaskId()`: the task of the focused element (a row, a conversation, an attention item), else the conversation that has the keyboard, never the first in the page. `runKeyCommand` (y, x, c, s) uses it; steer focuses that conversation's composer; the composer's Escape focuses its own transcript. `ApprovalDeck`'s scope resolves a key only where `ownsKeys(card)`: the card's conversation has the keyboard. The other task's card stays on screen and answers the pointer: routing, not hiding.
- **Entry and exit.** Alt+click on a task row (`agent-list-view.tsx`: the row's click passes `altKey`); Open in split view on the preview card (`agent-preview-split`). Both go through `requestSplit` and the width check; a refusal is said in the shell's notice (a live region). Close in either column's title bar leaves split view with the other task in the centre. Back and New task leave split view for home. Both rows of a split are marked open in the list (`data-in-split`).
- **Reload.** The shell's first conversation is the stored split's focused task, so the two tasks, the ratio and the focus come back after a reload; a stored split whose task the Core no longer lists closes with the task that is left ("Split view closed: a task in it is no longer listed.").
- **Narrow windows.** Entry below 848 of task area is refused: "Split view needs 848 px beside the task list for two tasks; this window gives 812. Widen the window or hide the task list." An open split in a window narrowed below it is paused: the focused task alone in the single view (with its rail where it fits), a line above it ("Split view is paused: two tasks need 848 px beside the task list and this window gives 812. <other task> is back beside this one when the window is wider; Close leaves split view.") and Close in its title bar; widening brings both columns back. Hiding the task list (rail 40) gives the room back.
- **Landmark names (`renderer/landmark.ts`).** Two tasks draw two transcripts, composers, notices and trays, and axe requires unique landmark names: in split view each says whose it is ("Composer: Add the release remotes", "Messages: …", "Notices: …", "Queued messages: …", "Plan: …"). With one task nothing is added. `TrayHost` gains a `label` prop.

No preload, main-process, protocol, CSP or Rust change. No locked file changed.

## How the details rail and the pane dock behave with two tasks (decided here)

- **Details rail**: not drawn beside two columns (board C2 draws none, and a 320 rail beside two 424 columns needs 1168 of task area). Each column's rail content is under its own More, the path MWS-09 already has for a task area too narrow for the rail. When split view is paused the single task gets its rail back where it fits.
- **Pane dock**: serves the focused task (the dock already follows the open task). A column's pane toggle gives that column the keyboard and opens or closes its pane. The dock sits beside the two columns only where each keeps PX-202's 560 (task area 1400 and up); otherwise it stacks behind Conversation | Apps, and the Conversation segment shows both columns. PX-202's rule for one task is unchanged (unit-tested: the same 1440 window keeps the dock beside one task).

## Left out, and why

- **The palette's way into split view** (MWS-11 names it; QUAL-PX-204 does not). The palette runs an item on Enter with no modifier and has no second action per item; adding one per task would double the task entries. Not drawn.
- **"Open in split view" in the task's title menu** (MWS-05 lists it). The title menu of the single view would need a second task to put beside it; the preview card and Alt+click are the ways in.
- **A keyboard shortcut to move the keyboard between columns.** Tab and a click move it; the record names none and `keyboard.ts` has none to reuse.
- **A split with an automation task from another session** closes on start (the shell knows another session's task only once opened).

## Verification

Unit: `@modbit/desktop` 424 of 424 (416 before, +8 in `shell/split.test.ts`); `@modbit/ui` 11 of 11; `pnpm typecheck` clean.

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a…), scripted model: `e2e/px-204-split-view.spec.ts`, 1 of 1, about 20 s, run several times locally on the final build. Three tasks: C running (its model held), A asking four times to run `git remote add <name> <bare>` (a real bare repository), B running (its model held). What it proves, in order:

- **Alt+click on A's row** opens split view: B left, A right with the keyboard; exactly two columns, each with one title bar (its title, Close), its own conversation of its own task and one composer (`splitDrift`); A's card in A's column only, none in B's; the columns 576 and 576 at 1440 (board C2); no details rail; both rows marked open; the accent rule on A's column and none on B's; A's group named "…, has the keyboard".
- **A third task is refused**: Alt+click on C and the preview card's Open in split view each say "Split view holds two tasks"; still two columns; the store still names B and A.
- **Each composer queues to its own task**: a message typed in A's box and one in B's, each read back from the Core's queue and transcript of both tasks (`misrouted`): on its own task, not on the other; B's queue tray in B's column holds B's message and not A's.
- **Axe** light, dark and compact on the split: no violation.
- **Keys in B's column decide nothing of A's**: Enter, Escape, y and x with B's transcript focused, and y, x, Enter with nothing focused while B has the keyboard: after each the Core still lists A's approval as pending, and its card is still in A's column.
- **Keys in A's column decide A's**: Enter on its card (the Core runs it; the next approval is asked), y (approved), x (denied), each read back from the Core's pending list; then the repository's remotes are exactly origin and mirror (the two approved; backup denied). B is still running.
- **A reload restores it**: the divider moved by keyboard (608 and 544), B given the keyboard, reload: the same two tasks in the same columns, 608 and 544, B with the keyboard, A's fourth card in A's column; Enter in B still decides nothing of A's.
- **A store that names three tasks** (written into `modbit.ui.v1`) reads as none after a reload: home, no column, `data-split="none"`.
- **The preview card opens split view**: Space on A's row, Open in split view.
- **Narrow**: at 1100 wide (812 of task area) the open split pauses with A alone and the paused line naming B; at 1440 both columns are back. Closed, then Alt+click at 1100 is refused with "Split view needs 848 px beside the task list for two tasks; this window gives 812. Widen the window or hide the task list." and no column is drawn.
- **Close** in B's title bar leaves split view with A in the centre, one title bar, nothing stored.

Whole desktop suite, local, macOS arm64, once (see `desktop-e2e-full-results.txt`): 212 passed, 1 failed, 3 skipped (the two live-model specs and the Windows-only one). The failure, `task-list.spec.ts:492`, was caused by this change (the preview card's pinned single button, see below), not the known flake; after moving its two assertions, `task-list.spec.ts` 2 of 2 and `px-204-split-view.spec.ts` 1 of 1. The suite was not run whole a second time.

## Retained mutations (each asserted to fail)

- **A key that decides the unfocused task's approval**: with B's transcript focused, the keyboard's owner is moved to A in the page (`data-key-focus` swapped on the two conversations) and Enter is pressed: A's card takes it and the Core no longer lists A's approval, and `unfocusedDecided` reports "the unfocused task's approval was decided".
- **A message that reaches the other task**: a message queued on A through the bridge as if typed in B's box: `misrouted` reports it is not on the task it was typed for and that it reached the other task (then it is removed from A's queue).
- **A third task accepted**: a third column drawn in the split view: `splitDrift` reports "3 columns are drawn". (And the app itself reads a stored split of three tasks as none.)
- `split.test.ts`: a stored list of three, one, the same task twice, a non-string id, a string, an array and a number each read as none.

## Test ids

- New: `split-view`, `split-column` (`data-task-id`, `data-column`, `data-focused`), `split-close`, `splitter-split`, `split-paused`, `agent-preview-split`; `data-split` on `shell`; `data-key-focus` on `conversation`; `data-in-split` on `agent-row`.
- In each column the existing ids of the title bar and the conversation are reused (`top-bar`, `top-bar-title`, `task-menu`, `pane-toggle-<kind>`, `panes-menu`, `task-details-toggle`, `conversation`, `conv-scroll`, `composer-region`, `composer-input`, `approval-card`, `tray-host`); with two columns there are two of each.
- None removed.

## Assertions deleted, replaced or changed

- None deleted.
- `shell/shell.test.ts` "a layout preference is renderer-local: it never carries task state": the stored keys gain `split` (two task ids, the ratio and the focus: a view choice, like the dock stored per task id). Same fact; the list gains an entry.
- `e2e/task-list.spec.ts:579` (PX-199): "The header's later fields and split view belong to later rows: none of them is drawn here", `card.getByRole("button")` count 1 and no text matching /pull request|split view/. Split view is now built (this row), so: the card's buttons are exactly "Open" and "Open in split view", and no text matches /pull request/ (the pull request still belongs to a later row).
- `e2e/task-list.spec.ts:607` (PX-199): "leaving the card with Tab closes it" pressed Tab once from Open. Open in split view now follows Open inside the card, so: the first Tab focuses Open in split view, the second leaves the card and it closes. Same fact.

## Differences from the boards and the record

- Board C2 is drawn at 1440 in Compact; the screenshots are at 1440 in both densities. The columns, title bars, Close and the focused column's accent rule match. The board's rule is the accent along the focused column's top; here it is the same, 2 px.
- The board's title bar shows Changes and Close; here each column's bar shows the task's pane toggles as PX-202's bar does (Changes, Terminal where the task has one, the Panes menu) and More, then Close.
- The board shows a task's composer footer as "Auto balance / Supervised / ring"; the build's footer is PX-200's (Agent, Auto Balance, Supervised, ring).
- The board shows no notice; the screenshots of a refusal show the shell's notice at the foot of the window.
- The board's preview card has "Open in split view" as a ghost button beside Open; the same here.
- The record lists the palette as a way in; not built (see Left out).

## Limitations

- The proof is local (macOS arm64); CI is recorded in `evidence.json` (run 1, 43755695976: the PX-204 spec raced a window resize on Ubuntu, fixed; three other failures not in this change's paths).
- Shift+Enter (Always allow) on the card is routed by the same `ownsKeys` check as Enter and Escape and is not driven separately in the spec.
- The owner has not seen the screenshots. No graph status was set.

## Closure (2026-11-16)

- CI run 43762302238 on head 209d21f0: all 19 checks passed after one rerun of failed jobs (release supply chain had been cancelled after hanging past its reproducible-build check). Merged to main as 9da82128 (moss101/modbit#221).
- The owner reviewed the screenshots of the running app beside board C2 and answered "Yes, seal when green" on 2026-11-16.
- Run 1's `packaged-update.spec.ts:108` failure (an update staged after the signed manifest was tampered with) is not caused by this change; it is being investigated in its own session as a suspected test race.
- Left open at the seal: the focus marker is a thin accent line only; "Notice: Queued messages" and "Queued follow_up" are raw wording; opening split view from the palette and a shortcut to move focus between columns are not built.
