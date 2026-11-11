---
id: DR-PX-2026-11-11-021
title: The desktop renderer adopts the measured workspace, with the shell's geometry and density taken from measurement, a details rail, a pane dock, split view and the pages in one language, and rows for the capabilities that had none
status: accepted
date: 2026-11-11
supersedes: scoped — DR-PX-2026-11-03-019 AWL-06's numbers for AFW-A03, AFW-A04, AFW-A05 and AFW-A07 and its surface for the conversation, AWL-07's 840 px cap and AWL-08's chip row and one-line status row; DR-PX-2026-11-04-020 AWS-01's arrangement of home, AWS-02's row heights and sections, AWS-03's details popover, AWS-04's 640 px column, AWS-05's two-row composer and AWS-06's one-app tab strip (AWS-07, AWS-08, AWS-09 and AWS-10 stay in force); doc 65's numbers for AFW-A03, AFW-A04, AFW-A05 and AFW-A07
approved_by: owner, chat instruction of 2026-11-11 ("the design would be not same", then "capture screenshots and read app bundle", then "Structure is right" on the sixteen boards, then "can you also close gaps as well that were found out and work autonomously to complete the build")
---

# DR-PX-2026-11-11-021 — The measured workspace

> **Accepted 2026-11-11** on the owner's chat instruction, after the owner rejected a first draft drawn from memory, chose measurement of the two reference products installed on their machine, approved the structure of the sixteen boards that followed and asked for the build and for the gaps the comparison found to be closed. The approval covers the geometry, density and structure of section 5, the capabilities of section 5.10 and their rows. It does **not** cover any relaxation of AFW-A12 (WCAG 2.2 AA), AFW-A13 (themes) or AFW-A14 (reduced motion), or any change to the clean-room posture of doc 65 section 2: layout, proportions, density and behaviour only, never a reference product's colours, theme file, icons, fonts, logo, brand colour or strings.

## 1. Trigger and evidence

On 2026-11-11 the owner said the desktop "seems very complex and rigid" and asked for a design like three named products. A first draft was drawn from memory of how one of them used to look (warm surfaces, serif prose, floating cards). The owner answered "it does not have design system like claude desktop (version 4.23118.1) devin (3.12.43) - frontend is too obslete ( features are missing as well )" and then "the design would be not same". Both statements were right: a window capture of the two installed products showed neutral grey surfaces, one sans face and edge-to-edge panes with hairlines, close to each other and unlike the draft.

Asked how the reference should be obtained, the owner chose "capture screenshots and read app bundle". Window captures by window id and a static read of the two installed bundles followed (section 4). Sixteen boards were then drawn on the measured shell in Modbit's own tokens; the owner answered "Structure is right" and then asked for the build and for the gaps to be closed.

What the screens of DR-PX-2026-11-04-020 show against the measurement, read from that record's own evidence (`evidence/m10/PX-195..197/after`): an approval that is a form of five controls with two native selects; the intent hash, the tool name, the generation and the economics in the open; one type size for the transcript, the rows and the labels; a right panel that shows one app at a time behind five tabs; two sections for one thing (Projects and Repositories); a home that is a path field and a checkbox.

Three read-only maps were made before this record was written and are the evidence for its scope: the dossier process for a record of this kind; the renderer's composition, state, styling and gates; and a trace of twenty capabilities through the protocol, the Core, the desktop bridge and the renderer. The trace corrected a first comparison that had been made by searching names: the side question already works from end to end (`AskSideQuestion`, `side.rs`, the composer's panel); the plan's steps already have state in the Core (`GetWorkGraph`), with no bridge to the desktop; pins exist on this machine only. Capabilities with nothing behind them in the Core are listed in section 5.10.

