<!-- Subagent report, 2026-11-07, code read at d4c5458a, nothing run. Spot-checked, not proof; see ../AUDIT.md. -->

# Group A audit against d4c5458a (PX-042, 046, 047, 064, 062, 068, 066)

**All seven rows are IMPLEMENTED-PARTIAL.** None is NOT_STARTED in substance and none reaches PRODUCTION-WORKING. Each has real Core, IPC and UI paths and non-mock tests, plus at least one unproven clause.

## Limits of this audit
- I read code and tests and ran nothing. I did not check CI results at this commit.
- Paths without a prefix sit under `apps/desktop/`.
- **Packaging:** there is no packaging config (no electron-builder, forge or asar). `launch()` runs `dist/main/main.cjs` unpackaged (`e2e/support/ui-harness.ts:35`), despite its comment saying "packaged". The "packaged app" clause of PX-046, 047, 062, 064 and 068 is unmet in every case.
- **Budgets:** `perf-budgets.spec.ts:37-38` asserts budgets only when `MODBIT_PERF_PROFILE=reference`. CI runs profile `ci`, which records them and does not assert. I found no retained perf result in `evidence/`.
- **Docs:** `docs/30` does not mention `GetAgentHeaders`, `GetTranscript`, `ListWorktrees` or `ApplyWorktree`, although these task cards require it to be updated.
- **Other e2e scope:** I traced the specs named below, not every spec in the suite. An absent test claim below means none in those specs or in my greps.

---

## PX-042: IMPLEMENTED-PARTIAL

**Proven**, all by `services/modbit-core/tests/stream_transcript.rs` driving the real binary over its socket:

| Clause | Evidence | Test |
|---|---|---|
| Headers load no transcript | `transcript.rs:1289`, `server.rs:4949`; `objects_read == 0`, `events_read` below half a transcript (:2411-2417) | `qual_px_042_headers_for_two_hundred_tasks_...` |
| Three densities, same atoms, after SIGKILL | `transcript.rs` `page`, `server.rs:4898` | `qual_px_042_the_transcript_reproduces_...` |
| Search over bodies, rebuilt after a kill | `conversation_search.rs` | `qual_px_042_search_finds_bodies_...` |
| Stale cursor and other-session refusals | :1911-1961 (`INVALID_CURSOR`, `WRONG_SESSION`) | same transcript test |
| Archive replay gives one event | `server.rs:4972` | `qual_px_042_read_markers_and_archive_...` |
| Precedence function | `transcript.rs:1248` | unit test `the_first_condition_of_the_precedence_wins...` |

**First missing link:** no test deletes the projection tables and restarts, and `recover_on_start` (`crates/event-store/src/store.rs:1006-1020`) rebuilds only on cursor lag or project-table disagreement.

