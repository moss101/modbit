---
id: DR-PX-2026-11-04-020
title: The desktop renderer rebuilds its screens as the approved agent workspace and removes the Fleet board
status: accepted
date: 2026-11-04
supersedes: scoped — DR-PX-2026-11-03-019 section 5.5 (the clause "Modbit keeps Fleet"), section 5.3 navigation rows, AWL-08's one-line status row and AWL-09's banner rule; doc 10 section "Home / Fleet" and doc 39's "Home / Fleet" screen as a board with columns (REQ-EV-0037 and REQ-EV-0143 stay and are met by the agent list and the Approvals page); doc 65's status row of AFW-A01 and AFW-D12, whose facts move as section 5 says
approved_by: owner, chat instruction of 2026-11-04 ("there is no parity with cursor frontend - seems like old app is not deleted", then "Yes, remove it" to removing the Fleet board entirely)
---

# DR-PX-2026-11-04-020 — The agent workspace screens

> **Accepted 2026-11-04** on the owner's chat instruction, after the owner opened the app merged under DR-PX-2026-11-03-019 and found that it did not match the mockup they had approved. The approval covers removing the Fleet board and rebuilding the desktop's screens to that mockup. It does **not** cover any relaxation of AFW-A12 (WCAG 2.2 AA), AFW-A13 (themes) or AFW-A14 (reduced motion), or any change to the clean-room posture of doc 65 section 2: layout, proportions and density only, never the reference product's colours, theme file, icons, fonts, logo, brand colour or strings.

## 1. Trigger and evidence

DR-PX-2026-11-03-019 changed tokens, geometry and control recipes and rearranged the navigation, and said in its section 5.5 that Modbit keeps Fleet. The mockup the owner approved has no board. Its gate measured tokens and geometry, so it passed while the old screens stayed. The retained captures (`evidence/m10/PX-194/after`) show the gap:

| Approved mockup | The app under DR-019 |
|---|---|
| A clean workspace | The Fleet home: welcome steps, a long New Task form, six status columns |
| Top bar: title, a pill, branch, overflow | Back arrow, a location pill, a state line, a languages banner and a toolbar of Undo all, Run mode and Density |
| Agent list: navigation, Projects, repositories as groups, repositories, the person's row | A search box, a group dropdown, filter icons, a flat list |
| Steps that say what happened (`Edited sign.rs +42 −6`), one approval card | "Worked for under a second" repeated; an inline approval and a docked one |
| Composer: text, then a bar of mode, model and send; location and policy under it | One line with a Queue button and a status row below |
| Panel: tabs in the top row, file list, numbered diff, commit bar at the foot | Tabs under a title, paragraphs of explanation, a form for rejecting, a large commit form |

The launch was also wrong: `pnpm start` ran `electron dist/main/main.cjs`, so `app.getAppPath()` was `dist/main` and the Core was looked for under `apps/desktop/target`.

## 2. Goal and exit criteria

The running app matches the approved mockup in layout and density, with the Fleet board's code removed rather than hidden, and every capability the board offered still reachable.

Exit criteria: section 5 holds on the real app against a real Core; the gate of section 6 fails when a removed surface returns or the layout drifts; the full desktop suite passes on macOS, Linux and Windows; the owner has seen real screenshots beside the mockup before merge.

## 3. Options

- **Restyle further and keep the board as the home.** Rejected: it is what the owner rejected.
- **Keep the board as a view reached from the command palette.** Offered to the owner; not chosen. It keeps an overview at the price of keeping the old surface alive.
- **Remove the board and move each capability to the surface the mockup gives it** (chosen by the owner).

## 4. Provenance and licence

The mockup is Modbit's own page, made to the layout doc 65 records, with no asset of the reference product shipped. Its colours are not used: the renderer keeps the tokens of DR-PX-2026-11-03-019 section 5.1. New Lucide shapes extend the table under the same licence (ISC, MIT for Feather-derived; `third-party/zcode-ui/LUCIDE-LICENSE`). "New task" stays Modbit's name.

## 5. Specification

### 5.1 Home (AWS-01)

