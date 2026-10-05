# Workspace Editor specification (clean-room)

> **Authority:** DR-PX-2026-10-03-009 (`decisions/DR-PX-2026-10-03-009-workspace-editor.md`), status **proposed** until the owner ratifies it. Rows PX-077..PX-081 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED and none may start before the record is accepted and DOC-PX-009 is COMPLETE.  
> **Nature:** a product specification in Modbit's own words and design. Provenance (clean-room posture), the four verification tags (LIVE, DISK, STATIC, UNVERIFIED) and the evidence key are those of `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` sections 2, 3 and 12 and are not repeated. It records functional facts and says how well each is known; it is not an implementation and not proof that anything exists.

## 1. Purpose

Review in Modbit is read-oriented and revision-bound: a person can see exactly what an agent changed but can change only one small thing at a time through the constrained inline patch of PX-005. The owner has made an editor a real goal. Modbit's constraints are real too: no Code-OSS, no Monaco, no editor buffer that owns file truth, and editing is not the primary UI (MOD-SURF-001, MOD-SURF-002, docs 20, 29 and 32). This document specifies the Workspace Editor as Modbit's own secondary surface that fits those constraints: a text editing view whose edits are a revision-pinned draft overlay, saved only through the canonical ChangeTransaction, with language intelligence served by the Core and agent-aware editing that reuses Review. It supersedes the editor non-goals of docs 20, 29 and 32 and the REJECTED status of MOD-IDE-002 in this scope only, and it does not reintroduce an IDE.

## 2. Constraints and ownership

No canonical subsystem is added. The renderer holds no authority and every effect goes through the Core (doc 81, REQ-EV-0076). Decision: `MOD-EDIT-001` in `02_AUTHORITY_AND_DECISIONS.md` (PROVISIONAL until the record is accepted). Common failure, cancellation, idempotency and restart semantics are those of doc 65 section 8.

| Row | Owner | Tier |
|---|---|---|
| PX-077 Editor file service: open with revision and encoding facts, multi-file save as one ChangeTransaction, merge basis and file revision events | workspace-git | release-critical |
| PX-078 Editor language intelligence: draft analysis overlay, outline, definitions and references served by the Core | context-engine | release-critical |
| PX-079 Workspace Editor surface: text engine, drafts, stale-draft merge, find and replace, tabs, accessibility and large-file behaviour | desktop | release-critical |
| PX-080 Agent-aware editing: selection context, quick edit, inline per-hunk review and agent-write notices | desktop | release-critical |
| PX-081 Editor text-safety and conformance suite: encodings, round trips, fuzzed save path, input method and screen reader, latency | verification | release-critical |

## 3. Existing implementation audit

Audit at `main` 5fdb47f, classes of doc 93.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Trusted Code Review surface | **PRODUCTION-WORKING** | `apps/desktop/src/renderer` Review and the code view: revision-bound code view model, syntax highlighting, hunks, per-hunk decisions at a revision (`GetCodeView`, `DecideReview`), stale references invalidated, evidence links. Read-only by design; it owns no buffer. | Reused for highlighting, hunks and stale marking. |
| Constrained inline patch (PX-005) | **PRODUCTION-WORKING** | `ApplyUserPatch` applies a small user edit through the canonical ChangeTransaction with revision precondition, path policy after symlink resolution, provenance `user_direct_edit`, one event and a revision advance; the review surface calls it with a one-hunk patch. | Extended to a multi-file save; the single-hunk entry stays. |
| Editing view, text model, tabs, find and replace | **NOT-FOUND** | No editable text component, draft model, file tabs, search in file or dirty handling exists in `apps/desktop`; dependency admission has not evaluated a text-engine component. | The whole surface (PX-079). |
| Language intelligence for files at a revision | **IMPLEMENTED-PARTIAL** | The Core has a headless language-service bridge, symbol and reference indexes and pull-based diagnostics (M3.3, M3.4, M3.6, REQ-EV-0020); they answer for files on disk at a workspace revision. There is no analysis of unsaved text and no client-facing outline, definition or reference command shaped for an editor. | Draft analysis and editor queries (PX-078). |
| Editor context bridge | **IMPLEMENTED-PARTIAL** | `SetTaskSelection` records the selection a person is working on and the inspector shows it (REQ-EV-0141, REQ-EV-0160); retrieval prefers it. No editor feeds it. | Selection source and quick edit (PX-080). |
| Merge basis and external-change detection | **IMPLEMENTED-PARTIAL** | `crates/git` offers merge transactions and `crates/workspace` revisions per worktree; a stale revision is refused. No three-way basis is served to a client and no event tells a client that a file it displays moved. | Basis and revision events (PX-077). |