**Other gaps:**
- The only delete test is store-level and forces cursor lag first (`migrations_projections_commands.rs:581-583`). It never goes through headers or a real Core.
- I could not determine what a literal `DROP TABLE` does at open.
- FAILED, WAITING and a true COMPLETED are not seeded on a real Core. COMPLETED appears only as a cancelled draft.
- The fixture has 199 tasks, not 200.
- Headers after restart compare counts and two classes, not bytes.
- Unarchive is never replayed with the same id.
- A "client-supplied class is ignored" holds by construction (no such field), with no test.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: services/modbit-core/src/transcript.rs, conversation_search.rs, server.rs; proof tests/stream_transcript.rs qual_px_042_*. First missing link: no test deletes projection tables and restarts; recover_on_start rebuilds only on cursor lag. FAILED, WAITING and a true COMPLETED are not seeded on a real Core; fixture has 199 tasks.`

---

## PX-046: IMPLEMENTED-PARTIAL

**Proven** (`e2e/agents.spec.ts` unless noted). Path: `renderer/agents/use-agents.ts:68`, `main/conversation-ipc.ts` `agents:headers`, Core headers.
- Glyph and words for DRAFT, plus words for RUNNING and READY_FOR_REVIEW seen and unseen (:43-65).
- NEEDS_ATTENTION with its attention dot surviving a reload and a real Core SIGKILL (`conversation.spec.ts:213-236`).
- Archive, Undo, and selection moving to the sibling (:120-135).
- Running-task archive asks first, and stopping is the Core's cancellation (:162-192).
- Filters, grouping, keyboard `p`/`e`, axe; 1000 tasks windowed (:200).

**First missing link:** no real-Core spec renders every class (FAILED, WAITING and COMPLETED are never driven), and the glyph is asserted only on DRAFT.

**Other gaps:**
- **Cloud clause:** vacuous, since the desktop has no cloud client (DR-020 section 5.8) and nothing seeds a cloud task.
- **150 ms p95:** recorded, not asserted (`perf-budgets.spec.ts:199`).
- **Undo for at least 8 s:** code only (`agent-list.tsx:22` sets `UNDO_MS = 12_000`); no test measures it.
- **No transcript loads:** comment-level only, with no counter assertion.
- **Pin cap and single-source check:** unit tests only (`list-model.test.ts:28,74`).
- **Renderer restart:** `page.reload`, not a kill.
- **Scope:** no recent-task switcher in `keyboard.ts` or `shell/`.
- **Packaging:** packaged app clause unmet.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: renderer/agents/use-agents.ts, main/conversation-ipc.ts, Core headers; proof e2e/agents.spec.ts, conversation.spec.ts. First missing link: no real-Core spec renders every class (FAILED, WAITING, COMPLETED missing). Cloud clause vacuous (no cloud client), 150 ms p95 only recorded on CI profile, no recent-task switcher, unpackaged.`

---

## PX-047: IMPLEMENTED-PARTIAL

**Proven** (`e2e/conversation.spec.ts`, scripted model):
- Progressive streaming, a monotonic length check and resume after reload (:24, :71).
- A Core kill leaves an aborted row, not an answer (:103).
- Follow freeze, the pill and scroll-to-bottom; HTML is inert (:132).
- Density and folding keep the rows, and the approval stays visible (:190-274).
- `TailStatus` labels from `transcript.rs:995`, with the stall ladder unit-tested (`model.test.ts:113`).

**First missing link:** a hung command raises no STALL. `attention.rs:203,242` raises it only for a Waiting task with `NO_PROGRESS`, there is no threshold or watchdog, and the wording ladder is renderer-only (`model.ts:268`) with no e2e on `data-stall`.

