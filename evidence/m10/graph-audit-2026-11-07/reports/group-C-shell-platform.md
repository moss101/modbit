<!-- Subagent report, 2026-11-07, code read at d4c5458a, nothing run. Spot-checked, not proof; see ../AUDIT.md. -->

**Audit of group C at d4c5458a. I read code and tests only and ran none. The worktree stayed clean.** The one live call was a read-only `gh pr view 170`.

Three things apply to most rows:
- **No packaged app exists.** There is no packaging config in the tree. Every spec launches `dist/main/main.cjs` (`e2e/support/ui-harness.ts:36-38`), even where its header says "packaged". So no "packaged" clause can be met.
- **Blocking records.** DR-009, DR-011 and DR-012 are still `proposed`. Doc 62 line 7 keeps PX-079/080, PX-093 and PX-094..097 blocked behind them.
- **Prerequisites.** M10.2, M10.3, M10.4, PX-055, PX-077, PX-078, PX-087, PX-088, PX-090 and PX-092 are NOT_STARTED. PX-030 is WIRED.

---

## PX-048: IMPLEMENTED-PARTIAL (the agent-terminal clause on its own is BROKEN-DRIFTED)

Row-level class is my judgement. Most clauses are proven, so I did not class the whole panel as drifted.

**Proven, unpackaged, real Core:**
- **Live stream:** `terminal-history.spec.ts:94`. The "build" is a paced Node program on a PTY, not a real build.
- **Reattach:** `apps-panel.spec.ts:178` compares bytes after a reload (Linux and macOS only). `terminal-renderer-restart.spec.ts:18` covers a real crash but only checks strings and `bytesSoFar >=`.
- **OutputRef scroll-back:** `terminal-history.spec.ts:116`, ended terminals only.
- **Stop warning:** `apps-panel.spec.ts:293`.
- **Revert, stage, stale refusal:** `changes-app.spec.ts:121,139,156`.
- **Commit and push with exact intent, and deny:** `changes-app.spec.ts:176,223`.
- **Per-task panel state across restart:** `apps-panel-state.spec.ts:33`.
- **Browser lease badge and takeover:** `apps-browser.spec.ts:17`.
- **PR #170's read fix:**
  - Code: `crates/git/src/lib.rs:759`, `review.rs:100`.
  - Tests: `px_048_changes.rs:700,797`, `changes-app.spec.ts:111-113`.
  - The mutation log is retained. The PR's 15 checks had no failures, and they include desktop E2E and rust on all three OSes, so the "Linux and Windows not run" item looks closed.

**The two claims you asked about:**
- **A person can type into an agent's terminal: confirmed, and the tests assert it.**
  - `terminal_stream.rs:408` only attaches to a terminal the task owns. `set_terminal_input` (:835) and `write_terminal` (:954) check the lease, never the owner.
  - `terminal-controller.ts:245-256` takes the lease on the first key, and `:349` tells the person so.
  - `apps-panel.spec.ts:118,146-149` and `terminal-history.spec.ts:306-311` assert that it works.
  - This contradicts the QUAL-PX-048 failure proof, AFW-I03 (doc 65:251) and QUAL-PX-043 (doc 62:276), and PX-043 is COMPLETE. A search of `docs/decisions` for the input-lease and read-only wording found no record superseding them.
  - Only the agent's own `shell.input` is refused while the person holds the lease (`apps-panel.spec.ts:153-176`).
- **Leave terminal does nothing: true of its code, not quite of a mouse click.**
  - `terminal-app.tsx:99-101` has no `onClick`, and `button.tsx` adds none. It exists as the focus target for Shift+Escape.
  - Enter or Space on it does nothing. If focus is already elsewhere, a click does nothing either.
  - If the terminal has focus, the click should blur it, which fires `xterm-view.tsx:107-111` and releases the lease. That comes from reading the code, not from running it.
  - No spec ever clicks it (`apps-panel.spec.ts:164,407`, `terminal-history.spec.ts:313` only focus it).

**First missing link:** the Core has no owner check on input, so an agent terminal accepts typing against the QUAL. An owner decision is needed: a Decision Record to change the spec, or a Core refusal.

