<!-- Subagent report, 2026-11-07, code read at d4c5458a, nothing run. Spot-checked, not proof; see ../AUDIT.md. -->

All four rows are IMPLEMENTED-PARTIAL. All audit was by reading code and tests at d4c5458a; I ran nothing. None of the four is NOT_STARTED in substance, so the graph is stale. A retained task card and mutation check exist only for PX-103 and PX-104 (`evidence/m10/`); PX-127 and PX-128 have none.

## PX-103: IMPLEMENTED-PARTIAL

**Proven clauses.** All are in `services/modbit-core/tests/px_103_hunks.rs`, which drives the real Core binary, socket, SQLite, Git and child processes. The model is scripted.

| Clause | Code path | Test |
|---|---|---|
| Hunk names the `change.apply` call | `hunks.rs:229-307`, `review.rs:144,185-210` | `qual_px_103_hunks_name_their_call...` :386-396 |
| Hunk names the shell redirect call | `tools.rs:1848`, `crates/tools/src/direct.rs:1252` | same test, :397-404 |
| Hunk names the test-runner call | barrier via `shell.exec` | same test, :405-407 |
| Mid-run reject reverts exactly that hunk, by hash | `server.rs:8002`, `changes.rs:497,636-637,704` | same test, :428-448 |
| Agent continues and answers the typed notice | `runtime.rs:3790`, `hunks.rs:856` | same test, :501-527 |
| Stale decisions refused | `changes.rs:544,640`, `hunks.rs:771,792` | same test, :413-426, :462-466 |
| SIGKILL after the ack recovers to one outcome | `Child::kill` is SIGKILL | `qual_px_103_sigkill_after...` :604 |
| Unattributed hunk shown and auto-accept blocked | `hunks.rs:807`, `review.rs:210` | `qual_px_103_an_unattributable_hunk...` :534 |

The shell-attribution mutation is retained in `evidence/m10/PX-103/mutation-check.txt`.

**First missing link:** the revert's file write (`changes.rs:704`) and the `HunkDecided` commit (`changes.rs:776`) are two separate steps with no journal I could find. A kill between them leaves the hunk reverted with no decision and no notice, and a retry gets `STALE_REVISION`. The comment at `hunks.rs:16-17` ("holds both or neither") overstates this.

**Other unproven:**
- The live gateway leg has not been run.
- The real `test.run` tool is not driven. It shares `run_guarded` (`direct.rs:2929`), but the test uses `sh run_tests.sh` through `shell.exec`.
- A reject during an in-flight shell call is not tested, since the test holds at the model. I could not determine whether the barrier would credit the person's revert to that call.
- Nothing sends `AUTO_ACCEPT` automatically, so the block is proven only at the command level.
- Prerequisite PX-061 is REAL_TESTING.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: services/modbit-core/src/hunks.rs, changes.rs revert_impl, tools.rs:1848, runtime.rs:3790; test services/modbit-core/tests/px_103_hunks.rs (read, not run; real Core, scripted model). First missing link: revert write (changes.rs:704) and HunkDecided commit (:776) are not atomic; a kill between leaves no decision or notice. Live gateway leg not run.`

## PX-104: IMPLEMENTED-PARTIAL

**Path:** `changes-app.tsx:121` → `preload.ts:528` → `changes-ipc.ts:216` → `ide-adapter-core/changes.ts:88` → Core. It is mounted at `apps-panel.tsx:56` and `shell.tsx:456`, so it survived #169. DR-020 section 5.5 changes only layout (hover actions, `styles.ts:212` covers focus), and no clause is superseded.

**Proven clauses,** in `apps/desktop/e2e/px-104-live-review.spec.ts` with a real Core and real Electron:
- Tab open while Running, with the chip naming the call (:104).
- Chip jumps to the producing call (:123).
- Reject by keyboard mid-run, file hash changes, agent continues, notice shown (:134).
- Waiting on `user.ask`: stale refusal shows the Core's code, and the refresh works (:156).
- Unattributed hunk shown and accepted only by the person (:172).
- Renderer crash restores the panel from the Core (:185).
- axe on the panel (:120, :182).
- ReadyForReview tab (`changes-app.spec.ts:107`).
- The dropped-revision mutation is retained.

**First missing link:** `canDecide` (`changes-model.ts`) checks only busy and binary. Reject therefore stays enabled on a staged hunk that the Core refuses with `DIRTY_CONFLICT` (`changes.rs:568`), which is exactly QUAL-PX-104's "control enabled for a hunk the Core would refuse".

**Other unproven:**
- The waiting-for-approval, failed and completed states are never driven; there is no assertion on them anywhere in `e2e`. The task card's claim that `changes-app.spec` covers completed is wrong: it asserts ReadyForReview.
- "Every control works by keyboard": only Reject is driven by keyboard; Accept and the chip are clicked.
- The packaged clause is not met. The spec header says "packaged", but `ui-harness.ts:38` runs `dist/main/main.cjs`.
- The live gateway leg has not been run.
- `hunks::decide` has no task-state gate, and the docblock at `changes-app.tsx:19-20` says ReadyForReview decisions stay in Review while the controls stay enabled. Behaviour on a Completed task is unproven.
- Prerequisites: PX-103 and PX-062 are NOT_STARTED, PX-048 is REAL_TESTING.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: changes-app.tsx, changes-ipc.ts, preload decideHunk, ide-adapter-core changes.ts; test apps/desktop/e2e/px-104-live-review.spec.ts (read, not run; unpackaged, scripted model). First missing link: canDecide in changes-model.ts leaves Reject enabled on a staged hunk the Core refuses DIRTY_CONFLICT. Approval-wait, failed, completed states undriven.`

## PX-127: IMPLEMENTED-PARTIAL

