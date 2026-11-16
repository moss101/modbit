# Task Card — PX-205 Measured home: the composer in a tray with where the task runs attached, what is waiting on the person, and the first-run steps

## Identity

- Task ID: PX-205 (REQ-PX-205; owner desktop); qualification QUAL-PX-205 / PX-E2E-205; Decision Record DR-PX-2026-11-11-021 (MWS-12, sections 5.1, 5.11 and 9; AFW-A12, A13, A14 unchanged); DR-PX-2026-11-04-020 AWS-01 and AWS-07..10 stay in force
- Evidence tier: release-critical (send starts a run, and approvals become decidable on home)
- Prerequisites: PX-199, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-home`, cut from origin/main 9da82128 (PX-198..204), with origin/main 978fe135 (the PX-204 seal) merged in.

## The rules implemented (quoted from the record)

- MWS-12: "The product's name, then the composer in a tray: the text; a bar with add context, mode, model and send; and, attached under it, where the task runs: location (the selector of AWS-10 item 6, cloud disabled with its reason until PX-128), folder, branch, own worktree and run mode. An issue URL is entered from add context. Send, Alt+Enter and the refusal rule of AWS-01 and AWS-10 item 3 are unchanged. Under the tray: what is waiting on the person, each decidable in place, except that a destructive effect is never allowed from a list and opens its task; then the provider and trust steps while they are incomplete, and starter tasks after."
- Section 9: "a destructive effect cannot be allowed from Inbox or home."
- QUAL-PX-205: send creates, starts and opens the task; Alt+Enter creates it without starting; the first model request is on the mode and model chosen on home; the location selector shows cloud disabled with its reason; a protected write is decided from the waiting list and a destructive effect opens its task. Fails if a destructive effect is allowed from home, cloud is enabled while PX-128 is not COMPLETE, the first model request is off the chosen pin, or a control does nothing.
- Section 5.11 / AWS-07: "Nothing is drawn that does nothing."

## Existing-code audit (before any edit)

- classification: IMPLEMENTED-PARTIAL. What was PRODUCTION-WORKING and was kept: send creates, starts and opens; Alt+Enter creates and opens without starting; the mode and the pin are sent after `CreateTask` and before `StartTask`, and a refusal leaves the task unstarted (`state/use-app.ts` `submit`, proven by `supervision.spec.ts`); the location menu with cloud disabled and its reason (`location.ts` `CLOUD_UNAVAILABLE`); the first-run steps' working controls (provider form with its live test call, repository trust, starter tasks: `home/welcome.tsx`, `onboarding.spec.ts`); the approval card's decision (`approvals/approval-dock.tsx`, `resolveApproval` bound to the intent hash); `isIrreversible` and the two-step confirm on the card.
- What was not there: no product name and no tray (home was the AWS-01 box: text, a bar holding mode, model, a repository path field and send, then a foot with location, own worktree and an issue field); no run mode on home (a new task always started Supervised; the run mode could only be changed in the task); nothing on home said what was waiting on the person (the Inbox page listed attention items and decided nothing); the provider setup sat above the composer; the issue field was always drawn.
- first missing link: the session's pending approvals could only be read for an open task (`useDockApprovals` returned nothing without a task id), and the only decision path lived inside `ApprovalDock`; so a list on home had no data and no shared decision path with a rule of its own for what a list may not decide.

## What was built

- **The tray (`home/home.tsx`, `styles.ts`).** The product's name, then a tray on the chrome fill (`--mb-surface-fill`, MWS-02) whose radius is the composer's 14 plus its 6 of padding, holding the composer box (radius `--mb-composer-radius`) and, attached under it, the environment strip. The column is the conversation's `--mb-column` (768). The box: the text (three body lines high), then the bar: add context, the mode, then at the right the model and send. The strip: location, folder, own worktree (a switch), then at the right the run mode. Every size is a token or a step from one; no font size is written as px (`measured-css.test.ts` still passes).
- **Add context.** A menu holding "From a GitHub issue": the issue field appears under the text, focused, with a remove control. The field's test id (`issue-url`) and its meaning are unchanged; it is no longer drawn until asked for.
- **Run mode on home.** A menu of the five presets in the Core's order (`RUN_MODES`), Supervised (the Core's default) chosen. A preset that approves more opens the acknowledgement dialog the task's run-mode panel uses: `RunModeWarning` was extracted from `approvals/run-mode.tsx` so both use one dialog (same test ids). The choice is held like the mode and the pin and sent with `setRunMode(..., acknowledged)` after `CreateTask` and before `StartTask`; a refusal leaves the task unstarted with the reason (the existing rule). Nothing carries over to the next task.
- **Waiting on you (`home/waiting.tsx`).** Under the tray: every pending approval of the session as the Core lists it (`useDockApprovals` now lists the whole session when no task is open; expiries stay on the task's card), oldest first, each with its task's title and what it wants in words ("Wants to run git remote add origin1 …"); then any other attention item the Core lists (a question, a stall, a failure …) in plain words with Open. An approval a list may decide has Skip and Allow once; one it may not (`decidableFromList`: an effect that cannot be undone, native control, a budget raise) has "Cannot be undone" where that is why, its reason in the accessibility tree and on the button's title, and Open to decide, which opens its task.
- **One decision path (`approvals/decide.ts`).** `resolveShown` is the one call the card and the list make (bound to the intent hash shown; the Core refuses any other). `decideFromList` checks `decidableFromList` first and throws `NOT_FROM_A_LIST` before anything reaches the bridge, so a destructive effect cannot be decided from home whatever asks for it; the task's card keeps its own two steps. `ApprovalDock` now calls `resolveShown`.
- **Get started (`home/welcome.tsx`, `home/model.ts`).** Under the waiting list, while a step is incomplete (`onboarding`, unchanged): a head with "N of 3" and a meter, then three steps with a ring that fills with a check when done: Connect a model provider (the provider form and its live test call), Trust a repository (the trust form, then the Core's starter tasks), Start your first task (puts the cursor in the box). A step is done when the Core or the app says so (`stepProgress`), never by clicking it. Test ids kept: `welcome`, `welcome-step-*`, `provider-*`, `welcome-repo-toggle`, `trust-*`, `repository-trusted`, `starter-tasks`, `starter-task`.

No preload, main-process, protocol, CSP or Rust change (the preload and main bundles hash as PX-204's). No locked file changed.

## Left out, and why

- **Branch on the strip** (MWS-12 names it). Nothing reads a folder's branch before a task exists without a new bridge method, and the card says no preload change; a task cannot be created on another branch either. Absent, not drawn as a control that does nothing. The task's branch is in its details rail (PX-201).
- **The board's Cloud | This Mac switch in the head.** The location menu in the strip is AWS-10 item 6's selector; a second control for the same choice was not drawn.
- **The board's "/ skills" button and "Schedule an automation" step, and the steps' Dismiss.** Slash commands belong to a task's composer; an automation is not a first-run need and nothing proves it is one; the steps leave by themselves when done.
- **Questions and mode proposals answered in place.** Home opens their task; answering across tasks is Inbox's (PX-206).
- **A keyboard decision on home.** Skip and Allow once are buttons (Tab and Enter); the card's Enter/Escape scope stays in the task.

## Verification

Unit: `@modbit/desktop` 430 of 430 (424 before, +6 in `home/home.test.ts`); `@modbit/ui` 11 of 11; `pnpm typecheck` (src and e2e) clean.

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a…), scripted model: `e2e/px-205-home.spec.ts`, 2 of 2, about 10 s each, run several times locally on the final build. The scripted model records every request body as the Core sent it and answers each task from its own script.

- **First run** (fresh profile, no provider, no trusted folder, 1440 x 900): `trayDrift` (the name above the tray; the tray at most 768 and centred; the composer and the strip inside it, the strip under the composer; the composer's radius 14 and the tray's larger; the bar's order add context, mode, model, send on one line with send at the end; the strip's order location, folder, own worktree, run mode with the run mode at the end; waiting and the steps under the tray); the steps show 0 of 3 with the provider step active; no waiting list; `inertControls` empty; axe clean. Then: the provider's live test call (1 of 3); a folder trusted through the Core, its starter tasks, the folder carried into the box (2 of 3); Start your first task focuses the box. The location menu: this machine chosen; cloud `aria-disabled` with "not available from this app yet", and the graph read from `graph/project-graph.json` says PX-128 is not COMPLETE; a click on cloud changes nothing; axe clean with the menu open. Add context opens the issue field focused; remove takes it away. Own worktree is disabled without a folder and says why; with one it switches on and off. The run mode: Supervised; Supervised with sandbox opens the acknowledgement with its confirm disabled; Cancel keeps Supervised; ticked and confirmed it reads Supervised with sandbox. Mode Plan, pin gpt-4.1. **Send**: the conversation opens; the first request the model received for the task is on `gpt-4.1`; the Core's posture is PLAN with the pin `openai/gpt-4.1`; its run mode is `ALLOWLIST_SANDBOX`; that first request offers no writing tool (`change.apply`, `shell.exec`, `git.commit`, `git.push`), while the second test shows an Agent task's first request does offer them. Back on home: Agent, Auto, Supervised; the steps are gone. **Alt+Enter**: a task opens, Queued, with Start; no request carries its goal.
- **Waiting** (provider set, a real repository with an untracked file): two tasks sent from home, one asking to run `git remote add origin1 …`, one `git clean -fd` (the Core's class: Destructive). Home lists both under the tray (`trayDrift` holds), each with its title and its exact command in words, the list bound to the intent hash the Core gave; the list holds no tool name, effect class or Core command; the destructive row has no Skip or Allow once, "Cannot be undone" and Open to decide (`destructiveDecidedFromHome` empty); `inertControls` empty; axe clean. **Open to decide** opens the destructive task's conversation; its card (Destructive) asks twice: Allow once shows "This effect cannot be undone. Run it?" with Back and Confirm run; Back; the Core still lists it. **Allow once** on the protected write from home: the note says so, the Core no longer lists it, and the repository's remotes include `origin1` (the real effect). The destructive effect is still pending, and the untracked file is still there.

Whole desktop suite, local, macOS arm64, once (see `desktop-e2e-full-results.txt`): 213 passed, 2 failed, 3 skipped (the two live-model specs and the Windows-only one). Both failures were caused by this change: `keyboard.spec.ts:80` (the home tab order, moved with the surface, see below) and `worktrees.spec.ts:64` (the own-worktree switch's input was 1 px under its track, so a click on the input was taken by the track; the input now covers the switch). After the fixes, on the final build: `keyboard.spec.ts` 1 of 1, `worktrees.spec.ts` 4 of 4, `px-205-home.spec.ts` 2 of 2. The suite was not run whole a second time. Before the suite, `shell.spec.ts:25` failed once because this worktree had no `target/debug/modbit-core` (that spec drops `MODBIT_CORE_BIN` on purpose); with the prebuilt Core linked there (git-ignored) it passed.

## Retained mutations (each asserted to fail)

- **A destructive effect allowed from home**: in the spec, the destructive approval is resolved through the bridge as a list that allowed it would; `destructiveDecidedFromHome` reports "the destructive effect is no longer pending: it was decided while home showed it". In `home.test.ts`, `decideFromList` on a Destructive effect (approve and skip) and on native control is refused with `NOT_FROM_A_LIST` and nothing reaches the bridge.
- **Cloud enabled while PX-128 is not COMPLETE**: the cloud entry's `aria-disabled` set to false in the open menu; `cloudDrift` reports "cloud can be chosen while PX-128 is not COMPLETE" (asserted only while the graph says PX-128 is not COMPLETE).
- **A first model request off the chosen pin**: gpt-4.1 chosen on home, the task kept with Alt+Enter, its pin moved through the bridge to another model the Core allows, then started; `offPin` reports the first request "is on <other>, not on the chosen gpt-4.1".
- **A control that does nothing**: a "Branch" button added to the strip; `inertControls` reports "control home-branch is drawn and nothing proves what it does". A disabled control without a reason is reported too (the census reads the title, its label's title or `aria-describedby`).
- **The strip above the composer**: `.home-tray` reversed; `trayDrift` reports "the environment strip is not under the composer"; removed, it holds again.

## Test ids

- New: `home-brand`, `home-tray`, `home-composer`, `home-bar`, `home-add-context`, `menu-item-home-add-issue`, `home-issue`, `home-issue-remove`, `home-env`, `home-folder`, `home-runmode`, `home-runmode-label`, `menu-item-home-runmode-<mode>`, `home-waiting`, `home-waiting-item` (`data-kind`, `data-task-id`, `data-approval-id`, `data-intent-hash`, `data-effect-class`, `data-decidable`), `home-waiting-line`, `home-waiting-why`, `home-waiting-skip`, `home-waiting-allow`, `home-waiting-open`, `home-waiting-note`, `welcome-progress`, `welcome-step-first-task`, `welcome-write-task`.
- Kept: `home`, `goal`, `workspace`, `isolate`, `issue-url`, `run`, `home-mode`, `home-mode-button`, `home-location`, `home-model-note`, `composer-no-provider`, `composer-state`, every `welcome-*`, `provider-*`, `trust-*`, `starter-*` id, and the run-mode warning's ids (now one component).
- None removed.

## Assertions deleted, replaced or changed

- None deleted, none weakened.
- `e2e/issue-intake.spec.ts:118`: the issue field is no longer drawn until asked for (MWS-12: "An issue URL is entered from add context"), so the spec opens it from add context before filling it. The assertions are unchanged.
- `e2e/keyboard.spec.ts:116`: the home tab order from the goal is now add context, the mode, the model, the location, the folder (was the mode, the model, the repository), and five Shift+Tab return to the goal (was three). Same fact: every control on home is reached by the keyboard in reading order.
- `measured-css.test.ts` is unchanged; the home list's rule was written with `background` before `overflow` so the retained mutation of `.shell-frame` still finds the shell frame's text first.

## Differences from the board (A1) and the record

- The board's column is 700; here the tray takes the conversation's column, `--mb-column` (768), as the task asked.
- Tray radius: the board has 22 (comfortable) and 26 (the Devin read); here 20 in both densities, the composer's 14 plus the tray's 6, so the curves are concentric.
- The board's head has a Cloud | This Mac switch; not drawn (one location control, in the strip).
- The board's strip has a branch chip; not drawn (see Left out).
- The board's bar has "/" (skills); not drawn on home.
- The board's waiting rows: "Skip" and "Allow once" ghost and primary, and "Destructive" with "Open to decide"; here the same buttons, and the tag reads "Cannot be undone" (plain words; the effect class stays as data).
- The board's Get started has four steps with Dismiss; here three, no Dismiss, and the provider and trust steps carry their forms inline when active.
- The brand is 24 px in Comfortable and follows the density (23 in Compact).

## Limitations

- CI is recorded in `evidence.json`. CI run 1 (43808273820) failed on Ubuntu in this spec: with the Linux location name ("This computer") and a long folder path the strip wrapped and the run mode went to a second line, which `trayDrift` caught. The strip is now one line where the folder gives way (its path ends in an ellipsis and stays whole in its title), and the spec checks the strip at 1100 and 900 wide as well. CI run 2 (43814166045) failed on the macOS runner in `awl.spec.ts` homeDrift and this spec's `trayDrift`, both "not centred": that Mac draws scroll bars that take room and first-run home is taller than its window, so the column moved by half a scroll bar. Home now keeps a scroll-bar gutter on both edges (checked locally with a 15 px scroll bar forced on: the tray's offset from the centre is 0).
- The task list's rows still say "shell.exec asks for a …" as their reason line (the Core's attention text, PX-199's surface); not changed here.
- The owner has not seen the screenshots. No graph status was set.
