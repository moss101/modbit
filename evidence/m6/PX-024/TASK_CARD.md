# Task Card — PX-024 Keyboard model and accessibility conformance

## Identity

- Task ID: PX-024
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-024; owner label: desktop; subsystem: `apps/desktop/src/renderer/keyboard.ts`, `apps/desktop/src/renderer/diff.ts`, `apps/desktop/src/renderer/index.tsx`, `apps/desktop/src/renderer/index.html`, `apps/desktop/src/main/main.ts` (`task:cancel`, `task:steer`; the profile's renderer storage under `MODBIT_DATA_DIR`), `packages/ide-adapter-core/src/client.ts` (`cancelTask`)
- Qualification: QUAL-PX-024 — every screen traversed by keyboard only in the packaged app; global shortcuts, list navigation, review navigation and approval with confirmation work; accessibility suite passes; focus retained across state changes; live regions announce attention changes. Failure proof: any control unreachable by keyboard, any colour-only status, or lost focus on a Core event fails.
- Evidence tier: real-system (the real Electron app against the real modbit-core, driven by keyboard only; axe-core 4.10 evaluated in the renderer)

## Goal

Global, list and review shortcuts; approval with confirmation for irreversible effects; focus retention; live regions; no colour-only status; the accessibility suite in the packaged E2E (docs/39 "Keyboard model").

## Existing-code audit

- classification: DOCUMENTED-ONLY before: docs/39 declared the keyboard model; the desktop had focusable cards (`tabIndex`) and one polite live region ("N tasks need attention") and nothing else — no shortcut, no list or review navigation, no confirmation, no focus retention (a card re-mounted in another column lost focus), no accessibility run. Found while building (by the suite): `main[hidden]` was still displayed under the Review (author `display: grid` beat the `hidden` attribute), so the fleet was never actually hidden; content outside landmarks; a nested `main` and `aside`; scrollable regions without keyboard access.
- production entry points: `keyboard.ts` (`commandFor`, `isEditable`, `isActivatable`, `SHORTCUTS`), `diff.ts` (`splitRows`), `index.tsx` (the window key handler and `runFleetCommand`; the search filter; the `?` help panel; the `alertdialog` confirmation; card focus retention; the `live-attention` region; the steer input; the Review's own key handler, focusable hunks with collapse, the split table, focusable checks, focus on open and return on close; `KIND_MARK` text marks on state lines), `index.html` (`main[hidden]`, focus outlines, the split table), `main.ts` (`task:cancel`, `task:steer`, `app.setPath("userData")` under the profile), `client.ts` (`cancelTask`).
- proof: `apps/desktop/e2e/keyboard.spec.ts`, without one click, on the real Core: `⌘/Ctrl+N` → goal, Tab through the composer, Enter submits (two tasks); Space toggles a Settings checkbox; `↓ ↑` within the column, `/` filters to one card and back, `?` opens and Escape closes the help; Enter on a card reaches Start, Enter starts; the task reaches its declared destructive effect — the card moves to Needs Attention and keeps the focus, the live region reads "Needs attention: approval on alpha task: git.worktree.close asks for a Destructive effect"; `r` and `a` jump, `←` twice returns to the card; `y` opens the confirmation ("cannot be undone"), Escape declines with nothing resolved (the worktree still exists), `y` then Enter approves the exact intent, the effect runs once (the worktree is gone), the focus survives Waiting → Running; the Review opens itself on the result and takes the focus, Escape returns it to the card; Enter reopens it; `↓` focuses the hunk, Enter collapses ("2 added, 1 removed") and expands, `u` shows the split table with the removal on the left and the additions on the right, `u` back, `f` focuses the verification list, Escape returns to the card; `→` reaches the other task, `c` then Enter cancels it (Cancelled) with the focus kept; every state line's `aria-label` starts with its kind; axe-core on eight screens/states with no violation. Unit: `keyboard.test.ts` (the mapping, the modifier per platform, typing never commands, confirmation takes only Enter/Escape, every documented shortcut maps), `diff.test.ts` (split pairing).

## Limitations

- "take or return browser control" waits for the Browser screen (M7, DR-M6-003); "pause" has no Core command — cancel (confirmed) and steer are offered; "jump to failing test" is verified with the fallback focus (the verification list) because the fixture has no failing check; the axe run is the renderer document per state (WCAG 2.x A/AA and best practices), not a screen-reader session; OS-level shortcuts (menu accelerators) are not registered.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/keyboard.spec.ts` — "keyboard: every screen by keyboard only — shortcuts, list and review navigation, confirmed approval and cancellation, focus retained across Core events, live announcements, the accessibility suite on each screen"
- `apps/desktop/src/renderer/keyboard.test.ts`, `apps/desktop/src/renderer/diff.test.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35143904650.json` (main at cc5e95e; the change commit 8b825dc and the keyboard E2E forward-fix cc5e95e)
