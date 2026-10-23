---
id: DR-PX-2026-10-03-009
title: A scoped Workspace Editor as a transactional secondary surface, under the no-Monaco and no-Code-OSS constraint (PX-077..081, doc 67)
status: proposed
date: 2026-10-03
supersedes: none
approved_by: pending owner ratification; basis is the owner's statement on 2026-10-03 that an editor is a real goal
---

# DR-PX-2026-10-03-009 — Workspace Editor

## Problem and goal

Review in Modbit is read-oriented and revision-bound. A person can see exactly what an agent changed but can change only one small thing at a time through the constrained inline patch (PX-005). To work in the product without leaving for another tool, a person needs to read and edit files, see diagnostics, and fold their edits into what the agents are doing. The owner has made an editor a real goal.

Modbit's own sealed constraints are real too: no Code-OSS foundation (MOD-SURF-001, LOCKED), no built-in full IDE or Monaco architecture and editing is not the primary UI (MOD-SURF-002, LOCKED), no new general-purpose code editor (MOD-IDE-002, REJECTED), filesystem plus Git revision as the only truth and no editor buffer ownership (docs 20, 29, 32). The goal is an editor designed under those constraints, with the minimal explicit supersessions the goal needs and no more.

## Trigger and evidence

- **Owner's statement of 2026-10-03:** the five previously out-of-scope items, including an editor, are real goals with no stubs.
- **What the research shows (tags per doc 65 section 3).** The reference product is a fork of a full IDE, so its editor is an IDE editor; that is the architecture Modbit has rejected and the research does not offer a design to follow. Relevant facts: DISK: the agents window persists per-task file tabs with a dirty flag and per-app preferences for line numbers, word wrap and diff view mode (`L12` §2.1, `L13` §3.13); STATIC: a classic-window inline edit shortcut exists (`SYN` §8) and the inline predictive completion product has its own failure taxonomy (`F05`); LIVE: the agents window shows file edits applied in an isolated worktree with per-file revert and stage and no accept step (`L14` P3.10). Nothing observed gives editor mechanics; the design below is independent and its requirements are tagged UNVERIFIED (design choice) accordingly. This is a finding of this record, stated plainly.
- **Existing Modbit mechanics that make it feasible without an IDE.** The Workspace File Service and ChangeTransaction (revision precondition, path policy after symlink resolution, provenance, one event, stale references invalidated); `ApplyUserPatch` (PX-005); a headless language-service bridge, symbol and reference indexes and pull-based diagnostics (M3.3, M3.4, M3.6); the editor context bridge (REQ-EV-0141, REQ-EV-0160); the Review surface with per-hunk decisions at a revision.

## Current behavior

No editing view, draft model, file tabs or find and replace; the only write from a person is the one-hunk inline patch. Language services answer for files on disk at a revision; nothing analyses unsaved text; `SetTaskSelection` exists with no editor to feed it.

## Proposed replacement

Specified in doc 67 (21 requirements) and rows PX-077 to PX-081, all release-critical:

1. **PX-077** the editor file service: `OpenForEdit` with revision, hash, encoding and protection facts; `SaveEdit` as the multi-file form of `ApplyUserPatch`; `GetMergeBasis`; a `FileRevisionAdvanced` event.
2. **PX-078** language intelligence for the editor: draft analysis through an ephemeral in-memory overlay, outline, definition, references, all revision-bound and pull-based.
3. **PX-079** the editor surface: engine admitted through dependency admission, draft overlay, stale-draft merge, find and replace with a linear-time expression engine, tabs, accessibility, large-file behaviour.
4. **PX-080** agent-aware editing: selection context, quick edit that returns as per-hunk review, inline hunk decisions, agent-write notices.
5. **PX-081** the text-safety and conformance suite: property round trips, fuzzed save, input-method and screen-reader sessions, latency, a mutation check.

## Independent Modbit design

The Workspace Editor is a **secondary** surface: a file tab in the Files app of the apps panel. Review stays where judgement happens. An open file is a base snapshot plus a **draft overlay**, an ordered list of text operations held by the client, non-authoritative, read by no Core component, never written to a file, rebased or discarded when its base moves. Saving is one ChangeTransaction (`SaveEdit`) with a revision precondition on every file; encoding, byte-order mark, line endings and trailing newline are preserved; there is no autosave. A file that moves under a draft (an agent applied a change) marks the draft stale; non-overlapping operations rebase and overlapping ones open a three-way merge; a save against a stale base is refused. Language intelligence comes from the Core, pulled after the text settles; unsaved text reaches it only as an ephemeral overlay for one request. Completion lists, signature help and inline predictive completion are not part of this record (below). The editing engine is **not Monaco and not derived from Code-OSS**, has a permissive licence and no extension host, and is chosen by the dependency-admission step of doc 36 and recorded in doc 35 (candidates are evaluated there; this record selects none, and if none meets the requirements the row stops and the choice returns to the owner).