**Still open:**
- **User terminal:** none exists. Owners are `agent|host` (`terminal_stream.rs:181`), and I found no start-terminal command in the proto or in `server.rs`. `ownerNote`'s "Terminal started from this window" describes something that does not exist.
- **Windows:** exact line order is not asserted (`terminal-history.spec.ts:212-224`).
- **Running terminals:** no history until exit, and the spec asserts the `NO_OUTPUT_REF` refusal (`:240-256`).
- **Packaged run:** no packaged run exists.
- **Preload pin:** whether a person reviewed it cannot be determined from code. `hardening-guard.test.ts` has been touched by 5 commits.
- **px_043 flake:** #170's green rust jobs are one data point, so it is not settled.
- **#170's own leftovers:** old `add -N` entries stay in a checkout, and a Core killed mid-read leaves an orphan temp index.

**Audit note:**
`IMPLEMENTED-PARTIAL; agent-terminal clause BROKEN-DRIFTED. terminal_stream.rs:408,954 lease only, no owner check; apps-panel.spec.ts:146 asserts typing works. Changes, panel state, browser, scroll-back proven unpackaged. Missing link: Core lacks agent-terminal refusal; no user terminal; Leave terminal has no click handler; no packaged run.`

---

## PX-025: IMPLEMENTED-PARTIAL

**Proven, unpackaged:**
- **Event-to-render:** `perf-budgets.spec.ts:55`.
- **List update after an event:** `perf-budgets.spec.ts:199`. It measures the agent list as a stand-in for the attention list.
- **Onboarding median under 5 minutes:** `onboarding.spec.ts:146`, with a scripted model and enforced in the suite.

**First missing link:** no packaged build exists for the E2E to run on.

**Other gaps:**
- Five of doc 39's eight budgets have no measurement: cold start, command acknowledgement, opening a Review diff, browser takeover and reconnect.
- The stream and list budgets are asserted only when `MODBIT_PERF_PROFILE=reference` (`:38`). `ci.yml` does not set it, so a miss cannot fail CI.
- The retained perf results are the local macOS debug-Core runs; none is a reference run.
- Traces are `retain-on-failure` only.
- Doc 39's "interactive Fleet" budget wording is stale after DR-020.

**Audit note:**
`IMPLEMENTED-PARTIAL. e2e/perf-budgets.spec.ts:55,199 measure two budgets, enforced only on MODBIT_PERF_PROFILE=reference, never in CI; onboarding.spec.ts:146 asserts a third. Five of eight doc 39 budgets unmeasured. All runs unpackaged. Missing link: no packaged build for the E2E to run on.`

---

## PX-031: SCAFFOLDED

**What exists:**
- Labels: `platform.ts:16-26` and `tools/platforms.json`, all three platforms CI_COMPATIBLE.
- A refusal gate: `support_claims.py:98-116`, run with its self-test at `ci.yml:358-360`.
- Windows and Linux do stay CI_COMPATIBLE today.

These belong to PX-030 rather than this row. Promotion itself does not exist.

**First missing link:** there is no packaged E2E catalog, and no promotion path that reads platform evidence.

**Other gaps:**
- No macOS promotion, no Release Zero subset, no Decision Record.
- The gate only checks that PX-031 is COMPLETE. It would not stop Windows or Linux promoting on macOS evidence, or without a Decision Record.
- `platform.test.ts:8` hard-asserts CI_COMPATIBLE and would need changing on promotion.

**Audit note:**
`SCAFFOLDED. apps/desktop/src/main/platform.ts, tools/platforms.json, tools/support_claims.py:98-116 label states and refuse RELEASE_GRADE until PX-031 COMPLETE; the gate reads no per-platform evidence or DR. Missing link: no packaged E2E catalog and no promotion path. All three stay CI_COMPATIBLE.`

---

## PX-093: IMPLEMENTED-PARTIAL (only what PX-106 built)

**Proven, unpackaged, in `px-106-customize.spec.ts`:**
- Import arrives quarantined and unsigned, with inert servers and a trust action naming the exact digest (`:236`).
- Keyboard operation (`:270`).
- Axe checks (`accessible()` in `:129,:270`).
- A real hook's last result (`:129`).

