# Task Card — PX-209 One search: tasks, conversation text, files and commands in the palette

## Identity

- Task ID: PX-209 (REQ-PX-209; owner desktop); qualification QUAL-PX-209 / PX-E2E-209; Decision Record DR-PX-2026-11-11-021 (MWS-15, and MWS-11 for the palette's way into split view; sections 5.11 and 9; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (the palette starts tasks and runs commands, and it gains file and conversation results)
- Prerequisites: PX-199, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-search`, cut from origin/main 5802b4a3 (PX-198..205).

## The rules implemented (quoted from the record)

- MWS-15: "The command palette searches tasks, conversation text, files of the open task's folder and commands in one list with a heading per kind."
- MWS-11: "Alt+click on a row, the preview card and the palette open a task in split view."
- Task card scope: "The palette lists tasks, conversation matches, files of the open task's folder and commands under a heading each; Enter opens the result; the empty state says what was searched." Production wiring: "the existing conversation search and workspace file bridge methods; no preload or protocol change."
- QUAL-PX-209: a task is found by its title, a conversation by text inside it, a file by name and a command by its label; each opens the right place; the keyboard alone drives it. Fails if a file outside the open task's folder is offered, or search text is sent anywhere but the Core's search and file reads (asserted on the bridge log).
- Section 5.11: "Nothing is drawn that does nothing."

## Existing-code audit (before any edit)

- Classification: IMPLEMENTED-PARTIAL.
- PRODUCTION-WORKING and kept: the palette (`shell/palette.tsx`): a modal combobox over tasks (titles from the session model), the shell's command registry (`shell/commands.ts`, the labels the shortcut help also lists) and the project/worktree surfaces; ranking (`@modbit/ui` `rank`); arrows, Home, End, Enter; Escape through the layer stack with focus returned (PX-045; the focus-containment fix f6162c20 and the keyboard spec's focus poll 9a54297e are both on main).
- The Core's conversation search exists end to end: `SearchConversations` -> main `conversation:search` (validated, trimmed, 1..256 characters) -> preload `searchConversations(sessionId, query, options)`, already read by the agent list (PX-046) and the composer's @ menu. Hits carry the task, a snippet's text, its row id and the matched byte ranges.
- Files: the Core's `ListWorkspaceDir` / `ReadWorkspaceFile` under the Workspace File Service's path policy -> main `files:list` / `files:read` -> preload `listWorkspaceDir(taskId, path)` / `readWorkspaceFile`. The Core lists one folder at a time; there is no search by file name in the Core.
- First missing link: the palette's items were only what the window already held (task titles, commands); nothing in it called the Core's search or the folder listing, the Files pane could not be asked to show a given file, and a palette item had one action only (PX-204 left "the palette's way into split view" out for that reason).
- Not built in the renderer: no conversation index (the Core's search is used as is) and no file reader (the Core's listing and read are used as is). No second owner.

## What was built

- **One list, a heading per kind (`shell/palette.tsx`).** Tasks, In conversations, Files, Commands, in that order; a group appears only when it has results. Tasks and commands keep the existing ranking within their group. Each row has a Lucide icon from the existing set (task `layers`, conversation `at-sign` as in the composer's @ menu, file `file-text`, command `chevron-right`, setting `settings`), its title, a muted detail and the command's chord. The active option is kept by identity, so results that arrive from the Core while one is highlighted do not move the highlight.
- **Conversation text (`shell/palette-search.ts`).** Typing two characters or more asks the Core's `searchConversations` (150 ms after typing pauses, a newer query supersedes an older one; at most 8 hits, one snippet each). A row is the Core's snippet with its matched ranges in bold (the same byte-range splitter as the agent list, `agents/snippets.ts`) and the task's title; a hit on the title alone is a task result, not a conversation row. Enter opens the conversation at that row (the shell's existing `openConversation(taskId, rowId)`, which highlights it).
- **Files of the open task's folder.** When the palette opens and the task shown in the centre has a workspace, the folder is walked through `listWorkspaceDir`, breadth first, at most 400 folders and 20,000 names, never entering a protected folder, `.git` or `node_modules`; only files the Core lists and does not protect are kept, only while the palette is open. A file matches when its name or path holds the typed text (name prefix first, then name, then path; no scattered-letter matches). Enter opens the Files pane on it: its folders open, its text is read through `readWorkspaceFile` (the Core's path policy decides again), and the keyboard is on the file in the tree (`apps/files-app.tsx` `reveal`, `shell/apps-panel.tsx` passes it). With no task open, or a task without a workspace, there is no Files group and the footer says so. The listing is asked for folders only; the typed text never goes to it.
- **Commands.** Unchanged: the registry's labels, each runs as before; an unavailable one says why and keeps the palette open.
- **Open in split view (closes PX-204's left-out item).** The highlighted task row shows "⌥Enter Open in split view" (Alt+Enter elsewhere); Alt+Enter, or Alt+click on the row, takes PX-204's own split entry (`openSplit` in `shell/shell.tsx`, the path Alt+click on a list row and the preview card use): the shown task stays on the left, the chosen one opens on the right with the keyboard. The third-task refusal and the narrow-window refusal apply unchanged. No other kind of row shows it.
- **The sheet (board C3).** 640 wide near the top (112 px or 12 % of the height), the field 52 high with the search icon and Esc, the results with sentence-case headings, the foot with ↑ ↓ move, ↵ open, and what is searched: "Searches task titles, every task's conversation, the files of “<task>” and the commands". Nothing found: "Nothing matches “…” in task titles, every task's conversation, the files of “<task>” and the commands." Sizes are tokens or steps from them; no text size is written in px (`measured-css.test.ts` passes).
- `keyboard.ts`: the "/" key's description now says what the palette finds.

No preload, main-process, protocol, CSP or Rust change. No locked file changed. No new icon (the generator's lucide-react 1.17.0 was not available; the set's existing icons are used).

## Left out, and why

- **A Files result when the Files pane is in a window of its own.** The palette brings that window forward and says where the file is; the window is not asked to open it (that needs a new pane-window message, which is preload/main).
- **Files under `node_modules` and `.git`, and beyond the walk's bound.** Not found by name; the footer says "as far as the listing reached" when the bound or a cut listing stopped the walk.
- **Conversation results in another session** (automation runs): the Core's search is per session, as for the agent list.
- **The board's "New task about “…”" action and "Change run mode"**: not in the command registry; not added here (a command nobody proves would be drawn doing nothing).

## Verification

Unit: `@modbit/desktop` 436 of 436 (430 before, +6 in `shell/palette-search.test.ts`: the walk's order, bound, skips, protected folders, refusals and cancel; file matching; conversation rows from hits); `pnpm typecheck` (src and e2e) clean.

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a…), scripted model: `e2e/px-209-search.spec.ts`, 1 of 1, about 10 s (30 s with screenshots), run several times locally on the final build. Three tasks: A in its own repository (files `src/deep/px209widget.ts`, `docs/files-index.md`, `px209-ledger.md`), B and C in another (`px209-outside.txt`). The model writes "The quokka ledger reconciles" in A's conversation only. A tap on main's handlers (every invoke and every send, with its arguments, added by the test as in PX-201) keeps the bridge's log. All by keyboard:

- Empty palette: Tasks and Commands only; the foot says what is searched; axe clean; Escape closes and focus is back in the composer.
- "files": four groups in order (Tasks, In conversations, Files, Commands), task A first, `docs/files-index.md`, "Show files"; axe clean; ArrowDown visits every option and wraps, ArrowUp wraps back, Home and End; `aria-activedescendant` names the highlighted option.
- A file by name ("px209widget"): the Files row is `src/deep/px209widget.ts` with the name in bold; Enter opens the Files pane on it with its text as the Core read it, and the keyboard is on the file in the tree.
- No file from outside the folder: with A open, "outside" finds nothing and says the files of A were searched; "px209" offers exactly A's two px209 files; every offered file is one the Core listed for A (`filesOutside` empty). With B open, "outside" offers B's file for B and "px209widget" finds nothing.
- A task by title ("beta summary"): Enter opens B.
- A conversation by text only the model wrote ("quokka ledger", from B): the row is A's snippet with "quokka ledger" in bold and A's title; the bridge log shows `conversation:search` asked for it; axe clean; Enter opens A with that row highlighted.
- A command by label ("keyboard shortcuts"): Enter opens the shortcut help; Escape closes it.
- Open in split view: the highlighted task row shows it, a command row does not, and only one row shows it; Alt+Enter on B with A open gives A | B with the keyboard on B (`splitDrift` empty); Alt+Enter on C is refused with "Split view holds two tasks" and the split is unchanged; at 1100 x 800 Alt+Enter is refused with "Split view needs … px" and no split opens.
- Nothing found ("zzqq nothing here") says what was searched.
- The text typed (every query of six characters or more) reached no bridge call but `conversation:search`, `files:list` and `files:read` (`leaks` empty).

Adjacent specs on the final build: `shell.spec.ts` 10 of 10 (PX-045 palette included), `keyboard.spec.ts` 1 of 1, `computer-surface.spec.ts` 9 of 9, `px-204-split-view.spec.ts` 1 of 1, `apps-panel.spec.ts` 10 of 10 (Files included). Whole desktop suite, local, macOS arm64, once (`desktop-e2e-full-results.txt`): 215 passed, 1 failed, 3 skipped (the two live-model specs and the Windows-only one). The failure, `context.spec.ts:45` (the context ring not found 30 s after Escape closed a Review page), is outside this change's paths; the spec rerun alone on the same build passed 6 of 6.

## Retained mutations (each asserted to fail)

- **A file from outside the open task's folder offered**: B's `px209-outside.txt` drawn as a Files row for A while A is open; `filesOutside` reports "px209-outside.txt (offered for the open task, not listed by the Core for the open task)"; removed, it holds again.
- **Search text sent to a non-search bridge call**: the typed "quokka ledger" sent through `listProjects` (a call that is neither the search nor a file read); `leaks` reports "“quokka ledger” went to projects:list".
- **A split from the palette that is not PX-204's**: the two columns drawn the other way round; `splitDrift` reports the columns are B | A and the keyboard is not on the chosen task; put back, it holds.

## Test ids

- New: `palette-group` (`data-group`), `palette-title`, `palette-scope`, `palette-foot`, `palette-split-action`, `palette-search-error`; on options `data-task-id`, `data-row-id` (conversation), `data-path` and `data-task-id` (file).
- Kept: `palette`, `palette-input`, `palette-list`, `palette-option` (`data-kind`, `data-id`, `data-active`), `palette-empty`.
- None removed. The kind badge on each option is gone (the heading says the kind).

## Assertions deleted, replaced or changed

- None deleted, none weakened, none changed. The existing palette specs (`shell.spec.ts` PX-045 tests, `keyboard.spec.ts`, `computer-surface.spec.ts`) pass unchanged.

## Differences from the board (C3) and the record

- Headings: "Commands" for the board's "Actions" (the record and the qualification say commands).
- Icons: the set's own Lucide shapes; a conversation row uses `at-sign` (the composer's @ menu icon for past conversations) for the board's speech bubble; a task row uses `layers` for the board's state glyph.
- The file row's detail is the task's title (the folder is the task's), where the board shows the project's name.
- The split hint is on the highlighted task row rather than in the foot, so it is drawn only where Alt+Enter does something.
- The board's "New task about “…”" and "Change run mode" rows are not drawn (see Left out).

## Limitations

- The proof is local (macOS arm64); CI is recorded in `evidence.json`.
- The owner has not seen the screenshots. No graph status was set.
