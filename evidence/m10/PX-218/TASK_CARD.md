# Task Card — PX-218 The task header carries the last line the assistant wrote

## Identity

- Task ID: PX-218 (REQ-PX-218; owner core-runtime); qualification QUAL-PX-218 / PX-E2E-218; Decision Record DR-PX-2026-11-11-021 (MWS-20)
- Evidence tier: release-critical (a projection the list reads gains a field, and the canonical completion record of an assistant stream gains one)
- Prerequisites: PX-199, DOC-PX-020. Neither is on `main` yet (the record and the rows arrive with moss101/modbit#196), so this row cannot be moved by `graph.py` and was not.
- Branch `wip/desktop-2-core-header-suggest`; built together with PX-210.

## Existing-code audit (before any edit)

- Traced from the production caller to the storage boundary: the renderer's `useAgents` -> `window.modbit.agentHeaders` (preload) -> `agents:headers` (`apps/desktop/src/main/conversation-ipc.ts` `headerView`) -> `CoreClient.getAgentHeaders` -> `GetAgentHeaders` (`services/modbit-core/src/server.rs`) -> `transcript::headers` -> `EventStore::task_digests` (`crates/event-store/src/digest.rs`: grouped SQL over the session's events) and the task projection.
- What is there: the header and its digest are PRODUCTION-WORKING and are pinned to load no object (`stream_transcript.rs` asserts `objects_read == 0`). An assistant message's text reaches the log redacted (`stream.rs` `publish` redacts before any append) and its finished form exists only as a content-addressed object named by `AssistantMessageCompleted.text_ref`; the completion record held no text.
- Classification for the field: NOT-FOUND, on a PRODUCTION-WORKING projection. `AgentHeader` carried no assistant output, in the protocol, the Core, the client or the renderer's type.
- First missing link: nothing on the log that a grouped read can take the line from. The line is only inside the text object, and the header read may not open objects.

## What was built

- `crates/domain/src/stream.rs`: `last_line(text)` (the last non-empty line, trimmed; over 200 characters it is cut at a character boundary and ends in an ellipsis) and an additive `last_line` on `StreamEvent::AssistantMessageCompleted`. The store refuses a completion whose line is over the bound or is not one line. An old record (no field) still reads.
- `services/modbit-core/src/stream.rs`: the completion record is written with the line of the redacted text it points at, so the header's words are the transcript's words by construction.
- `crates/event-store/src/digest.rs`: `TaskDigest.last_message`, the latest finished TEXT stream's record per task, read by the database. A stream that is open or aborted, and a reasoning summary, are not read. For an archived task it is the latest message finished before the archive.
- `services/modbit-core/src/transcript.rs`: `AgentHeader.last_line` (`stream.proto`, field 460, additive). A record older than the field has its line only in its object: that object is read once per Core process and the line is held to the bound.
- Mapped through: generated TypeScript (`packages/surface-protocol`), `conversation-ipc.ts` `headerView`, `AgentHeaderView.lastLine` (`apps/desktop/src/shared/conversation-types.ts`); the preload passes the view through. Protocol fixture `agent_headers` carries the field on both sides.
- No UI: the row's preview card is another row's (PX-199) and reads `lastLine`.

## Verification

All local, macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1. The model is the repository's scripted stand-in; the Core, its SQLite store, its object store and its socket are real.

