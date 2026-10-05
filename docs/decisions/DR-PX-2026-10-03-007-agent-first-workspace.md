---
id: DR-PX-2026-10-03-007
title: Agent-first workspace (roadmap phases 1 to 3) enters the product ledger as PX-041..068, specified clean-room by doc 65
status: accepted
date: 2026-10-03
supersedes: none
approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03
---

# DR-PX-2026-10-03-007 — Agent-first workspace

## Problem and goal

Modbit's Core is far ahead of its renderer. The Core serves about 118 commands; the desktop uses about 38 of them and shows a supervision console (Fleet, New Task, Review, Browser, Dashboard) instead of an agent workspace. There is no conversation, no streamed text, no terminal view, no agent list with attention-first ordering, no composer controls that work while a turn runs, no checkpoint or approval surface next to the conversation, no projects and no worktree management. The product thesis of doc 10 (an agent-first Work + Code workspace) is not yet experienced by a user.

The goal is a real agent-first workspace: a conversation that streams, an agent list that puts what needs the person first, composer and in-run controls, exact-intent approvals and reversible checkpoints beside the conversation, and projects and worktrees that organise parallel work. It must be built on the existing canonical owners with Modbit's strengths intact (exact-intent approvals, revision-bound per-hunk review, takeover lease, secret broker, crash-recovery proofs) and with fail-closed behaviour where the reference product is weaker.

## Trigger and evidence

- **Owner decisions of 2026-10-03.** Clean-room posture accepted (no counsel review first; no code, stylesheet, asset, brand colour or copy of the reference product); the first target is the agent-first workspace (roadmap phases 1 to 3) ahead of extensibility and eval; live verification of the reference product is requested before flows become specs. Each of local operating-system control, an editor, automations, a marketplace and an Electron change gets its own Decision Record (DR-PX-2026-10-03-008 to 012).
- **Static analysis** of the reference product (untracked research archive, held by the owner; ids in doc 65 section 12): two shells, an agent-first window with a left agent list, a centre conversation, a right panel of typed apps and a status row under the input.
- **Live verification** in a signed-in running instance and its on-disk artifacts (tags LIVE and DISK in doc 65): geometry (agent list 260 pt, top bar 40 pt, navigation rows 30 pt, agent rows about 43 pt, centre minimum 424 pt), the mode cycle, the queue tray and its first-use prompt, the checkpoint dialog and its in-place edit, the approval card under a manual allowlist mode, the policy-blocked-model tray, the background-terminal tray, the eight-category context breakdown, worktree cleanup cadence and lease, and a hook lifecycle failure at window close. Live observation corrected several static claims; doc 65 uses the corrected facts and records 26 items still unverified with the evidence that would close each.
- **Implementation audit** of `main` 5fdb47f (doc 65 section 5): provider streaming, durable terminals, typed input, exact-intent approvals, the context inspector, checkpoints and worktree primitives are production-working in the Core and are reused in place; the experience layer and several protocol messages are NOT-FOUND, SCAFFOLDED or DOCUMENTED-ONLY.

## Current behavior

The Core consumes provider deltas but only accumulates them; no delta event, transcript projection, terminal client stream, queue management, task mode field, skill listing command, execution-preference command, run-mode presets, category context accounting, reversible restore, project concept, worktree lifecycle policy, apply-back transaction or hardened Git runner exists. `packages/design-tokens` and `packages/ui` are empty stubs. The window is 1200 x 800 with no minimum size and quits without asking.

## Proposed replacement

1. Append 28 rows REQ-PX-041..068 to doc 62 (task, qualification and PX-E2E scenario each), grouped in three phases: phase 1 the agent-first shell (PX-041 to PX-049), phase 2 input, control and review (PX-050 to PX-062), phase 3 projects and worktrees (PX-063 to PX-068). Phase 0 (design tokens and primitives) is included as PX-044 because the shell cannot be built without it.
2. Specify them in doc 65 with 108 numbered requirements (AFW-*), each carrying a verification tag (LIVE, DISK, STATIC, UNVERIFIED), a traceability table to rows and source sections, a 26-item register of what is still unverified and the evidence that would close each, an implementation audit and the ownership map.
3. Declare the evidence tier of every row by behavioural risk: 22 release-critical and 6 iteration (PX-044, PX-045, PX-046, PX-047, PX-060, PX-064). Every protocol addition is release-critical: the streaming delta event with transcript and header projections, the terminal stream, queue management, task mode, the skill listing, `SetExecutionPreference`, run modes and rules, context accounting, reversible checkpoints, project records, worktree lifecycle, apply-back and Git hardening.
4. Every row without another PX prerequisite lists DOC-PX-007 as a prerequisite, so no row of this group can start until this record is accepted and the dossier task is COMPLETE.

## Independent Modbit design

The full design is doc 65. Its load-bearing choices:

- Three regions and a status row, with the Work timeline inside the conversation and the apps panel hidden until a task has an artifact; attention-first ordering is computed by the Core header projection so every client agrees (MOD-UX-001).
- The transcript, agent headers, context breakdown, queue view and project grouping are projections of the canonical event log, never stores.
- Streaming deltas are bounded, coalesced, redacted before append and closed by a completion record; a stream that fails ends as aborted with a typed source; recovery never resumes mid-token.
- Run modes are presets of who approves inside the Capability Kernel's envelope and never widen it; always-ask classes (outside-workspace write, network, secret use, protected path, deletion, push, escalation) ask in every mode; no classifier approves effects.
- Checkpoint restore is exactly reversible through a pre-restore checkpoint; worktree removal and apply-back are receipted effects with exact undo; a dirty or unapplied worktree is never removed automatically.
- Tokens are Modbit's own with a computed contrast test; no reference colour, asset, icon or string is used.