## Security model

All writes go through the Core and the path policy after symlink resolution, whatever the client sends; protected files open read-only or are refused; a save needs the session lease. Draft analysis overlays are in-memory, rate-limited and discarded. File content is untrusted: no HTML executes, rich paste is reduced to plain text, links open only through the main-process allow-list. Regular expressions run in a linear-time engine so a pattern cannot backtrack catastrophically. The editor opens the task worktree or the trusted checkout; agent tasks write in their own worktrees, so they do not write the same file by default. The text-safety suite proves the editor cannot corrupt a file (encodings, line endings, byte-order mark, fuzzed saves).

## Canonical owner mapping (doc 81)

`workspace-git` owns the file service and the save path (PX-077); `context-engine` owns language intelligence (PX-078); `desktop` owns the surface (PX-079, PX-080); `verification` owns the conformance suite (PX-081). No second change engine, no second source of file truth, no editor-owned state in the Core.

## Alternatives rejected

- **Embed Monaco or fork Code-OSS.** Rejected: MOD-SURF-001 and MOD-SURF-002 (LOCKED) and MOD-IDE-001 (REJECTED); it brings an extension host and a workbench, which is the product Modbit decided not to be.
- **A canonical Core-owned editor buffer** (the Core holds unsaved text and syncs it). Rejected: a second source of file truth and a new recovery surface; the draft overlay is a client convenience and disposable.
- **Autosave.** Rejected: every write must be an explicit ChangeTransaction with a revision precondition.
- **Write a general-purpose editor engine from scratch.** Rejected as the default: large cost for IME, bidirectional text and accessibility; the dependency-admission step decides between an admitted component and a build, and builds only if none qualifies.
- **LSP completion and predictive inline completion in this record.** Rejected for now: doc 29 keeps replacing an IDE's language features as a non-goal, and a predictive path needs a latency-class route in the Provider Gateway that touches EPR-pinned routing; it would need its own record.
- **An external-editor handoff only.** Rejected as the sole path: the owner asked for an editor; the handoff (`open externally`) remains available.

## Supersessions

Recorded in docs 02 and 03 and in doc 67 section 6, each effective only on acceptance of this record and only in the stated scope:

- **MOD-IDE-002 (REJECTED, build a new general-purpose editor)** is narrowed: a scoped Workspace Editor is admitted; a general-purpose editor, Code-OSS and Monaco stay rejected.
- **MOD-SURF-002 (LOCKED)**: editing becomes a secondary surface with a draft overlay; Monaco, an IDE architecture, an extension host and editing as the primary UI stay prohibited. This is a change to a LOCKED row, made through this record with its justification: the owner's goal cannot be met under the row's literal wording ("code shown through trusted review surfaces"), while every purpose behind it (no IDE, no Monaco, filesystem and Git as truth, review as primary) is preserved.
- **docs 20, 29, 32**: the statements that no client keeps an unsaved buffer and that an embedded editor is a non-goal are superseded for this surface only; Monaco and Code-OSS bans, the Tab-completion and language-feature non-goals, and the canonicality of filesystem plus Git revision are retained.
- **PX-005 qualification** (no client keeps an unsaved buffer) applies to the single-hunk inline patch; the editor's drafts are the stated exception.
- MOD-SURF-001 and MOD-IDE-001 are unchanged. Locked requirement rows (doc 46) are untouched: REQ-EV-0141 and REQ-EV-0160 (ADAPT, editor context bridge "not IDE ownership") are consistent with this design.

## Migration

Additive. A new package under the desktop owner for the engine (doc 12 is a locked path; the implementing task cites this record for the layout note).

## Compatibility

Rows are M10, RELEASE_ZERO, depending on PX-044, PX-048 and PX-055 of DR-PX-2026-10-03-007. PX-005's inline patch keeps working unchanged.

## Security impact

A new write surface, mitigated as above. The new dependency must pass the licence and provenance scan with no Monaco or Code-OSS code in its closure.

## Test impact

Five qualifications and scenarios in doc 62, a property and fuzz suite, packaged-app accessibility and latency traces, a seeded-corruption mutation check.

## Rollback

Revert the commits adding the rows and spec and rerun the reseal; a new record is needed for the locked paths.

## Consequences and owner questions

- **Question.** Are completion lists and inline predictive completion wanted in the first release? If so a further record is needed (it touches EPR-pinned routing for the predictive path).
- **Consequence.** The editor engine's choice is made by dependency admission and could stop the row; that is deliberate, not a gap.
- RELEASE_ZERO grows by five work items.

## Explicit user approval

Pending. Acceptance flips `status` to `accepted` in the commit that lands the rows; because it changes a LOCKED row in scope, the owner's acceptance must be explicit about MOD-SURF-002.
