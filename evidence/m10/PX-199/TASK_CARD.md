# Task Card — PX-199 Measured task list: navigation, project groups, one-line rows with the state at the right, child rows, the preview card and the person's row

## Identity

- Task ID: PX-199 (REQ-PX-199; related REQ-EV-0037, REQ-EV-0143; owner desktop); qualification QUAL-PX-199 / PX-E2E-199; Decision Record DR-PX-2026-11-11-021 (MWS-04; AFW-A12, A13, A14 unchanged); DR-PX-2026-11-04-020 section 5.9 (AWS-10) stays in force
- Evidence tier: release-critical (archive and pin act on tasks from a new place and the rule for when a row shows its reason changes)
- Prerequisites: PX-198, DOC-PX-020. The record and the row were not on main when this was built (PR #196); nothing here sets a graph status.
- Branch `wip/desktop-2-shell-list`, one change with PX-198.

## Existing-code audit (before any edit)

- classification: IMPLEMENTED-PARTIAL. Traced from the Core to the row: `useAgents` reads `agentHeaders` (the Core's header projection) and re-reads on events; `AgentList` holds the preferences, pins, archive with undo and search; `AgentListView` windows the items `list-model.ts` lays out; `AgentRegion` puts the navigation rows above and the person's row below. PX-046 and PX-064 behaviour (classes from the Core, filters, pins, archive, stored views, project parents, search over the Core's index) worked and is kept.
- `GROUPINGS` already had every grouping the record names: `repository` (the default: the Core's projects as parents, then each repository as a group), `status`, `location` (environment), `time` (date), `project` and `workspace`. Nothing was added to the model; the options' words changed.
- first missing link: the row and its layout. A row was always two lines at one constant height (`ROW_HEIGHT` 50), the status in words on the second; the virtual list knew one row height. The list never read the session model's parent map, so a child task was a row like any other. There was no preview. The list's options and New project lived in a "Projects" heading that was an item of the list; the Search row and a Repositories section were in `AgentRegion`.

## What was built

- **Order (MWS-04).** Head; navigation rows New task, Inbox, Automations, Customize; the Tasks head with search, list options and More; the state views of AWS-10, unchanged; the groups; the person's row with the Core's state in a word, help and Settings. No Review row is drawn: its page belongs to a later row.
- **Inbox** is the Approvals page renamed: same command, same `open-inbox` id, the count still the number of tasks in the Core's attention view; the page's title, the top bar, the `a` key's help and Settings say Inbox.
- **Tasks head.** Search is the navigation row's command (the palette) under its id `agents-search-open`; list options is `agents-tools-toggle` and unfolds what it unfolded; More holds New project (`agents-new-project`, now a menu item) and Row details.
- **Groups.** Projects by default; status, environment and date under list options. A group that is a place (a repository, a project) has a menu of its own with "New task in <name>", which is what the Repositories section did. The Projects heading and its collapse are gone; a stored "projects" collapse key hides nothing.
- **Rows.** One line at the row height: the title, fading at its end, then the state's glyph, the changed files of a task ready for review, and the unread dot. A task whose class is Needs attention or Failed, or for which the Core gives a reason, keeps a second line with that reason. Row details: when needed (default), always, never; the reason stays in the row's name and tooltip in every case. The virtual list lays out both heights in both densities from `densityMetrics` (design tokens), the same source the stylesheet reads.
- **Child tasks** are listed directly under their parent, indented, with a mark, wherever both are in one group; the parent map is the session model's (`SubagentAdmitted`), already delivered. A child's name says whose child it is.
- **Pin and archive** show under the pointer and on keyboard focus in place of the row's marks; `p` and `e` still act on the focused row; archive keeps its confirmation for a running task and its undo. Focus now follows a row that a pin moved (it used to be dropped when the row moved to the Pinned group).
- **Preview card.** Opens when the pointer rests on a row (600 ms) and on Space; Escape, leaving it, scrolling or opening a task closes it. It shows the Core's state, the reason or next action, the size of the change, context use when the header has one, and Open. It reads the header and the facts the list holds and makes no call. The last assistant line, the pull request and Open in split view are left out (MWS-20, MWS-17, MWS-11).
- **Removed in the same change:** the Repositories section, the Search navigation row, the Projects heading as a list item, the row's age, `SECTION_HEIGHT`, `ROW_HEIGHT` and `PROJECT_HEIGHT`, and the list rules that were in `projects-css.ts`.

## Verification

- Unit (`agents/list-model.test.ts`, 41 tests in the folder): one-line and two-line rows by class, reason and Row details; the two heights of each density and the window over a thousand mixed rows; children under parents (other group, missing parent, grandchild, pinned, a looped map); group folders; the default grouping's shape; the row's name; the card's reason and change size; the stored preference.
- Real app, real Core, scripted model (`e2e/task-list.spec.ts`, two tests): two projects; tasks needing an approval, ready for review and unseen with two changed files, running with a running child, and two drafts. The order at 1280, 1000 and 900; the Inbox count against the Core's attention view; each row's lines, height, inset, fade, marks and words; the child under its parent; pin and archive on hover and focus; a group's menu starting a task in its folder; grouping by status, environment and date with no reload; Row details in its three values and across a reload; both densities; axe on the list and on home in light, dark and high contrast. By keyboard alone: every row with the arrows, `j`, `k`, Home and End; `r`; `a`; `p` pins and unpins; `e` archives and the toast's Undo restores; Space opens the card, Escape closes it, Enter on Open opens the task. By pointer: rest opens the card, a pass does not, the card can be entered, scrolling closes it, Open opens the task.
- The card makes no transcript read: the test wraps every IPC handler in the main process and records the channels. While the cards opened and closed the log held the list's own reads and no `conversation:` call; opening the task from the card then logged `conversation:transcript`, so the log could have caught one.
- Retained mutations, each asserted to fail: a quiet row with a second line (Row details set to always: five rows); a row that needs the person on one line (never); a missing reason; the old 50 px row; a wrapped title without the fade; the unread dot before the glyph; a child that is not indented; rows flush with the list's edge.
- Existing specs that drive the list pass unchanged in what they assert: `agents.spec.ts`, `supervision.spec.ts`, `keyboard.spec.ts`, `attention.spec.ts`, `conversation.spec.ts`, `fleet-agents.spec.ts`, `automations.spec.ts`, `worktrees.spec.ts`, `states.spec.ts`. `projects.spec.ts` and `gallery-projects.spec.ts` open More before New project (below).
- Whole desktop suite, local, macOS arm64, real Core, scripted models, on the final code: 198 passed, 0 failed, 2 skipped (the live-provider specs). An earlier run had one failure, in this row's own new spec (Limitations); that run, the first one and the run on untouched main are in evidence.json; results by test in `../PX-198/desktop-e2e-full-results.txt`.
- Screenshots of the running app at 1440 by 900 (`after/`): the list with several states and a child; grouped by status (light and dark, both densities), environment and date; the card on a task ready for review, on one that needs an approval and opened by keyboard; pin and archive on hover; a group's menu; More; Row details always and never; Inbox; the list at 900 by 600. Files named `fixture-gallery-*` are the model-free gallery, not the running app.

## Removed test ids

- `agent-repositories`, `agent-repository` (the Repositories section). `awl.spec.ts` and `task-list.spec.ts` assert they are absent.
- The element `agents-section` with `data-section="projects"` (the Projects heading); `agents-section` itself stays on every group. Asserted absent.
- Moved, same id: `agents-search-open` (navigation row to the Tasks head), `agents-tools-toggle` (Projects heading to the Tasks head), `agents-new-project` (a button in the Projects heading to an item of More), `agent-status` (on a one-line row it is the state glyph's text alternative), `agent-attention-dot` (the state's glyph while the class is Needs attention; a dot of its own otherwise).
- New: `agents-head`, `agents-more`, `agent-nav`, `open-help`, `agent-title`, `agent-glyph`, `agent-changes`, `agent-child-mark`, `group-menu`, `agent-preview` and its `-state`, `-age`, `-title`, `-reason`, `-changes`, `-context`, `-open`, `help-list-key`.

## Assertions deleted or moved

- Deleted, surface removed on purpose: `list-model.test.ts` "collapsing the group hides the projects" (the Projects heading's collapse); in its place a stored "projects" key is asserted to hide nothing. `list-model.test.ts` `items(...)[0].kind === "projects-head"`; in its place the first item is a group and an empty list has no item.
- Moved with their fact: `projects.spec.ts` and `gallery-projects.spec.ts` "the Projects heading is visible" to "the Tasks head is visible", and "New project is visible" to "visible in More"; four `kinds` lists in `list-model.test.ts` lose their `projects-head` entry; the thousand-row total is computed from the density's heights instead of the two removed constants.
- No assertion was weakened. One existing assertion caught a fault in this work and was left as it was: `gallery.spec.ts` requires every status glyph to carry a text alternative, and the first row markup did not.

## Differences from the boards (what I can see in `after/` beside `../PX-198/boards/`)

- Four navigation rows, not five: no Review.
- The state views (Attention, Review, Running with their counts) sit between the Tasks head and the groups. The boards do not draw them; the record keeps them.
- On the board a task that needs the person is one line with a filled mark. In the app it keeps its second line with the Core's reason, as the record's text says, so those rows are 46 high.
- State glyphs are Modbit's own status shapes: a triangle for needs attention, an arc for running, a circled i for ready and unseen, a check for ready and seen and for completed, a ring for waiting, draft and archived, a cross for failed. The board draws a filled circle, a ring, a merge glyph and a branch glyph, and a small grey dot for a draft.
- Inbox has the shield icon it had as Approvals: the generated Lucide table has no inbox shape and its generator is under `tools/`, which this row may not touch. Customize keeps its blocks icon (the board draws sliders). A child's mark is an elbow drawn in CSS, not an icon.
- No "Archived" group at the foot of the list; archived tasks are shown by the Show archived filter, as before.
- Grouped by status (board Split) the headings are the Core's classes in its own words and order (Needs attention, Ready for review, Running, Draft), not the board's Needs you, Working, Ready, Inactive; a heading shows its count after a dot, as the board does.
- The preview card has the state as a glyph with its words (the board draws a tinted tag), the age, the title, the reason, a Changes line and Open. It has no last assistant line, no pull request line and no Open in split view. Its Context line appears only when the Core reports a context share, which a scripted model never produces: it is shown from the fixture gallery only (`fixture-gallery-preview-card-with-context-use-light.png`).
- The person's row matches in content; its icons are 16 px against the board's 20.
- A running parent that has unseen events shows the unread dot beside its glyph; the board's running rows show none (it is the Core's fact, not a layout choice).
- Compact rows are 2 apart; the board's Compact draws 1.

## Differences from the Decision Record

- "Review" is not among the navigation rows (the brief: nothing is drawn that does nothing).
- The card leaves out the last assistant line, the pull request and Open in split view, and adds context use, as the brief says.
- A row keeps its second line also when the Core gives a reason for a task whose class is neither Needs attention nor Failed (for example waiting for capacity). The record names only the two classes.
- The grouping options read Project (the default, id `repository`), Status, Environment (`location`), Date (`time`), Project membership (`project`) and Folder path (`workspace`). The last two are PX-064's and PX-046's groupings, kept; the ids did not change.
- Search in the Tasks head opens the palette, as the row it replaces did. The list's own search over conversations is still under list options.
- Sorting, filters and stored views are unchanged (PX-046, PX-064); the filter toggle is now the word Filters, so the head's list options is the only funnel.
- Settings gained Density only (PX-198); task rows and grouping under Appearance are PX-208's.
- Inbox is renamed only; its two parts (MWS-13) are a later row.

## Limitations

- **No real task in the Failed class.** Nothing in the Core emits `TaskFailed` (the only references outside the domain crate are readers and a test that asserts its absence), so the class cannot be reached from the app. A task that stops on an error waits on the person: `list-task-stopped-on-an-error-is-needs-attention-light-comfortable.png` is the running app with a provider that answers 400, and the banner across the centre is the app's own, caused by that fixture. The Failed row is seen in the fixture gallery only (`fixture-gallery-list-every-class-incl-failed-*.png`) and in `gallery.spec.ts`; the Waiting, Completed and Archived classes likewise. This is not proof of the Failed row against a Core.
- **Found, not fixed here: the Core's reason can reach a row late.** The app re-reads the Core's attention view 150 ms after the last task event, and every event restarts that wait (`renderer/state/use-app.ts`, the same as on main; this change does not touch it). While other tasks keep emitting events the read does not happen, and a row that needs the person shows the task's own next action ("Approve or deny the pending effect") in place of the Core's reason. That is read from the code and was seen once: in one whole-suite run the new spec read the row in that window and failed. The spec now waits for the Core's words before it measures, as `supervision.spec.ts` does. The row is not blank in that window and keeps its second line; how long the window can last under a long stream was not measured.
- **Found by CI on Windows and Linux, corrected in the spec.** The first CI run (43030968493 on a8112ba1) passed every other desktop test on Windows (193 passed, 6 skipped) and Ubuntu (197 passed, 2 skipped) and failed one, this row's own `task-list.spec.ts`, on both: at 900 by 600 the six rows overflow the list by a few pixels, those systems draw a scrollbar 16 wide that takes room (macOS overlays it), and the spec measured the rows' right inset from the list's outer edge (24, not 8). The rows are 8 from the area they are laid out in, which is what the spec now measures; it also makes that condition on every platform (a short list and a bar 16 wide) so the case is no longer seen only on two of the three. No product code changed for this. The list's scrollbar on Windows and Linux is the system's, unstyled; it was not looked at by eye.
- The card's Context line is unit- and gallery-proven only, for the reason above.
- The bridge log reads Electron's internal handler table (`ipcMain._invokeHandlers`) from the test; the test throws if it is absent, and proves the log records by finding the transcript read when a task is opened.
- The preview card is placed with CSS anchor positioning; it was looked at beside rows near the top and middle of the list, not at the foot of a long list.
- The proof is local (macOS arm64) until CI has run on the pull request; the suite's result and CI are recorded in evidence.json.
- The owner has not yet seen these screenshots beside the boards, which the record makes an exit criterion. The row is not COMPLETE and no graph status was set.

## Closure (2026-11-12)

- CI run 43088097664 on head 621ee37f: all 19 checks passed (three single-job reruns across two heads, recorded in the pull request). Merged to main as 0ff0d38b (moss101/modbit#198).
- The owner reviewed the screenshots of the running app beside the boards and answered "Yes, seal them" on 2026-11-12.