With no task open and no page open the centre shows one composer in the middle of the window: the goal, the repository path, send; under it the location, the own-worktree option and an issue URL. **Send creates the task, starts it and opens its conversation; Alt+Enter creates it without starting it.** With no provider configured the task is created and not started, and the screen says so. Provider setup is above the composer while no provider exists; trusting a repository for starter tasks is folded under it. Fleet columns, the type-ahead filter box and the state line are gone.

### 5.2 Agent list (AWS-02)

Top to bottom: the 40 px head (list toggle, back, forward; on macOS the window controls sit in it, `titleBarStyle: hiddenInset`), navigation rows of 28 px (New task, Search, Automations, Approvals with its count, Customize), Projects with its `+` and list options, the repository groups with their tasks (two-line rows, 50 px), Repositories (a click starts a new task there), and the person's row: initials and the local account name, which open a menu (Dashboard, Worktrees, command palette, shortcuts, theme, zoom), the Core's state as a dot with its words in the accessibility tree, and the gear, which opens Settings. Search, grouping, filters and views fold behind list options and show by themselves while they narrow the list.

### 5.3 Top bar and task details (AWS-03)

The title, the task's state as a pill, the branch once the Changes app has read it, an overflow, and the apps-panel toggle while the panel is closed. No toolbar sits under it. The overflow opens the task's details: state, phase, agents, generation, attachments, risk, security, evidence, next action, the state line, subagents, context and economics for this task, density, undo all, browser and cancel.

### 5.4 Conversation (AWS-04) and composer (AWS-05)

One 640 px column; the composer is about 96 px high with its two rows. A plain task message has no label; an answer, a queued input and text from outside say what they are. A step or fold is one line with an icon and what it did, read from the Core's facts for the calls (`Explored 2 files`, `Edited notes.txt +2 −1`, `Ran git status`); the Core's own label stays in its title and the status words in the accessibility tree. A failure, a denial and a running call show their words. The unread divider marks only what was new when the conversation was opened.

**A pending approval has one decision surface:** the docked card, the width of the composer, on the editor surface with the 3:1 edge: what it does, the exact command, why it asks, the intent hash, Skip, the allow prefix and scope, Always, Run. The transcript records it without buttons. A question stays inline. A pending keyboard confirmation owns Enter and Escape so that a docked card behind it cannot take them.

The composer is the text, then one bar: mode (always shown), model, add context, the context ring, stop, a round send. Under the box: where the task runs and the run-mode control. Suggestion chips are gone; the mode menu replaces them.

### 5.5 Apps panel (AWS-06)

Tabs with icons in the panel's own 40 px top row, with widen and close at its end; the title is not repeated. Changes: a 40 px head (summary, refresh, Review, Stage all), the file list (status letter, path, line counts; stage and revert on hover), the diff with old and new line numbers and tinted lines, hunk actions on hover, and a one-line commit bar fixed at the foot. Explanatory paragraphs stay in the accessibility tree. The default panel ratio is 0.39.

### 5.6 Where the board's facts and controls went (AWS-07)

| Was on the board | Now |
|---|---|
| Task state, class | Row `data-state`, its glyph and words; the top bar's pill |
| Phase, agents, risk, evidence, attachments, security, state line, subagents | Task details |
| Approve, deny | The docked card; `y` and `n` keep working with the confirmation |
| Answer a question | Inline in the conversation |
| Steer | The composer |
| Start, resume, review | The composer's chips; send starts |
| Browser, cancel | Task details; `c` keeps working |
| Attention items, notifications | The Approvals page; its count in the list |
| Filter box | The command palette; `/` opens it |
| Settings, languages, platform statement | The Settings page |
| Core status | The person's row |

Keyboard: `j`, `k` and the arrows move over rows; `a` opens Approvals on the first item; `r` goes to a running row. Column keys are gone.

### 5.7 Settings (AWS-08; added 2026-11-05 after the owner reported "settings are missing")

