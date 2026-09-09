# Task Card — M2.1 Workspace File Service with safe paths/revisions

## Identity

- Task ID: M2.1
- Milestone: M2 — Real local engineering loop
- Canonical owner: workspace-git; crate `modbit-workspace` (THE doc 81 workspace/change engine)
- Requirement IDs: docs/20 "Canonical workspace" and "Workspace File Service" (typed read/stat/list/apply patch/atomic replace/create/delete/mkdir/move; normalization before policy; symlink resolution against allowed roots/protected paths; optimistic revision preconditions; `WorkspaceRevision` bound to Git HEAD, worktree identity and content fingerprint), docs/23 "Protected paths" (checks after symlink resolution, before each open). Capabilities registered against REQ-EV-0014 and REQ-EV-0106, whose Change Engine tasks build on this service.
- Risk class: high (filesystem effects; security boundary)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

A canonical file service for one workspace root with typed operations, path policy applied after symlink resolution to reads and writes alike, optimistic preconditions that make blind overwrites impossible, atomic replacement, and a persisted monotonic revision linked to the real Git HEAD.

## Non-goals

- Edit match ladder, multi-edit transactions, undo plans (IMP-EV-0015/0016/0064/0065); Git branches/worktrees (M2.2); wiring into the tool registry and Core commands (M2.4); index.db persistence of revisions (M3).

## Required reading

`AGENTS.md`, `docs/20`, `docs/23`, `docs/13` (WorkspaceRevision binding), `docs/81`.

## Existing-code audit

- classification: NOT-FOUND (empty shell)
- production entry point: `WorkspaceService::open` + typed methods (consumed by M2.4 tools)
- real effector/storage boundary: the real filesystem under the root; revision records under the Core state dir; the real `git` binary for HEAD
- first missing/broken link: no path policy

## Invariants

- normalize → resolve symlinks hop by hop → root containment → protected patterns, for every operation; any hop leaving the root fails; protected matches deny reads, writes and deletes and never advance the revision.
- A failed precondition or I/O error writes nothing and leaves no temp file; replacements are temp + fsync + rename.
- Revision numbers strictly increase; the changed-file set and fingerprint reset when the Git HEAD moves; state lives outside the worktree.

## Implementation slice

- `paths.rs` (`PathPolicy`, `DEFAULT_PROTECTED`), `revision.rs` (`WorkspaceRevision`, `RevisionStore`, real `git rev-parse`), `service.rs` (`WorkspaceService`, `WritePrecondition`, `ApplyPatch`/`Edit`, `WorkspaceChange`)

## Verification

- real fs: typed ops advance a persisted monotonic revision with change records; revision binds to a real Git HEAD and resets on a new commit; symlink escapes (file and directory links) and `..` traversal rejected for reads and writes with nothing outside touched; protected paths denied after resolution and listed opaque; stale content-hash / workspace-revision / create-over-existing / malformed and overlapping edits rejected with nothing written; rename failure leaves the original intact
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
