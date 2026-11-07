<!-- Subagent report, 2026-11-07, code read at d4c5458a, nothing run. Spot-checked, not proof; see ../AUDIT.md. -->

All eight rows are IMPLEMENTED-PARTIAL; none is NOT_STARTED in substance, and none qualifies as PRODUCTION-WORKING. I read the code and tests and ran no tests or app, only read-only commands. The one-line look was right that the code exists, but "status drifted" overstates it, because every row has real unproven clauses and some have real gaps.

Paths are relative to `/private/tmp/claude-501/-Users-mohsin-projects-modbit/5259dee3-ce69-40d7-96a7-c7cfe8d0e8af/scratchpad/graph-fix`. `e2e/` means `apps/desktop/e2e`, and `src/` means `apps/desktop/src`.

## Applies to every row
- **Packaged app:** no packaging config exists. `launch` runs `dist/main/main.cjs` (`e2e/support/ui-harness.ts:38`), so every "packaged app" clause is unmet.
- **Test realism:** the desktop specs I read use a real Core, event log, preload and renderer. Only the model is scripted, and PX-074 also uses the fixture actuator. No spec mocks the Core.
- **Recorded CI:** run 41983983308 succeeded on all jobs, but at `fafa801d`, not `d4c5458a` (`evidence/m10/PX-197/ci-run-41983983308.json`). I did not check the diff between them. The older log `evidence/m10/PX-196/desktop-e2e-full-after-section-5-9.log` has 2 failures (keyboard, shell).
- **Prerequisites not COMPLETE in the graph:**

| Row | Open prerequisites |
|---|---|
| PX-054 | PX-051 |
| PX-055 | PX-047, PX-050, PX-054 |
| PX-056 | PX-054, PX-055 |
| PX-058 | PX-047, PX-057 |
| PX-060 | PX-059 |
| PX-100 | PX-051 (DOC-PX-013 not checked) |
| PX-074 | PX-058, PX-069, PX-070, PX-071 (DOC-PX-008 not checked) |
| PX-086 | PX-046, PX-058 |

## PX-054: IMPLEMENTED-PARTIAL
**Proven:**
- **Shift+Tab cycle:** label, placeholder, the Core's mode and pointer menu, in `e2e/composer.spec.ts:204` "Shift+Tab walks the modes". DR-020's geometry (640 wide, 96 high) is at :221-224.
- **Write in Ask refused, nothing written:** `e2e/qual-gaps.spec.ts:18`.
- **Unconfirmed mode:** `e2e/qual-gaps.spec.ts:45`.
- **@ menu, protected path, draft:** `e2e/composer.spec.ts:510`.
- **Slash menu, divider, detail card, planted markup:** `e2e/composer.spec.ts:281`.
- **Attachment reaches the model:** `e2e/composer.spec.ts:334`.

**First missing link:** `composer:attachPath` reads any path the renderer names, in main (`src/main/composer-ipc.ts:288`, `src/main/main.ts:847`), and the Core's `IngestAttachment` takes only bytes (`services/modbit-core/src/server.rs:3873`), so no path policy exists.

**Other gaps:**
- "Read only by the Core" is false: main reads the file, checks its type and makes the thumbnail. `src/shared/attachments.ts:5-6` calls main the authority.
- No test asserts tint or icon, or opens the chip menu by keyboard.
- History after a reload is unasserted.
- "Renderer restart" is only `page.reload`.
- Only the file source of the @ menu is driven; terminals, past conversations and branch are not.
- The path-policy reason is renderer text driven by a Core boolean (`src/renderer/composer/menus.tsx:59`), not the Core's words.

**Note:**
`IMPLEMENTED-PARTIAL. Real path: renderer/composer/composer.tsx, main/composer-ipc.ts, Core; proven in e2e/composer.spec.ts, qual-gaps.spec.ts on unpackaged Electron. First missing link: composer:attachPath reads any path in main (main.ts:847); Core IngestAttachment has no path policy, so outside-workspace refusal and read-only-by-Core fail. Tint, icon, chip keyboard untested.`

