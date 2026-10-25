# Workspace, Git, Worktrees, Diagnostics, and Trusted Code Surface

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Canonical workspace

Filesystem + Git revision are authoritative. UI buffers are never canonical because Modbit has no embedded IDE/editor architecture.

**Proposed scoped supersession (DR-PX-2026-10-03-009, `67_WORKSPACE_EDITOR_SPECIFICATION.md`, not in force until the record is accepted).** A Workspace Editor may hold a non-authoritative draft overlay over a revision-pinned snapshot; filesystem plus Git revision stay the only truth and every write is a ChangeTransaction. No IDE architecture, Monaco or Code-OSS.

`WorkspaceRevision` is a monotonic Modbit revision linked to Git HEAD, worktree identity and a content fingerprint of changed files. Every CodeReference and ContextPack binds to it.

## Workspace File Service

All model/tool writes use typed operations: read, stat, list, apply patch, atomic replace, create, delete, mkdir and move. Path normalization occurs before policy; symlink traversal is resolved and checked against allowed roots/protected paths. Writes use optimistic revision preconditions to prevent blind overwrite.

## Git strategy

- Coding task defaults to a dedicated branch + worktree.
- Read-only analysis can share immutable snapshot.
- Concurrent builders use separate worktrees.
- Merge/rebase is a typed Git operation with conflict evidence, never hidden shell magic.
- User can choose to merge, export patch/branch, open PR, or discard.

No task writes directly to the user's active worktree unless the explicit permission profile allows it.

## Headless diagnostics

Modbit launches language servers independently of any IDE. `diagnostics` crate manages server discovery/configuration, document sync from canonical files, health, timeout and normalized errors/warnings/symbols. Language capability follows the tiers of `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`: Tier A has language services, Tier B structural parsing with build-output diagnostics, Tier C text-safe edits with configured-command evidence, and Unsupported languages are read-only unless the user opts in per task; degradation is always explicit.

## Trusted Code Surface

Renderer requests immutable file/diff payloads from Core:

```text
CodeViewModel {
  workspace_revision
  file_revision
  path
  content_ref
  syntax_language
  symbols[]
  diagnostics[]
  changed_ranges[]
  evidence_links[]
}
```

Display supports syntax highlighting, line anchors, symbol outline, diff, diagnostics and test/evidence links. It does not own unsaved editor buffers.

## Stale reference handling

A CodeReference carries workspace/file revision. If later edits invalidate the line/symbol mapping, UI marks it stale and asks Core for remapping; agent context must not treat old line numbers as current truth.

## Direct user edits

P0 does not build a general editor. “Open externally” uses OS/editor URI integrations where available. A constrained inline patch action is approved for Beta (DR-PX-2026-09-05, PX-005): it goes exclusively through the canonical ChangeTransaction on the Workspace File Service with revision precondition, path policy after symlink resolution and provenance `user_direct_edit`; it creates no buffer model in any client, and external development-environment adapters use the same command (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`). Pull requests from a reviewed result use the typed Git branch push plus the `forge.pr.*` tools as protected external effects (PX-007).

As built (PX-005): `services/modbit-core/src/user_patch.rs` serves `ApplyUserPatch` (doc 30). The task's loop must not be running; the workspace revision the client saw is required and must be current, and an optional file revision (the content hash the review showed) must be too — otherwise `STALE_REVISION` and nothing is opened; the path is resolved through symlinks and checked against the protected patterns before the file is read (`PROTECTED_PATH`, `PATH_OUTSIDE_ROOT`; a symlink named inside the tree that resolves to `.env` is refused as `.env`); `old` must match exactly once at a ladder tier (`NO_UNIQUE_MATCH`); the write is one `ChangeOp::Edit` through `WorkspaceService::apply_transaction` with both preconditions, so the file service advances the revision exactly once; `FileChanged` (provenance `user_direct_edit`, the command id where a tool call id would be) and `UserPatchApplied` land in one transaction under the command record, and a retry with the same command id replays without writing. A code reference bound to the old file revision reads `stale` from `GetCodeView`. Clients: the review surface's per-file "Edit" panel (two fields that are the command's arguments, cleared once the Core answers, re-read from the Core after) and `modbit-cli task patch`. Test: `qual_px_005_a_user_edit_lands_only_through_the_change_transaction`; desktop `e2e/user-patch.spec.ts`.

## Hunk attribution and live per-hunk decisions

As built (PX-103, `services/modbit-core/src/hunks.rs`): each hunk of the review projection (`HunkView`) carries the tool call, turn and run that produced it, whether it is attributed, the basis (`CHANGE_TRANSACTION`, `BARRIER_DIFF`, `USER`, `UNATTRIBUTED`), a `hunk_digest` (sha256 over its lines, so a neighbour's revert does not change what the hunk is), a recorded `ACCEPTED` decision and `auto_accept_eligible`. Attribution is a replay of the workspace log over the file: the base (HEAD) text is folded forward through every `FileChanged` the file service recorded and every `BarrierChangeRecorded` the change barrier recorded for a shell or test process (FIX-06: the snapshot diff, with the content after the call kept by reference), and each line remembers the call that wrote it. A file that moved without a recorded call, an unreadable record or a base line that does not resolve leaves lines unattributed: the hunk says so and is never accepted automatically (`UNATTRIBUTED_HUNK`). The barrier record is a separate event type so that undo, the completion gate and verification continue to act only on the file service's own `FileChanged`.

`DecideHunk` (doc 30) is the person's per-hunk decision in any task state, a live run included. REJECT reverts exactly that hunk through the same ChangeTransaction as `RevertChanges` (checkpoint first, file-revision and content-hash preconditions, protected paths refused) and records `HunkDecided` in the same transaction; a decision against a moved file is `STALE_REVISION`, against a hunk that is no longer the one shown `STALE_HUNK`. The agent is told at its next round boundary by a labelled Core notice carrying the person's reason (`HunkNoticeDelivered` records the delivery, so a resumed run does not hear it twice and an undelivered notice is offered again); the run is not restarted. ACCEPT records that a person accepted that exact hunk digest and writes nothing; `AUTO_ACCEPT` is refused for an unattributed hunk.

## Verified compound candidate application

Conditional transaction autonomous mutation reuses the existing isolated worktree/checkpoint/change engine. Gate, reviewer and risk evidence binds an exact candidate revision. Escalation explicitly continues from that candidate or restores the base; revisions never merge silently. Compare-and-apply verifies current user/workspace preconditions before accepting the complete patch. A cancelled, failed or stale plan leaves no partially accepted patch; preserve unaccepted evidence and safe recovery. Irreversible external effects use the ordinary Effect Ledger and cannot be undone by worktree rollback. Test EPR-006/007 and their fault scenarios in doc 61.
