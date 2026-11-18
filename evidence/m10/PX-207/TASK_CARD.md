# Task Card — PX-207 Automations and Customize in the page shape: a details rail for an automation, one search across Customize, and Rules as a read-only section

## Identity

- Task ID: PX-207 (REQ-PX-207; owner desktop); qualification QUAL-PX-207 / PX-E2E-207; Decision Record DR-PX-2026-11-11-021 (MWS-14, MWS-09's rail shape, sections 5.11 and 9; AFW-A12, A13, A14 unchanged)
- Evidence tier: release-critical (the preload passes the Core's instruction layers through, main opens a rule's file, and automations are enabled and run from a new layout)
- Prerequisites: PX-199, DOC-PX-020. Nothing here sets a graph status.
- Branch `wip/desktop-2-automations`, cut from origin/main 68f39e5e (PX-198..206, 209).

## The rules implemented (quoted from the record)

- MWS-14: "Automations, Customize, Review and Settings use one page shape: a 45 px head with the page's name and its one primary action, an optional section list at the left, rows of at least 44 px in lists with a 1 px edge. Automations shows its list with a details rail for the chosen one. Customize has one search across its sections and gains Rules, a read-only list of the instruction layers the Core loaded with Open source for each (PX-093 and PX-106 own the rest)."
- Section 5.11: "Nothing is drawn that does nothing."
- QUAL-PX-207: enable, pause, run now and kill behave as before on the new layout; the search finds a skill, a hook and a rule by text; Rules lists the layers the Core loaded for the folder with their sources and Open source opens the file; a skill that is not trusted still cannot be enabled without review. Fails if a rule is shown that the Core did not load, a rule is editable in place, or a search result opens the wrong section.

## Existing-code audit (before any edit)

- Automations (`renderer/automations/*`, `main/automation-ipc.ts`): PRODUCTION-WORKING behaviour (list, editor with the Core's validation, enable approval, test run, run now, per-definition and global pause, kill, history with typed reasons, attention items), proven by `automations.spec.ts` against a real Core. Layout only: a wide seven-column table, the detail as a card under it, an eight-column history table, a Close button, raw Core codes in visible text (`SOURCE_CHANGED` as a run's reason, `read_only` as reach).
- Customize (`renderer/customize/*`, `main/customize-ipc.ts`): PRODUCTION-WORKING skills (trust by content hash, revoke), hooks (switch at scope), extensions with their tool servers (trust the manifest, unload, import), memory (PX-188), Open source for a skill and an extension, proven by `px-106-customize.spec.ts` and `px-188-memory.spec.ts`. No search; no Rules; four bordered card lists stacked under a Close button; raw codes in visible text (`USER`, `before_tool · observe · fails closed`, `signature UNSIGNED`). Trust was one click on the row ("Trust this version"): the row showed the hash, nothing more was asked.
- Rules: the Core already reads the instruction layers it recorded for a task (`ContextInspectorView.instructions`, field 180, REQ-PX-107; `services/modbit-core/src/inspector.rs` reads the task's newest `RulesSelected`, with each layer's source, hash, reason, and the files found and not loaded with the Core's reason). DOCUMENTED-ONLY on the desktop: main's `context:inspector` handler dropped the field, the preload type had no such field, and `customize:openSource` accepted only `skill` and `extension`.
- First missing link: main's `context:inspector` handler did not pass `instructions` through, so no surface could list the Core's loaded layers. The Core read exists; no protocol was added.

## What was built

- **Bridge (main, preload types; no new channel or method).** `context:inspector` passes `instructions` through as plain data (`main.ts`). `customize:openSource` accepts the kind `rule` with the session and the task: main reads the task's instruction layers from the Core and shows a file only if the Core recorded that exact source as loaded; an absolute source is shown where the Core read it, a relative one (AGENTS.md, CLAUDE.md) is resolved inside the task's folder as the Core's session snapshot holds it and never outside it (`customize-ipc.ts` `validRuleSource`, `rulePath`). The file is shown through the existing open path (`shell.showItemInFolder`, the one skills and extensions use). The preload's only change is two types (the `instructions` field and the `rule` kind); its pin in `hardening-guard.test.ts` moves with a review note.
- **The page shape (`automations-css.ts`, shared `.pg-*` rules).** A head the bar's height (`--mb-top-bar`, 45 / 40) with a 1 px edge; the page's name is the top bar's (as on Inbox) and is kept in the head for assistive technology; the head holds the page's one primary action; lists of rows of at least 44 px with a 1 px edge and radius 12. All sizes are tokens or steps from them (`measured-css.test.ts` passes).
- **Automations (`automations.tsx`).** Head: Pause all, Kill all…, and the primary New automation. Body: the folder to read `.modbit/automations` from with Load definitions, the attention list as rows, the editor when open, the definitions as rows (icon by trigger, name, reach in words and source, state, triggers, last run, Pause). A row opens a details rail beside the list in the task rail's shape and width (320, `DETAILS_RAIL`): name with Close, state and version, description; Run now (primary where the definition has a manual trigger), Review and approve… or Disable, Test run…, Edit or View definition, Reload from the repository, Kill…; then Trigger (each with its next slot), Runs in (folder, its own worktree, runs as), Limits (may do, cost, time and approval-wait limits, the approved version, activity), Definition (hash, versions), Recent runs (status, time, the typed reason in words with the code as data, trigger and event id, cost, duration, Open task, Acknowledge). Every call is the one the surface made before. At a page width of 800 px or less the rail stands under the list.
- **Customize (`customize.tsx`, `customize-model.ts`).** Head: one search ("Search skills, rules, hooks and more") and Read everything again. A section list at the left (Skills, Rules, Hooks, Extensions, Memory, each with the Core's count) that brings its section into view. The sections as rows with plain words: a skill's scope ("yours"), who can call it, its tag (Trusted / Not trusted / Changed / Forbidden); a hook's point, mode and fail policy in words ("Before a tool runs · only watches · if it fails the step does not go ahead"), who set it, its command and last result; an extension's signature in words ("not signed"). The search filters what the page already read (no index, nothing sent anywhere); results list the skill, rule, hook, extension or memory with their section; Enter or a click opens that section with that row focused; Escape clears it.
- **Rules.** For the task in view (its title named): the layers the Core recorded as loaded, in its order, each with the file's name, its layer in words ("This repository's instructions", "This repository's rules", "Your rules, in every project", "From the extension …"), why it is in force ("Always in force", "In force because <path> matches <glob>"), size and whether it was cut, the source and hash, any scanner finding, and Open source. The files the Core found and did not load sit apart behind "Files the Core found and did not load (n)" with the Core's reason and no control. Nothing in Rules is a field.
- **Review before trust.** A skill that is not trusted offers "Review and trust" and Open source on its row and no trust control of its own. The review (a dialog) shows what it does, version, where it comes from with its path, who can call it, where it applies, provenance, its trust now and the full content hash; Trust this version sends that hash (the Core refuses any other) and the review stays open, busy, until the Core answers. Cancel changes nothing.

## Left out, and why

- **The board's "Add" in Customize and its "Applies to" (folder / all my projects) filter.** No row provides an add or a scope filter; drawn, they would do nothing (5.11).
- **The board's per-skill and per-server switches and a separate Tool servers section.** The Core has no switch for a skill other than trust, nor one for a tool server outside its extension; servers stay listed in their extension's row.
- **Rules for a folder without a task.** The Core records the loaded layers per task (its newest `RulesSelected`); with no task the section says so.
- **The board's Automations segmented trigger control, Edit prompt, "Tell me" and budget in currency.** The trigger is changed in the definition (the editor); the Core reports cost in minor units and no notification preference per automation.
- **Cloud as a place an automation runs.** Not built (PX-128).

## Differences from the board (B2 Automations, B3 Customize) and the record

- The page's name is not repeated in the page head: the top bar above it already carries it (as Inbox, PX-206). It stays in the head as a heading for assistive technology.
- B2's rail is 372 px; here the task rail's 320 (MWS-01), never under 280.
- B2's rows end with an on/off switch; here Pause / Resume (the Core's pause, unchanged), and the state in words with a glyph.
- B2's head has only New automation; here Pause all and Kill all… sit beside it as quiet buttons (the global switch and kill had no other place).
- B2's rail has a description and Run now + Edit prompt; here the full set of actions the definition allows, with Run now first.
- B3's skill rows are slash names in mono with a switch; here the name and version, the description and scope, the hash and provenance on a second and third line, and Review and trust / Revoke / Open source.
- B3's search placeholder names tool servers; here "skills, rules, hooks and more" (tool servers are found through their extension).
- B3's sections show counts; here the counts are the Core's list lengths and appear once read.

## Verification

Unit: `@modbit/desktop` 453 of 453 (+5: Rules, search and words in `customize-model.test.ts`; rule validation and `rulePath` in `customize-ipc.test.ts`); `tsc -p apps/desktop` (src and e2e) clean.

Real app, real Core (prebuilt `research/desktop-2/bin/modbit-core`, sha256 c5ec630a…), scripted model recording every request: `e2e/px-207-automations-customize.spec.ts`, 5 of 5.

- **Rules.** A task runs in a trusted repository with AGENTS.md, a global rule (`kestrel-style.md`), a rule scoped to `docs/**` the task never touches and a rule file that does not parse; the profile has `rules/mine.md`. The model's requests carry the AGENTS.md, style and personal rule markers and not the dormant one. Rules lists exactly three rows; `rulesDrift` compares them, in order and with their hashes and layers, with the instruction layers main answered for the task (a tap the test puts on main's handlers; the product has none) and finds nothing. The dormant rule is nowhere; the broken file is only under "Files the Core found and did not load" with the Core's reason. `editableRules` finds no field and no control but Open source. Open source shows `<repo>/AGENTS.md` (a relative source resolved in the task's folder) and the style rule's file; main refuses the dormant rule, `../../etc/passwd` and a made-up README.md with "the Core lists no such source", and nothing more is shown. Axe clean; no raw Core code in the page's words.
- **Search.** "kestrel" finds the skill, the rule and the hook (in that order, with their sections); each result, by pointer and by keyboard, opens its own section with its own row focused (`openedDrift`). Text inside a description ("merged work") finds its skill; an unknown word finds nothing and says so; Escape clears. Axe clean with the results shown.
- **Review before trust.** The untrusted skill's row has exactly Review and trust and Open source. The review shows the full hash, provenance and trust; axe clean with it open; Cancel leaves it untrusted. Trust from the review by keyboard makes it TRUSTED_BY_OWNER and enabled; the bridge tap shows one trust call, for that hash, made while the review was open in the window. Revoke restores it.
- **Automations.** A manual definition and a schedule one are created through the bridge. The rail is closed until a row is chosen; then it shows the name, description, the manual trigger, "Reads only", Run now disabled until approved. Review and approve… → the enable dialog's confirm → ENABLED; Run now → a run row (manual) that succeeds with a task id, and the row's last run is Succeeded; the Core's list says ENABLED and succeeded. Pause from the row → PAUSED (the Core says paused), Resume → ENABLED. Kill… from the rail asks ("cancels the runs in progress"), then the report says paused, and the row is PAUSED. Close takes the rail away. Axe clean with the rail open; no raw Core code in the page's words.
- **Page shape.** In Comfortable and Compact, both pages: the head is the bar's height (45 / 40) with a 1 px edge and at most one primary action; every list has a 1 px edge and rows of at least 44 px; the Automations rail is 320 px beside the list, also at 1100 x 760.