## PX-055: IMPLEMENTED-PARTIAL
**Proven (all in `e2e/`):**
- **Queue, edit, delete, order:** `composer.spec.ts:22`.
- **Send now and partial text:** `composer.spec.ts:133`.
- **Stop re-sends nothing:** `composer.spec.ts:169`.
- **Education once:** `qual-gaps.spec.ts:77`.
- **Terminal kill wakes once, side question:** `composer.spec.ts:609`.
- **15 s skip:** `qual-gaps.spec.ts:494`.
- **Queue survives a Core kill:** `composer.spec.ts:712`.

**First missing link:** no test bounds Stop at about 1 s; the spec polls for 30 s (`composer.spec.ts:181`) and the Core test allows 10 s (`services/modbit-core/tests/run_control.rs:1043`).

**Other gaps:**
- No live-gateway leg; the model is always scripted.
- Send now while an effect outcome is unknown: no surface in `trays.tsx` and no test. I read only the first part of the Core's `send_now`.
- Replaying a Send now after a reload is not tested on the desktop. Only the queue send is (`composer.spec.ts:669`).
- Keyboard use of Send now and of the terminals tray is not driven.

**Note:**
`IMPLEMENTED-PARTIAL. Queue, Send now, Stop, education, terminals, side question, 15 s proposal are real: composer/trays.tsx, core run_control.rs, e2e/composer.spec.ts, qual-gaps.spec.ts; scripted model, unpackaged. First missing link: no test bounds Stop near 1 s (30 s poll, Core 10 s). Send now during unknown outcome has no surface or test; no live leg.`

## PX-056: IMPLEMENTED-PARTIAL
**Proven:**
- **Variants, blocked row, pin on the wire, tray, Enter keeps the text:** `e2e/composer.spec.ts:388`, including `reasoning_effort` at :473.
- **Blocked at run with zero requests:** `e2e/composer.spec.ts:480`.
- **Another session's pin refused:** `e2e/qual-gaps.spec.ts:154`.

**First missing link:** the spec runs with no registry and asserts only the recorded preference (`composer.spec.ts:399-426`), so the next compile's profile is not driven from the surface. It is proven only at Core level (`services/modbit-core/tests/task_modes.rs:1517`, `:1874`).

**Other gaps:**
- "Survives restart" is a reload here. Core restart is proven only for the mode (`apps/cli/tests/task_modes.rs`, each CLI call is its own Core), with objective, effort and tier read back from later Core processes.
- The new badge and scrolling (task-card scope) were not seen or asserted.

**Note:**
`IMPLEMENTED-PARTIAL. Picker, pin, blocked tray and prompt restore are real: composer/model-picker.tsx, composer-ipc.ts, e2e/composer.spec.ts, qual-gaps.spec.ts; unpackaged. First missing link: the desktop spec runs with no registry and asserts only the recorded preference, not the next compile's profile (Core-only: tests/task_modes.rs). Restart is reload only.`

## PX-058: IMPLEMENTED-PARTIAL
**Proven (all in `e2e/`):**
- **Exact intent, stale hash refused, Core SIGKILL, receipt names the hash:** `approvals.spec.ts:42`.
- **N pending, one decision per effect, typed Skip, task switch (indirect):** `approvals.spec.ts:104`.
- **Shift+Enter makes a durable rule:** `approvals.spec.ts:225`.
- **Composer Enter decides nothing:** `approvals.spec.ts:198-203` and `wave4.spec.ts`.
- **Expiry:** `qual-gaps.spec.ts:376`.
- **Live model, retained:** `evidence/m10/PX-144/live-approvals-run-3.json`, Supervised only, with Always by button.

**First missing link:** no spec runs under the Ask run mode; `ASK` appears only as a bad-argument probe (`approvals.spec.ts:353`).