## 4. Requirements

### A. Authority, scope and the draft overlay

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-A01 | The Workspace Editor is a secondary surface: it opens as a file tab in the Files app of the apps panel (and may be expanded to the full window) and is reached from Review, the conversation and the palette. Review stays the primary place where agent work is judged. There is no explorer tree beyond the Files app's own list, no debugger, no extension host, no workbench and no IDE settings page. | UNVERIFIED (design choice) | PX-079 |
| WED-A02 | Filesystem plus Git revision stay canonical (doc 20). An open file is a base snapshot (bytes, revision, content hash, encoding facts) plus a draft overlay: an ordered list of text operations held by the client. The overlay is non-authoritative, never read by any Core component, never written to a source file, and is discarded or rebased when its base moves. This supersedes, for this surface only, the statements that no client keeps an unsaved buffer (PX-005, docs 20, 29, 32): the overlay is volatile convenience state, like a composer draft, and never a second source of truth. | UNVERIFIED (design choice) | PX-077, PX-079 |
| WED-A03 | The editing engine is not Monaco and not derived from Code-OSS, has a permissive licence and no extension host, and is admitted through dependency admission (doc 36) with recorded owner, licence, maintenance check, justification and an exit plan; the choice is recorded in doc 35 by the implementing task and nowhere else. It must provide a virtualised view of large files, input-method composition, bidirectional text, an accessible text model for screen readers, undo history, multiple selections and soft wrap. If no candidate meets them the row stops and a build decision goes to the owner. | UNVERIFIED (design choice) | PX-079 |
| WED-A04 | The editor opens either a task's worktree (the default when opened from a task) or the trusted repository's own checkout. Edits to the checkout are the user's direct edits and are ChangeTransactions with provenance `user_direct_edit` like any other; agent tasks write in their own worktrees, so the two never write the same file by default. | UNVERIFIED (design choice) | PX-077 |
| WED-A05 | Protected files (secrets, configuration the policy protects, Git internals, files outside the workspace after symlink resolution) open read-only with the reason shown, or are refused; a write to them is denied by the Core regardless of what the editor sent. | UNVERIFIED (design choice) | PX-077, PX-079 |

### B. Save path and concurrency

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-B01 | Opening a file is a Core read that returns the bytes, the workspace and file revision, a content hash, the detected encoding (UTF-8 with or without a byte-order mark, UTF-16), the line-ending convention, the trailing-newline fact and whether the path is protected or binary. The editor never reads the filesystem. | UNVERIFIED (design choice) | PX-077, PX-079 |
| WED-B02 | Saving is one ChangeTransaction (the multi-file form of `ApplyUserPatch`): a revision precondition on every file, path policy after symlink resolution, provenance `user_direct_edit`, one event and one revision advance, stale code references invalidated, atomic for the set. The save preserves encoding, byte-order mark, line endings and the trailing-newline fact unless the person changed them; an edit that would corrupt any of them is refused. | UNVERIFIED (design choice) | PX-077, PX-081 |
| WED-B03 | When a file's revision advances while a draft exists (an agent applied a change, a merge, a checkout), the Core emits a revision event for the file and the draft is marked stale. A stale draft is rebased automatically only when its operations apply without overlap to the new content; otherwise the editor shows a three-way merge of the base snapshot, the current file and the draft and the person resolves it. A save against a stale base is refused until rebased. Nothing is ever overwritten silently. | UNVERIFIED (design choice) | PX-077, PX-079 |
| WED-B04 | There is no autosave. A save is an explicit action. Drafts are persisted locally in the profile, keyed by workspace, file and base revision, so a renderer crash or restart does not lose typing; they are restored only against an unchanged base, otherwise shown as stale drafts, and are never applied automatically. A close with a dirty draft asks to save, discard or cancel. | UNVERIFIED (design choice); DISK (the reference persists per-task file tabs with dirty flags) | PX-079 |
| WED-B05 | A save is idempotent by command id: replaying it applies once. A save that races a revision advance is refused with the new revision and changes nothing. A Core restart between request and result is reconciled by the effect ledger to one applied or not-applied state. | UNVERIFIED (design choice) | PX-077 |