- `cargo test -p modbit-core --test stream_transcript qual_px_218`: 1 passed (`px218-core.log`). One real Core, one task: no line before it answers; none while the first message streams (held open by the scripted model with its first deltas on the log); after it finishes the line equals the end of the transcript's message, with the held credential and the credential-shaped token both `[redacted]`, and the header read loads no object; the same line after the Core is killed and started again on the same store; a second message held mid-stream leaves the first one's line; a 360-character line of two- and four-byte characters comes back as 200 characters ending in an ellipsis and is a prefix of the line; archived, the header keeps its line when the task answers again; unarchived, it shows the new one.
- `cargo test -p modbit-event-store --test last_message_digest`: 3 passed (`px218-digest.log`): latest finished text only (open, aborted and reasoning streams ignored), the same after the store is reopened, frozen at each archive, and an old record named without a line.
- `cargo test -p modbit-domain --lib stream`: 5 passed, 2 of them new (the cut at a character boundary; the store's bound on the record).
- `cargo test -p modbit-core --bin modbit-core the_last_line_of_a_record`: 1 passed: on a real store, an old record's object is read once and not again; a record with its line reads no object.
- Protocol fixtures: `cargo run -p protocol-fixtures`, `pnpm --filter @modbit/surface-protocol test` 53 passed, `cargo test -p protocol-fixtures` 2 passed.
- Desktop: `e2e/px-210-suggestion.spec.ts` reads `agentHeaders` over the real bridge after a real turn and finds `lastLine === "It holds three lines."`.
- Negative proofs kept: `mutation-check-2026-11-11.txt`. With the archive bound removed the Core test fails at "the header of an archived task changed"; with unfinished streams counted it fails at "an unfinished message contributes nothing".
- The Core's full suite, the workspace suite and the lints: see `../PX-210/TASK_CARD.md` (one run covers both rows).

## Limitations

- The preview card (PX-199) is not in this change, so PX-E2E-218's "read the header and the card" is proven for the header only.
- The line is the message's last non-empty line as written: a message that ends in a code fence or a list marker gives that. Nothing is summarised.
- A message finished before this build has no line on its record. The header reads it from the text object, once per Core process per message; for such a store the first header read after a start opens one object per task whose latest message is old. That path is proven on a real store by a unit test, not by a real-Core test (a real Core cannot write an old record).
- "Unchanged for an archived task" is built as: the line is read as of the archive. A task archived before it ever answered therefore shows none until it is brought back.
- macOS only so far. Linux and Windows are CI's.
- Not sealed: the row is not on `main`, and sealing is not this task's.

## Addition (2026-11-12, with PX-200): the preview card and the desktop's end-to-end proof

Built on `wip/desktop-2-task-view`, one change with PX-200. It closes the first limitation above: the card now shows the line, and PX-E2E-218's "read the header and the card; restart; read them again" is proven for both.

- **What was built.** `apps/desktop/src/renderer/agents/preview-card.tsx` shows `AgentHeaderView.lastLine` under the state and its reason, when the header has one (`agent-preview-last`; at most three lines of the card; "The assistant's last line:" before it for a screen reader). The card still makes no read of its own: the line is the header's field, which the list already holds. No Core, protocol or bridge change.
- **Proof** (`apps/desktop/e2e/px-218-last-line.spec.ts`, the real app, a real Core, a scripted model whose answer ends with a known line holding a credential-shaped token; `px218-e2e.log`): while the message is unfinished (the model held mid-answer) the header has no line; after the turn the header's line is "The token [redacted] is in the last one.", equal to the last line of the transcript's message; the token is nowhere in the transcript or the page; the card shows exactly the header's line, under the state; a task that has not answered has no line and its card draws none; after the app and its Core are stopped and started again on the same data the header's line and the card are the same, and the unanswered task still has none. Axe on the card.
- **Screenshots** (`after/`, the running app at 1440 by 900, light and dark): the card with the line before and after the restart.
- **Still not shown here:** a long line cut at a character boundary and an archived task's line are the Core's tests (above), not repeated through the desktop. The card's line was looked at on one line; the three-line clamp was not looked at with a long line.
- The card closes when focus leaves it, and the list moves focus to a row once after a task is created or the app starts; the spec opens the card again if that happened under it. That is PX-199's behaviour, seen while writing this proof and not changed.

## Closure (2026-11-12)

- CI run 43027055220 (header field, merged as 418af649, #197) and CI run 43198251756 (the preview card shows the line; PX-E2E-218 on the real app, merged as 441a70ac, #199): all checks passed.
- The owner reviewed the screenshots of the running app beside the boards and answered "Yes, seal them" on 2026-11-12.