Retained with this record: `evidence/dossier-px-020/boards/` (the sources of the sixteen boards, with the stylesheet that carries every size) and `evidence/dossier-px-020/coverage-2026-11-11.pdf` (the owner's export of the comparison before the trace corrected it). The captures and the two geometry notes are not retained in the repository: the captures show the owner's other sessions and account, and `research/` is git-ignored.

## 2. Goal and exit criteria

**Goal.** The desktop reads and behaves as the measured shell does, in Modbit's own colours, icons and words, and every capability the comparison names is built, owned by a row that already exists, or deferred by name.

**Exit criteria.** Rows PX-198 to PX-218 are COMPLETE with the evidence their cards name. The pins on the superseded geometry are replaced, not deleted (section 6). No screen this record replaces remains in the build. For each row that changes a screen, real screenshots of the running app beside its board are in the row's evidence with every visible difference listed, and the owner has seen them before the row is sealed.

## 3. Options

- **A. Keep the screens of DR-PX-2026-11-04-020 and restyle them.** Rejected: it is what DR-PX-2026-11-03-019 did, and the owner rejected that result on 2026-11-04 as "no parity" with "old app is not deleted".
- **B. Reproduce a reference product's look.** Rejected: doc 65 section 2 and docs 02 and 03 allow mechanism-level inspiration only, and both products' terms restrict copying.
- **C. The measured structure in Modbit's own tokens (chosen).** Geometry, density and behaviour follow the measurement; every colour input of DR-PX-2026-11-03-019 (AWL-01 to AWL-04) stays; icons stay the Lucide set of DR-PX-2026-10-24-016.

## 4. Provenance and licence

Two products installed on the owner's machine were measured on 2026-11-11 at the owner's instruction: window captures made by window id, and a static read of each installed bundle's shipped style sheets and scripts. Nothing was launched, modified or executed from either bundle. Only dimensions, structure and behaviour were recorded (bar heights, row heights, column widths, control sizes, pane rules, shortcut maps): no colour value, font name, icon, image, string or source text was recorded or copied, and the extracted working copies were deleted the same day. Both vendors' terms restrict reverse engineering; the owner chose the static read knowing that, as on 2026-10-03 for the first reference product. Feature inventories came from the vendors' public documentation and release notes.

The boards are original work. They use an open icon font and a webfont as stand-ins; the build uses the Lucide set and the system font stack the renderer already ships.

## 5. Specification

Rule ids are `MWS-NN`. Numbers are CSS px at zoom 1 in the comfortable density.

### 5.1 Shell geometry and surfaces (MWS-01, MWS-02)

| AFW row | After this record (MWS-01) | Before |
|---|---|---|
| AFW-A03 | task list 288 px default, 200 to 420 | 260, 210 to 400 |
| AFW-A04 | the top bar and every pane head 45 px, a 1 px stroke 8 % rule under each | 40 |
| AFW-A05 | navigation rows and task rows 32 px on one line, 2 px apart, radius 8, inset 8 | navigation 28, task rows two lines at 50 |
| AFW-A07 | composer radius 14, one line that grows to 40 % of the window's height, cap 768 | radius 12, about 96 px in two rows, cap 640 |

The conversation's column is 768 px with its text inset to 736. The details rail is 320 px, never under 280, and is hidden when the task area is 800 px or narrower. A pane is never under 280 by 100 px. The centre's minimum, the single-pane threshold and the window's minimum size keep their numbers.

**MWS-02.** The colour inputs and the role map of DR-PX-2026-11-03-019 stay as they are. One mapping changes: the conversation, the top bar, the details rail and the panes sit on `editor`; the task list stays on `sidebar`; `chrome` is the fill for the person's message, chips, the home tray and sunken fields. Panes stay unframed with 1 px stroke 8 % edges.

### 5.2 Density (MWS-03)

One setting with two values, stored with the layout preferences and separate from zoom. **Comfortable** (the default): chrome text 14 px, transcript 15 px on 21, the numbers of section 5.1. **Compact**: chrome text 13 px on 18, rows 30 px, bars 40 px, transcript 14 px on 20. The conversation's own density (compact, balanced, detailed) is a different thing and keeps its meaning. Every size comes from a token; a component sets no size of its own.

### 5.3 Task list (MWS-04)

Top to bottom: the head (list toggle, back, forward; the window controls on macOS); navigation rows New task, Inbox with its count, Automations, Review and Customize; the Tasks head with search, list options and more; the state views of AWS-10 item 4, unchanged; the groups; the person's row with the Core's state, help and Settings.

Groups are projects by default; status, environment and date are list options, as are sorting, filters and stored views (PX-046 and PX-064 own their behaviour). A task row is one line: the title, which fades at its end where it does not fit, then at the right the state glyph, the number of changed files when a task is ready for review, and the unread dot. A row whose task needs the person or has failed keeps its second line with the Core's reason (AWS-10 item 2); Row details can turn second lines on for every row or off. Child tasks indent under their parent. Pin and archive show on hover and on keyboard focus. The Search row moves into the Tasks head; the Approvals row is named Inbox; Repositories is no longer a section, because a project group's own menu starts a task there.

Resting on a row, or pressing Space on it, opens a preview card: the state, the last line the assistant wrote, the pull request and its checks where the task has one, the size of the change, Open and Open in split view. The card reads the task's header; it opens no transcript.

### 5.4 Task view (MWS-05 to MWS-08)

**MWS-05, title bar.** The title, which opens the task's menu (rename, pin, archive, open in split view), and at the right one toggle per pane the task can show, each with its state, then More. The state pill and the branch move to the details rail.

**MWS-06, transcript.** Text at the density's size. A step or fold stays one line read from the Core's facts (AWS-04's wording rules stay). The plan is a card of steps with their state, read from the Core's work graph through a new bridge method, in place of repeated "Updated the plan" lines. The line under the last block says what is running, for how long and the tokens used; AWS-10 item 1 decides when a notice stands above the composer.

**MWS-07, composer.** One box: add context, the text, then stop or send. Under the box, at the right: the model, the run mode and the context ring, each opening the menu it opens today. Queued input sits in a tray above the box with Send now and Remove (PX-055 owns its behaviour). The side question stays where it is.

**MWS-08, approval.** One docked card above the composer, as AWS-04 requires: what it will do in words, the exact command, why it asks; then Skip, Always allow and Allow once. Always allow opens one menu holding the prefix and the scope that were two selects. The intent hash and the exact arguments sit behind one disclosure and stay in the accessibility tree. Enter, Escape and the confirmation rule of AWS-04 and AWS-10 are unchanged. An irreversible effect still asks twice.

### 5.5 Details rail (MWS-09)

The rail replaces the details popover of AWS-03 and holds what it held: Environment (location, folder, branch or worktree, run mode); Approvals (waiting, saved rules); Context (use, with its tray); Tool servers; Changes (size, Review); Evidence (latest verification, checkpoints with restore); Background tasks (child tasks and terminals with Stop); Cost (spend against budget, model calls). State, phase, agents, generation, attachments, risk, security and next action stay in the accessibility tree and behind Details in the rail's head. The rail reads the Core's facts after a reload, as AWS-10 fixed for the popover.

### 5.6 Pane dock, pop-out and split view (MWS-10, MWS-11)

**MWS-10.** The apps panel becomes a dock: any of the task's panes can be open together, tiled in a grid of at most two columns; each pane has a 45 px head with its name, its own actions, Minimize, Pop out and Close; open panes and their sizes are remembered per task. With panes open the rail gives way. Panes: Changes, Terminal, Browser, Files and Evidence (PX-048 owns their content), and three new ones: Activity (the task's events in order, searchable and filterable by kind), Background tasks, and Pull request (the request, its checks, people's comments). Pop out moves a pane to a window of its own with the same preload and content security policy; closing that window returns the pane to the dock.

**MWS-11.** Two tasks can sit side by side in the centre, each with its own title bar, transcript, composer and approval card; one has focus and shows it; the keyboard acts on the focused one. Alt+click on a row, the preview card and the palette open a task in split view.

### 5.7 Home (MWS-12)

The product's name, then the composer in a tray: the text; a bar with add context, mode, model and send; and, attached under it, where the task runs: location (the selector of AWS-10 item 6, cloud disabled with its reason until PX-128), folder, branch, own worktree and run mode. An issue URL is entered from add context. Send, Alt+Enter and the refusal rule of AWS-01 and AWS-10 item 3 are unchanged. Under the tray: what is waiting on the person, each decidable in place, except that a destructive effect is never allowed from a list and opens its task; then the provider and trust steps while they are incomplete, and starter tasks after.

### 5.8 Inbox (MWS-13)

The Approvals page becomes Inbox with two parts. **Needs you**: every pending approval across tasks as the card of MWS-08, mode proposals, and questions. **Updates**: ready for review, a finished child task, an automation's run, a failure, each with its time, a read state and a link that opens its task. Updates are a persisted record kept by the desktop's main process, capped and cleared by the person; an operating-system notification opens the same link.

### 5.9 Pages and search (MWS-14, MWS-15)

**MWS-14.** Automations, Customize, Review and Settings use one page shape: a 45 px head with the page's name and its one primary action, an optional section list at the left, rows of at least 44 px in lists with a 1 px edge. Automations shows its list with a details rail for the chosen one. Customize has one search across its sections and gains Rules, a read-only list of the instruction layers the Core loaded with Open source for each (PX-093 and PX-106 own the rest). Review lists the tasks' pull requests and local changes not yet in one, and beside them the findings and people's comments for the chosen one. Settings keeps AWS-08 and gains, under Appearance, density, task rows and grouping, and a Shortcuts section that lists the bindings the app really has.

**MWS-15.** The command palette searches tasks, conversation text, files of the open task's folder and commands in one list with a heading per kind.

### 5.10 Capabilities that had no row (MWS-16 to MWS-20)

| ID | Capability | Rule |
|---|---|---|
| MWS-16 | Next-prompt suggestion | When a turn ends and nothing is queued, the Core may produce one suggested next message with a bounded model call that writes nothing to the task's log. It shows as placeholder text; Tab accepts it into the box; nothing is ever sent by itself. It is charged to the task's budget, respects its limits and can be turned off. |
| MWS-17 | Pull request state, auto-fix and merge when ready | The Core records the pull request a task opened and refreshes its state and checks through the forge adapter; the task's header carries the number and state. Auto-fix is a switch per task: a failing check or a comment addressed to the agent queues a repair turn inside the task's run mode and budgets. Merge when ready is a request the person makes: a `forge.pr.merge` effect, approval-bound like every external effect, asked once the checks pass. Archive after merge is a preference. |
| MWS-18 | Reviewer findings | The review bundle carries the isolated reviewer's findings one by one (severity, path, line, claim, suggested action) and a command asks the agent to fix one of them. |
| MWS-19 | Standing access | Computer control gains a level per application (view only, click, full, never) held by the Core, which can only narrow what a task's grant allows. The browser pane's site decisions gain Allow once and Deny beside Always allow. |
| MWS-20 | Compact now, and the header's last line | A person can ask for a compaction of the open task. The task's header carries the last line the assistant wrote, for the preview card. |

### 5.11 What existing rows keep

This record moves and resizes surfaces whose behaviour other rows own; it does not restate them. The agent list's classes, pins, archive, filters, grouping and search: PX-046 and PX-064. Streaming, tail status and folding: PX-047. The panes' content: PX-048. Queue, stop, side question and the terminals tray: PX-055. The model chip: PX-056. The approval stack: PX-058. The computer-control surface: PX-074. Tool-server trust and connection state: PX-090. The Customize tabs: PX-093 and PX-106. Forge read, webhooks and their Review surface: PX-125, PX-126, PX-127. Cloud from the desktop: PX-128. Where a board shows a control whose behaviour belongs to a row that is not built, the control is absent or disabled with its reason. Nothing is drawn that does nothing.

### 5.12 Not adopted

Any colour, font, icon, logo or string of a reference product; their floating or translucent chrome (PX-094 owns native chrome); a voice input; pins held by the Core; a person creating a child task directly; an SSH execution environment (PX-219, deferred).

## 6. The gate

`apps/desktop/e2e/awl.spec.ts` keeps its token check and its recipe checks, and its geometry checks take the numbers of section 5.1 and 5.2 in both densities: the list at 288, bars at 45, rows at 32 on one line with a second line only where MWS-04 keeps it, the column at most 768, the composer on one line with the model, run mode and ring under it, panes tiled with heads of 45. Retained mutations show the checks failing on a surface pushed back to the superseded numbers (list 260, bar 40, column 640, a two-line composer, one app at a time). The pins in `packages/design-tokens`, `renderer/shell/shell.test.ts`, `e2e/shell.spec.ts`, `e2e/composer.spec.ts` and `e2e/gallery.spec.ts` move to the new numbers in the same change that moves the surface.

A gate on measurements is not acceptance. DR-PX-2026-11-03-019 closed on one while the screens were wrong. Screenshots of the running app beside the boards, with the differences listed, are part of each row's exit criteria.

## 7. Migration

Rows land in the order of their prerequisites. A change that replaces a screen deletes what it replaces in the same change. Test ids stay wherever the control survives; the details popover's ids move to the rail. Stored layout preferences keep their keys: a stored list width is kept if it is inside the new range, a stored panel tab becomes the one open pane.

## 8. Compatibility

Rows PX-198 to PX-209 and PX-216 change no protocol message. PX-200 adds bridge methods over Core reads that already exist; PX-203 adds a second window; PX-206 adds a store in the main process; PX-216 adds decision kinds to the desktop's browser policy store. Rows PX-210 to PX-215, PX-217 and PX-218 add protocol messages and fields, all additive, with fixtures and the version negotiation the protocol already has.

## 9. Security impact

No effect path changes for rows PX-198 to PX-209: every effect still goes through the Capability Kernel and its approvals, the exact intent stays on the card, and a destructive effect cannot be allowed from Inbox or home. A popped-out pane runs under the same preload and content security policy. The suggestion call reads a snapshot, writes nothing and sends nothing. Merge and the pushes of auto-fix are external effects and ask as every external effect does; auto-fix cannot raise a task's run mode. A per-application level can only narrow a grant. The updates record holds titles and ids, never transcript text.

## 10. Test impact

An assertion moves with its fact. One is deleted only when its surface was removed on purpose, and each deletion is listed in the row's evidence. PX-198 is iteration tier: it moves sizes and one surface mapping. Every other row is release-critical (doc 83): the full desktop suite against a real Core on macOS, Linux and Windows, axe and the computed contrast test are their proof; rows that add protocol also run the Core's suites and the protocol fixtures.

## 11. Rollback

Each row is one change and can be reverted by itself in reverse order of the prerequisites. Preferences written by the new shell are ignored by the old one, which falls back to its defaults. Protocol additions are additive and are left in place by a revert of the renderer alone.

## 12. Risks and open items (not decided here)

- **Rows that cannot be sealed yet.** PX-211 to PX-213 depend on PX-125, PX-215 on PX-069 and PX-217 on PX-109, none of which is COMPLETE. They can be built; `graph.py` will not move them until those rows close.
- **Scope.** This record adds twenty-one rows to RELEASE_ZERO on the owner's instruction to close the gaps.
- **Shortcuts.** The board for Settings lists bindings taken from the measurement. The app's own map (`renderer/keyboard.ts`, `renderer/shell/commands.ts`) is the authority; PX-208 lists what exists and proposes nothing new.
- **One reference's task view was not captured.** Its geometry comes from the static read and its behaviour from public documentation.
- **Per-step plan state** exists in the Core's work graph; whether every plan the agent writes reaches it as steps is checked in PX-200's audit before the card is built.
- **A concurrent record.** DR-GOV-2026-11-11-009 withdraws PX-077 to PX-081 and changes the same pinned counts in `tools/test_dossier.py`; whichever lands second rebases its pins.
- The boards show a count beside each Customize section and an "Add" action; they are built only for sections whose row provides them.