### C. Language intelligence

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-C01 | Syntax highlighting reuses the review surface's highlighter and grammars so a file looks the same in Review and in the editor; no second highlighter is built. | UNVERIFIED (design choice) | PX-079 |
| WED-C02 | Diagnostics, symbols, definitions and references come from the Core's language-service bridge and indexes, bound to a revision, pulled after the text settles and never pushed per keystroke. For unsaved text the editor sends the draft to the Core as an ephemeral analysis overlay for that one request: it lives in memory, never touches the filesystem, is rate-limited and is discarded. A result is marked stale once the draft or the revision moves. | UNVERIFIED (design choice) | PX-078 |
| WED-C03 | Completion lists, signature help and inline predictive completion are not part of this record. Replacing an IDE's language features remains a non-goal (doc 29). A predictive path needs a latency-class route in the Provider Gateway that touches EPR-pinned routing behaviour and would need its own record. | UNVERIFIED (scope decision); STATIC (the reference's inline completion product has its own failure taxonomy) | PX-078 |

### D. Agent-aware editing

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-D01 | The active file, the selection and the symbol under the cursor feed the editor context bridge (REQ-EV-0141, REQ-EV-0160): they influence retrieval and appear in the context inspector and never mutate source. | UNVERIFIED (design choice) | PX-080 |
| WED-D02 | Quick edit: with a selection the person types an instruction and Modbit creates or steers a task scoped to that file and range, with the selection as context. The result returns as per-hunk review at its revision, never as a silent write to the draft. | UNVERIFIED (design choice); STATIC (the reference's classic window has an inline edit shortcut) | PX-080 |
| WED-D03 | Lines changed by a task or a user since the base revision show in the gutter, and the existing per-hunk Review decisions (accept, reject) are available inline at their revision. A file with a dirty draft that an agent task is about to change raises a notice, not an approval. | UNVERIFIED (design choice) | PX-080 |

### E. Interaction, accessibility and performance

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-E01 | Editing is keyboard-first with Modbit's own bindings per platform convention: undo and redo, find and replace in file (literal or regular expression, the expression compiled by a linear-time engine so a pattern cannot cause catastrophic backtracking), go to line, indent and outdent, toggle comment, bracket matching, soft wrap, whitespace display and line numbers. Line numbers, wrap and unified or split diff are persisted per app as preferences. | DISK (line numbers, word wrap and diff view mode are stored per app); UNVERIFIED (binding set) | PX-079 |
| WED-E02 | The editor is accessible: a screen reader reads the text model and announces cursor movement, diagnostics and save state; input-method composition and bidirectional text work; a visible focus ring, high-contrast and reduced-motion are honoured; Escape leaves the editor and Tab inserts indentation unless the person turns on Tab moves focus, so there is no keyboard trap. The axe-core suite reports no violations. | UNVERIFIED (design choice) | PX-079, PX-081 |
| WED-E03 | Performance budgets are Modbit's own, measured in the packaged app: first paint of a 1 MB file within 150 ms; keystroke to paint at most 16 ms at p95 in a 100,000-line file; a file above 5 MB opens read-only with a stated reason and above 50 MB is not opened in the editor; memory for one open file stays within a stated multiple of its size. | UNVERIFIED (design choice) | PX-079, PX-081 |

### F. Safety

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| WED-F01 | File content is untrusted: rich-text paste is reduced to plain text, links open only through the main process allow-list, nothing in a document executes, and a huge paste is bounded. Binary files are not opened as text; they show type and size. | UNVERIFIED (design choice) | PX-079, PX-081 |
| WED-F02 | The same path works for cloud tasks: the file service answers from the task's own workspace through the Core, and the editor is read-only when the session lease is not held or the task's control is with another actor. | UNVERIFIED (design choice) | PX-077, PX-079 |

## 5. Rows and evidence tiers

Every row is release-critical: each changes effect-bearing behaviour, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics (doc 83); none is eligible for the iteration tier.

| Row | Title | Owner | Prerequisites |
|---|---|---|---|
| PX-077 | Editor file service: open with revision and encoding facts, multi-file save as one ChangeTransaction, merge basis and file revision events | workspace-git | PX-005, M2.1, M2.9, DOC-PX-009 |
| PX-078 | Editor language intelligence: draft analysis overlay, outline, definitions and references served by the Core | context-engine | M3.3, M3.4, M3.6, PX-077, DOC-PX-009 |
| PX-079 | Workspace Editor surface: text engine, drafts, stale-draft merge, find and replace, tabs, accessibility and large-file behaviour | desktop | PX-044, PX-048, PX-077, PX-078 |
| PX-080 | Agent-aware editing: selection context, quick edit, inline per-hunk review and agent-write notices | desktop | PX-079, PX-055, M2.9, DOC-PX-009 |
| PX-081 | Editor text-safety and conformance suite: encodings, round trips, fuzzed save path, input method and screen reader, latency | verification | PX-077, PX-079, DOC-PX-009 |

## 6. Supersessions and clarifications

Nothing sealed changes until the record is accepted. The explicit records, also entered in docs 02 and 03:

| Earlier decision, row or text | Kind | Scope and effect | What survives |
|---|---|---|---|
| MOD-IDE-002 (REJECTED): build a new general-purpose code editor | SUPERSEDED IN SCOPE | A scoped Workspace Editor (a transactional editing surface that saves only through ChangeTransaction) is admitted. A general-purpose editor stays rejected. | The rejection of a general-purpose editor, Code-OSS and Monaco. |
| MOD-SURF-002 (LOCKED): no built-in full IDE or Monaco architecture; code shown through trusted review surfaces; editing is not the primary UI | SUPERSEDED IN SCOPE | Editing becomes a secondary surface with a draft overlay. Monaco, an IDE architecture, an extension host and editing as the primary UI remain prohibited. | No Monaco, no IDE, review as the primary judgement surface. |
| doc 20: UI buffers are never canonical; the surface does not own unsaved editor buffers; P0 does not build a general editor | SUPERSEDED IN SCOPE | The draft overlay of WED-A02 is volatile, non-authoritative, rebased or discarded on a moved base and read by no Core component; filesystem plus Git revision stay canonical. | Canonicality of the filesystem and Git revision. |
| doc 29: an embedded editor remains a non-goal | SUPERSEDED IN SCOPE (editor); RETAINED (Tab completion, replacing an IDE's language features) | The editor is admitted; completion lists, signature help and predictive completion stay out (WED-C03). | The non-goals for completion and language-feature replacement. |
| doc 32: no Monaco, Code-OSS or editor-buffer architecture | SUPERSEDED IN SCOPE (buffer); RETAINED (Monaco, Code-OSS) | A draft overlay replaces the editor-buffer ban for the Workspace Editor only. | No Monaco, no Code-OSS. |
| PX-005 qualification: no client keeps an unsaved buffer | SUPERSEDED IN SCOPE | Applies to the single-hunk inline patch; the Workspace Editor's drafts are the stated exception (WED-A02). | The inline patch path and its qualification. |
| MOD-SURF-001 (LOCKED, no Code-OSS foundation) and MOD-IDE-001 (REJECTED, Code-OSS fallback) | UNCHANGED | Nothing in this record uses or depends on Code-OSS. | Both rows. |

## 7. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| WED-A01 | PX-079 | UNVERIFIED (design choice) | doc 10, doc 32 |
| WED-A02 | PX-077, PX-079 | UNVERIFIED (design choice) | doc 20, doc 29, doc 62 PX-005 |
| WED-A03 | PX-079 | UNVERIFIED (design choice) | doc 36, doc 35 |
| WED-A04 | PX-077 | UNVERIFIED (design choice) | doc 20, doc 29 |
| WED-A05 | PX-077, PX-079 | UNVERIFIED (design choice) | doc 23, doc 52 |
| WED-B01 | PX-077, PX-079 | UNVERIFIED (design choice) | doc 29 |
| WED-B02 | PX-077, PX-081 | UNVERIFIED (design choice) | doc 62 PX-005, doc 62 PX-026 |
| WED-B03 | PX-077, PX-079 | UNVERIFIED (design choice) | doc 20 |
| WED-B04 | PX-079 | UNVERIFIED (design choice); DISK (the reference persists per-task file tabs with dirty flags) | L12 §2.1, L13 §3.13 |
| WED-B05 | PX-077 | UNVERIFIED (design choice) | doc 23 |
| WED-C01 | PX-079 | UNVERIFIED (design choice) | doc 20 |
| WED-C02 | PX-078 | UNVERIFIED (design choice) | doc 18, doc 40 REQ-EV-0020 |
| WED-C03 | PX-078 | UNVERIFIED (scope decision); STATIC (the reference's inline completion product has its own failure taxonomy) | doc 29, F05 |
| WED-D01 | PX-080 | UNVERIFIED (design choice) | doc 40 REQ-EV-0141, doc 40 REQ-EV-0160 |
| WED-D02 | PX-080 | UNVERIFIED (design choice); STATIC (the reference's classic window has an inline edit shortcut) | SYN §8 |
| WED-D03 | PX-080 | UNVERIFIED (design choice) | doc 20, doc 48 |
| WED-E01 | PX-079 | DISK (line numbers, word wrap and diff view mode are stored per app); UNVERIFIED (binding set) | L12 §2.1, doc 39 |
| WED-E02 | PX-079, PX-081 | UNVERIFIED (design choice) | doc 39 |
| WED-E03 | PX-079, PX-081 | UNVERIFIED (design choice) | doc 53 |
| WED-F01 | PX-079, PX-081 | UNVERIFIED (design choice) | doc 52 |
| WED-F02 | PX-077, PX-079 | UNVERIFIED (design choice) | doc 24 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-077 | WED-A02, WED-A04, WED-A05, WED-B01, WED-B02, WED-B03, WED-B05, WED-F02 |
| PX-078 | WED-C02, WED-C03 |
| PX-079 | WED-A01, WED-A02, WED-A03, WED-A05, WED-B01, WED-B03, WED-B04, WED-C01, WED-E01, WED-E02, WED-E03, WED-F01, WED-F02 |
| PX-080 | WED-D01, WED-D02, WED-D03 |
| PX-081 | WED-B02, WED-E02, WED-E03, WED-F01 |

## 8. Non-goals

| Not in scope | Why |
|---|---|
| An IDE: explorer-first workbench, debugger, extension host, task runner UI, source-control panel | MOD-SURF-001 and MOD-SURF-002 stand. The Files app and Review are the surfaces. |
| Monaco or a Code-OSS derivative as engine or fork | Rejected; see the DR. |
| Completion lists, signature help, inline predictive completion | Needs a latency route that touches EPR-pinned routing (WED-C03). |
| Autosave | Every write is an explicit ChangeTransaction (WED-B04). |
| Notebook editing | A separate surface, not in these rows. |
| Real-time multi-user editing | Team collaboration is DEFERRED (REQ-PX-012, REQ-PX-013). |

## 9. Still unverified

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| V01 | Which text-engine component meets WED-A03 under the licence, accessibility and large-file requirements | The dependency-admission evaluation of the implementing task, with a recorded benchmark of IME, bidirectional text, a screen reader and a 100,000-line file for each candidate. |
| V02 | Whether the editor's keystroke budget is reachable in the packaged Electron renderer with the chosen engine | Packaged-app traces on reference hardware. |
| V03 | Whether users need completion lists in the first release | Usage evidence from the editor's first release and the owner's decision; a new record is the route if so. |
| V04 | Behaviour of three-way rebase on large agent diffs | Corpus of real agent change sets applied under open drafts; measure automatic rebase rate and conflicts. |
| V05 | The reference product's own editing surface behaviour in its classic window | Not studied: the research covered the agents window; a live observation would add facts but none is required for this record. |
