# Task Card — PX-208 Review and Settings in the page shape

## Identity

- Task ID: PX-208 (REQ-PX-208; owner desktop); qualification QUAL-PX-208 / PX-E2E-208; Decision Record DR-PX-2026-11-11-021 (MWS-14; MWS-04 for the Review navigation row; section 12 "Shortcuts"; sections 5.11 and 9; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (review decisions are reached from a new layout; Settings gains controls)
- Prerequisites: PX-199, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-review`, cut from origin/main 68f39e5e (PX-198..206, 209).

## The rules implemented (quoted from the record)

- MWS-14: "Automations, Customize, Review and Settings use one page shape: a 45 px head with the page's name and its one primary action, an optional section list at the left, rows of at least 44 px in lists with a 1 px edge. … Review lists the tasks' pull requests and local changes not yet in one, and beside them the findings and people's comments for the chosen one. Settings keeps AWS-08 and gains, under Appearance, density, task rows and grouping, and a Shortcuts section that lists the bindings the app really has."
- MWS-04: navigation rows "New task, Inbox with its count, Automations, Review and Customize".
- Section 12: "The app's own map (`renderer/keyboard.ts`, `renderer/shell/commands.ts`) is the authority; PX-208 lists what exists and proposes nothing new."
- QUAL-PX-208: Review shows a task's change set and its findings and a hunk decision still works; each Appearance option takes effect at once and persists; the Shortcuts section equals the registry and the key map, binding for binding. Fails if: a listed shortcut is not bound; a setting that belongs to one task is shown as global; a density change needs a reload.

## Existing-code audit (before any edit)

- Classification: IMPLEMENTED-PARTIAL.
- PRODUCTION-WORKING and kept: the task's own review (`review/review.tsx`): the Core's revision-bound review bundle (`review:bundle` -> Core `ReviewBundle`), per-hunk reject, context marks, constrained patch, accept and return (`review:decide` -> Core `DecideReview`), the pull request flow and the forge reads (`review/use-pull-request.ts`, PX-007, PX-127), the review keys of `keyboard.ts`. It opened only from a task (its button, Enter on a finished task, by itself at the first result); there was no page that listed tasks with changes.
- Findings the Core provides: the bundle carries the verification runs and their checks, the diff-invariant strings, the agent's self-review JSON (`findings: [{ text, resolved }]`), the forge's check runs and people's comments. The isolated reviewer's findings one by one are not in the bundle (PX-214). The Core has no read of a pull request's state (PX-211); a request is known only from the Core's answer to this window.
- Settings (`settings/settings-page.tsx`): the sections of AWS-08 with theme, zoom and density under Appearance; per-task settings (run mode, allowlist, send behaviour) already said to belong to each task, with no control. A "Keyboard shortcuts / Show" row under This build opened the help dialog.
- The settings stores: the shell's layout preferences (`shell/prefs.ts`, `modbit.ui.v1`: theme, zoom, density) and the list's preferences (`agents/prefs.ts`, `modbit.agents.v1`: grouping, row details), the latter held in the task list's own React state, so nothing outside the list could change them while it was mounted.
- Key maps: the command registry's chords (dispatched by the shell's key scope), `commandFor` (the app listener and the Review), and the task row's own switch in `agent-list-view.tsx`. The help dialog lists the registry plus a hand-written `SHORTCUTS` / `LIST_KEYS` table.
- First missing link: no Review entry or page existed (the navigation had no Review row, by the task-list spec's own "no Review row until its page exists"), Appearance had no task-row or grouping option and could not have applied one to the mounted list, and no shortcut list was derived from the key maps themselves.

## What was built

- **Review navigation row and page** (`shell/agent-region.tsx`, `shell/shell.tsx`, `review/review-page.tsx`, `review/review-model.ts`, `review/review-css.ts`). Review sits between Automations and Customize (Lucide `git-compare`), pressed while the page shows. The page: the top bar names it; a head the bar's height (45 / 40) says how many tasks have changes and holds the one primary action, Open review; at the left "Pull requests from tasks" (only a request the Core answered this window with, PX-211's seam; the section is absent when there is none) and "Local changes not in a pull request": rows of at least 44 px with a 1 px edge, each the task's title, its state in words, where (the folder or its own worktree), files and +/− from the Core's header. Beside them, for the chosen task, read from the Core's review bundle and read again whenever its log moves: Changes (each file, what happened to it, +/−), Findings (the latest verification run counted, "4 checks passed, 1 skipped"; failed checks of that run, broken change rules as path and evidence, the agent's self-review items open first, the forge's failed checks), Comments from people (author, where, what became of it), and for a pull request row Read comments / Read checks (the existing forge reads). Plain words: no task-state enum, no rule id or check-kind prefix is shown.
- **Hunk decisions unchanged.** Open review shows the task's own review over the page; Back (button or Escape) returns to the Review page. Accept, return and per-hunk reject still go through `decideReview` exactly as before. The task's review takes the page head (the bar's height with a 1 px edge); its body is unchanged.
- **Appearance** (`settings/settings-page.tsx`): beside Theme, Zoom and Density, "Second line on task rows" (When needed / Always / Never: MWS-04's Row details) and "Group tasks by" (the list's groupings). Both write the list's own preferences through one observable copy (`agents/prefs.ts` `currentListPrefs` / `updateListPrefs` / `subscribeListPrefs`, the same `modbit.agents.v1` entry); the task list now reads that copy too, so a choice in either place is the other's at once. Not a second store: the entry, parser and serializer are the existing ones.
- **Shortcuts section** (`settings/shortcut-sheet.ts`): generated, never written by hand, from the three maps the app dispatches through: the registry's chords (each kept only if `registry.forEvent` resolves its own event to that command), `commandFor` (every key alone and with the primary modifier, in the list, review and confirmation contexts; a key the registry takes first is listed once, as the registry's), and the task row's key map, now `rowKeyAction` in `keyboard.ts`, which the list's key handler calls (the switch moved there unchanged). Words: the registry's titles, and `COMMAND_WORDS` / `ROW_KEY_WORDS`, records over the whole command type, so a command added without words fails the typecheck. Grouped as Anywhere, Single keys, On a task in the list, In a task's review, While a confirmation is open. "Show this list anywhere" with the help chord moved here from This build (test id `settings-shortcuts` kept). Nothing new is bound.
- **Per-task settings stay with the task**: Permissions keeps "These belong to each task" with no control; nothing per task was added.

No preload, main-process, protocol, CSP or Rust change. No locked file changed. Shared files touched minimally: `shell/shell.tsx` (one state flag and its page branch), `shell/agent-region.tsx` (one navigation entry), `build.mjs` (two style sheets appended), `review/use-pull-request.ts` (an additive read of the requests the window knows), `agents/agent-list-view.tsx` (its row switch reads `rowKeyAction`). No shared page-shape component exists on main; this row's head and row styles are its own (`rp-*`) and can be folded into one when PX-207 lands its pages.

## Left out, and why

- Pull request state, checks, merge and auto-fix, and the board's Open / Merged / All filter and repository picker: PX-211..213 (the Core has no read of a request's state). Only requests this window opened are listed as pull requests, and only until the app restarts.
- The reviewer's findings one by one with Fix this one, Dismiss, Apply all fixes and Run the review again: PX-214 (the bundle does not carry them).
- Comment Address actions: comments become steer inputs through the existing forge read; no new action.
- The task's own review keeps its two-column body; its words were made plain in the wording pass below.
- The help dialog keeps its own list (`SHORTCUTS`, `LIST_KEYS`); the Settings section is the generated one.
- The board's Settings sections Run modes and rules, Models and budgets, Environments, Computer use per app (PX-215): not this row.

## Verification

Unit: `@modbit/desktop` 457 of 457 (+9: `settings/shortcut-sheet.test.ts` 6, `review/review-model.test.ts` 3); `pnpm typecheck` (src and e2e) clean; `measured-css.test.ts` covers the two new sheets (no px text size).

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a2c92680c…), scripted model: `e2e/px-208-review-settings.spec.ts`, 1 of 1 (about 16 s; 30 s with screenshots), run repeatedly on the final build. Two tasks in two repositories reach review:

- Navigation rows are New task, Inbox, Automations, Review, Customize; Review opens the page, pressed, the top bar says Review; no pull request section (none exists); two local rows; page shape holds (head 45, rows ≥ 44 with a 1 px edge, one primary action).
- For task A: the file list equals the Core's `reviewBundle` files; notes.txt +2 −2; the checks counted from the Core's latest run; the two self-review items as recorded; "Comments arrive once the changes are in a pull request"; no raw state or code in the page's text; axe clean.
- Hunk decision: Open review over the page, reject the second hunk, accept with a note: the bridge log (a tap on main's handlers, as PX-209) has exactly one `review:decide` with `[A, ACCEPT, [{ path: notes.txt, index: 1 }], note]`; Back returns to the Review page; Git holds the file with only the first change and the commit message is the note.
- Appearance: with a marker on the window, Compact applies at once (root attribute, chrome text 13 px, bar 40, row 30, marker kept); dark theme, zoom 108 %, second line Always and grouping Status apply at once (the list's `data-grouping` and `data-row-details` on Back, marker kept); the list's own Row details menu shows the same choice; after an app restart every one is kept and Settings shows it; then back to the defaults; axe clean on Appearance.
- Per-task: no control on the Settings page is labelled with a per-task setting (run mode, send behaviour, allowlist, model, objective, budget, auto-fix, merge when ready; the provider form excepted); Permissions says they belong to each task.
- Shortcuts: the rows (scope, id, keys) equal the sheet the spec builds from the same registry and key maps, and the registry rows equal the registry's chords; axe clean. Every listed key, dispatched in the running app in its own context, is taken: Anywhere and Single keys on home with nothing focused (each key's effect closed after it), task-row keys on a focused row (archive undone, pin pressed twice), review keys in a task's review with a change focused, confirmation keys with a cancel confirmation open (Enter confirms: task B is cancelled).

Whole desktop suite, local, macOS arm64, once, on the final renderer (`desktop-e2e-full-results.txt`): 221 tests, 218 passed, 0 failed, 3 skipped (the two live-model specs and the Windows-only one), 22.8 min. Adjacent specs inside it: `review.spec.ts`, `px-104-live-review.spec.ts`, `review-escape.spec.ts`, `forge-evidence.spec.ts`, `shell.spec.ts`, `keyboard.spec.ts`, `task-list.spec.ts`, `awl.spec.ts`, `agents.spec.ts`, `gallery*.spec.ts` all pass.

## Retained mutations (each asserted to fail)

- **A listed shortcut that is not bound**: a row for "m" (Change run mode) added to the section; `unboundListed` reports "key-keys:changeRunMode: m is listed and not bound"; removed, the section holds. Unit: a registry chord the resolver no longer answers is reported in `unbound` and not listed.
- **A setting that belongs to one task shown as global**: a Run mode radio group added to Appearance; `perTaskAsGlobal` reports it; removed, it holds.
- **A density change that needs a reload**: the density left unapplied after Compact (the root's attribute removed); `densityDrift` reports "the root says comfortable after compact was chosen" and "the bar is 45 px, not 40"; chosen again, it holds.

## Test ids

- New: `open-review`; `review-page`, `review-page-count`, `review-page-open-review`, `review-page-pulls`, `review-page-local`, `review-page-row` (`data-task-id`, `data-kind`), `review-page-empty`, `review-page-detail`, `review-page-open-task`, `review-page-files`, `review-page-file` (`data-path`), `review-page-no-files`, `review-page-checks`, `review-page-findings`, `review-page-finding` (`data-kind`, `data-open`), `review-page-no-findings`, `review-page-comments`, `review-page-comment`, `review-page-no-comments`, `review-page-read-comments`, `review-page-read-checks`, `review-page-error`; `settings-row-details` and `settings-row-details-<id>`, `settings-grouping`, `settings-section-shortcuts`, `settings-nav-shortcuts`, `settings-shortcut-sheet`, `settings-shortcut-group` (`data-scope`), `settings-shortcut` (`data-id`, `data-scope`, `data-probes`).
- Moved: `settings-shortcuts` (Show) from This build to Shortcuts.
- None removed.

## Assertions deleted, replaced or changed

- `e2e/task-list.spec.ts` `orderDrift`: "no Review navigation row is drawn" (the row was absent because its page did not exist) becomes "the Review navigation row is drawn once"; the expected navigation is now "New task | Inbox | Automations | Review | Customize". The fact moved with the surface (MWS-04 lists Review).
- `e2e/awl.spec.ts` and `e2e/task-list.spec.ts` `NAV`: `open-review` added between `open-automations` and `open-customize`. The geometry checks over every navigation row now include it.
- None deleted, none weakened.

## Differences from the board and the record

- Review head: the count and Open review; the board's Open / Merged / All and repository picker are not drawn (PX-211). Rows use `git-compare` for a pull request and `git-branch` for local changes.
- Findings: the bundle's findings (checks, change rules, self-review, forge checks) with a label each, where the board shows the reviewer's Bug / Risk / Tidy with Fix this one (PX-214).
- The task's own review keeps its two-column body under the page head.
- Settings keeps its section list in the left column (AFW-A10). Appearance adds "Second line on task rows" with the list's three values (When needed, Always, Never) for the board's One line / Two lines, and "Group tasks by" as a select of the list's groupings (the board shows four in a segmented control; the list has six).
- Shortcuts: the app's bindings, not the board's proposals (section 12); grouped by where they act.

## Wording pass (2026-11-18, after the owner's review: "Fix wording first")

The owner read the first screenshots and asked for plain words before anything else. Visible text changed; every raw fact is kept as a data attribute (or a tooltip), and each assertion that read the raw text now reads the plain words and the attribute.

- **The task's own review** (`review/review.tsx`, words in `review/review-model.ts`): the meta line is "Ready for review · 1 file, 2 changes · based on 7f30b63" (`data-task-state`, `data-workspace-revision`, `data-base`, `data-receipts`; task id, revision and receipts in the tooltip); a file's head is "Changed" / "Added" / "Deleted" / "Renamed" / "Binary" (`data-status`, `data-file-revision`, revision in the tooltip); the hunk controls are "Reject this change" and "Use as context" (the existing record's meaning: the change is given to retrieval as context; not "Show more lines", which would be a different action), and a rejected change says "Rejected: undone when you accept"; collapsed says "Collapsed · 2 added, 1 removed lines"; the split toggle says "Side by side" / "One column" (`aria-keyshortcuts="u"`); accept says "Accepted as commit … ; 1 change undone" (`data-reverted`); an edit says "Your edit was applied, and the review was read again." or "The edit was refused: the review is out of date; reload it and try again." (`data-outcome`, `data-code`, `data-workspace-revision`, `data-previous-revision`); the selection note says "2 changes used as context".
- **The side panel**: Checks lists each run as "Before the change: 1 of 1 check passed", "Checks for this change: …", "Final checks: 4 of 5 checks passed, 1 skipped", with a check that did not pass as "Skipped: boundary pinned" / "✖ Failed: lint" (`data-stage`, `data-status`, `data-check-id`); "Checks on the pull request" with a one-sentence note; "Comments on the pull request" with a note that they come from outside Modbit, each comment as author, place and what became of it ("ignored: disallowed author"; `data-trust`, `data-reason`, `data-kind` keep the codes); "Read checks" / "Read comments" and their notes ("Read 1 check run for commit …"); the forge's refusal in words (`data-code`); Plan as its outcome and "Files it expected to change: notes.txt"; Self-review as its summary and a list with "(resolved)" or "(not resolved)"; Evidence folded as "Evidence (8)" with each link's kind in words and a 12-character hash (`data-link`); attributions, flaky checks and change rules in words (`data-raw`, `data-check-id`). The pull request block: "Open a pull request", "Push <branch> and open the pull request? This asks once, for exactly this push." (`data-intent`, `data-revision`), "Opened #1 at … · 1 receipt" (`data-head`, `data-receipts`), "Denied: the branch stays local and nothing reached the forge."
- **The review's state line** (`screens.ts`): "1 changed file, ready for your decision"; an inconclusive gate's cause says "the acceptance check could not decide".
- **The restart notice** (`main/lifecycle.ts` `recoverySummary`, a pure wording function only the renderer imports; no main-process behaviour changes): "2 tasks were restored after the restart." or "4 tasks were restored after the restart: 3 are running again, 1 needs you."; the counts are on `data-recovered`, `data-resumed`, `data-attention`.
- **Settings**: the platform row is in words from the recorded state (`data-state` kept, the record's sentence in the tooltip; `tools/support_claims.py` passes); "(PX-128)" and "(PX-128, PX-129)" are gone from Locations and Account; a login credential shows its label, not its handle (`data-handle` kept, the handle in the tooltip); the languages row drops "Tier C" and "structural intelligence".
- **Guard**: `rawWords` in the PX-208 spec fails on an enum or code, a row id, a task state, a "(s)" plural, a check kind, a revision id or JSON in the visible text of the Review page, the task's review and Settings; retained mutation: "ReadyForReview · 2 hunk(s)" put back into the review's meta is reported.

### Assertions changed in the wording pass (none deleted, none weakened)

| Spec | Was | Now |
|---|---|---|
| `review.spec.ts` | meta contains "ReadyForReview" | contains "Ready for review" and `data-task-state` = ReadyForReview |
| `review.spec.ts` | verification contains "BASELINE" | contains "Before the change" and one `li[data-stage=BASELINE]` |
| `review.spec.ts` | result contains "1 hunk(s) reverted" | contains "1 change undone" and `data-reverted` = 1 |
| `onboarding.spec.ts` | meta contains "ReadyForReview"; verification contains "COMPLETION" | `data-task-state` = ReadyForReview and "Ready for review"; "Final checks" and one `li[data-stage=COMPLETION]` |
| `forge-evidence.spec.ts` | forge error contains "NO_PULL_REQUEST" | `data-code` = NO_PULL_REQUEST and "no pull request for this task" |
| `forge-evidence.spec.ts` | note contains "read 1 check run(s)" | contains "Read 1 check run" |
| `forge-evidence.spec.ts` | meta contains "ReadyForReview" | `data-task-state` = ReadyForReview and "Ready for review" |
| `forge-evidence.spec.ts` | the stranger's comment contains "DISALLOWED_AUTHOR" | `data-reason` = DISALLOWED_AUTHOR and "ignored: disallowed author" |
| `user-patch.spec.ts` | revision parsed from "workspace revision N" in the meta; meta contains "workspace revision N+1", "N+2" | read from `data-workspace-revision`; `data-workspace-revision` = N+1, N+2 |
| `user-patch.spec.ts` | patch result contains "applied at workspace revision N+1 (was N)" | `data-outcome` = applied, `data-workspace-revision` = N+1, `data-previous-revision` = N, "Your edit was applied" |
| `user-patch.spec.ts` | patch result contains "refused" and "STALE_REVISION" | `data-outcome` = refused, `data-code` = STALE_REVISION, "The edit was refused: the review is out of date" |
| `states.spec.ts` | the intent read from `pr-intent`'s text | read from `data-intent` (still 16 hex) |
| `states.spec.ts` | pr result contains "opened #1 at …" and "1 receipt(s)" | "Opened #1 at …", "1 receipt" and `data-receipts` = 1 |
| `lifecycle.spec.ts` | summary matches `^Recovered at start: …`; counts from "needs attention" / "tasks resumed" | matches `^N tasks? (was|were) restored after the restart`; counts from "need(s) you" / "is/are running again" and from `data-attention` / `data-resumed` |
| `session-end.spec.ts` | summary matches `^Recovered at start: `; resumed from "tasks resumed" | matches the new sentence; resumed from "running again" and `data-resumed` |
| `fleet.spec.ts` | platform contains "CI_COMPATIBLE" | contains "built and tested in CI" (`data-state` = CI_COMPATIBLE and "not a support claim" kept) |
| `px-208-review-settings.spec.ts` | result contains "1 hunk(s) reverted" | contains "1 change undone" and `data-reverted` = 1 |
| `main/lifecycle.test.ts` | four `recoverySummary` sentences | the same four counts in the new sentence, plus the one-task case |

`keyboard.spec.ts`'s "2 added, 1 removed" holds unchanged. The screenshots in `after/` were all retaken after this pass. Run again on the final build: the PX-208 spec, and fifteen spec files in all (review, forge-evidence, user-patch, states, lifecycle, session-end, keyboard, onboarding, fleet, shell, px-104-live-review, review-escape, browser, awl, px-208-review-settings): 48 passed; unit 457 of 457; typecheck clean; `tools/support_claims.py` clean.

## Limitations

- CI (`evidence.json`, `ci-run-44345701253.json`): run 44263235263 on 48be2e9a, green after one rerun of a Linux `fleet-agents.spec.ts:80` failure outside this change; run 44345701253 on f58233e0 (the wording pass), 19 of 19 checks green on the first attempt.
- The owner has not seen the screenshots. No graph status was set.

## Closure (2026-11-19)

Owner reviewed the screenshots, asked for a wording pass ("Fix wording first"), and said "Yes, seal both". Branch brought up to date with main after PX-207 (manifest regenerated only; PX-207 and PX-208 specs plus awl, task-list, automations and px-106 passed together locally, 22 of 22). Merged as #230 (squash a5867e87) from head 5b3457a1, CI run 44397630544 green on every job after one rerun of the known task-list.spec.ts:492 flake on Windows. Sealed COMPLETE through graph.py with run, commit and evidence references.