**First missing link:** no catalog exists anywhere. `customize.proto:4` and `customize-ipc.ts:11` both say so, and `:157` asserts the absence, so that test must be replaced when this row is built.

**Other gaps:**
- Seven tabs in order (EXM-E01); today it is lists on one view.
- Trust disclosure before install, and updates that block on widened capabilities.
- A malformed package's typed failure in the UI, and hash approval of a project plugin list.
- A hook log. `customize-hook-last` shows one result, not a list.
- CLI parity: `skill trust` and `skill untrust` exist. Parity for extensions and hooks I could not determine.

**Audit note:**
`IMPLEMENTED-PARTIAL. renderer/customize/customize.tsx, main/customize-ipc.ts, e2e/px-106-customize.spec.ts prove quarantined import, digest trust, keyboard, axe. Missing link: no catalog in Core or protocol (customize.proto:4); DR-011 proposed. No seven tabs, disclosure, update or hook log; spec :157 asserts no catalog.`

---

## PX-094: IMPLEMENTED-PARTIAL

**Code exists, but I found no test for it:**
- Inset title bar: `main.ts:961`.
- Drag regions: `styles.ts:117-118,159-160`.
- Mac padding: `styles.ts:161`, `index.tsx:33`.
- Zoom clamp: `prefs.ts:31-40`.

No spec mentions `app-region`, the traffic lights or the title bar.

**First missing link:** window bounds are never saved. `desktop-state.json` holds only `sessionId` (`main.ts:56-65`), and `createWindow(bounds)` is used only to replace a crashed window (`:1001`).

**Other gaps:**
- Geometry reads at each zoom, drag versus click behaviour, and per-OS-version checks.
- Translucency and Reduce transparency: nothing exists.
- Removed-display handling: no screen API is used.

**Drift risk:**
- `hardening-guard.test.ts`'s window-options check does not see spread keys, so `main.ts:961` passes it unnoticed.
- The doc 62:1021 card reserves hidden title bars for DR-012. DR-020 sections 5.2 and 8 allow this one change, but DR-012 itself is still unratified.

**Audit note:**
`IMPLEMENTED-PARTIAL. main.ts:961 hiddenInset; styles.ts:117-118,159-161 drag regions and mac padding; no spec covers them. Missing link: window bounds never saved (main.ts:56-65 stores sessionId only). No translucency, display fallback or geometry test. Guard test misses spread window options.`

---

## PX-095: DOCUMENTED-ONLY

Nothing here is proven. `main.ts:8` imports no `Menu` or `Tray`, and there is no badge, single-instance lock, `open-url` handler or protocol registration.

**Nearby code, none of which meets a clause:**
- `main.ts:729` shows a notification with no click handler.
- `main.ts:823` is a folder dialog used only for importing another agent's configuration.

**First missing link:** the main process has no menu, tray, badge, instance lock or deep-link handler at all.

**Audit note:**
`DOCUMENTED-ONLY (doc 78, DR-012 proposed). main/main.ts:8 imports no Menu or Tray; no setBadge, single-instance lock, open-url or protocol handler. Notification at :729 has no click route; dialog at :823 serves import only. Missing link: no native integration code in main. No test.`

---

## PX-096: IMPLEMENTED-PARTIAL

**Proven, unpackaged:**
- Sender and frame refusal: `main.ts:272-285`, `workers.spec.ts:141` (foreign window gets `SENDER_REFUSED`; a child frame has no bridge).
- Bridge allow-list: `BRIDGE_ALLOW_LIST` in `workers.spec.ts`.
- CSP pin: `hardening-guard.test.ts`, which hashes source text rather than the served header.
- `webPreferences` pin: same file.
- Killed renderer recovered: `workers.spec.ts:245`.

**Code exists, no test found:** navigation and window-open denial on the app window (`main.ts:985-986`).

**First missing link:** with no packaged build or fuse config, neither the fuses nor ASAR integrity exist.