**Other gaps:**
- Renderer kill is a reload only (`approvals.spec.ts:75`); no crash spec holds a pending card.
- **Drift against the failure proof:** "a decision on a card of another task is refused" is contradicted. Foreign cards are deliberately decidable by button (`src/renderer/approvals/approval-deck.tsx:121`, `approvals.spec.ts:138`), and the Core checks only the session lease (`server.rs:3617` onward). This needs an owner decision or a Decision Record.
- "Stale card replaced" is unproven; the test asserts the card stays (`approvals.spec.ts:72`).
- Sandbox containment is screen-reader-only, and the plain card shows no expiry.

**Note:**
`IMPLEMENTED-PARTIAL. Dock, hash binding, deck, Skip, Always, Core SIGKILL are real: approvals/approval-deck.tsx, control-ipc.ts, e2e/approvals.spec.ts, qual-gaps.spec.ts; unpackaged. First missing link: no spec runs under the Ask run mode. Renderer kill is reload only; foreign cards are decidable by button, against the failure proof (approval-deck.tsx:121).`

## PX-060: IMPLEMENTED-PARTIAL
**Proven (all in `e2e/`):**
- **Ring tracks the Core, categories sum to the total, usage setting:** `context.spec.ts:45`.
- **After compaction:** `qual-gaps.spec.ts:173`.
- **Across a model switch:** `qual-gaps.spec.ts:202`.
- **Limit tray and raise through policy, ceiling, approval, kill:** `context.spec.ts:133`, `:223`, `:263`.
- **Retry starts one run:** `qual-gaps.spec.ts:254`.

**First missing link:** the limit tray has no staged text and no lower-cost action; a grep of the context and gallery renderers finds no staged wording, and the tray text is only the Core's detail (`src/renderer/context/model.ts:137`).

**Other gaps:**
- **Offline is not implemented as specified:** the tray keys on the Core's state only (`model.ts:127`), with no `navigator.onLine` or provider check. The test kills the Core rather than cutting the network (`context.spec.ts:306`).
- "Sending fails with the cause" is unasserted.
- The error tray is a conversation block with no request-id copy (`src/renderer/conversation/conversation.tsx:401`).
- No test for "a tray that traps Enter" (`composer.spec.ts:451` covers only the policy tray).
- No live gateway leg.
- The task card's inspector report link was not found in the ring tray.
- The per-category check compares sums, not each category against the Core.

**Note:**
`IMPLEMENTED-PARTIAL. Ring, categories, raise via budget policy, ceiling, retry are real: context/ring.tsx, tray-director.tsx, e2e/context.spec.ts, qual-gaps.spec.ts; unpackaged, scripted. First missing link: limit tray has no staged text or lower-cost action. Offline tray keys on Core state, never network; error tray is a block with no request id.`

## PX-100: IMPLEMENTED-PARTIAL
**Proven:**
- **`--mode ask` refused by the Kernel:** `apps/cli/tests/task_modes.rs:254-270`, with a control task that does write.
- **`--mode plan` writes nothing, with typed refusals:** same test, :404-460 and :308-380.
- **Plan acceptance:** `services/modbit-core/tests/task_modes.rs:910`.

**First missing link:** `packages/vscode-adapter` (`adapter.ts`, `extension.ts`, `test/suite.ts`) has no mode, objective or preference command, so the extension-host leg has nothing to drive.

**Other gaps:**
- **`packages/ide-adapter-core/src/modes.test.ts` appears never to run with a Core in CI.** The node job sets no `MODBIT_CORE_BIN`, and desktop-e2e runs only that package's `conformance` script, despite the test's comment. This is from reading `.github/workflows/ci.yml`, not run logs.
- A pin forbidden by policy is untested on CLI and IDE, which use `UNKNOWN_MODEL` only.
- The objective shows in posture but is not proven in a routing decision from the CLI (no registry) or visible from the desktop.
- The conformance suite lacks the three commands.
- CLI-to-IDE after a restart is proven at client level only.
- No retained mutation evidence, and no live gateway leg.