**Other gaps:**
- **Provider failure before any token:** shows `FailureBlock` in the transcript with Retry and "Copy your prompt" (`conversation.tsx:381`, `qual-gaps.spec.ts:254`). The prompt goes back in the box and a tray docks only for model refusals (`composer.tsx:434`). DR-020 section 5.9 places the block in the conversation but does not supersede this clause, so it needs an owner decision.
- **Planted instruction:** tested in assistant text only. Tool rows show arguments, not results, so the tool-output case is untested.
- **400 words and live gateway:** not exercised. The two live specs are env-gated and do not cover these clauses.
- **100 ms p95:** recorded only.
- **1,000 rows:** CI default reaches about 300, and the dossier gives no frame budget (flagged in the spec as an owner question).
- **Humanised running-command wording:** no e2e assertion.
- **Scope absent:** find in conversation, jump to previous or next user message, and a virtualised list (it uses `content-visibility`, with all rows in the DOM).
- **Packaging:** packaged app clause unmet.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: renderer/conversation/*, GetTranscript, delta events; proof e2e/conversation.spec.ts. First missing link: a hung command raises no STALL: attention.rs raises it only for a Waiting task with NO_PROGRESS; ladder is renderer wording, no e2e. Provider failure does not restore prompt or dock a tray; no tool-output plant test; unpackaged.`

---

## PX-064: IMPLEMENTED-PARTIAL

**Proven** by `e2e/projects.spec.ts:90` (real Core and Git). Path: `agent-list-view.tsx:384-399,489`, `main/project-ipc.ts:260-306`, `server.rs:1493`, `projects.rs`.
- Projects group separate from repositories (:133).
- Add by menu, by drag and by keyboard, read back from the Core (:147-158, :178-187).
- Three forbidden moves show the Core's reason and change nothing (:161-174).
- Remove from the project page (:209).
- Project grouping, filter chip and stored view (:215-242).
- Layout persists across a full app restart (:245-253).
- Archive and undo by keyboard (:264-283).

**First missing link:** there is no pending state for a two-phase project operation in `projects.proto`, `project-types.ts` or the renderer. The Core-side kill recovery is proven (`projects_px.rs:1388`) but never surfaced.

**Other gaps:**
- Remove by drag does not exist; the drop handler only adds.
- "Remove from X" in the list menu exists (`project-model.ts:108`) but no spec drives it.
- Keyboard-only remove is not driven.
- There is no transcript-read counter assertion.
- Packaged app clause unmet.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: renderer/agents/agent-list-view.tsx, main/project-ipc.ts, Core projects.rs; proof e2e/projects.spec.ts PX-063/064 test. First missing link: no pending state for a two-phase project operation in proto or renderer. Remove by drag absent, list menu remove undriven, no transcript-read counter assertion, unpackaged.`

---

## PX-062: IMPLEMENTED-PARTIAL (nearest to done of the desktop rows)

**Proven** by SHA-256 on disk in `e2e/checkpoints.spec.ts`. Path: `restore-dialog.tsx`, `use-checkpoint-surface.tsx`, `main/control-ipc.ts:283,307,337`, Core `checkpoint.rs`.
- Preview-driven dialog and Cancel (:61-72).
- Double-clicked Continue restores once (:79-82).
- Redo restores exactly, by keyboard (:86-96).
- Undo all needs a second click (:99-109).
- An edited file blocks Continue, needs a per-file choice, and there is no do-not-ask option (:161-172).
- Edit in place without re-sending (:285-289).
- Keep on Shift+Enter and Revert (:306-321).
- Fork (:180).
- A foreign checkpoint is refused, and a same-command-id replay restores once (`qual-gaps.spec.ts:105`).
- A stale redo is refused (`qual-gaps.spec.ts:303`).

**First missing link:** the "renderer kill during the dialog" clause uses `page.reload()` (:154), although a real `forcefullyCrashRenderer` harness exists at `workers.spec.ts:299`.

**Other gaps:**
- "Replayed after a reload" is sent twice without a reload.
- The Core's counts are shown, but only the file list is compared directly.
- Task-card scope not found: the changes scope menu and the Changes +N, Commit, Continue chips.
- Prerequisite PX-061 is REAL_TESTING.
- Packaged app clause unmet.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: conversation/restore-dialog.tsx, use-checkpoint-surface.tsx, main/control-ipc.ts, Core checkpoint.rs; proof e2e/checkpoints.spec.ts, qual-gaps.spec.ts, SHA-256 on disk. First missing link: the renderer-kill clause uses page.reload, not a real crash. Changes scope menu and Changes chips not found; unpackaged.`

---

## PX-068: IMPLEMENTED-PARTIAL

**Proven** by `e2e/worktrees.spec.ts`. Path: `renderer/worktrees/*`, `main/project-ipc.ts:316-356`, `server.rs:4547`.
- The list matches the filesystem and the Core's flags (:99-121).
- Typed removal refusals (:127-156).
- A real conflict opens the modal, with Cancel as the default (:245-261).
- The Core refuses an untyped overwrite (:265).
- Typed overwrite, an approval with the intent, and hashes on disk (:271-297).
- Undo restores exactly (:301-307).
- A destructive choice is not remembered and a non-destructive one is (:312, :323-347).
- A skipped cleanup is shown as Skipped (:378).
- The renderer dependency scan (`projects/renderer-scan.test.ts`).

**First missing link:** STASH, FULL_OVERWRITE and UNDO_AND_APPLY are never driven through the modal. Only Cancel, Overwrite and Merge Manually are, so "each option" is unproven at this layer.

**Other gaps:**
- No keyboard-only operation of the modal; the spec uses mouse clicks and checks focus only.
- Task-header actions, Apply to specific files, Open terminal in worktree and Copy path are absent.
- Worktrees open from the gear menu or Settings (`settings-page.tsx:297`), and Apply lives only on that page (`worktrees-panel.tsx:176`).
- DR-020 section 5.3 does not supersede the header actions.
- Retention is read-only text (`worktrees-panel.tsx:92`).
- Prerequisites PX-048, 062 and 066 are not COMPLETE.
- Packaged app clause unmet.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: renderer/worktrees/*, main/project-ipc.ts, Core worktrees.rs, apply_back.rs; proof e2e/worktrees.spec.ts, hashes on disk. First missing link: STASH, FULL_OVERWRITE and UNDO_AND_APPLY are never driven through the modal. No keyboard-only modal proof; task-header actions, specific-file apply, copy path absent; unpackaged.`

---

## PX-066: IMPLEMENTED-PARTIAL (closest overall)

No packaged clause applies to this row.

**Proven** by `services/modbit-core/tests/worktrees_px.rs`, the real CLI test `apps/cli/tests/px_066_apply_back.rs`, and `crates/git/tests/apply_back.rs`. Path: `apply_back.rs:449,744,839,1035,1143`, `crates/git/src/apply.rs`.
- A clean apply equals the candidate by hash, and approval is required (`px_066_apply_is_a_protected_effect...`).
- Stale revision and stale plan are refused, and a forged plan is refused under a real approval (`px_066_a_stale_review_or_plan...`).
- Merge manually, Overwrite, Stash, Full overwrite and Cancel, each checked by hash after the option and after Undo (`px_066_conflicts_are_typed...`, `qual_px_066_cli_every_conflict_option...`).
- Undo after a second edit reports the conflict (`px_066_undo_after_a_second_user_edit...`).
- A remembered destructive choice is refused (`px_066_a_remembered_choice...`).
- Process death at three points recovers whole (`px_066_a_core_killed_*`).
- The receipt names the intent, with both revisions on the ack and `WorktreeApplied` (`qual_px_066_cli_clean_apply...`).
- A mutation check is retained (`evidence/m10/PX-066/mutation-check.txt`).

**First missing link:** UNDO_AND_APPLY is never exercised with a prior apply in force. Every test asserts only `available == false`, and `apply_back.rs:482` returns the undo's ack where its comment says the call "goes on as a plain apply".

**Other gaps:**
- **Same command id:** "replay with the same command id" is never sent, since `Fx::cmd` issues a fresh id each time. The ack at `server.rs:4547-4562` is never `Replayed`, and the second call's safety is `NOTHING_TO_APPLY`. The retained task card admits this.
- **Kill method:** the kill is `abort` at fault points, not an external SIGKILL at an arbitrary instant. It is a real process death, but the wording differs.
- **User edit makes it stale:** proven only at the digest level (`the_plan_digest_changes_when_the_checkout...`). The Core's `STALE_PLAN` test moves the worktree, not the checkout.
- **`WorktreeApplyUndone`:** the event exists (`worktrees.rs:58`) but no test asserts it.
- **Graph drift:** the graph says NOT_STARTED while `evidence/m10/PX-066/TASK_CARD.md` records this work. Prerequisite PX-061 is REAL_TESTING.

**Audit note:**
`IMPLEMENTED-PARTIAL. Real path: services/modbit-core/src/apply_back.rs, crates/git/src/apply.rs, apps/cli/src/worktree_verbs.rs; proof tests/worktrees_px.rs px_066_*, apps/cli/tests/px_066_apply_back.rs. First missing link: UNDO_AND_APPLY is never exercised with a prior apply in force. Same-command-id replay is never sent; ack is never Replayed; PX-061 open.`

All seven notes are plain ASCII, have no newlines, and run 341 to 366 characters.

## Leads from the earlier look
"Look built" was right about real code existing and wrong as a closure claim. The real product gaps, as opposed to test-fidelity gaps, are:
- the hung-command STALL (PX-047)
- prompt restore and error tray (PX-047, needs an owner decision)
- the project pending state and remove by drag (PX-064)
- worktree header actions (PX-068)
- the changes scope menu (PX-062)
- the recent-task switcher (PX-046)