**Other gaps:**
- **App-session permissions:** no handler is set. `main.ts:1036` only sets headers, and `browser.ts:436-447` covers browser partitions only. From memory of Electron's docs, the default with no handler is to grant, so check this one first.
- **Web-view:** no attach denial and no test. It relies on the default that web views are off.
- **Channel inventory:** the test covers bridge names, not a schema or sender check per channel.
- **Main-thread lag:** no measurement. `main.ts:48` also blocks synchronously for up to 1 s, once.
- **Hung renderer:** not detected. The watch at `:1009-1030` only checks that the process exists, and there is no `unresponsive` handler.
- **Full suite with fuses:** not possible without a packaged build.

**Audit note:**
`IMPLEMENTED-PARTIAL. main.ts:272-285 sender checks, workers.spec.ts:141,245, hardening-guard.test.ts CSP pin proven. main.ts:985-986 untested. Missing link: no packaged build or fuses, no ASAR integrity. No app-session permission handler, unresponsive handler, lag budget or channel schema inventory.`

---

## PX-097: DOCUMENTED-ONLY on this commit

Nothing is proven. A search for `autoUpdater`, `electron-updater`, `UpdateState`, `applyUpdate`, Squirrel and Sparkle found nothing, and no tracked file name contains updater, signing or sbom. M10.2 is NOT_STARTED here. The branch `wip/m10-2-updater-signing-sbom` exists, but I did not read it, as you instructed.

Pieces it could reuse: `activeWork()` at `main.ts:1058` (no pending-approval check) and the recovery report at `:165-179`.

**First missing link:** no updater code exists in this tree to check, stage, verify or apply anything.

**Audit note:**
`DOCUMENTED-ONLY on d4c5458a (doc 78, DR-012 proposed). No updater, signing or staging code in tree; grep for autoUpdater, UpdateState, applyUpdate empty. wip/m10-2 branch exists, unread. Missing link: nothing checks, stages, verifies or applies an update. No test. No packaged signed build.`

---

## PX-079: DOCUMENTED-ONLY

Nothing is proven. `files-app.tsx` is explicitly read-only. There is no `SaveEdit` or editor file service in the Core, the proto or the desktop, and `package.json` has no text engine apart from xterm. Doc 67 and DR-009 (proposed) specify it, and PX-077/078 are NOT_STARTED.

Neighbours only: the Files read refusals (`apps-panel.spec.ts:305`) and the one-hunk `review:patch` (`main.ts:568`).

**First missing link:** no editing surface or save command exists.

**Audit note:**
`DOCUMENTED-ONLY (doc 67, DR-009 proposed; PX-077, PX-078 NOT_STARTED). renderer/apps/files-app.tsx is read-only; no SaveEdit in Core, proto or desktop; no text engine in apps/desktop/package.json. Missing link: no editing surface or save command. No draft, merge, find or large-file path.`

---

## PX-080: IMPLEMENTED-PARTIAL (only by reuse of other rows)

None of this row's own surface exists. The classification rests on real neighbours.

**Proven, unpackaged, scripted model, not a live gateway, and reached from Changes or Review rather than an editor:**
- Rejecting one hunk, checked by file hash: `px-104-live-review.spec.ts:134`.
- A moved file refused as `STALE_REVISION`: `:156`.
- Accepting a hunk: `:172-178`. I did not confirm a hash check there.
- A selection reaching the context inspector: `surface_protocol.rs:14465` (`qual_ev_0141_0160_a_selection_steers_retrieval_is_visible_and_grants_no_write`). Its name says no write is granted, but I did not read the assertion, so treat "never changes a file" as not confirmed.

The desktop path exists at `review.tsx:122` → `main.ts:293`. It sends hunks and paths only, never a line range.

**First missing link:** no editor selection or quick-edit command exists to start from.

**Other gaps:** the range shown in the inspector, the range check, the agent-write notice and its merge (they need PX-079), and a live gateway run.

**Audit note:**
`IMPLEMENTED-PARTIAL by reuse only. px-104-live-review.spec.ts:134,156 prove per-hunk reject and stale refusal; surface_protocol.rs:14465 selection reaches inspector; review.tsx:122 sends hunks only. Missing link: no editor selection or quick-edit command. No range check or agent-write notice; PX-079 absent.`

---

**For the graph:** the NOT_STARTED status looks right for all nine of those rows, because neither a real packaged run nor a satisfied blocker exists. The partial classes describe code that exists, not grounds to move status.