**Note:**
`IMPLEMENTED-PARTIAL. CLI verbs and Core enforcement are real: apps/cli/src/main.rs, apps/cli/tests/task_modes.rs; client in ide-adapter-core/src/modes.test.ts, which CI skips (no MODBIT_CORE_BIN). First missing link: packages/vscode-adapter has no mode or preference command, so no extension-host leg. Policy-forbidden pin untested on CLI or IDE.`

## PX-074: IMPLEMENTED-PARTIAL
**Proven against the fixture actuator process (`e2e/computer-surface.spec.ts`):**
- **Permission rows and grant noticed:** :44 (a file stands in for System Settings).
- **Stop, USER_ABORTED, exact card, no Always, live region for acquired and stopped:** :96, :289.
- **Stop chord behind a modal:** :392.
- **Renderer kill:** :451.
- **Core kill:** :353.

**First missing link:** no desktop spec drives the real macOS helper or real permissions; the rig always uses `modbit-actuator-fixture` (`e2e/support/computer-harness.ts:65`).

**Other gaps:**
- Presence in a second window has no product path, because the app has one window (`src/main/main.ts:955`, `:986`).
- The Stop bound in the spec is under 20 s. The 500 ms test is in `services/modbit-actuator-macos/tests/ax_fixture.rs:617`, helper-level only and in a `continue-on-error` CI step that skips without permissions.
- Presence disappearing at a plain turn end is not driven; it clears only after a latch is resolved.
- "Released" is announced only in the unit test (`src/renderer/computer/model.test.ts:29`).
- The refocus re-check exists (`src/renderer/computer/use-computer.ts:79`), but the 1 s beat can satisfy the test.
- The helper restart after a grant was not found.
- The before-thumbnail is unasserted.

**Note:**
`IMPLEMENTED-PARTIAL. Permission window, presence, Stop, cards run against the fixture actuator: renderer/computer/*, main/computer-ipc.ts, e2e/computer-surface.spec.ts. First missing link: no desktop spec drives the real macOS helper or real permissions. Single-window app, so second-window presence has no path; 500 ms Stop only in ignored ax_fixture.rs; unpackaged.`

## PX-086: IMPLEMENTED-PARTIAL
This is the closest row to done.

**Proven:**
- **Core-typed validation, Save disabled, exact enable approval, keyboard dialog:** `e2e/automations.spec.ts:75`.
- **Repository hash and revision, changed bytes:** `e2e/automations.spec.ts:231`, revision at :273 and :279.
- **Dry run, skipped and succeeded rows, origin badge, global pause:** `e2e/automations.spec.ts:328`.
- **Kill:** `e2e/automations.spec.ts:450`.
- **No file, no receipt after a test run:** `services/modbit-core/tests/automations_px.rs:3568`.
- **CLI parity, including failed:** `apps/cli/tests/automation.rs:71`, `:346`, failed at :202-221.

**First missing link:** no desktop spec shows a failed run row; only the CLI test covers failed.

**Other gaps:**
- "Pause stops the next slot" is tested on manual runs. The scheduler filter exists (`services/modbit-core/src/automation.rs:1838`), but I found no test of a paused schedule slot.
- Per-run retry and cancel are unbuilt (task-card scope, per `evidence/m10/PX-086/TASK_CARD.md`).
- Keyboard use of the editor is not driven.
- That task card is stale on two points: revision is covered, and PX-084 is now COMPLETE.

**Note:**
`IMPLEMENTED-PARTIAL. Editor, enable approval, dry run, history, pause, kill, origin badge, CLI parity are real: renderer/automations/*, automation-ipc.ts, e2e/automations.spec.ts, apps/cli/tests/automation.rs, core automations_px.rs; unpackaged. First missing link: no desktop spec shows a failed run row. Pause tested on manual runs, not a schedule slot; run retry/cancel unbuilt.`

## Could not determine
- Whether `d4c5458a` passes CI.
- Whether `modes.test.ts` ever ran with a Core outside CI.
- The Core's `send_now` behaviour past its first part, including whether it waits on an unknown outcome.

All eight notes are ASCII with no newlines, 346 to 381 characters by script count.