**Proven clauses, against a GitHub fake only** (a real bare remote, a fake REST API):

`apps/cli/tests/px_127_forge.rs`, `qual_px_127_issue_to_pull_request...`, which is real CLI binary to real Core:
- Issue to task to approved PR.
- `task evidence` shows the failing run with provenance (:345-364).
- Green and failing CI do not change task state (:365, :402).
- Untrusted comments with escapes stripped (:432-458).
- Follow-up turn answers the comment (:464-499).
- Evidence survives a new Core (:502-517), by idle exit rather than SIGKILL.

`apps/desktop/e2e/forge-evidence.spec.ts`:
- Review shows the failing run and provenance (`review.tsx:384-401`).
- Comments stay inert text.
- Evidence survives a full app restart.
- Answered flips to true.

**First missing link:** nothing has ever run against real GitHub. Every test points `MODBIT_GITHUB_API_BASE_URL` at a fake, no workflow sets a real token, and there is no retained run id or PR URL. The row says this records BLOCKED until the owner supplies a test repo and token.

**Other unproven:**
- Digest: Review shows only `logRef`, and only when a log exists.
- "The thread": comments carry path and line but no reply grouping (`preload.ts:65`), and Review does not render the URL.
- Green CI in Review is not driven; only the CLI leg covers it.
- The failing-to-success mutation is not retained.
- The renderer-only restart is not separated from the full app restart.
- The desktop spec starts from a typed goal, not an issue.
- Locally, CI arrives only through the "Read CI results" button or `ci ingest`. The webhook route is cloud-side only (`session.rs:711`).
- Packaged app not exercised.
- Prerequisites: PX-125 is WIRED, PX-126 is NOT_STARTED.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path over a GitHub fake: ci_evidence.rs, review_comments.rs, review.tsx:384-423, apps/cli/src/forge_verbs.rs; tests apps/cli/tests/px_127_forge.rs, e2e/forge-evidence.spec.ts (read, not run). First missing link: no run on real GitHub, so no retained run id or PR URL (needs owner token and repo). Digest partial, no reply thread, unpackaged.`

## PX-128: IMPLEMENTED-PARTIAL

**Proven clauses, CLI half only,** in `apps/cloud-worker/tests/px_128_cloud_client.rs` with the real CLI, Core, in-process Cloud API, Postgres and worker. It runs in the CI cloud job and is skipped without a database.

| Clause | Code path | Test |
|---|---|---|
| Sign-in | `cloud.rs:324` | :221 |
| Policy refusal, nothing leaves | `server.rs:6117` | :260-281 |
| Secret refusal | `handoff.rs:140,182` | :285-299 |
| Confirmation lists what leaves; nothing sent before it | `cloud.rs:520,605` | :302-349 |
| Cloud continuation in a sandbox | worker | :391-405, :549-560 |
| Approval with exact intent, second answer refused | `cloud.rs:1076` | :456-495 |
| Watch resumes by cursor | `cloud.rs:994` | :578-651 |

**First missing link:** the desktop has no cloud client. There is no cloud code in preload or main IPC, `location.ts:3` says so, home's menu item is disabled (`home.tsx:34`), and DR-020 section 5.8 confirms PX-128 owns it.

**Other missing or unproven:**
- `cloud return` and the delta coming back do not exist in the CLI or the API. The test header says so, and the local task never continues or completes.
- No Continue in Cloud action or cloud-sessions list in the desktop. `list-model.ts` can show `executionLocation`, but no source supplies cloud rows.
- `ide-adapter-core` has only `exportHandoff`, with no HTTP client.
- "Answered from the desktop" is a raw test HTTP call. The second refusal asserts only a non-zero exit, not "already decided".
- `handoff.rs:138-139` admits a secret committed in history crosses in `repo.bundle`. Files over 1 MiB or containing NUL are also not scanned.
- The local-only clause has code (`crates/tools/src/computer.rs:40`, `local_trusted` only) but no handoff test asserts it.
- The plan step already parks the local run (`handoff.rs:190`) before confirmation, and declining is not tested.
- The API is not a real staging Cloud API, the sandbox in this test uses the reference backend, and the app is not packaged.
- Prerequisite PX-042 is NOT_STARTED.

**Audit note:**
`IMPLEMENTED-PARTIAL. CLI half real: apps/cli/src/cloud.rs, core handoff.rs; test apps/cloud-worker/tests/px_128_cloud_client.rs (read, not run; real Postgres and worker, local API). First missing link: desktop has no cloud client (no preload or main IPC; location.ts says so). Also no cloud return or delta back; committed secrets cross; unpackaged.`

## Who owns the desktop rendering of the cloud browser stream

No open work item owns it. Searches of docs/62 and docs/49 found no row covering it, and neither did a search of the graph's open rows. The desktop and `ide-adapter-core` have no `WatchBrowserView`, `browser_frame` or `/v1/browser-sessions` call.

The nearest rows do not cover it:
- **REQ-EV-0283 (docs/40:295):** "Dock/expand/pop-out/remote viewer share one session". This is the only clause that names a remote viewer. IMP-EV-0283 is already COMPLETE at M7 on local-browser evidence, and its qualification speaks only of takeover.
- **M8.8:** "cloud browser remote stream/CDP" is COMPLETE with empty acceptance text. Its proof is a protocol client, and docs/22 carves the desktop out.
- **PX-128:** names "a desktop Continue in Cloud action with a cloud-sessions list" and no browser view.
- **PX-177:** "a live view of the target" is about human handover, not the cloud stream.
- **REQ-PX-048:** "the browser tab keeps its lease badge and takeover" concerns the existing tab.

So the gap is orphaned. Giving it an owner would need a new row, or a Decision Record adding it to PX-128.
