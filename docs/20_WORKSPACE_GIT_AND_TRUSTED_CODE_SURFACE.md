# Workspace, Git, Worktrees, Diagnostics, and Trusted Code Surface

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Canonical workspace

Filesystem + Git revision are authoritative. UI buffers are never canonical because Modbit has no embedded IDE/editor architecture.

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

## Verified compound candidate application

Conditional transaction autonomous mutation reuses the existing isolated worktree/checkpoint/change engine. Gate, reviewer and risk evidence binds an exact candidate revision. Escalation explicitly continues from that candidate or restores the base; revisions never merge silently. Compare-and-apply verifies current user/workspace preconditions before accepting the complete patch. A cancelled, failed or stale plan leaves no partially accepted patch; preserve unaccepted evidence and safe recovery. Irreversible external effects use the ordinary Effect Ledger and cannot be undone by worktree rollback. Test EPR-006/007 and their fault scenarios in doc 61.