## Security model

The renderer holds no authority and calls nothing but typed preload functions (doc 81, REQ-EV-0076). Streamed text is redacted and tagged for injection provenance before it is appended and is rendered as untrusted content. Approvals stay bound to the intent hash, scope and expiry; the card never approves by Enter unless it has visible focus or the composer is empty. Run Everything is per session, confirmed every time and never persisted. Terminal input is refused for agent-owned terminals. Git runs hardened against repository hooks, fsmonitor and attributes. Setup commands read from a repository are data that run only in a trusted repository, sandboxed. The window and quit handling change only BrowserWindow constructor size options and `before-quit`; preload, CSP and webPreferences stay byte-identical (any other Electron change is DR-PX-2026-10-03-012).

## Canonical owner mapping (doc 81)

No subsystem is added. Behaviours map to `domain-events`, `terminal`, `core-runtime`, `skills`, `model-gateway`, `effects-security`, `context-engine`, `durability`, `workspace-git` and `desktop` as listed in doc 65 section 4. `SetExecutionPreference` only wires the command already specified in docs 30 and 32; any change to routing, the quality floor, gate calibration, outcome statistics or reviewer isolation (EPR-pinned behaviour) stops the row and needs its own Decision Record. The pinned surface (291 REQ-EV, 265 IMP-EV, 291 QUAL-EV, EPR-000..019, ADR-R-039..056, gates A to G) is untouched.

## Alternatives rejected

- **Pixel-copy the reference product.** Rejected: bars reverse engineering, imitates one product's choices where Modbit has stronger rules, and the owner chose clean-room.
- **A new chat sidebar next to the Fleet.** Rejected: leaves attention ordering split between two surfaces and the transcript without an owner.
- **Compute the status class and transcript in the renderer.** Rejected: duplicates Core state, breaks CLI and IDE-adapter parity and violates doc 81.
- **An approval classifier ("auto-review").** Rejected: a second policy engine (doc 81) and it approves effects on a model's judgement.
- **Persist every token as a store row forever.** Rejected: unbounded growth; deltas carry a retention class and collapse into the completed record.
- **Skip the foundation row and style ad hoc.** Rejected: the dark primary button already fails AA; the shell needs one token source and primitives.

## Supersessions

No sealed row, decision or pinned count is superseded. Two clarifications are recorded so nothing is silent: the Work timeline column of doc 10 moves inside the conversation (doc 10 and doc 32 are aligned when the first renderer row starts, by the implementing task); and docs 30, 32, 14, 18, 19, 20, 23 and 26 gain additive notes from the implementing tasks. Docs 12 and 13 are locked paths: an implementing task that must touch them cites this record and adds only the additive protocol and layout notes the rows name.

## Migration

Additive. New rows, tasks, qualifications and scenarios attach to M10 and RELEASE_ZERO. Existing statuses and evidence are untouched. The graph gains a change record, a dossier task (DOC-PX-007) and the PX nodes; `tools/dossier_px.py` pins nothing, so no constant changes.

## Compatibility

- **Release effect (ratified as drafted on 2026-10-05).** The 28 rows are ADOPT rows with release RELEASE_ZERO and milestone M10: ALPHA and BETA are READY and stay READY, but RELEASE_ZERO grows from 401 to 429 included work items before the other records of this batch, and M10 grows. This is forced by doc 75 (a task joins the release named in its row; RELEASE_ZERO includes every work item; DEFERRED rows can carry no task). If the owner would rather sequence the agent-first workspace after Release Zero, the alternative is to ratify with the rows re-classified DEFERRED in a follow-up record, which means they carry no tasks until a later record adopts them.
- Graph schema 1.2 is unchanged.

## Security impact

Positive: an exact-intent card with typed reasons, always-ask classes in every mode, hardened Git, redaction before append, terminal ownership enforcement. New surface: the streamed delta path, terminal write, the apply-back effect and the setup-command runner; each has a negative proof in its qualification.

## Test impact

28 qualifications and 28 scenarios in doc 62. Every protocol row proves a kill and restart of the real Core and a replay by cursor; every desktop row runs the packaged app on a real Core with the live gateway where model behaviour is visible; the dependency scan of PX-001 applies to the new renderer code. Tooling: `tools/build_graph.py` gains the change record, the dossier task and a range rule for authorization edges; `tools/test_dossier.py` gains a chain assertion. No pinned constant changes.

## Rollback

Revert the commits that add the rows, the spec and the graph nodes and rerun the reseal. Because doc 62 and this directory are locked paths, reversal needs a new Decision Record that names this one in `supersedes`.

## Explicit user approval

Accepted on 2026-10-05 by the owner's instruction recorded in `approved_by`: the goal "end to end implementation for tasks in research/audit/01-TASK-LIST.md" asks for the agent-first workspace, native computer control and automations to be built, which this record, DR-PX-2026-10-03-008 and DR-PX-2026-10-03-010 specify. The owner's decisions of 2026-10-03 remain the basis of this record. The record is ratified as drafted: the 28 rows stay ADOPT rows scheduled in M10 for RELEASE_ZERO (the alternative of re-classifying them DEFERRED was offered and not chosen), and the reference-derived surface values of doc 65 stay Modbit-chosen. DOC-PX-007 is therefore COMPLETE and PX-041..068 are startable as their other prerequisites complete. DR-PX-2026-10-03-009, -011 and -012 are not part of this instruction and stay proposed.