Whole desktop suite, local, macOS arm64, once (`desktop-e2e-full-results.txt`): 217 passed, 5 failed, 3 skipped. All five failures were gallery specs and caused by this change: the rail's sections were labelled `section` elements, so the gallery (several Automations surfaces on one page) had duplicate region landmarks (axe `landmark-unique`, which failed the whole-gallery checks of gallery, gallery-controls and gallery-projects), and a schedule trigger in the list no longer said its next slot in UTC (`gallery-automations.spec.ts:49`). The rail's sections are now plain blocks under their headings, and a schedule trigger shows "next <date> UTC" (or "no slot due") under its summary. After the fix: the four gallery specs 11 of 11; `px-207-automations-customize`, `automations`, `px-106-customize`, `px-188-memory` 18 of 18. The suite was not run whole a second time.

## Retained mutations (each asserted to fail)

- **A rule shown that the Core did not load**: a cloned rule row with `data-source="/tmp/never-loaded.md"` appended; `rulesDrift` reports "rule /tmp/never-loaded.md is shown and the Core did not load it"; removed, it holds.
- **A rule editable in place**: a textarea and a contenteditable heading put in the AGENTS.md row; `editableRules` reports "an editable strong in AGENTS.md" and "an editable textarea in AGENTS.md".
- **A search result that opens the wrong section**: after the rule's result opened Rules, Hooks is shown; `openedDrift("rules", …)` reports "the section shown is hooks, not rules" and that the focus is not on the rule's row.
- **A skill trusted without its review**: a button added to the row that calls the trust bridge directly; `trustedWithoutReview` reports "kestrel-notes was trusted without its review open" (the tap reads the window when main is asked).
- **The page head at the superseded 40 px**: `pageDrift` reports "the head is 40 px, not 45".

## Assertions deleted, replaced or changed

- None deleted, none weakened.
- `e2e/px-106-customize.spec.ts`: each trust now opens the review first (`customize-skill-review`), then presses `customize-skill-trust` in the review (lines for alpha, beta twice and pdf-extract); the accessible name check is unchanged. The tab-order check starts on the review button (was the trust button) and still lands on Open source. "signature UNSIGNED" became `data-signature="UNSIGNED"` on the row plus "not signed" in its words (same fact).
- `e2e/automations.spec.ts:304`: the refused run's reason was its raw code as text; now `data-code="SOURCE_CHANGED"` on the reason and its words as text (same fact).
- Test ids removed with their surface: `customize-close`, `automations-close` (no spec used them; the navigation row opens and closes the page, as for Inbox), `automation-principal` (now `detail-principal` in the rail). Kept: every other automations and customize id.

## Limitations

- The owner has not seen the screenshots. No graph status was set.
