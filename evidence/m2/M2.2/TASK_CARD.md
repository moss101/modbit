# Task Card — M2.2 Git branch/worktree/diff operations

## Identity

- Task ID: M2.2
- Milestone: M2
- Canonical owner: workspace-git; crate `modbit-git`
- Requirement IDs: docs/20 "Git strategy" (dedicated branch + worktree per coding task; separate worktrees for concurrent builders; merge/rebase as typed operations with conflict evidence; no writes to the user's active worktree). Capabilities registered against REQ-EV-0125, REQ-EV-0124, REQ-EV-0067, REQ-EV-0022 whose named qualification tests ship here; their tasks close when the Core wires them (M2.4/M6).
- Risk class: high (repository effects)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Typed Git operations over the real `git` binary: repository open/init, HEAD/branch/status, branch creation, worktree add/list/remove, typed diffs (numstat + unified), commits, a merge transaction (begin → conflicts as evidence → resolve → commit | abort restoring the target), and provenance-bound dirty-state snapshots under `refs/modbit/snapshots/` that leave HEAD, index and worktree untouched.

## Non-goals

- Change Engine edit ladder / undo (IMP-EV-0015/0064/0065); forge PRs (PX-007, M6); cloud handoff transport (M8); persistence of MergeTransaction records in core.db (with M2.9 review).

## Required reading

`AGENTS.md`, `docs/20`, `docs/12`, `docs/81`.

## Existing-code audit

- classification: NOT-FOUND (the workspace crate only read HEAD)
- production entry point: `modbit_git::Repo` (consumed by M2.4 tools and M2.9 review)
- real effector/storage boundary: real git repositories and worktrees on disk

## Invariants

- Every operation is a typed call with structured results; conflicts are data, not failures; abort restores `target_before` exactly.
- Snapshots use a temporary index (removed afterwards) and never move HEAD; cleanup deletes the ref.
- Author identity for Modbit commits is explicit (`-c user.*`), never inherited silently.

## Verification

- real git: QUAL-EV-0125/0124 (parallel worktree writes isolated, main worktree untouched, lineage via merge-base, typed diff, worktree removal); QUAL-EV-0067 (injected conflict → Conflicted with paths and markers; commit refused; abort restores; resolve → Staged → Committed with feature as ancestor); QUAL-EV-0022 (dirty tracked + untracked + deleted state snapshotted, reconstructed exactly in a fresh worktree, cleanup removes the ref and the temp index); typed errors for non-repositories and bad revisions
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
