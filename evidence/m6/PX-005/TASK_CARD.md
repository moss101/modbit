# Task Card — PX-005 Constrained inline patch through ChangeTransaction

## Identity

- Task ID: PX-005
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-005; owner label: workspace-git; subsystem: core (`user_patch.rs`), workspace file service, protocol, desktop review, CLI
- Qualification: QUAL-PX-005 — from the review surface and from the CLI a one-hunk user edit reaches a real worktree only through ChangeTransaction: revision precondition checked, provenance `user_direct_edit` recorded, workspace revision advanced, stale code references invalidated; a stale edit is refused, a protected path is denied after symlink resolution, no client keeps an unsaved buffer.
- Evidence tier: real-system (the real Core on a real git worktree; the real Electron app; the real CLI)

## Goal

Allow a user to apply a small direct edit from the review surface or CLI exclusively through the canonical ChangeTransaction on the Workspace File Service: revision precondition, path policy after symlink resolution, provenance user_direct_edit, one event and revision advance, stale CodeReferences invalidated. No editor buffer model anywhere (docs/20, docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: the Workspace File Service had the transaction, preconditions and path policy (M2.1), the review surface read and decided (M2.9), but no command let a person write through it; nothing recorded `user_direct_edit`; `FileChanged` had no provenance.
- production entry points: `services/modbit-core/src/user_patch.rs` (`apply`: loop not running, workspace and optional file revision preconditions, `WorkspaceService::resolve` then `read` then one `ChangeOp::Edit` through `apply_transaction`, `FileChanged` + `UserPatchApplied` in one transaction under the command record, replay without writing), `server.rs` (`ApplyUserPatch`, `task.author`), `crates/event-store` (`prior_command`, `execute_command_all`), `crates/domain` (`TaskEvent::UserPatchApplied`; `FileChanged.provenance`), `surface.proto` (`ApplyUserPatch` / `UserPatchAppliedAck`), desktop (`review:patch` IPC, `applyUserPatch` in preload and the protocol client, the review's per-file "Edit" panel), CLI `task patch`.
- proof: on a real worktree the review's edit of `line 7` lands as one `FileChanged` (`provenance: user_direct_edit`, `op: user_direct_edit:edit`, diff by reference, the command id where a tool call id would be) and one `UserPatchApplied` (path, both hashes, both revisions, source, match tier, command id); the workspace revision advances exactly once; `GetCodeView` bound to the old file revision reads `stale`; the same command id replayed writes nothing; an edit against the previous workspace revision or the previous file revision is refused `STALE_REVISION` with the file untouched; `link.txt` → `.env` is refused `PROTECTED_PATH` after resolution and so is `.env` itself; an ambiguous target is `NO_UNIQUE_MATCH`. Desktop: the review's "Edit" panel applies at the revision shown, the file on disk is exactly what the Core applied, the panel is closed and empty afterwards and after a reload; a second window's edit moves the workspace on and the first window's stale edit is refused `STALE_REVISION`; a client naming `.env` through the same preload API is refused `PROTECTED_PATH`.

## Limitations

- One text edit per command (one hunk): a multi-hunk change is several commands, each bound to the revision the previous one produced.
- The desktop offers the panel for files in the candidate diff; any path can be named through the API and the Core's path policy is the gate either way.
- The CLI reads `--old`/`--new` from arguments or files; it holds nothing between commands.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_005_a_user_edit_lands_only_through_the_change_transaction`
- desktop `e2e/user-patch.spec.ts` ("inline patch: a one-hunk user edit lands through the Core's change transaction; stale and protected attempts are refused; no buffer survives")

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