The first version of this record left Settings as the old form under a menu, with no way to change a provider. That was wrong against AFW-A10 and doc 10, neither of which had been read for it. Settings now replaces the left column: Back, a search field, grouped entries and the pinned person's row; the content is one centred column of at most 680 px, rows of at least 44 px, switches of 32 x 18 px. The sections are doc 10's list (models and providers with execution policy, execution locations, permissions, login credentials, browser permissions, memory, account and cloud) and then the app's own (appearance, notifications, automations and worktrees, this build). What belongs to one task stays with the task and the page says so.

### 5.8 Execution location: what is true (AWS-09; added 2026-11-05)

The product is local and isolated cloud on one Core (doc 01, doc 10, M8). **The desktop today is local only.** It has no cloud client: PX-128 owns "Continue in Cloud", its cloud-sessions list and sign-in with PX-129, and neither is delivered by this record. So this record does not satisfy AFW-B09's location pill or AFW-D12's location selector as selectors; it shows where a task runs as text read from the Core's header (`executionLocation`, never hard-coded on a conversation), and Settings says in words that cloud is not available from this app yet and names the CLI's `modbit cloud handoff`. Nothing is drawn that does nothing.

## 6. The gate

`apps/desktop/e2e/awl.spec.ts` keeps the token, geometry and recipe checks and adds `homeDrift` and `workspaceDrift`: no removed surface is present, the composer sits centred, the list reads in order, the conversation is one column at most 640 wide, the composer's bar is under its text with a round send, the location is under the box, steps say what they did, the panel's tabs are in the top row and the commit bar at its foot. Retained mutations show the checks failing on a surface pushed back to the old layout.

A gate on measurements alone is what let DR-019 close while the screens were wrong. The owner sees real screenshots beside the mockup before merge; that is part of this record's exit criteria and is not replaced by the gate.

## 7. Migration

No data migrates; layout preferences keep their keys. The panel ratio default changes for people with no stored ratio. Specs move to `e2e/support/new-task.ts`; test ids stay wherever the control survives.

## 8. Compatibility

No protocol, persistence, policy or preload change. The main process gains the macOS window style and the account name in `core:localState`. Send starting the task is the one change to when work begins.

## 9. Security impact

Send starting a run does not widen authority: every effect still goes through the Capability Kernel and its approvals, and a repository is still trusted explicitly and only for its own root. The approval's exact intent stays on the card. The account name is local display only and is not sent anywhere.

## 10. Test impact

The specs that drove the board now drive the workspace. An assertion moves with its fact; it is deleted only when its surface was removed on purpose, and each such deletion is listed in the evidence. Because send now starts work and the approval surface moved, the rows are release-critical (doc 83): the full suite on all three operating systems is their proof.

## 11. Rollback

Revert the merge. The tokens and the graph entries of DR-019 are untouched by a revert of the renderer alone.

## 12. Risks and open items (not decided here)

- **How this record was prepared.** It was written after the rebuild and without following the read order of `AGENTS.md` and doc 01: doc 10's Settings list, AFW-A10, AFW-B09 and AFW-D12 were not read first, and the agents that migrated the specs were given a migration guide, not the specifications. Sections 5.7 and 5.8 are the corrections found so far. A full comparison of the app with docs 10, 32, 39 and 65 has not been done.
- Doc 10 lists more that the desktop does not show: objective profile on a task, Agents and agent profiles, Workspaces with engineering memory, and an Artifacts area. The composer's model menu holds the objective; the others are not addressed here.

- The Review still opens by itself on `TaskReadyForReview` (docs/39 step 5). The mockup does not show this; it was not asked to change.
- Tasks of different turns are not merged into one `Explored` row: the Core groups by turn.
- The `+` beside the panel's tabs and `Create PR` in the commit bar of the mockup are not built: the tabs are a fixed typed set (Files is a fifth), and opening a pull request stays in the Review. Nothing is drawn that does nothing.
- Subagents keep their own marked rows in the list.
- The top bar's branch appears only after the Changes app has read it, because `changesState` is the one Core read of the branch and `review::candidate` behind it runs `git add -N` on untracked files. A read that changes the index is a bug of its own (a real `git clean -fd` then leaves those files), older than this record and not fixed here.
