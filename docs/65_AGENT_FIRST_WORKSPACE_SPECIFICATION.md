# Agent-first workspace specification (clean-room)

> **Authority:** DR-PX-2026-10-03-007 (`decisions/DR-PX-2026-10-03-007-agent-first-workspace.md`), status **accepted** on 2026-10-05 (owner instruction, goal: implement research/audit/01-TASK-LIST.md; the basis is the owner's decisions of 2026-10-03). Product rows PX-041..PX-068 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED; with DOC-PX-007 COMPLETE each is startable as its other prerequisites complete (every root row lists DOC-PX-007 as a prerequisite). PX-067 carries a note that the audit fix FIX-01 delivered most of its scope (see its card).
> **Scope:** roadmap phases 1 to 3 of the parity research: the agent-first shell, input, control and review, and projects and worktrees. Phase 0 (design tokens and primitives) is included as the foundation row PX-044 because the shell cannot be built without it.
> **Nature:** a product specification written in Modbit's own words and design language. It records functional facts and measurable parameters and says for every one how well it is known. It is not an implementation, not a copy and not proof that anything exists.

## 1. Purpose and how to use this document

Modbit's desktop today is a supervision console: Fleet, New Task, Review, Browser and Dashboard overlays over a Core that is far ahead of its renderer. The product thesis of doc 10 is an agent-first workspace, and the missing piece is the experience layer: a conversation that streams, an agent list that shows what needs the user, composer controls that work while a turn runs, approvals and checkpoints that live next to the conversation, and projects and worktrees that organise parallel work.

This document gives each part of that layer a numbered requirement (`AFW-*`), says how well the underlying fact is known (a verification tag), maps it to one or more product rows in doc 62, and records what already exists in the repository so that no row builds a second implementation of anything. Build agents read this document with the PX task card they were assigned; the card names its requirements and the qualification names the proof. Requirements here never replace `REQ-EV-*` rows (LOCKED) or `REQ-PX-*` rows; they are the detailed parameter source for the new rows.

## 2. Clean-room provenance

This specification was derived from functional observation of a commercial agent-first workspace, kept here as "the reference product", and from independent design. The reference product's licence bars reverse engineering, so the observation was limited to behaviour: layout proportions, state machines, shortcut semantics, ordering and timings, read from a running, signed-in instance, from the files that instance left on disk, and from a static reading of its shipped bundle. The research notes are untracked, are kept by the owner outside the repository, and are not part of this package; section 12 names the short ids used to cite them.

What is carried over is facts, not expression. No code, stylesheet, theme file, colour value taken from a theme, icon, icon name, font, logo, brand colour, string of copy or file layout of the reference product appears in this document or may appear in any implementation derived from it. Where the reference product's behaviour is weaker or riskier than Modbit's strengths, the requirement states Modbit's stricter rule and says so. Tokens are Modbit's own proposal; the concrete surface colours are chosen by the implementing task from the constraints in AFW-A11 and AFW-A12 and verified by the computed contrast test, not copied from any source. Whether this posture is acceptable for a release was accepted by the owner on 2026-10-03 without prior counsel review; the DR records that decision and does not extend it.

## 3. Verification tags

Every requirement carries the tag of the parity fact it derives from. A tag says how the fact was learned, not whether Modbit must follow it.

| Tag | Meaning |
|---|---|
| LIVE | Observed in a running, signed-in instance of the reference product (or, where stated, a signed-out instance). Where a live observation corrected an earlier static claim, the requirement uses the corrected fact and says so. |
| DISK | Read from the real profile, logs or runtime artifacts that instance left on disk: key names, enums, defaults, schemas, counts, timings. |
| STATIC | Read from the shipped bundle only: never seen running. Treated as a design reference, not as observed behaviour. |
| UNVERIFIED | Not seen by any method, or only a Modbit design choice with no parity fact behind it. The value is Modbit's own and no parity claim is made. |

A tag may have a qualifier in parentheses (for example `LIVE (260); STATIC (210 and 400)`), meaning different parts of one requirement are known differently. The rule that binds acceptance: a PX row's proof is always against Modbit's own measurable parameter, on a real Core. A tag never lowers that bar. An UNVERIFIED parameter is not evidence that a behaviour is right, only that Modbit chose it; the register in section 11 says what would close each such item.

## 4. Architectural constraints and ownership

The agent-first workspace adds no canonical subsystem. Every behaviour maps to an existing owner (doc 81), and the renderer is a view of the Core's session (REQ-EV-0076).

- The UI may not call providers, the filesystem, Git, a shell, the browser's CDP, a guest or a database; every effect goes through a typed preload function to the Core, which validates it. Each new protocol command is added to the thin-client conformance suite of doc 29 and passes it on the CLI and the desktop client.
- One orchestration graph, one event store, one protocol state, one policy kernel, one approval and effect ledger, one checkpoint engine, one terminal broker, one workspace and change engine, one provider gateway: nothing here introduces a second.
- A transcript, an agent header, a context breakdown, a queue view and a project grouping are projections of canonical state, rebuildable from the event log. None is a store.

| Behaviour | Canonical owner | Rows |
|---|---|---|
| Streamed text events, transcript and agent-header projections, archive and unread | `domain-events` | PX-041, PX-042 |
| Terminal stream and background terminals | `terminal` | PX-043 |
| Input queue management, typed interrupt, task mode and capability posture, project records | `core-runtime` | PX-050, PX-051, PX-063 |
| Skill inventory command | `skills` | PX-052 |
| Execution preference command (no routing change) | `model-gateway` | PX-053 |
| Run-mode presets and durable allowlist rules | `effects-security` | PX-057 |
| Context accounting by category | `context-engine` | PX-059 |
| Reversible per-turn checkpoints | `durability` | PX-061 |
| Worktree lifecycle, apply-back, Git hardening | `workspace-git` | PX-065, PX-066, PX-067 |
| Tokens, primitives, shell, agent list, conversation, apps panel, composer, trays, approval stack, dialogs, lifecycle UI | `desktop` | PX-044 to PX-049, PX-054 to PX-056, PX-058, PX-060, PX-062, PX-064, PX-068 |

### What Modbit keeps and does not downgrade

| Strength | How this specification preserves it |
|---|---|
| Exact-intent, revision-bound approvals (doc 23) | The approval card (AFW-F01, F09) shows the exact intent and Run binds the intent hash and candidate revision. Only the reference product's layout, shortcut and persistence behaviours are taken. A plain one-time Run button is rejected. |
| Revision-bound per-hunk review | Edits apply in the isolated worktree; acceptance stays at the review boundary on exact revisions (AFW-G05). |
| Takeover lease and emergency stop | The browser tab keeps its lease badge and takeover (AFW-I01); stop semantics reconcile effects first (AFW-E06, E07). |
| Secret broker, injection scanner | Deltas are redacted before append (PX-041); transcript content is untrusted input (AFW-C11). |
| Crash-recovery proofs | Every protocol row names a kill-and-restart case; presentation never claims progress the Core did not report. |
| Fail-closed policy | Always-ask classes ask in every run mode (AFW-F08); no classifier approves anything (AFW-F06). |

### Where the reference behaviour is weaker or riskier

| Reference behaviour (tag) | Modbit's stricter requirement |
|---|---|
| Under its default mode an outside-workspace write and a network fetch ran with no card (LIVE) | Both always ask, in every mode, unless a narrowly scoped durable rule says otherwise (AFW-F08). |
| Approvals are one-time with no visible intent binding (LIVE) | Approval binds the intent hash, scope and expiry; retries with changed parameters ask again (AFW-F09). |
| A classifier auto-approves effects (LIVE) | Out of scope: a second policy engine (AFW-F06). |
| Enter was inert while a policy tray was up (LIVE) | Enter shows the reason inline and keeps the draft (AFW-H02). |
| Archive removed the row with no undo seen and selection jumped elsewhere (LIVE) | Undo for at least 8 s, selection to the nearest sibling, confirmation for running work (AFW-B08). |
| A new chat inherited a cloud target silently (LIVE) | A new task is local by default; cloud is never inherited (AFW-B09). |
| Session-end hooks failed at window close (DISK) | Hooks run to completion or fail visibly and are journaled (AFW-J11). |
| Worktrees are force-removed by a cap (STATIC) | Only terminal, applied or discarded worktrees are removable; dirty ones raise attention (AFW-J07). |
| Apply-back offers overwrite choices with a remembered default (STATIC) | Cancel is the default, overwrite needs typed confirmation, destructive choices are never remembered (AFW-J08). |
| Git runs with repository hooks and fsmonitor live (STATIC; Modbit's runner is equally unhardened today) | Hardened runner (AFW-J09). |

## 5. Existing implementation audit

Performed for DOC-PX-007 at `main` 5fdb47f by tracing from the renderer and CLI through the SurfaceProtocol to the Core and its stores (docs 84 and 82). Classes are those of doc 93. The question for each area is what already exists, so that a row completes it in place and never duplicates it.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Streamed assistant text | **IMPLEMENTED-PARTIAL** | `crates/providers` streams real `ModelEvent::MessageDelta` and `ReasoningDelta` (M2.6, conformance suites). `services/modbit-core/src/runtime.rs` consumes the stream, accumulates `MessageDelta` text, drops reasoning deltas and emits only one first-token SLO event. No delta event, no completion record and no protocol message exist; the renderer cannot see text until the turn is done. | The runtime loop never appends a delta event (PX-041). |
| Transcript rows and agent headers | **NOT-FOUND** | The protocol serves `TaskView`, `SessionSnapshot`, `GetAttention` and the event stream; the renderer reducer (`model.ts`) keeps task, run and approval facts only. No transcript row model, header projection, status-class precedence, unread, archive of tasks or conversation search exists. A session can be archived in the domain (`SessionState::Archived`) but no client command does it. | No projection exists (PX-042). |
| Terminal stream to clients | **IMPLEMENTED-PARTIAL** | Durable terminal, replay window and OutputRef spill are production-working in the broker (`crates/terminal`, `services/modbit-execd`, IMP-EV-0271); the protocol state carries `TerminalCursorView`. There is no attach, list, kill or write message for clients, and the renderer has no terminal view. Release Zero's terminal-visibility step has no owning task. | No client-facing terminal messages (PX-043). |
| Design foundation | **SCAFFOLDED** | `packages/design-tokens` and `packages/ui` are `export {}` stubs. The renderer styles itself with one inline `<style>` of about 90 lines and nine variables, dark only through `prefers-color-scheme`, one button style, no focus-ring system. The whole UI is one 1,871-line `index.tsx`. The dark primary button measures 2.49:1 and fails AA. | Tokens, primitives and the module split (PX-044). |
| Shell, agent list, conversation | **NOT-FOUND** | Fleet, New Task, Review, Browser, Dashboard and Settings are production-working as a supervision console (M1.4, M6.6, PX-023, PX-024), with screen states, notifications and keyboard commands. There is no agent-first shell, agent list, conversation, apps panel, status row or palette. | Shell, list and conversation components (PX-045 to PX-048). |
| Window and quit lifecycle | **IMPLEMENTED-PARTIAL** | `main.ts` opens a 1200 x 800 window with remembered bounds and no minimum size; `before-quit` stops the browser host and the Core immediately with no prompt. A recovery banner shows the Core's `GetRecoveryReport` after a restart. The hook bus is production-working (IMP-EV-0139, IMP-EV-0242); its session-end ordering at shutdown was not traced. | No quit protection, minimum size or hook-completion guarantee (PX-049). |
| Input dispatch and queue | **IMPLEMENTED-PARTIAL** | Typed `QueueInput` STEER, COLLECT and FOLLOW_UP is production-working (REQ-EV-0191, doc 14). The renderer offers a one-line steer on a card. There is no list, edit, delete, reorder or send-now command, no typed interrupt of a live stream and no composer queue tray. | Queue management commands and interrupt semantics (PX-050). |
| Task modes | **IMPLEMENTED-PARTIAL** | The mechanisms exist: plan-before-write (PX-014), reproduction-first (PX-039), subagent admission with capacity tickets (M6.3), task-scoped tool projection (M5.1). `CreateTask` carries only an `execution_profile` string. There is no typed mode field and no mode-bound capability posture. | The mode field and its enforcement (PX-051). |
| Skills listing and slash menu | **IMPLEMENTED-PARTIAL** | The registry is production-working with CLI install, list, revoke and remove (M5.5, `apps/cli`). No `ListSkills` message exists in `surface.proto` and the renderer has no skill, mention or slash surface; its composer is the New Task goal field with `attachFile`. | No protocol command and no composer (PX-052, PX-054). |
| Execution preference and model picker | **DOCUMENTED-ONLY** | `SetExecutionPreference` is specified in docs 30 and 32 and appears in no `.proto` and no task card. `ListModels` and the dashboard model rows exist; the renderer has no picker. | The command is unwired (PX-053, PX-056). |
| Approvals and run modes | **IMPLEMENTED-PARTIAL** | Exact-intent approvals are production-working: `ListApprovals`, `ResolveApproval` with the intent hash and `INTENT_MISMATCH`, a receipt chain, and the renderer's approve and deny with confirmation (PX-001, PX-024). The policy kernel resolves ALLOW, ASK or DENY by device, admin, project and user; no run-mode preset, durable allowlist rule or rule command exists (no allowlist handling in `crates/policy`). | Presets and rules (PX-057); the docked card (PX-058). |
| Context accounting | **IMPLEMENTED-PARTIAL** | The Context Inspector is production-working: entries, token budget and use, compaction epochs and prefix-cache counters (`GetContextInspector`, IMP-EV-0035, IMP-EV-0131). There is no eight-category breakdown, no model-aware window and no ring in the renderer. | Category accounting (PX-059) and the ring (PX-060). |
| Checkpoints and restore | **IMPLEMENTED-PARTIAL** | The baseline-plus-delta engine, epoch fencing and kill-point recovery are production-working (M4.3, M4.6). `RestoreCheckpoint` carries optimistic content hashes and refuses on `HASH_MISMATCH`; `PreviewRewind` and `ForkTask` exist and a fork carries no pending approval. Per-turn recording, a pre-restore checkpoint for an exact redo and a per-file choice for user-edited files were not found; the renderer has no checkpoint surface. | Reversible restore (PX-061) and its dialog (PX-062). |
| Projects | **NOT-FOUND** | No project concept exists in `surface.proto`, the domain or the renderer. Sessions, tasks and the WorkGraph exist. | Records and membership (PX-063), sidebar (PX-064). |
| Worktree lifecycle and apply-back | **IMPLEMENTED-PARTIAL** | `crates/git` creates, lists and removes worktrees and runs a merge transaction with `snapshot_dirty`; task and child worktrees are created under the profile's `worktrees/` root (`branch.rs`, `agent_tools.rs`). No scheduled cleanup, lease, retention caps, dirty-state copy policy, setup-command policy, apply-to-checkout transaction with exact undo, or conflict modal was found. | Lifecycle policy (PX-065), apply-back (PX-066), UI (PX-068). |
| Git hardening | **NOT-FOUND** | The shared Git runner in `crates/git` sets only `GIT_TERMINAL_PROMPT` and `LC_ALL`. No neutralisation of repository hooks, `core.fsmonitor` or attributes files, and no bare-repository safeguard, was found in any crate. | Hardened runner (PX-067). **Note (2026-10-05):** the audit fix FIX-01 (pull request moss101/modbit#61, not merged) delivered the hook, fsmonitor, attributes, filter and textconv neutralisation, ref validation and error redaction in `crates/git`; the remaining scope of PX-067 is listed on its card in doc 62. This row's classification describes `main` at the audit baseline. |

Summary of the 16 areas: 1 DOCUMENTED-ONLY, 10 IMPLEMENTED-PARTIAL, 4 NOT-FOUND, 1 SCAFFOLDED; none is PRODUCTION-WORKING as an agent-first experience, although the Core mechanisms beneath it (provider streaming, durable terminals, typed input, exact-intent approvals, the context inspector, checkpoints, worktree primitives) are production-working and are reused in place. No duplicate generation was found. The mock-only risk to avoid is the one named in doc 80: a state gallery or scripted provider that passes while the Core path does not exist.

## 6. Requirements

Requirement text is Modbit's own. The tag column follows section 3; the rows column names the PX rows that deliver it (doc 62).

### A. Shell, geometry and design foundation

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-A01 | The workspace has three regions: an agent list on the left, one conversation in the centre, and a collapsible typed-apps panel on the right. A status row sits under the composer and a 40 pt top bar spans the centre. The apps panel is hidden until a task has an artifact to show. There is never a fourth permanent column; the Work timeline of doc 10 lives inside the conversation. | LIVE | PX-045 |
| AFW-A02 | Default window 1280 x 800 pt. The minimum window is 900 x 600 pt until single-pane mode exists; the 500 x 520 pt minimum of the reference product is not adopted. | LIVE (default size); UNVERIFIED (minimum) | PX-045, PX-049 |
| AFW-A03 | Agent list width 260 pt by default, user-resizable between 210 and 400 pt, persisted per profile. | LIVE (260); STATIC (210 and 400) | PX-045 |
| AFW-A04 | Top bar height 40 pt. It carries, left to right: sidebar toggle, history back and forward, the task title with its execution-location glyph, an overflow menu, and the apps-panel toggle. The standard control height is 28 pt (the search field measured 28 pt). | LIVE | PX-045 |
| AFW-A05 | Navigation rows are 30 pt high with a radius of about 8 pt and an 8 pt inset. An agent row is two lines, about 43 pt: title and a repository subtitle, a status glyph at the left and a relative age at the right. The age is joined by pin and archive actions on hover or focus. | LIVE (corrects STATIC 28 pt row) | PX-046 |
| AFW-A06 | The centre pane never narrows below 424 pt. With the apps panel open at a 1280 pt window the panel takes the remainder (597 pt was observed). Panel width is a persisted ratio, default 0.5, clamped so the centre keeps 424 pt and the panel keeps at least 384 pt. | LIVE (centre minimum and 597 pt); DISK (ratio is stored as a float); UNVERIFIED (384 pt minimum, default ratio) | PX-045 |
| AFW-A07 | Composer proportions at a 1280 x 800 window are acceptance targets, not pixel mandates: empty-state composer about 608 x 106 pt with a radius of about 17 pt; in-conversation composer a single-line pill about 718 x 42 pt. The composer fills the content width up to a cap of 840 pt, which is Modbit's own value because the reference cap was never reached on screen. | LIVE (608, 106, 718, 42); UNVERIFIED (840 cap) | PX-047, PX-054 |
| AFW-A08 | Below a centre-pane width of 448 pt the shell becomes a single pane with a segmented control (Conversation, Apps) and the agent list collapses to a 40 pt rail. | STATIC | PX-045 |
| AFW-A09 | Apps-panel visibility, fullscreen state and the remembered app tab per kind are persisted per task, not globally. Inner tree widths default to 180 (files), 220 (changes), 220 (terminals) and 220 (browser) pt. | DISK | PX-045, PX-048 |
| AFW-A10 | Settings replaces the shell: the left column becomes a settings navigation with a Back row, a search field, grouped entries separated by extra gap, and a pinned footer. The content column is a single centred column of about 680 pt; list rows are 44 pt; toggles are about 32 x 18 pt. Modbit chooses its own group and entry names. | LIVE | PX-045 |
| AFW-A11 | Design tokens are Modbit's own, emitted by one typed function in packages/design-tokens for dark and light with identical key sets: a neutral surface ladder derived from one base colour (text 100/74/60/36 percent, icon 100/66/52/28, fill 20/14/8/6/4, stroke 20/12/8/4), a 4 pt spacing grid with half steps, radii 2 to 14 plus full, control heights 20/24/28/32, type 13/18 for chrome and a conversation size scaled by a Text size setting, motion 50/100/150/200/300 ms with press scale 0.98, and z-layers for modal, popover and tooltip. No colour, theme file, icon, font or string of the reference product is used; Modbit chooses its own brand mark. | LIVE (signed-out instance: the alpha ladder); STATIC (spacing, radii, control sizes, motion) | PX-044 |
| AFW-A12 | Every text and control pair meets WCAG 2.2 AA, checked by a computed contrast test in CI for both themes: 4.5:1 for text, 3:1 for control edges and icons. The 36 percent text level and 28 percent icon level are for disabled and decorative use only. Keyboard focus is a visible 2 px ring. The reference product misses AA in several places and Modbit deliberately deviates. | STATIC | PX-044 |
| AFW-A13 | Themes are Dark, Light and Follow system, persisted as a preference. Light was observed in a running instance; dark was not, so dark values rest on the contrast test and the packaged accessibility suite, not on parity. | LIVE (light); UNVERIFIED (dark) | PX-044 |
| AFW-A14 | Reduced motion and forced colours are honoured: with prefers-reduced-motion every transition and animation collapses to an instant change and the shimmer stops; status is never conveyed by colour alone. | STATIC | PX-044, PX-045 |
| AFW-A15 | A command palette opens with the primary-modifier K chord and searches agents, actions and settings. It subsumes the existing fleet type-ahead search of doc 39. | DISK | PX-045 |
| AFW-A16 | Default shortcuts (macOS notation; the primary modifier is Ctrl on other platforms) keep the PX-024 bindings and add: new task (primary N), palette (primary K), changes app (primary E), terminal app toggle (primary J), browser app (shift primary B), files app (primary G), settings (primary comma), shortcut help (ctrl shift slash), zoom (primary plus, minus, zero). The full registry of the reference product (about 340 bindings) was not recovered, so Modbit defines only this set and adds more through the shortcut registry with a conflict test. | DISK (21 accelerators); UNVERIFIED (full registry) | PX-045 |

### B. Agent list

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-B01 | Each agent row shows a status glyph, a truncated title, the repository as a second line, a relative age, and a small attention dot when the task needs the user. Hovering a row shows the full title and the workspace path in a tooltip. | LIVE | PX-046 |
| AFW-B02 | Row status class is computed by the Core header projection, in this precedence: needs attention, failed, ready for review (unseen before seen), running, waiting, completed, draft, archived. The renderer renders the class it is given and never recomputes it, so MOD-UX-001 (attention first) holds on every client. Status is never colour alone: each class has its own glyph and text label. | STATIC (precedence order); DISK (the inputs exist as header fields) | PX-042, PX-046 |
| AFW-B03 | A task waiting on an approval shows the pending state on its row (attention dot) and in the conversation tail status. The state survives switching to another task and restarting the renderer. | LIVE | PX-046, PX-058 |
| AFW-B04 | Unread is a first-class header field. An unread dot appears on the row; inside a conversation an unread divider marks the first unseen row, and a floating pill counts new messages that landed while the user was scrolled up. | LIVE (pill); DISK (header field); STATIC (divider) | PX-042, PX-046, PX-047 |
| AFW-B05 | Grouping is by repository by default. Selectable groupings are repository, workspace, status, execution location and time buckets; in phase 3 a Project grouping is added. Section order, collapsed state and removed sections are persisted per grouping. | DISK (repository default and schema); STATIC (other groupings) | PX-046, PX-064 |
| AFW-B06 | The list is a stored query with filter chips: Fleet state classes (needs attention, unread, running, draft, done, failed), execution location (local, cloud), pull-request state (draft, open, merged, closed, none) and task origin (desktop, CLI, IDE adapter, forge issue, forge webhook). Row subtitle fields (workspace, location, branch, timestamp, origin, pull-request state) are individually toggleable. | DISK | PX-046 |
| AFW-B07 | Pinning has a hard cap and shows the cap message when reached. Pinned tasks form the first section. | STATIC | PX-046 |
| AFW-B08 | Archiving is a durable Core event that is reversible. The reference product removed the row at once and showed no undo toast in the first 0.8 s, and its selection then jumped to an unrelated most-recent task. Modbit shows an undo affordance for at least 8 s, moves selection to the nearest remaining sibling, and asks for confirmation before archiving a running task because it stops the run. | LIVE (negative observation); STATIC (undo toast, running-task warning) | PX-046 |
| AFW-B09 | A new task never silently inherits a cloud execution location from the previously viewed task. The reference product did, and a new chat would have started a cloud run. Modbit defaults a new task to the last local trusted workspace and shows the location as an explicit pill that must be chosen to leave local. | LIVE (negative observation) | PX-046, PX-054 |
| AFW-B10 | The list renders from a small header projection (id, workspace, title, subtitle, timestamps, status class, unread, pending approval, pending plan, context percent, files changed, lines added and removed, checkpoint time, subagent flag, archived flag, execution location) and never loads a transcript. Search covers headers and a Core-side full-text index of conversations. | DISK | PX-042, PX-046 |
| AFW-B11 | Next and previous task shortcuts exist, and a hold-to-switch recent-task switcher opens on control Tab. Exact chords other than those in AFW-A16 are decided by the shortcut registry. | STATIC | PX-045, PX-046 |

### C. Conversation

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-C01 | The conversation is a projection of the canonical event log, never a second store. Row kinds: user message card, assistant text without a bubble, work group, tool card, approval card, tail status, turn footer, time boundary, unread divider. Each row carries render hints (renderable, groupable, has reasoning, duration, short plain text, edit line counts, status) so folding and density never re-read bodies. | STATIC (row kinds); DISK (render hints) | PX-042, PX-047 |
| AFW-C02 | Streaming text appears chunk by chunk; the newest words fade from dim to full; the view follows the tail while pinned to the bottom. While streaming a row only grows, never shrinks, to avoid jitter. | LIVE (progressive text, auto-follow); STATIC (monotonic growth) | PX-041, PX-047 |
| AFW-C03 | While a turn runs there is always a tail-status line naming the current action, with a shimmer: planning, running a named command, editing a file, using the browser, waiting for approval, waiting for a subagent. Labels are humanised from the typed tool intent and are Modbit's own words. A stall ladder escalates the wording and, past the configured threshold, raises the existing STALL attention item. | LIVE (line and shimmer); DISK (stall ladder 2 s to 32 s) | PX-047 |
| AFW-C04 | A finished turn folds its steps under a header such as Worked for N s. Reads and searches fold into one summary line, shell results into Ran N commands, expandable to the command and its output. Reasoning shows as a collapsed row. Code blocks are collapsed by default and the preference is persisted. | LIVE (Worked for, Ran N command); DISK (code block preference) | PX-042, PX-047 |
| AFW-C05 | Three density settings (Compact, Balanced, Detailed) are projections of the same event log, never separate data. Compact is the default. | STATIC | PX-042, PX-047 |
| AFW-C06 | Scrolling is pinned to the bottom while the user has not scrolled up. When new content lands above the fold a pill reads N new messages with a dismiss control, and a scroll-to-bottom button appears. Jump to previous and next user message exist as shortcuts. | LIVE (pill, button); STATIC (jump shortcuts) | PX-047 |
| AFW-C07 | The latest user message stays pinned as a sticky header card while its turn is on screen. Conversation line pitch is about 24 pt at the default text size, and a Text size setting scales it. | LIVE (sticky card, 24.8 pt pitch); STATIC (scale values) | PX-047, PX-044 |
| AFW-C08 | A turn footer shows a relative timestamp (full date on hover), copy, and fork. A thumbs-up and thumbs-down pair exists in the reference product; it feeds online quality telemetry, which is phase 6 and out of scope, so Modbit ships no rating control in these rows. | LIVE | PX-047 |
| AFW-C09 | A failed turn restores the unsent prompt text to the composer, keeps the partial transcript readable, and docks an error tray with a retry action. Open conversations stay readable offline. | LIVE (prompt restored, tray, retry on hover); STATIC (offline tray) | PX-047, PX-060 |
| AFW-C10 | Find in conversation opens with the primary F chord and searches rows, not the DOM. | STATIC | PX-047 |
| AFW-C11 | All transcript content is untrusted input: no raw HTML, links open only through the main process allow-list, images load through Core artifact reads, and nothing in a row can reach IPC beyond the preload allow-list (REQ-EV-0076). | UNVERIFIED (design choice) | PX-047 |

### D. Composer and modes

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-D01 | The composer is a rounded container with a bottom row: a round add-context button on the left, the mode chip when a mode is active, the model chip (name plus a dimmer variant), and on the right a send button that becomes a stop square while a turn runs. Placeholders are context-specific (new task, follow-up, one per mode) and are Modbit's own words. | LIVE | PX-054 |
| AFW-D02 | Suggestion chips below the empty composer offer Plan and Multitask with the cycle key as a hint; the chip of the active mode is hidden. | LIVE | PX-054 |
| AFW-D03 | Modes are Agent (default, no chip), Plan, Debug, Multitask and Ask. Shift+Tab cycles from the default in the order Plan, Debug, Multitask, Ask. A click on the chip label does nothing in the reference product, and an X on the chip returns to the default. The classic window of the reference product opens a menu instead; that is a different shell. Modbit's chip is also a button that opens a menu, so the mode is reachable by pointer and by assistive technology, and the cycle key stays the fast path. Wrap-around after Ask is not observed and Modbit defines it as returning to the default. | LIVE (corrects STATIC; the cycle, order, X); UNVERIFIED (wrap-around) | PX-054, PX-051 |
| AFW-D04 | Mode tints are Ask green, Plan amber, Debug red, Multitask purple, using Modbit's own tint tokens. The send button takes the Ask tint when text is present in Ask mode. The tint is never the only signal: the chip carries an icon and a label. | LIVE | PX-054, PX-044 |
| AFW-D05 | A mode is a bundle of presentation (tint, icon, placeholder) and an enforced posture owned by the Core: Ask is read-only, Plan records a plan and writes nothing until it is accepted, Debug requires a reproduction before a fix (PX-039), Multitask enables subagent admission with capacity tickets. The mode is a typed field the Core receives; the renderer cannot grant a posture. | DISK (mode records separate presentation from behaviour flags) | PX-051 |
| AFW-D06 | An agent may propose a mode switch as a typed question card with Skip and Switch; consent can be remembered per transition. An unanswered proposal expires to skipped after 15 s and never to accepted. | STATIC (card, 15 s); DISK (remembered-consent lists) | PX-051, PX-055 |
| AFW-D07 | The add-context menu and the @ menu list typed sources: files and folders, past tasks, terminals, the branch diff against base, browser tabs, skills and subagents. Chips are typed; a path the Core's path policy denies shows the reason on the chip and cannot be attached. | LIVE (five pre-login categories); DISK (wider type inventory); STATIC (blocked reasons) | PX-054 |
| AFW-D08 | The / menu is a typed union of skills, commands, subagents, actions and model switches. Built-in entries come first, then a divider, then imported entries alphabetically; a detail card for the highlighted entry floats beside the list; ranking uses recency and usage. Each entry carries a sync-eligibility flag (local-only or shareable) and the menu is rebuilt when the skill inventory changes. | LIVE (order, divider, detail card); DISK (union, counters, eligibility flag) | PX-052, PX-054 |
| AFW-D09 | Attachments (images, documents, text) enter only through the Core's IngestAttachment path with its type and size limits; the renderer never reads the file. | STATIC | PX-054 |
| AFW-D10 | Alt+Up and Alt+Down walk previous prompts. An unsent draft survives navigation and restart as a local convenience; it is never authoritative state. | STATIC (history keys); LIVE (prompt restored after failure) | PX-054 |
| AFW-D11 | Enter sends. While a turn runs the primary-modifier Enter is the alternate behaviour, and a setting swaps them. Only plain Enter was exercised in the reference product, so the alternate is Modbit's definition (AFW-E05). | LIVE (Enter); UNVERIFIED (modified Enter) | PX-054, PX-055 |
| AFW-D12 | A status row under the composer shows an execution-location selector, the branch pill and the context ring. Changing location is never silent and is refused while a move or run is in progress. | LIVE | PX-054, PX-060 |
| AFW-D13 | The model chip shows the model name with a dimmer variant label. Its picker opens upward from the chip, about 228 pt wide with a radius of about 14 pt, and holds a search field, an Auto row with a toggle and a short caption, a divider, then model rows (name in primary text, variant such as effort or speed dimmed), a check on the current choice, a new badge and a scrolling list. A variant is part of a model's identity. In Modbit the Auto row is the objective profile (Cost, Balance, Intelligence or a permitted organisation profile), set through SetExecutionPreference, and a manual pin appears only where policy allows. | LIVE (picker, Auto row, variants, check, badge); UNVERIFIED (lower part: Max toggle, multi-model count) | PX-053, PX-056 |
| AFW-D14 | Model parameters are enum dimensions (for example reasoning effort, speed, context window) with a precomputed variant list per model from the registry. The picker consumes variants and never computes combinations, and a value that raises cost is marked. Per-feature model slots and a multi-model comparison are not in scope. | DISK | PX-053, PX-056 |

### E. In-run control

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-E01 | Pressing Enter while a turn is running queues the message by default; the message is not lost and does not interrupt the run. In Modbit's typed input vocabulary a queued message is a FOLLOW_UP. | LIVE | PX-050, PX-055 |
| AFW-E02 | A queue tray docks above the composer with the header N queued, a clear control, and one row per message (single line, truncated, about 22 pt). On hover or focus each row offers Send now, Edit, Delete and an overflow menu. The tray can collapse. Keyboard: arrow keys move between rows, Right or Space edits, the primary modifier with Backspace deletes, Escape leaves. Drag reorder is part of the reference design but was not observed. | LIVE (header, per-item controls); STATIC (keyboard, drag reorder) | PX-050, PX-055 |
| AFW-E03 | Queued messages are durable Core input: they drain in order, each as its own turn, when the current turn ends, and they survive renderer and Core restarts. Each item keeps its own mode and model selection. | LIVE (drained in order at turn end); STATIC (per-item overrides) | PX-050 |
| AFW-E04 | The first time a message is queued, an education tray explains the three behaviours (queue, collect, steer) and offers Keep queuing or Open settings. It is shown once and the acknowledgement is persisted. | LIVE | PX-055 |
| AFW-E05 | The send behaviour while running is a setting with the values queue (FOLLOW_UP, after the turn), collect (COLLECT, coalesced into one next turn), steer (STEER, applied at the next safe boundary) and stop-and-send. A second setting says what Send now does (interrupt or steer). Labels must state the real consequence: Modbit's STEER may interrupt the current model call at a safe boundary, so it is never labelled as sending without stopping the agent. | STATIC (settings and values); DISK (defaults, no override stored) | PX-050, PX-055 |
| AFW-E06 | Send now moves the queued item into the conversation immediately as a new user turn. Whether the reference product cuts a live stream was not shown. Modbit defines it: Send now is a typed interrupt-and-replace; effect-bearing tool calls in flight reconcile first at the safe boundary; the interrupted stream ends as aborted with source user-interrupt; the partial text stays visible and marked partial; the new turn starts only after reconciliation. | LIVE (moves at once); UNVERIFIED (interrupt of a live stream) | PX-050, PX-055 |
| AFW-E07 | Stop is one control, a square in place of send. It ends the current turn at a safe boundary within about 1 s. A user interrupt and a runtime or transport abort are different typed outcomes with different texts. After Stop the last user message becomes editable in place and a Continue chip appears. | LIVE (stop in about 1 s, editable message, chips); DISK (two abort outcomes) | PX-050, PX-055 |
| AFW-E08 | A chip above the composer counts background terminals. Its tray lists each with its title, an elapsed timer and, on hover, a kill control. Killing one records the outcome and wakes the agent through a typed background-process-ended event, not a simulated user message. | LIVE (tray, timer, kill wakes agent); DISK (completion injects a synthetic wake-up) | PX-043, PX-055 |
| AFW-E09 | An ephemeral side question about the current work can be asked without derailing the main run, using the existing AskSideQuestion command; the answer does not enter the main transcript's context unless the user adds it. | STATIC (semantics); LIVE (a Side Chat entry exists in the apps menu) | PX-055 |

### F. Approvals and run modes

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-F01 | A pending protected effect docks a card at the transcript tail. Header: a humanised title and the command name. Body: the exact command line with operators and substitutions visible. Footer: a policy menu on the left, then Skip, Always, and the primary Run. Modbit's card also shows the effect class, a one-line exact-intent summary (argv, working directory, paths written, hosts contacted), why it is asking (a typed reason), whether the effect is contained in a sandbox, and the scope and expiry of the grant. Run is bound to the intent hash and candidate revision; any change of parameters invalidates the card. | LIVE (layout, buttons); UNVERIFIED (additions are Modbit's) | PX-058, PX-057 |
| AFW-F02 | The pending state is shown in the tail-status line and on the agent row, and it persists across switching to another task, a renderer restart and a Core restart. A pending card never expires into an approval. | LIVE (persists across thread switch) | PX-058 |
| AFW-F03 | Skip is a typed user decision (denied by user). It ends that call, returns a plain 'skipped by the user' result to the model that is distinct from a tool error, and the agent continues. There is no reject-with-instruction text box on the card; a follow-up is an ordinary message. This corrects the reading of stored data that a rejection carries an instruction. | LIVE (corrects DISK inference) | PX-058 |
| AFW-F04 | Enter runs and Shift+Enter means Always while the card is the active tray. Escape is Skip. Enter and Shift+Enter were confirmed on the card; Escape was not exercised. Modbit's focus rule is stricter: these keys act on the card only when it has visible keyboard focus or the composer is empty, so a person typing a message cannot approve an effect by accident, and irreversible effects keep the confirmation step of doc 39. | LIVE (Enter, Shift+Enter); UNVERIFIED (Escape) | PX-058 |
| AFW-F05 | With several pending effects the surfaced card is the oldest by Core sequence, the header shows N pending, and each decision applies to exactly one effect. The multi-call deck was not observed live. Modbit forbids batch approval of protected effects. | UNVERIFIED | PX-058 |
| AFW-F06 | Run modes are presets of who approves effects inside the Capability Kernel's envelope; they never widen the envelope (capabilities, paths, egress). The reference product offers Allowlist, Allowlist with sandbox, Auto-review with sandbox (a classifier decides; the account default), and Run everything unsandboxed, with no ask-every-time option. Modbit offers Ask (default: every protected effect asks), Allowlist, Allowlist with sandbox and, session-scoped, Run everything. A classifier that approves effects is a second policy engine (doc 81) and is out of scope. | LIVE (the four modes and the account default) | PX-057 |
| AFW-F07 | Moving to a mode that approves more shows a confirmation dialog naming the risk (prompt injection and exfiltration). Run everything is per session, never persisted across a restart, requires the dialog every time, and still excludes the always-ask classes of AFW-F08. The reference product also confirmed the move to Allowlist and showed no dialog when the original mode was restored; Modbit asks only when the move approves more. | LIVE (dialog for Allowlist); STATIC (Run everything dialog) | PX-057, PX-058 |
| AFW-F08 | Always-ask classes never auto-run in any mode without an explicit, narrowly scoped durable rule: a write outside the workspace, a network fetch or connection, any use of a secret, a protected or configuration path, a deletion, a push, and an escalation beyond the declared capability. The reference product ran an outside-workspace write and a network fetch silently under its default mode. | LIVE (negative observation) | PX-057 |
| AFW-F09 | Approval is bound to the normalised intent hash, scope and expiry (doc 23). Run executes that one effect once; the same command asked again asks again. A retry with changed parameters needs a new approval. This is stricter than the reference one-time Run, whose binding is not shown. | LIVE (Run executed once, the same card appeared again) | PX-057, PX-058 |
| AFW-F10 | Always creates a durable allowlist rule through the Core, never in the renderer: an argv-prefix pattern chosen from a ladder of prefixes (default, selected and applied sets are separate), a repository scope, creator, time, optional expiry and a receipt. Rules are listed and revocable in settings. A pattern never matches shell operators, redirections, substitutions or pipelines, and compound commands are matched by their sub-commands. The ladder's on-screen form was not observed. | DISK (ladder sets, compound-command list); UNVERIFIED (on-screen form) | PX-057, PX-058 |
| AFW-F11 | A tool call declares the escalation it needs (none, network, all) as a typed field; the Kernel approves the escalation, not the command alone. The reference card showed no network or permission wording for a network command, so Modbit's wording is its own. | DISK (declared escalation); UNVERIFIED (card wording) | PX-057, PX-058 |
| AFW-F12 | Every block carries a typed reason shown on the card: not in allowlist, protected path or configuration, hook, read-only mode, policy. Two audiences get two messages: a client-visible message and a model-visible message; the raw error is never returned to the model. | DISK | PX-057, PX-058 |
| AFW-F13 | The instruction text a user typed never bounds what the agent may do. In the reference product an 'exactly this command' prompt did not stop the agent exploring other tools after a hung command. Only the capability lease and the approval bound behaviour. | LIVE (negative observation) | PX-057, PX-058 |

### G. Checkpoints and review

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-G01 | A checkpoint is taken at each user turn. It records the files changed since the pre-conversation state, files that did not exist, and folders created, as the Core's baseline-plus-delta checkpoint (not Git). Restoring replays the deltas and deletes files that did not exist. | DISK | PX-061 |
| AFW-G02 | A restore control appears on hover on each user message. It opens a confirmation dialog: discard all changes up to this checkpoint, with Cancel and Continue and a note that this can be undone. Modbit's dialog additionally counts files to change and delete and warns about files edited since the checkpoint; it offers no do-not-ask-again for a restore that would overwrite such files. | LIVE (dialog and wording) | PX-062 |
| AFW-G03 | After Continue the files are restored, files created after the checkpoint are removed, the user message becomes editable in place, and a Redo checkpoint link appears. The reference dialog for Redo reads the same as for restore, so it was not shown whether it redoes or discards again. In Modbit restore first records a pre-restore checkpoint and Redo restores exactly that one, with distinct wording. | LIVE (restore behaviour); UNVERIFIED (redo semantics) | PX-061, PX-062 |
| AFW-G04 | Editing an older message asks Revert files or Keep files (Keep is Shift+Enter) when the files differ from that message's checkpoint, and removes later messages. This dialog was not shown live (the restore dialog offered only Continue and Cancel), so it is a static design that Modbit adopts with its own wording. | STATIC; UNVERIFIED (live) | PX-062 |
| AFW-G05 | File edits by the agent apply inside the task's isolated worktree through ChangeTransaction with no accept step, as in the reference product's default. Acceptance happens at the boundary where work leaves the worktree: revision-bound per-hunk review (kept and stronger than line-range diffs), then apply-back or a pull request as protected effects. The changes panel offers per-file revert and stage; Undo all needs a second confirming click. | LIVE (edits auto-applied, revert and stage controls); STATIC (Undo all two-step) | PX-048, PX-066, PX-068 |
| AFW-G06 | The changes panel scope menu offers last turn, uncommitted, branch versus base and pull request. A split Commit and push button and stage checkboxes exist. Commit, push and pull request creation are protected effects with approval and receipts (docs 29 and 23). Pull request and CI panels were not seen live. | LIVE (staged and uncommitted scopes, last turn, commit split button); STATIC (the rest) | PX-048, PX-062 |
| AFW-G07 | Fork from a message is available on the turn footer through the existing ForkTask command; a fork never carries a stale pending approval. | LIVE (control present); STATIC (semantics) | PX-047, PX-062 |
| AFW-G08 | Chips above the composer summarise the changes (Changes +N) and offer the next action (Commit, Continue). Labels are Modbit's, and any chip that performs an effect goes through its approval. | LIVE | PX-048, PX-055 |

### H. Trays, failure and context accounting

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-H01 | Transient attention UI docks above the composer as a tray and is never a modal: queue, steer education, error, plan or question, mode switch, approval, background terminals, context usage, usage limit. One active tray owns its scoped keybindings, and trays persist across navigation until resolved or dismissed. | LIVE (error, steer-education, queue and terminal trays); STATIC (the single-owner rule) | PX-044, PX-055, PX-058 |
| AFW-H02 | When policy blocks the selected model for this person (the reference free plan allows only its routed Auto model), the turn fails before any token, the prompt is restored, and a tray names the cause with two actions: switch to an allowed model and an upgrade or policy-contact action. In the reference product Enter was inert while the tray was up. Modbit's Enter instead shows the reason inline and keeps the draft. The cause is a typed Core error class, and Modbit ships no billing UI. | LIVE | PX-056, PX-060 |
| AFW-H03 | Budget or usage exhaustion shows a tray with staged outcomes (slower pool, hard block, auto-switch) mapped to Modbit's existing budget policy, with actions to raise the budget or continue at lower cost. The stages are the static design; none was observed. | STATIC | PX-060 |
| AFW-H04 | The context ring in the status row shows used over window. Its tray lists exactly eight categories whose estimated tokens sum to the total: system instructions, tool definitions, rules and instructions, skills, external tool definitions, subagent definitions, summarised conversation, conversation. The ring was seen; its tray was not opened. | DISK (eight categories summing to the total); LIVE (ring present); UNVERIFIED (tray) | PX-059, PX-060 |
| AFW-H05 | The Core computes the category breakdown, the model-aware window and a usage percent, and the same figures feed the agent list header and the context inspector. Totals must match the provider request envelope within the estimator's declared error. | DISK | PX-059 |
| AFW-H06 | A usage summary appears only near a limit (auto), always, or never, by setting. It shows cost of the task from the Core's usage ledger. | STATIC | PX-060 |

### I. Right-panel apps and terminal

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-I01 | Apps are typed tabs in the right panel: Changes, Terminal, Browser, Files (read-only trusted code view) and Evidence. A plus menu with a search field opens an app or a side question and shows its shortcut. Modbit has no canvas app and no editor. | LIVE (list and plus menu); UNVERIFIED (Evidence, Modbit's addition) | PX-048 |
| AFW-I02 | When the panel is closed a rail of per-task artifact icons with counters opens the matching tab. | LIVE (rail with app list and counts); STATIC (counter set) | PX-045, PX-048 |
| AFW-I03 | The terminal app attaches to a terminal session by cursor, shows the replay window, and loads older output as OutputRef ranges. Agent-owned terminals are read-only with a clear label; closing one warns that it stops a running process. User input is enabled only where policy says the terminal is user-controlled, and ownership is explicit. | STATIC; UNVERIFIED (the terminal app itself was not opened live) | PX-043, PX-048 |
| AFW-I04 | The Core serves the terminal stream to every client by cursor: ordered chunks, replay window, OutputRef spill for the rest, and a list of background terminals with state. Release Zero's terminal-visibility step needs this and no graph task owned it before this ledger. | STATIC | PX-043 |

### J. Projects, worktrees and lifecycle

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-J01 | A Project is a named, archivable record bound to one workspace, with an external membership map from task to project. Its icon, colour, views and any coordinator behaviour are not evidenced: stored records carry only id, name, workspace, timestamps and an archived flag. Modbit's Project is a grouping over the existing WorkGraph and carries no coordinator semantics. | DISK (record and map); UNVERIFIED (coordinator behaviour) | PX-063 |
| AFW-J02 | The agent list shows Projects as a group separate from repositories, with a New Project row. | LIVE | PX-064 |
| AFW-J03 | Membership rules: only top-level tasks can join; no move across projects; a project's task cannot be re-parented in place; draft tasks and imported sessions cannot join. Re-parenting is a two-phase, recoverable operation with a startup recovery pass. | STATIC | PX-063, PX-064 |
| AFW-J04 | Project pages (list, board, brief, library), notes with a task database, a pull-request tray and scheduled tasks are not in these rows. They need their own Decision Record. | STATIC | PX-063 |
| AFW-J05 | A task's worktree lives under a Core-managed root outside the user's checkout. Creation prunes stale entries, handles an empty repository, retries path collisions, carries uncommitted and untracked files by copy (not stash) subject to path policy, and types the 'branch already checked out elsewhere' failure. A setup command list read from the repository is data: it runs only in a trusted repository, as a typed sandboxed process with a 300 s timeout, its output as an OutputRef, and a failure is visible on the card and not fatal. | STATIC | PX-065 |
| AFW-J06 | Worktree cleanup runs every 6 h. At startup, if the last run is older than the interval, a catch-up runs after 30 s. The last-run time is persisted. A single lease with an owner and a reason prevents concurrent cleanup; a second request is refused while it is held. Each run reports scanned, removed, bytes freed and errors, and a skipped run is never reported as completed. | DISK | PX-065 |
| AFW-J07 | Retention defaults: at most 25 worktrees and 50 GB in total, orphaned first then least recently used, with hysteresis; a worktree younger than 10 minutes or with a running task is protected. These caps never fired in the sampled profile. Modbit is stricter: only a worktree whose task is terminal and whose changes are applied, exported or discarded is removable; a dirty or unapplied worktree is never removed automatically and instead raises an attention item. | STATIC; UNVERIFIED (live) | PX-065 |
| AFW-J08 | Applying a worktree's result to the user's checkout is a protected effect with exact revisions. On conflict a modal offers Merge manually, Stash, Overwrite (conflicting files only), Full overwrite, Undo and apply, and Cancel. Modbit records a pre-apply checkpoint so Undo is exact, makes Cancel the default, requires typed confirmation for any overwrite, and remembers a choice per task only for non-destructive options. | STATIC; UNVERIFIED (live) | PX-066, PX-068 |
| AFW-J09 | Git runs hardened: repository hooks, fsmonitor and attributes files are neutralised, a bare-repository safeguard is on, URL credentials are redacted from logs, and prompts are disabled. Modbit's git crate today sets only the prompt and locale environment. | STATIC | PX-067 |
| AFW-J10 | Changing a task's execution location mid-turn, including the migration strategies and their dialogs, is out of scope here: Modbit's local-to-cloud handoff (M8.7) is the existing owner. The location history is append-only. | DISK (location history shape); STATIC (strategies) | PX-054 |
| AFW-J11 | Quitting or closing the window with runs active asks first and offers to resume them automatically on reopen. On startup interrupted runs are recovered and the counts are shown. Lifecycle hooks run to completion before the process exits or fail closed visibly: in the reference product every session-end hook failed at window close because the host was already torn down. | STATIC (quit protection); DISK (startup recovery, hook failure) | PX-049 |

### K. Cross-cutting

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AFW-K01 | Interaction budgets extend doc 39: a streamed delta renders within 100 ms of its event timestamp at p95; opening a task from the list within 300 ms; the agent list reflects a Core event within 150 ms; shell cold start stays under 2 s. Budgets are measured from Playwright traces and Core timestamps in the packaged app. | UNVERIFIED (Modbit budgets) | PX-041, PX-046, PX-047, PX-045 |
| AFW-K02 | A state gallery renders approvals, generating, error and tray states without a model for design review. It is a QA aid and never proof of a feature: every behaviour is proven against a real Core. | STATIC | PX-044 |
| AFW-K03 | The renderer holds no authority: it never calls a provider, the filesystem, Git, a shell or the browser's CDP, and every effect goes through a typed preload function to the Core (doc 81, REQ-EV-0076). Each new protocol command is added to the thin-client conformance suite and passes it on the CLI and the desktop client. | UNVERIFIED (Modbit invariant) | PX-044, PX-045, PX-054 |
| AFW-K04 | Every new surface passes the axe-core suite with no violations, is operable by keyboard alone, announces attention changes through live regions and shows state with text as well as colour (PX-024). | UNVERIFIED (Modbit invariant) | PX-045, PX-046, PX-047, PX-058 |
| AFW-K05 | No asset, string, icon name, colour, theme file or layout file of the reference product is copied. Icons are an open-licence set with notices, fonts are the system fonts, labels and copy are Modbit's. | STATIC | PX-044 |

## 7. Roadmap phases and rows

| Phase | Theme | Rows |
|---|---|---|
| 1 | Agent-first shell (needs protocol work first) | PX-041 to PX-049 |
| 2 | Input, control and review | PX-050 to PX-062 |
| 3 | Projects and worktrees | PX-063 to PX-068 |

Order inside a phase follows the dependency edges in doc 62: protocol rows block the renderer rows that consume them. All rows are scheduled in M10 and join RELEASE_ZERO only, so ALPHA and BETA readiness is untouched (the DR states this consequence).

### Evidence tier by behavioural risk

Release-critical (real Core, protocol and stores; the proof includes a kill-and-restart and the packaged-app E2E where a client is involved): every protocol addition (the streaming delta event and transcript projection, the terminal stream, queue management, task mode, the skill listing, `SetExecutionPreference`, run modes and rules, context accounting, reversible checkpoints, project records, apply-back, Git hardening, worktree lifecycle) and every renderer row that sends an effect-bearing, destructive or policy-bound command (apps panel, window lifecycle, composer, run controls, model picker, approval stack, checkpoint dialog, worktree and apply-back modal).

Iteration (allowed only because the row modifies none of effect-bearing behaviour, canonical persistence, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics; the packaged UI smoke suite against a real Core applies): tokens and primitives (PX-044), shell layout (PX-045), agent list (PX-046), conversation rendering (PX-047), context ring and trays (PX-060) and the project-aware sidebar (PX-064). Anything mixed or uncertain is release-critical (doc 83). An iteration row that discovers it needs a protocol or main-process change stops and is re-tiered by a Decision Record.

| Row | Title | Owner | Tier | Prerequisites |
|---|---|---|---|---|
| PX-041 | Streaming assistant-text delta events with a completion record and cursor replay | domain-events | release-critical | M1.3, M2.6, DOC-PX-007 |
| PX-042 | Transcript rows and agent header projections served by the Core | domain-events | release-critical | M4.1, PX-041 |
| PX-043 | Terminal output stream and background-terminal registry for every client | terminal | release-critical | M4.5, IMP-EV-0271, DOC-PX-007 |
| PX-044 | Design tokens, UI primitives, tray host and a model-free state gallery | desktop | iteration | M1.4, DOC-PX-007 |
| PX-045 | Agent-first shell: three regions, top bar, apps-panel host, status row, palette and shortcut registry | desktop | iteration | M6.6, PX-024, PX-044 |
| PX-046 | Agent list: status classes, unread, pins, archive with undo, filters, grouping and search | desktop | iteration | PX-042, PX-045 |
| PX-047 | Conversation surface: streaming render, tail status, step folding, scroll, density and failure presentation | desktop | iteration | PX-041, PX-042, PX-045 |
| PX-048 | Typed apps panel: Changes, Terminal, Browser, Files and Evidence with per-task state | desktop | release-critical | M2.9, M7.6, PX-043, PX-045 |
| PX-049 | Window and quit lifecycle: minimum size, quit protection, startup recovery counts and hook completion | desktop | release-critical | M1.5, IMP-EV-0139, PX-045 |
| PX-050 | Durable input queue management and typed interrupt | core-runtime | release-critical | M2.7, IMP-EV-0191, DOC-PX-007 |
| PX-051 | Task mode as a typed Core field with an enforced capability posture | core-runtime | release-critical | M6.3, PX-014, PX-039, DOC-PX-007 |
| PX-052 | ListSkills protocol command for the slash menu | skills | release-critical | M5.5, DOC-PX-007 |
| PX-053 | SetExecutionPreference command wired to the existing routing owner | model-gateway | release-critical | M5.5, EPR-005, M2.6, DOC-PX-007 |
| PX-054 | Composer: placeholders, mode chips, add-context and @ menus, slash menu, attachments, history and drafts | desktop | release-critical | PX-044, PX-045, PX-051, PX-052 |
| PX-055 | Run controls: queue tray, send-behaviour education, stop and edit-in-place, side question, background-terminals tray | desktop | release-critical | PX-047, PX-050, PX-043, PX-054 |
| PX-056 | Model chip and picker with objective profiles, variants and the policy-blocked tray | desktop | release-critical | PX-053, PX-054, PX-055 |
| PX-057 | Run-mode presets and durable allowlist rules owned by the policy kernel | effects-security | release-critical | M2.5, M9.2, DOC-PX-007 |
| PX-058 | Docked approval stack: exact-intent card, deck, shortcuts and persistence | desktop | release-critical | PX-044, PX-045, PX-047, PX-057 |
| PX-059 | Context accounting by category served by the Core | context-engine | release-critical | M3.8, IMP-EV-0131, DOC-PX-007 |
| PX-060 | Context ring, usage summary and the docked usage, limit and offline trays | desktop | iteration | PX-045, PX-059 |
| PX-061 | Per-turn checkpoints with exact reversible restore | durability | release-critical | M4.3, IMP-EV-0012, IMP-EV-0013, DOC-PX-007 |
| PX-062 | Checkpoint restore, redo, edit-in-place and fork in the conversation | desktop | release-critical | PX-047, PX-061 |
| PX-063 | Project records and membership over the WorkGraph | core-runtime | release-critical | M6.1, DOC-PX-007 |
| PX-064 | Project-aware agent list: Projects group, stored views and project grouping | desktop | iteration | PX-046, PX-063 |
| PX-065 | Worktree lifecycle: creation policy, setup commands, scheduled cleanup, lease and retention | workspace-git | release-critical | M2.2, M4.3, IMP-EV-0125, DOC-PX-007 |
| PX-066 | Apply-back of a worktree result to the user's checkout with conflict handling and exact undo | workspace-git | release-critical | M2.2, M9.2, PX-061, PX-065 |
| PX-067 | Hardened Git execution: repository hooks, fsmonitor, attributes and credential redaction | workspace-git | release-critical | M2.2, DOC-PX-007 |
| PX-068 | Worktree management and apply-back conflict modal in the desktop | desktop | release-critical | PX-048, PX-062, PX-065, PX-066 |

## 8. Failure, cancellation, idempotency and restart semantics common to every row

- **Idempotency:** every new command is idempotent by the caller's command id and replays to the same result (doc 29 conformance); a retry with a new id is a new command.
- **Cancellation:** user cancellation applies at safe boundaries and reconciles in-flight effects through the existing cancellation domains before the next step starts; an unknown outcome blocks dependent work until reconciled.
- **Restart:** each protocol row proves a kill and restart of the real Core and, where a client is involved, of the renderer; durable facts come back from the stores, never from the transcript, and presentation never claims progress the Core did not report.
- **Fail closed:** a policy, trust, lease, epoch or intent mismatch refuses the command with a typed reason; no fallback widens authority. A destructive restore, apply or removal needs the confirmation specified in its row and the Core enforces it independently of the renderer.
- **Real effect:** a row's proof uses the real Core, protocol and stores; scripted providers and the state gallery are deterministic aids and never close a feature; the packaged-app E2E applies wherever a renderer is part of the behaviour.

The sibling specifications (docs 66, 67, 68, 69 and 78) apply the same semantics to their rows.

## 9. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| AFW-A01 | PX-045 | LIVE | L14 §P3, L14 §P4, F06 §14.2 |
| AFW-A02 | PX-045, PX-049 | LIVE (default size); UNVERIFIED (minimum) | L14 §2, L12 §2.1, F06 §14.1 |
| AFW-A03 | PX-045 | LIVE (260); STATIC (210 and 400) | L14 §P3, F06 §7.2 |
| AFW-A04 | PX-045 | LIVE | L14 §P3, L14 §3.1 |
| AFW-A05 | PX-046 | LIVE (corrects STATIC 28 pt row) | L14 §P3, L14 §P4 |
| AFW-A06 | PX-045 | LIVE (centre minimum and 597 pt); DISK (ratio is stored as a float); UNVERIFIED (384 pt minimum, default ratio) | L14 §P3, L12 §2.1, F06 §7.2 |
| AFW-A07 | PX-047, PX-054 | LIVE (608, 106, 718, 42); UNVERIFIED (840 cap) | L14 §P3, L14 §P4, F01 §4.1 |
| AFW-A08 | PX-045 | STATIC | F06 §14.2, F06 §7.2 |
| AFW-A09 | PX-045, PX-048 | DISK | L12 §2.1, L13 §3.13 |
| AFW-A10 | PX-045 | LIVE | L14 §3.1, L14 §4 |
| AFW-A11 | PX-044 | LIVE (signed-out instance: the alpha ladder); STATIC (spacing, radii, control sizes, motion) | SYN §8, F06 §13, F06 §2.3, SYN §0 |
| AFW-A12 | PX-044 | STATIC | F06 §3.7, F06 §14.5 |
| AFW-A13 | PX-044 | LIVE (light); UNVERIFIED (dark) | L14 §P3, L12 §2.7 |
| AFW-A14 | PX-044, PX-045 | STATIC | F06 §6, F06 §14.5 |
| AFW-A15 | PX-045 | DISK | L12 §3, F01 §1.6 |
| AFW-A16 | PX-045 | DISK (21 accelerators); UNVERIFIED (full registry) | L12 §3, L14 §P5 |
| AFW-B01 | PX-046 | LIVE | L14 §P3, L14 §P4 |
| AFW-B02 | PX-042, PX-046 | STATIC (precedence order); DISK (the inputs exist as header fields) | F01 §1.4, L12 §2.1, L13 §2.1 |
| AFW-B03 | PX-046, PX-058 | LIVE | L14 §P3.12b |
| AFW-B04 | PX-042, PX-046, PX-047 | LIVE (pill); DISK (header field); STATIC (divider) | L14 §P3.8, L12 §2.6 |
| AFW-B05 | PX-046, PX-064 | DISK (repository default and schema); STATIC (other groupings) | L12 §2.1, F01 §1.4 |
| AFW-B06 | PX-046 | DISK | L12 §2.1, L13 §2.1 |
| AFW-B07 | PX-046 | STATIC | F01 §1.4 |
| AFW-B08 | PX-046 | LIVE (negative observation); STATIC (undo toast, running-task warning) | L14 §P3.11, L14 §P3.7, F01 §1.4 |
| AFW-B09 | PX-046, PX-054 | LIVE (negative observation) | L14 §P3.11 |
| AFW-B10 | PX-042, PX-046 | DISK | L12 §2.6, L13 §3.2 |
| AFW-B11 | PX-045, PX-046 | STATIC | F01 §1.4 |
| AFW-C01 | PX-042, PX-047 | STATIC (row kinds); DISK (render hints) | F01 §4.1, L13 §3.1, L13 §2.1 |
| AFW-C02 | PX-041, PX-047 | LIVE (progressive text, auto-follow); STATIC (monotonic growth) | L14 §P3.8, F01 §4.1 |
| AFW-C03 | PX-047 | LIVE (line and shimmer); DISK (stall ladder 2 s to 32 s) | L14 §P3.4, L14 §P3.9, L12 §2.6 |
| AFW-C04 | PX-042, PX-047 | LIVE (Worked for, Ran N command); DISK (code block preference) | L14 §P3.9, L12 §2.6 |
| AFW-C05 | PX-042, PX-047 | STATIC | F01 §4.2, L12 §2.6 |
| AFW-C06 | PX-047 | LIVE (pill, button); STATIC (jump shortcuts) | L14 §P3.8, F01 §4.1 |
| AFW-C07 | PX-047, PX-044 | LIVE (sticky card, 24.8 pt pitch); STATIC (scale values) | L14 §P3, L14 §P4, F01 §4.1 |
| AFW-C08 | PX-047 | LIVE | L14 §P3, F05 |
| AFW-C09 | PX-047, PX-060 | LIVE (prompt restored, tray, retry on hover); STATIC (offline tray) | L14 §P3.4, F01 §4.6, F01 §4.11 |
| AFW-C10 | PX-047 | STATIC | F01 §4.1 |
| AFW-C11 | PX-047 | UNVERIFIED (design choice) | doc 32 |
| AFW-D01 | PX-054 | LIVE | L14 §P3, L14 §P3.2, L14 §P3.3 |
| AFW-D02 | PX-054 | LIVE | L14 §P3.3 |
| AFW-D03 | PX-054, PX-051 | LIVE (corrects STATIC; the cycle, order, X); UNVERIFIED (wrap-around) | L14 §P3.3, SYN §8, L12 §2.4 |
| AFW-D04 | PX-054, PX-044 | LIVE | L14 §P3.3 |
| AFW-D05 | PX-051 | DISK (mode records separate presentation from behaviour flags) | L12 §2.4, L13 §2.1, F01 §2.1 |
| AFW-D06 | PX-051, PX-055 | STATIC (card, 15 s); DISK (remembered-consent lists) | F01 §2.1, L12 §2.2 |
| AFW-D07 | PX-054 | LIVE (five pre-login categories); DISK (wider type inventory); STATIC (blocked reasons) | SYN §8, L12 §2.5, F01 §3.2 |
| AFW-D08 | PX-052, PX-054 | LIVE (order, divider, detail card); DISK (union, counters, eligibility flag) | L14 §P3.2, L12 §2.5, L13 §3.14 |
| AFW-D09 | PX-054 | STATIC | F01 §3.6 |
| AFW-D10 | PX-054 | STATIC (history keys); LIVE (prompt restored after failure) | F01 §3.1, L14 §P3.4 |
| AFW-D11 | PX-054, PX-055 | LIVE (Enter); UNVERIFIED (modified Enter) | L14 §P3.8, F01 §3.4 |
| AFW-D12 | PX-054, PX-060 | LIVE | L14 §P3.1, L14 §P4, F01 §1.2 |
| AFW-D13 | PX-053, PX-056 | LIVE (picker, Auto row, variants, check, badge); UNVERIFIED (lower part: Max toggle, multi-model count) | L14 §P3, L14 §P4 |
| AFW-D14 | PX-053, PX-056 | DISK | L12 §2.3, L12 §4 |
| AFW-E01 | PX-050, PX-055 | LIVE | L14 §P3.8, L14 §P3.5 |
| AFW-E02 | PX-050, PX-055 | LIVE (header, per-item controls); STATIC (keyboard, drag reorder) | L14 §P3.8, F01 §3.4 |
| AFW-E03 | PX-050 | LIVE (drained in order at turn end); STATIC (per-item overrides) | L14 §P3.8, F01 §3.4 |
| AFW-E04 | PX-055 | LIVE | L14 §P3.8 |
| AFW-E05 | PX-050, PX-055 | STATIC (settings and values); DISK (defaults, no override stored) | F01 §3.4, L12 §2.2, L13 §3.4 |
| AFW-E06 | PX-050, PX-055 | LIVE (moves at once); UNVERIFIED (interrupt of a live stream) | L14 §P3.8, L14 §P3.6 |
| AFW-E07 | PX-050, PX-055 | LIVE (stop in about 1 s, editable message, chips); DISK (two abort outcomes) | L14 §P3.12, L13 §3.1 |
| AFW-E08 | PX-043, PX-055 | LIVE (tray, timer, kill wakes agent); DISK (completion injects a synthetic wake-up) | L14 §P3.12, L13 §3.3, L13 §4 |
| AFW-E09 | PX-055 | STATIC (semantics); LIVE (a Side Chat entry exists in the apps menu) | F01 §1.5, L14 §P3 |
| AFW-F01 | PX-058, PX-057 | LIVE (layout, buttons); UNVERIFIED (additions are Modbit's) | L14 §P3.12b, doc 23 |
| AFW-F02 | PX-058 | LIVE (persists across thread switch) | L14 §P3.12b |
| AFW-F03 | PX-058 | LIVE (corrects DISK inference) | L14 §P3.12b, L13 §3.3, SYN §8 |
| AFW-F04 | PX-058 | LIVE (Enter, Shift+Enter); UNVERIFIED (Escape) | L14 §P3.12b, F01 §4.9 |
| AFW-F05 | PX-058 | UNVERIFIED | L14 §P3.6, F01 §4.9 |
| AFW-F06 | PX-057 | LIVE (the four modes and the account default) | L14 §P3.12b, L12 §2.2, doc 81 |
| AFW-F07 | PX-057, PX-058 | LIVE (dialog for Allowlist); STATIC (Run everything dialog) | L14 §P3.12b, F01 §4.9 |
| AFW-F08 | PX-057 | LIVE (negative observation) | L14 §P3.12, L14 §P3.12b |
| AFW-F09 | PX-057, PX-058 | LIVE (Run executed once, the same card appeared again) | L14 §P3.12b, doc 23 |
| AFW-F10 | PX-057, PX-058 | DISK (ladder sets, compound-command list); UNVERIFIED (on-screen form) | L13 §3.3, L12 §2.2, L14 §P3.6 |
| AFW-F11 | PX-057, PX-058 | DISK (declared escalation); UNVERIFIED (card wording) | L13 §3.3, L14 §P3.12b |
| AFW-F12 | PX-057, PX-058 | DISK | L13 §3.3, L13 §4 |
| AFW-F13 | PX-057, PX-058 | LIVE (negative observation) | L14 §P3.12, L14 §P3.9 |
| AFW-G01 | PX-061 | DISK | L13 §3.5, L12 §2.6 |
| AFW-G02 | PX-062 | LIVE (dialog and wording) | L14 §P3.10 |
| AFW-G03 | PX-061, PX-062 | LIVE (restore behaviour); UNVERIFIED (redo semantics) | L14 §P3.10 |
| AFW-G04 | PX-062 | STATIC; UNVERIFIED (live) | F01 §4.5b, L14 §P3.10 |
| AFW-G05 | PX-048, PX-066, PX-068 | LIVE (edits auto-applied, revert and stage controls); STATIC (Undo all two-step) | L14 §P3.10, F01 §4.8 |
| AFW-G06 | PX-048, PX-062 | LIVE (staged and uncommitted scopes, last turn, commit split button); STATIC (the rest) | L14 §P3.10, F01 §4.8, F02 §5.7 |
| AFW-G07 | PX-047, PX-062 | LIVE (control present); STATIC (semantics) | L14 §P3, F01 §4.5 |
| AFW-G08 | PX-048, PX-055 | LIVE | L14 §P3.10, L14 §P3.12 |
| AFW-H01 | PX-044, PX-055, PX-058 | LIVE (error, steer-education, queue and terminal trays); STATIC (the single-owner rule) | L14 §P3.5, F01 §4.6 |
| AFW-H02 | PX-056, PX-060 | LIVE | L14 §P3.4, L14 §P3.5 |
| AFW-H03 | PX-060 | STATIC | F01 §4.10 |
| AFW-H04 | PX-059, PX-060 | DISK (eight categories summing to the total); LIVE (ring present); UNVERIFIED (tray) | L12 §2.6, L13 §2.1, L14 §P4 |
| AFW-H05 | PX-059 | DISK | L12 §2.6, L13 §3.12, SYN §7.1 |
| AFW-H06 | PX-060 | STATIC | F01 §4.10 |
| AFW-I01 | PX-048 | LIVE (list and plus menu); UNVERIFIED (Evidence, Modbit's addition) | L14 §P3, L14 §P4 |
| AFW-I02 | PX-045, PX-048 | LIVE (rail with app list and counts); STATIC (counter set) | L14 §P3, F01 §1.2 |
| AFW-I03 | PX-043, PX-048 | STATIC; UNVERIFIED (the terminal app itself was not opened live) | F01 §4.3, doc 32 |
| AFW-I04 | PX-043 | STATIC | SYN §3, doc 59 |
| AFW-J01 | PX-063 | DISK (record and map); UNVERIFIED (coordinator behaviour) | L12 §2.6, L13 §2.2, F02 §3.1 |
| AFW-J02 | PX-064 | LIVE | L14 §P3 |
| AFW-J03 | PX-063, PX-064 | STATIC | F02 §3.2 |
| AFW-J04 | PX-063 | STATIC | F02 §3.3, F02 §3.4 |
| AFW-J05 | PX-065 | STATIC | F02 §5.2, F02 §5.3 |
| AFW-J06 | PX-065 | DISK | L13 §2.2, L12 §2.6 |
| AFW-J07 | PX-065 | STATIC; UNVERIFIED (live) | F02 §5.4, L13 §2.2 |
| AFW-J08 | PX-066, PX-068 | STATIC; UNVERIFIED (live) | F02 §5.5 |
| AFW-J09 | PX-067 | STATIC | F02 §5.6 |
| AFW-J10 | PX-054 | DISK (location history shape); STATIC (strategies) | L12 §2.6, F02 §5.6 |
| AFW-J11 | PX-049 | STATIC (quit protection); DISK (startup recovery, hook failure) | F01 §1.3, L13 §2.4, L13 §2.2 |
| AFW-K01 | PX-041, PX-046, PX-047, PX-045 | UNVERIFIED (Modbit budgets) | doc 39 |
| AFW-K02 | PX-044 | STATIC | F01 §4.2, L13 §2.1, doc 80 |
| AFW-K03 | PX-044, PX-045, PX-054 | UNVERIFIED (Modbit invariant) | doc 81, doc 29 |
| AFW-K04 | PX-045, PX-046, PX-047, PX-058 | UNVERIFIED (Modbit invariant) | doc 39 |
| AFW-K05 | PX-044 | STATIC | SYN §0, F06 §14.4 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-041 | AFW-C02, AFW-K01 |
| PX-042 | AFW-B02, AFW-B04, AFW-B10, AFW-C01, AFW-C04, AFW-C05 |
| PX-043 | AFW-E08, AFW-I03, AFW-I04 |
| PX-044 | AFW-A11, AFW-A12, AFW-A13, AFW-A14, AFW-C07, AFW-D04, AFW-H01, AFW-K02, AFW-K03, AFW-K05 |
| PX-045 | AFW-A01, AFW-A02, AFW-A03, AFW-A04, AFW-A06, AFW-A08, AFW-A09, AFW-A10, AFW-A14, AFW-A15, AFW-A16, AFW-B11, AFW-I02, AFW-K01, AFW-K03, AFW-K04 |
| PX-046 | AFW-A05, AFW-B01, AFW-B02, AFW-B03, AFW-B04, AFW-B05, AFW-B06, AFW-B07, AFW-B08, AFW-B09, AFW-B10, AFW-B11, AFW-K01, AFW-K04 |
| PX-047 | AFW-A07, AFW-B04, AFW-C01, AFW-C02, AFW-C03, AFW-C04, AFW-C05, AFW-C06, AFW-C07, AFW-C08, AFW-C09, AFW-C10, AFW-C11, AFW-G07, AFW-K01, AFW-K04 |
| PX-048 | AFW-A09, AFW-G05, AFW-G06, AFW-G08, AFW-I01, AFW-I02, AFW-I03 |
| PX-049 | AFW-A02, AFW-J11 |
| PX-050 | AFW-E01, AFW-E02, AFW-E03, AFW-E05, AFW-E06, AFW-E07 |
| PX-051 | AFW-D03, AFW-D05, AFW-D06 |
| PX-052 | AFW-D08 |
| PX-053 | AFW-D13, AFW-D14 |
| PX-054 | AFW-A07, AFW-B09, AFW-D01, AFW-D02, AFW-D03, AFW-D04, AFW-D07, AFW-D08, AFW-D09, AFW-D10, AFW-D11, AFW-D12, AFW-J10, AFW-K03 |
| PX-055 | AFW-D06, AFW-D11, AFW-E01, AFW-E02, AFW-E04, AFW-E05, AFW-E06, AFW-E07, AFW-E08, AFW-E09, AFW-G08, AFW-H01 |
| PX-056 | AFW-D13, AFW-D14, AFW-H02 |
| PX-057 | AFW-F01, AFW-F06, AFW-F07, AFW-F08, AFW-F09, AFW-F10, AFW-F11, AFW-F12, AFW-F13 |
| PX-058 | AFW-B03, AFW-F01, AFW-F02, AFW-F03, AFW-F04, AFW-F05, AFW-F07, AFW-F09, AFW-F10, AFW-F11, AFW-F12, AFW-F13, AFW-H01, AFW-K04 |
| PX-059 | AFW-H04, AFW-H05 |
| PX-060 | AFW-C09, AFW-D12, AFW-H02, AFW-H03, AFW-H04, AFW-H06 |
| PX-061 | AFW-G01, AFW-G03 |
| PX-062 | AFW-G02, AFW-G03, AFW-G04, AFW-G06, AFW-G07 |
| PX-063 | AFW-J01, AFW-J03, AFW-J04 |
| PX-064 | AFW-B05, AFW-J02, AFW-J03 |
| PX-065 | AFW-J05, AFW-J06, AFW-J07 |
| PX-066 | AFW-G05, AFW-J08 |
| PX-067 | AFW-J09 |
| PX-068 | AFW-G05, AFW-J08 |

## 10. Non-goals and sibling records

Five goals sit beside the agent-first workspace and each has its own Decision Record, specification and rows; they are not specified here. The rest are out of scope without a record.

| Goal or non-goal | Where it is specified, or why it is out |
|---|---|
| Local operating-system (native computer) control and browser control hardening | DR-PX-2026-10-03-008, doc 66, PX-069 to PX-076. |
| A code editor | DR-PX-2026-10-03-009, doc 67, PX-077 to PX-081 (a transactional Workspace Editor; no Monaco, no Code-OSS, no IDE). |
| Automations | DR-PX-2026-10-03-010, doc 68, PX-082 to PX-086 (through the existing Scheduler, policy and approval owners). |
| A plugin marketplace, the Customize surface, hooks, MCP trust | DR-PX-2026-10-03-011, doc 69, PX-087 to PX-093. |
| An Electron change: inset title bar, translucency opt-in, native integration, fuses, update application, version policy | DR-PX-2026-10-03-012, doc 78, PX-094 to PX-098. PX-049 here is limited to constructor size options and quit handling. |
| Anything touching EPR-pinned behaviour: routing, quality floor, gate calibration, outcome statistics, reviewer isolation, best-of-N or multi-model comparison | EPR-000..019, ADR-R-039..056. PX-053 only wires a command. |
| A classifier that approves effects ("auto-review") | Out: a second policy engine (doc 81). |
| Canvas, design mode, visualisation artifacts, voice | Not Modbit product scope in these rows. |
| Online quality telemetry: ratings, feedback prompts, attribution, retention | Roadmap phase 6; not specified. |
| Project pages, notes with a task database, boards, briefs, libraries, project coordinator behaviour | Not evidenced (AFW-J01, J04). |
| Mid-turn migration between local, worktree and cloud and its dialogs | M8.7 handoff is the owner. |
| Billing, plans and upgrade UI | Not Modbit scope; policy blocks are shown with their typed cause only. |
| Team collaboration | REQ-PX-012 and REQ-PX-013 (DEFERRED). |

## 11. Still unverified

Items no method has closed. Each requirement that depends on one carries an UNVERIFIED or qualified tag. The register lists what evidence would close it; closing it never lowers a Modbit bar, it only lets the parity claim be made.

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| U01 | Whether Send now cuts a live stream, and what steer does once enabled | A generation long enough to interact with (about 60 s or a slow tool), Send now pressed mid-stream, with the stream, any tool state and the transcript label captured before and after; then the same with steering enabled in settings. |
| U02 | The allowlist token-prefix ladder on screen and the card wording for a network or permission escalation | Always on a multi-token command to see the ladder rows and what is applied; a command that declares network escalation under a sandboxed mode, with the card text captured and compared with the stored options. |
| U03 | Absence of a reject-with-follow-up box under other modes and card kinds | Skip repeated under every run mode and for an external-tool call, a deletion and a protected-path card, and with Escape; capture whether any text box ever appears. |
| U04 | The context-ring tray (categories, report link, wording) | Click the ring in a running task and capture the tray, its categories and the report, with the stored breakdown for the same turn. |
| U05 | The dark theme of the agents window | Switch theme in a signed-in window and capture the same geometry and colours, then run the contrast test on the measured pairs. |
| U06 | The named-model picker: Max window toggle, multi-model count, per-model parameter menu, section layout | An account or local setup with named models; open the picker and capture its lower part and the parameter menu. |
| U07 | The Team scope group in the customisation editor | A team account; open each tab and capture the scope groups. |
| U08 | The full shortcut registry of the agents window (21 menu accelerators recovered of an estimated 340) | Open the shortcut help and export it, or exercise each binding; compare with the stored menu data. |
| U09 | Project coordinator behaviour, icon, colour and views | Enable a project as an agent where the gate allows and observe whether anything coordinates; or confirm from storage and logs that nothing does. |
| U10 | Escape on the approval card | Press Escape on a pending card and capture the transcript line and the stored decision. |
| U11 | The N-tools-pending deck with parallel gated calls | A prompt that issues two or more gated calls at once; capture ordering, per-call versus batch approval, and what happens to the rest when one is skipped. |
| U12 | Whether Redo checkpoint redoes or discards again | Restore, then Redo, with the file hashes before and after each step. |
| U13 | The Revert files or Keep files dialog when editing an older message | Edit an older message after the files differ from its checkpoint and capture the dialog and the resulting files. |
| U14 | Two-step Undo all and per-hunk keep and undo | Use Undo all on a multi-file change and capture the first and second click states. |
| U15 | Behaviour of the primary-modifier Enter while a turn runs | Press it during a turn under each send setting and capture what is sent and when. |
| U16 | Wrap-around of Shift+Tab after the last mode | One more press after Ask. |
| U17 | Sidebar minimum and maximum, window minimum, composer 840 pt cap, narrow-mode threshold, right-panel minimum and ratio | Resize runs at several window sizes with measured geometry. |
| U18 | Whether archive shows an undo toast and where selection goes | Capture the first 10 s after archiving, including after a slower frame interval. |
| U19 | Worktree retention caps, protection windows, setup-command handling and the apply-back conflict modal | Create real worktrees past the caps, with a setup file and a conflicting checkout edit, and capture the cleanup report and the modal. |
| U20 | The Run everything confirmation dialog and its re-prompt rule | Choose the mode and capture the dialog, its buttons and the persistence of the choice. |
| U21 | Density rendering in the compact, balanced and detailed settings | Switch settings on the same finished turn and capture the three renderings. |
| U22 | Tooltip delays, menu and popup shadows and radii in the agents window | Timed hover captures and pixel measurement of the popups. |
| U23 | Queue drag reorder, per-item edit and the Start multitasking action | Interact with a queue of three items and capture each state. |
| U24 | Conversation font size (14/22 versus 15/24) | A computed-style read of the transcript text; the measured 24.8 pt pitch suggests the larger one. |
| U25 | The terminal, files, browser and changes apps with real content, and the pull-request and CI panels | Open each app on a task that produced content and capture it. |
| U26 | Stall and hang ladder wording and the reconnect states | A forced stall and a dropped connection with the tail status captured at each threshold. |

## 12. Evidence key

The research notes cited in the traceability table are untracked and held by the owner; they are not in this package. Short ids:

| Id | Source |
|---|---|
| SYN | the synthesis report and its four addenda (SYN §8 is the signed-out live addendum; the later addenda cover the signed-in passive and window probes) |
| F01 to F09 | the static teardown reports: F01 agent panel, F02 projects and worktrees, F03 computer use, F04 plugins and skills, F05 eval harness, F06 visual design and layout, F07 Modbit baseline, F08 context engine, F09 Modbit context, graph and router |
| L10, L11 | the isolated signed-out live verification |
| L12 | signed-in profile state read from disk |
| L13 | runtime artifacts: logs, stores, transcripts, lifecycle |
| L14 | signed-in window observations (Part 1 settings page; Part 2 agent view geometry; Part 3 triggered probes P3.1 to P3.12b) |
| doc N | a document of this dossier |
