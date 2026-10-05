# Product extension requirements, tasks and qualifications

Additive ledger authorized by DR-PX-2026-09-05 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`). It never modifies the 291 base rows or the EPR ledger. `tools/dossier_px.py` parses it into the same graph node types as the base and EPR ledgers; it pins no count, so effective totals are whatever the rows sum to, and `tools/check_dossier.py` D9 enforces structure: contiguous IDs from 000, one task card and one `PX-E2E` scenario per ADOPT row, one qualification per row with the same owner, and DEFERRED rows with no task, milestone or prerequisites. Rows are appended by stage; earlier rows are never edited except by a new Decision Record.

Columns: requirement, title, task, qualification, canonical owner, milestone, release (ALPHA, BETA, RELEASE_ZERO or POST_ZERO for DEFERRED rows), disposition, prerequisites that must be COMPLETE first. A task joins the release named in its row and every later release; readiness is derived in `75_PHASED_RELEASE_PLAN_AND_READINESS.md`.

Rows REQ-PX-041..098 are additionally authorized by six Decision Records of 2026-10-03. DR-PX-2026-10-03-007, -008 and -010 were accepted on 2026-10-05 by the owner's instruction (goal: implement research/audit/01-TASK-LIST.md); -009, -011 and -012 stay proposed until the owner ratifies them, and their rows (PX-077..081, PX-087..093, PX-094..098) stay blocked behind DOC-PX-009, DOC-PX-011 and DOC-PX-012 (`decisions/`): DR-PX-2026-10-03-007 (agent-first workspace, PX-041..068, doc 65), -008 (native computer control, PX-069..076, doc 66), -009 (Workspace Editor, PX-077..081, doc 67), -010 (automations, PX-082..086, doc 68), -011 (extension marketplace and Customize, PX-087..093, doc 69) and -012 (desktop shell platform integration, PX-094..098, doc 78). The specification requirements of those documents (each with a verification tag) are the parameter source of the task cards. All rows are scheduled in M10 for RELEASE_ZERO, so ALPHA and BETA readiness is unchanged. Every row with no other PX prerequisite lists the dossier task of its record (DOC-PX-007 to DOC-PX-012) as a prerequisite, so no row can start before its record is accepted and the dossier task is COMPLETE (`graph.py set ... COMPLETE` itself refuses a dossier task whose change record is still proposed). No row of an accepted record depends on a row of a proposed one. Each card declares its evidence tier by behavioural risk (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`).

Rows REQ-PX-099..139 are additionally authorized by DR-PX-2026-10-05-013 (audit-driven capability completion, doc 79), accepted on 2026-10-05 by the owner's instruction (goal: implement research/audit/01-TASK-LIST.md). They cover the build tasks of that list that no earlier PX row fully covers (the traceability table of doc 79 section 7 maps every BLD task to rows), are scheduled in M10 for RELEASE_ZERO, list DOC-PX-013 as a prerequisite, name an existing owner (no subsystem is added), and none depends on a row or dossier task of a proposed record (DR-PX-2026-10-03-009, -011 or -012). They change no EPR row, gate, threshold or algorithm: PX-133 to PX-136 only conform the implementation to ADR-R-051 and ADR-R-055 and measure.

## Ledger

| Requirement | Title | Task | Qualification | Owner | Milestone | Release | Disposition | After |
|---|---|---|---|---|---|---|---|---|
| REQ-PX-000 | Headless CLI thin client for the task lifecycle | PX-000 | QUAL-PX-000 | desktop | M2 | ALPHA | ADOPT | M1.3,M2.7 |
| REQ-PX-001 | Thin-client conformance contract for external development-environment adapters | PX-001 | QUAL-PX-001 | desktop | M2 | ALPHA | ADOPT | PX-000 |
| REQ-PX-002 | VS Code adapter as a conformant thin client | PX-002 | QUAL-PX-002 | desktop | M6 | BETA | ADOPT | PX-001,M6.6 |
| REQ-PX-003 | JetBrains adapter after abstraction conformance | — | QUAL-PX-003 | desktop | — | POST_ZERO | DEFERRED | — |
| REQ-PX-004 | Provenance-bound external diagnostics intake | PX-004 | QUAL-PX-004 | context-engine | M6 | BETA | ADOPT | PX-001,M3.4 |
| REQ-PX-005 | Constrained inline patch through ChangeTransaction | PX-005 | QUAL-PX-005 | workspace-git | M6 | BETA | ADOPT | M2.1,M2.9 |
| REQ-PX-006 | GitHub forge adapter behind the External Tool Hub | PX-006 | QUAL-PX-006 | external-tools | M6 | BETA | ADOPT | M2.5,M6.5 |
| REQ-PX-007 | Pull request create and update from a reviewed result | PX-007 | QUAL-PX-007 | workspace-git | M6 | BETA | ADOPT | PX-006,M2.2 |
| REQ-PX-008 | Review-comment steering as untrusted durable input | PX-008 | QUAL-PX-008 | core-runtime | M9 | RELEASE_ZERO | ADOPT | PX-007,M6.5 |
| REQ-PX-009 | CI-result ingestion as provenance-bearing evidence | PX-009 | QUAL-PX-009 | verification | M9 | RELEASE_ZERO | ADOPT | PX-007,M2.8 |
| REQ-PX-010 | Issue-to-task intake from CLI and desktop | PX-010 | QUAL-PX-010 | desktop | M6 | BETA | ADOPT | PX-006,PX-000 |
| REQ-PX-011 | Forge webhook intake through the Cloud API | PX-011 | QUAL-PX-011 | sandbox-cloud | M8 | RELEASE_ZERO | ADOPT | PX-010,M8.1 |
| REQ-PX-012 | Team collaboration: shared task history and assignment | — | QUAL-PX-012 | desktop | — | POST_ZERO | DEFERRED | — |
| REQ-PX-013 | Team collaboration: comments and chat notifications | — | QUAL-PX-013 | desktop | — | POST_ZERO | DEFERRED | — |
| REQ-PX-014 | Understanding and planning contract | PX-014 | QUAL-PX-014 | core-runtime | M2 | ALPHA | ADOPT | M2.7 |
| REQ-PX-015 | Retrieval-before-edit contract | PX-015 | QUAL-PX-015 | context-engine | M3 | BETA | ADOPT | M3.7,PX-014 |
| REQ-PX-016 | Change strategy contract | PX-016 | QUAL-PX-016 | workspace-git | M2 | ALPHA | ADOPT | M2.1,M2.2,PX-014 |
| REQ-PX-017 | Verification plan derivation contract | PX-017 | QUAL-PX-017 | verification | M2 | ALPHA | ADOPT | M2.8,PX-014 |
| REQ-PX-018 | Bounded evidence-driven repair loop with RepairAttempt records | PX-018 | QUAL-PX-018 | core-runtime | M2 | ALPHA | ADOPT | PX-017 |
| REQ-PX-019 | Self-review and completion contract | PX-019 | QUAL-PX-019 | verification | M2 | ALPHA | ADOPT | PX-018 |
| REQ-PX-020 | Fixed M2 competence baseline on public and internal suites | PX-020 | QUAL-PX-020 | eval-bench | M3 | BETA | ADOPT | M2.9,PX-019,M3.9 |
| REQ-PX-021 | Competence regression gate and targets after baseline | PX-021 | QUAL-PX-021 | eval-bench | M10 | RELEASE_ZERO | ADOPT | PX-020,M10.4 |
| REQ-PX-022 | Onboarding to a first useful task within five minutes | PX-022 | QUAL-PX-022 | desktop | M2 | ALPHA | ADOPT | M1.4,M2.9 |
| REQ-PX-023 | Screen flow and state completeness with notification model | PX-023 | QUAL-PX-023 | desktop | M6 | BETA | ADOPT | M6.6,PX-022 |
| REQ-PX-024 | Keyboard model and accessibility conformance | PX-024 | QUAL-PX-024 | desktop | M6 | BETA | ADOPT | M6.6 |
| REQ-PX-025 | Interaction budgets enforced in packaged E2E | PX-025 | QUAL-PX-025 | desktop | M10 | RELEASE_ZERO | ADOPT | M10.4,PX-023 |
| REQ-PX-026 | Alpha language baseline for TypeScript/JavaScript, Python and Rust | PX-026 | QUAL-PX-026 | verification | M2 | ALPHA | ADOPT | M2.8 |
| REQ-PX-027 | Language tier conformance suites A, B and C | PX-027 | QUAL-PX-027 | verification | M3 | BETA | ADOPT | M3.3,M3.4,PX-026 |
| REQ-PX-028 | Tier A conformance for TypeScript/JavaScript, Python and Rust | PX-028 | QUAL-PX-028 | context-engine | M3 | BETA | ADOPT | PX-027 |
| REQ-PX-029 | Explicit degradation path for Tier C and Unsupported languages | PX-029 | QUAL-PX-029 | context-engine | M3 | BETA | ADOPT | PX-027 |
| REQ-PX-030 | Platform CI compatibility matrix from M0, never release-grade by itself | PX-030 | QUAL-PX-030 | governance | M10 | RELEASE_ZERO | ADOPT | M0.1 |
| REQ-PX-031 | Desktop platform release promotion by platform-specific E2E | PX-031 | QUAL-PX-031 | desktop | M10 | RELEASE_ZERO | ADOPT | M10.3,PX-030 |
| REQ-PX-032 | Pre-change verification baseline and regression attribution | PX-032 | QUAL-PX-032 | verification | M2 | ALPHA | ADOPT | M2.8,PX-017 |
| REQ-PX-033 | Normalized test reports and failing-check identity | PX-033 | QUAL-PX-033 | verification | M2 | ALPHA | ADOPT | M2.8,IMP-EV-0107 |
| REQ-PX-034 | Staged test targeting and mandatory completion run | PX-034 | QUAL-PX-034 | verification | M2 | ALPHA | ADOPT | PX-032,PX-033 |
| REQ-PX-035 | Impact-based test selection from the evidence graph | PX-035 | QUAL-PX-035 | context-engine | M3 | BETA | ADOPT | M3.6,PX-034 |
| REQ-PX-036 | Flaky-check detection, rerun protocol and quarantine | PX-036 | QUAL-PX-036 | verification | M2 | ALPHA | ADOPT | PX-033 |
| REQ-PX-037 | Diff invariants including test-integrity detection | PX-037 | QUAL-PX-037 | workspace-git | M2 | ALPHA | ADOPT | M2.1,M2.2,PX-016 |
| REQ-PX-038 | Scope policy with bounded expansion and mandatory questions | PX-038 | QUAL-PX-038 | core-runtime | M2 | ALPHA | ADOPT | PX-014,PX-016 |
| REQ-PX-039 | Repair policy defaults, reproduction-first and no-progress detection | PX-039 | QUAL-PX-039 | core-runtime | M2 | ALPHA | ADOPT | PX-018,PX-033 |
| REQ-PX-040 | Agent harness contracts | PX-040 | QUAL-PX-040 | core-runtime | M2 | ALPHA | ADOPT | M2.7,PX-039 |
| REQ-PX-041 | Streaming assistant-text delta events with a completion record and cursor replay | PX-041 | QUAL-PX-041 | domain-events | M10 | RELEASE_ZERO | ADOPT | M1.3,M2.6,DOC-PX-007 |
| REQ-PX-042 | Transcript rows and agent header projections served by the Core | PX-042 | QUAL-PX-042 | domain-events | M10 | RELEASE_ZERO | ADOPT | M4.1,PX-041 |
| REQ-PX-043 | Terminal output stream and background-terminal registry for every client | PX-043 | QUAL-PX-043 | terminal | M10 | RELEASE_ZERO | ADOPT | M4.5,IMP-EV-0271,DOC-PX-007 |
| REQ-PX-044 | Design tokens, UI primitives, tray host and a model-free state gallery | PX-044 | QUAL-PX-044 | desktop | M10 | RELEASE_ZERO | ADOPT | M1.4,DOC-PX-007 |
| REQ-PX-045 | Agent-first shell: three regions, top bar, apps-panel host, status row, palette and shortcut registry | PX-045 | QUAL-PX-045 | desktop | M10 | RELEASE_ZERO | ADOPT | M6.6,PX-024,PX-044 |
| REQ-PX-046 | Agent list: status classes, unread, pins, archive with undo, filters, grouping and search | PX-046 | QUAL-PX-046 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-042,PX-045 |
| REQ-PX-047 | Conversation surface: streaming render, tail status, step folding, scroll, density and failure presentation | PX-047 | QUAL-PX-047 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-041,PX-042,PX-045 |
| REQ-PX-048 | Typed apps panel: Changes, Terminal, Browser, Files and Evidence with per-task state | PX-048 | QUAL-PX-048 | desktop | M10 | RELEASE_ZERO | ADOPT | M2.9,M7.6,PX-043,PX-045 |
| REQ-PX-049 | Window and quit lifecycle: minimum size, quit protection, startup recovery counts and hook completion | PX-049 | QUAL-PX-049 | desktop | M10 | RELEASE_ZERO | ADOPT | M1.5,IMP-EV-0139,PX-045 |
| REQ-PX-050 | Durable input queue management and typed interrupt | PX-050 | QUAL-PX-050 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M2.7,IMP-EV-0191,DOC-PX-007 |
| REQ-PX-051 | Task mode as a typed Core field with an enforced capability posture | PX-051 | QUAL-PX-051 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M6.3,PX-014,PX-039,DOC-PX-007 |
| REQ-PX-052 | ListSkills protocol command for the slash menu | PX-052 | QUAL-PX-052 | skills | M10 | RELEASE_ZERO | ADOPT | M5.5,DOC-PX-007 |
| REQ-PX-053 | SetExecutionPreference command wired to the existing routing owner | PX-053 | QUAL-PX-053 | model-gateway | M10 | RELEASE_ZERO | ADOPT | M5.5,EPR-005,M2.6,DOC-PX-007 |
| REQ-PX-054 | Composer: placeholders, mode chips, add-context and @ menus, slash menu, attachments, history and drafts | PX-054 | QUAL-PX-054 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-045,PX-051,PX-052 |
| REQ-PX-055 | Run controls: queue tray, send-behaviour education, stop and edit-in-place, side question, background-terminals tray | PX-055 | QUAL-PX-055 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-047,PX-050,PX-043,PX-054 |
| REQ-PX-056 | Model chip and picker with objective profiles, variants and the policy-blocked tray | PX-056 | QUAL-PX-056 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-053,PX-054,PX-055 |
| REQ-PX-057 | Run-mode presets and durable allowlist rules owned by the policy kernel | PX-057 | QUAL-PX-057 | effects-security | M10 | RELEASE_ZERO | ADOPT | M2.5,M9.2,DOC-PX-007 |
| REQ-PX-058 | Docked approval stack: exact-intent card, deck, shortcuts and persistence | PX-058 | QUAL-PX-058 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-045,PX-047,PX-057 |
| REQ-PX-059 | Context accounting by category served by the Core | PX-059 | QUAL-PX-059 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.8,IMP-EV-0131,DOC-PX-007 |
| REQ-PX-060 | Context ring, usage summary and the docked usage, limit and offline trays | PX-060 | QUAL-PX-060 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-045,PX-059 |
| REQ-PX-061 | Per-turn checkpoints with exact reversible restore | PX-061 | QUAL-PX-061 | durability | M10 | RELEASE_ZERO | ADOPT | M4.3,IMP-EV-0012,IMP-EV-0013,DOC-PX-007 |
| REQ-PX-062 | Checkpoint restore, redo, edit-in-place and fork in the conversation | PX-062 | QUAL-PX-062 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-047,PX-061 |
| REQ-PX-063 | Project records and membership over the WorkGraph | PX-063 | QUAL-PX-063 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M6.1,DOC-PX-007 |
| REQ-PX-064 | Project-aware agent list: Projects group, stored views and project grouping | PX-064 | QUAL-PX-064 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-046,PX-063 |
| REQ-PX-065 | Worktree lifecycle: creation policy, setup commands, scheduled cleanup, lease and retention | PX-065 | QUAL-PX-065 | workspace-git | M10 | RELEASE_ZERO | ADOPT | M2.2,M4.3,IMP-EV-0125,DOC-PX-007 |
| REQ-PX-066 | Apply-back of a worktree result to the user's checkout with conflict handling and exact undo | PX-066 | QUAL-PX-066 | workspace-git | M10 | RELEASE_ZERO | ADOPT | M2.2,M9.2,PX-061,PX-065 |
| REQ-PX-067 | Hardened Git execution: repository hooks, fsmonitor, attributes and credential redaction | PX-067 | QUAL-PX-067 | workspace-git | M10 | RELEASE_ZERO | ADOPT | M2.2,DOC-PX-007 |
| REQ-PX-068 | Worktree management and apply-back conflict modal in the desktop | PX-068 | QUAL-PX-068 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-048,PX-062,PX-065,PX-066 |
| REQ-PX-069 | Computer Runtime contract: typed computer tools, scopes, handles, refusal taxonomy and unknown-outcome latch | PX-069 | QUAL-PX-069 | browser | M10 | RELEASE_ZERO | ADOPT | M7.6,M5.1,DOC-PX-008 |
| REQ-PX-070 | Per-call exact-intent approvals for native control, never allowlistable | PX-070 | QUAL-PX-070 | effects-security | M10 | RELEASE_ZERO | ADOPT | M2.5,M9.2,PX-057,DOC-PX-008 |
| REQ-PX-071 | macOS actuator helper: separate signed process, authenticated local RPC, accessibility-first background control, takeover lease and stop | PX-071 | QUAL-PX-071 | browser | M10 | RELEASE_ZERO | ADOPT | PX-069,M7.6,M9.5,DOC-PX-008 |
| REQ-PX-072 | Actuator supply chain: bundled signed helper, hash-pinned manifest, rollback and process identity | PX-072 | QUAL-PX-072 | browser | M10 | RELEASE_ZERO | ADOPT | PX-071,M10.2,DOC-PX-008 |
| REQ-PX-073 | Browser control hardening: CDP deny list, per-call origin gate, certificate trust, permissions and per-task view ownership | PX-073 | QUAL-PX-073 | browser | M10 | RELEASE_ZERO | ADOPT | M7.6,M7.7,DOC-PX-008 |
| REQ-PX-074 | Computer-control desktop surface: permission window, presence and Stop, action cards and approval card | PX-074 | QUAL-PX-074 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-058,PX-069,PX-070,PX-071,DOC-PX-008 |
| REQ-PX-075 | Computer-use subagent profile with an isolated target, constrained tools and stall rules | PX-075 | QUAL-PX-075 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M6.3,PX-039,PX-051,PX-069,DOC-PX-008 |
| REQ-PX-076 | Screenshot and accessibility artifacts: window capture, masking, content-aware codec, retention and audit | PX-076 | QUAL-PX-076 | media | M10 | RELEASE_ZERO | ADOPT | PX-069,M2.10,DOC-PX-008 |
| REQ-PX-077 | Editor file service: open with revision and encoding facts, multi-file save as one ChangeTransaction, merge basis and file revision events | PX-077 | QUAL-PX-077 | workspace-git | M10 | RELEASE_ZERO | ADOPT | PX-005,M2.1,M2.9,DOC-PX-009 |
| REQ-PX-078 | Editor language intelligence: draft analysis overlay, outline, definitions and references served by the Core | PX-078 | QUAL-PX-078 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.3,M3.4,M3.6,PX-077,DOC-PX-009 |
| REQ-PX-079 | Workspace Editor surface: text engine, drafts, stale-draft merge, find and replace, tabs, accessibility and large-file behaviour | PX-079 | QUAL-PX-079 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-048,PX-077,PX-078 |
| REQ-PX-080 | Agent-aware editing: selection context, quick edit, inline per-hunk review and agent-write notices | PX-080 | QUAL-PX-080 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-079,PX-055,M2.9,DOC-PX-009 |
| REQ-PX-081 | Editor text-safety and conformance suite: encodings, round trips, fuzzed save path, input method and screen reader, latency | PX-081 | QUAL-PX-081 | verification | M10 | RELEASE_ZERO | ADOPT | PX-077,PX-079,DOC-PX-009 |
| REQ-PX-082 | Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions | PX-082 | QUAL-PX-082 | automation | M10 | RELEASE_ZERO | ADOPT | M1.2,M2.5,DOC-PX-010 |
| REQ-PX-083 | Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets | PX-083 | QUAL-PX-083 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M6.2,PX-082,DOC-PX-010 |
| REQ-PX-084 | Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches | PX-084 | QUAL-PX-084 | effects-security | M10 | RELEASE_ZERO | ADOPT | M2.5,M6.7,PX-040,PX-082,DOC-PX-010 |
| REQ-PX-085 | Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source | PX-085 | QUAL-PX-085 | sandbox-cloud | M10 | RELEASE_ZERO | ADOPT | PX-011,M8.1,PX-082,PX-083,DOC-PX-010 |
| REQ-PX-086 | Automations surface and CLI: list, editor with validation, enable approval, test run, run history and kill switches | PX-086 | QUAL-PX-086 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-045,PX-046,PX-058,PX-082,PX-083,PX-084 |
| REQ-PX-087 | Plugin package format extension and signed catalog manifest with strict parsing and containment | PX-087 | QUAL-PX-087 | extensions-hooks | M10 | RELEASE_ZERO | ADOPT | IMP-EV-0138,IMP-EV-0225,M5.5,DOC-PX-011 |
| REQ-PX-088 | Install, update and rollback pipeline with trust disclosure, digest-pinned fetch and hardened extraction | PX-088 | QUAL-PX-088 | extensions-hooks | M10 | RELEASE_ZERO | ADOPT | PX-087,M9.4,DOC-PX-011 |
| REQ-PX-089 | Plugin variables, secrets and hostile third-party content handling | PX-089 | QUAL-PX-089 | extensions-hooks | M10 | RELEASE_ZERO | ADOPT | PX-087,M9.3,M7.7,DOC-PX-011 |
| REQ-PX-090 | External tool server trust by configuration hash, annotation classes, OAuth and connection state | PX-090 | QUAL-PX-090 | external-tools | M10 | RELEASE_ZERO | ADOPT | M9.4,PX-089,DOC-PX-011 |
| REQ-PX-091 | Agent self-extension write-protection map and organisation plugin and server policy | PX-091 | QUAL-PX-091 | effects-security | M10 | RELEASE_ZERO | ADOPT | M9.3,PX-088,DOC-PX-011 |
| REQ-PX-092 | Hook contract completion: merge order, exit-code blocking, fail-closed permission events, trusted-workspace gating and execution log | PX-092 | QUAL-PX-092 | extensions-hooks | M10 | RELEASE_ZERO | ADOPT | IMP-EV-0139,IMP-EV-0239,PX-049,DOC-PX-011 |
| REQ-PX-093 | Customize surface: seven tabs, scopes, view options, catalog browsing, trust disclosure, update and the hook log | PX-093 | QUAL-PX-093 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-045,PX-052,PX-087,PX-088,PX-090,PX-092 |
| REQ-PX-094 | Window chrome: inset title bar, drag regions, window-control placement, optional translucency and persisted window state | PX-094 | QUAL-PX-094 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-049,PX-045,DOC-PX-012 |
| REQ-PX-095 | Native menus, badge, tray icon, deep links with confirmation, single instance and native dialogs | PX-095 | QUAL-PX-095 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-088,PX-090,PX-094,PX-045,DOC-PX-012 |
| REQ-PX-096 | Main-process hardening: fuses, ASAR integrity, permission and navigation denials, IPC inventory and main-thread responsiveness | PX-096 | QUAL-PX-096 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-049,M10.2,DOC-PX-012 |
| REQ-PX-097 | Update application flow: apply at idle, staged and verified, with rollback and truthful states | PX-097 | QUAL-PX-097 | desktop | M10 | RELEASE_ZERO | ADOPT | M10.2,PX-049,PX-096,DOC-PX-012 |
| REQ-PX-098 | Electron and Chromium version policy and upgrade qualification gate | PX-098 | QUAL-PX-098 | governance | M10 | RELEASE_ZERO | ADOPT | PX-096,PX-073,DOC-PX-012 |
| REQ-PX-099 | Terminal flow control (ACK, resize, bounded ring) and agent-facing shell.input and shell.attach tools | PX-099 | QUAL-PX-099 | terminal | M10 | RELEASE_ZERO | ADOPT | PX-043,M2.3,M4.5,DOC-PX-013 |
| REQ-PX-100 | CLI and IDE-adapter controls for task mode and execution preference | PX-100 | QUAL-PX-100 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-051,PX-053,PX-002,DOC-PX-013 |
| REQ-PX-101 | PauseTask and ResumeTask over the runtime park, with local persistence | PX-101 | QUAL-PX-101 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M2.7,M6.7,M4.4,DOC-PX-013 |
| REQ-PX-102 | Named checkpoints, retention and garbage collection | PX-102 | QUAL-PX-102 | durability | M10 | RELEASE_ZERO | ADOPT | PX-061,M4.3,DOC-PX-013 |
| REQ-PX-103 | Hunk attribution to the originating tool call and per-hunk decisions that keep the run going | PX-103 | QUAL-PX-103 | workspace-git | M10 | RELEASE_ZERO | ADOPT | M2.9,M4.3,PX-061,DOC-PX-013 |
| REQ-PX-104 | Review reachable from any task state, with live per-hunk controls and the originating-call chip | PX-104 | QUAL-PX-104 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-048,PX-103,PX-062,DOC-PX-013 |
| REQ-PX-105 | Skills a user can use: trust by command, a System scope, an index under a budget, skill.load and path gating | PX-105 | QUAL-PX-105 | skills | M10 | RELEASE_ZERO | ADOPT | M5.5,PX-052,DOC-PX-013 |
| REQ-PX-106 | Customize view over the existing skills, hooks and extensions (no catalog) | PX-106 | QUAL-PX-106 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-044,PX-045,PX-052,PX-105,DOC-PX-013 |
| REQ-PX-107 | AGENTS.md and CLAUDE.md as a trust-gated native rules layer | PX-107 | QUAL-PX-107 | skills | M10 | RELEASE_ZERO | ADOPT | IMP-EV-0129,IMP-EV-0059,DOC-PX-013 |
| REQ-PX-108 | Goal-seeded pre-turn context pack | PX-108 | QUAL-PX-108 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.7,M3.8,PX-015,DOC-PX-013 |
| REQ-PX-109 | Structured compaction summary with extractive fail-closed fallback, model-aware thresholds and a searchable transcript pointer | PX-109 | QUAL-PX-109 | durability | M10 | RELEASE_ZERO | ADOPT | M4.2,IMP-EV-0130,M2.6,DOC-PX-013 |
| REQ-PX-110 | Code graph: symbol reference and call edges, implementors and non-test impact selection | PX-110 | QUAL-PX-110 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.3,M3.6,IMP-EV-0157,DOC-PX-013 |
| REQ-PX-111 | Persisted incremental indexes and an indexed grep path | PX-111 | QUAL-PX-111 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.1,M3.2,M3.5,IMP-EV-0172,DOC-PX-013 |
| REQ-PX-112 | Learned embedder and rerank option behind dependency admission | PX-112 | QUAL-PX-112 | context-engine | M10 | RELEASE_ZERO | ADOPT | M3.5,PX-111,DOC-PX-013 |
| REQ-PX-113 | Engineering memory wired into the prompt and the event log, with Agent and Space scopes | PX-113 | QUAL-PX-113 | memory | M10 | RELEASE_ZERO | ADOPT | M9.1,IMP-EV-0162,IMP-EV-0129,DOC-PX-013 |
| REQ-PX-114 | Exec-only tool projection mode, a schema-bytes budget and a live paired trial | PX-114 | QUAL-PX-114 | procedural-runtime | M10 | RELEASE_ZERO | ADOPT | M5.1,M5.4,M5.6,IMP-EV-0116,DOC-PX-013 |
| REQ-PX-115 | Lazy MCP discovery, list_changed, idle reaper and a large-catalog benchmark | PX-115 | QUAL-PX-115 | external-tools | M10 | RELEASE_ZERO | ADOPT | M9.4,IMP-EV-0128,IMP-EV-0177,DOC-PX-013 |
| REQ-PX-116 | Hierarchical cost and wall-clock budgets, max_children and read-scope enforcement | PX-116 | QUAL-PX-116 | core-runtime | M10 | RELEASE_ZERO | ADOPT | M6.2,M6.3,IMP-EV-0048,IMP-EV-0032,DOC-PX-013 |
| REQ-PX-117 | More hook points, kernel-gated hook context injection and prompt-type hooks | PX-117 | QUAL-PX-117 | extensions-hooks | M10 | RELEASE_ZERO | ADOPT | IMP-EV-0139,IMP-EV-0239,IMP-EV-0042,DOC-PX-013 |
| REQ-PX-118 | Worktree isolation as a typed task option: the run executes in its own worktree | PX-118 | QUAL-PX-118 | workspace-git | M10 | RELEASE_ZERO | ADOPT | PX-065,PX-067,IMP-EV-0145,DOC-PX-013 |
| REQ-PX-119 | Agent merge integration: git.merge tools, persisted merge state, post-merge verification and parent completion gated on integrated children | PX-119 | QUAL-PX-119 | workspace-git | M10 | RELEASE_ZERO | ADOPT | PX-118,PX-066,M6.5,IMP-EV-0067,DOC-PX-013 |
| REQ-PX-120 | Browser navigation target policy: loopback, private ranges, link-local, metadata endpoints, redirects and rebinding | PX-120 | QUAL-PX-120 | browser | M10 | RELEASE_ZERO | ADOPT | PX-073,M7.1,M7.7,DOC-PX-013 |
| REQ-PX-121 | Browser feedback primitives: viewport capture, console, network, scroll, wait, an escalation field and the unknown-outcome latch | PX-121 | QUAL-PX-121 | browser | M10 | RELEASE_ZERO | ADOPT | PX-073,PX-069,M7.4,M7.5,DOC-PX-013 |
| REQ-PX-122 | Page-state deltas from a host mutation observer, since_fingerprint, multi-frame accessibility trees and stable identities across restart | PX-122 | QUAL-PX-122 | browser | M10 | RELEASE_ZERO | ADOPT | M7.2,M7.3,PX-121,DOC-PX-013 |
| REQ-PX-123 | Semantic compiler: page classification, forms and fill_form, intent filter, derived actions and stronger action risk classification | PX-123 | QUAL-PX-123 | browser | M10 | RELEASE_ZERO | ADOPT | PX-122,M7.4,IMP-EV-0088,DOC-PX-013 |
| REQ-PX-124 | WebMCP rung: page-declared tools with origin binding and trust labels | PX-124 | QUAL-PX-124 | browser | M10 | RELEASE_ZERO | ADOPT | PX-073,PX-120,M9.4,IMP-EV-0281,DOC-PX-013 |
| REQ-PX-125 | GitHub pull request read, diff and status comments through the forge adapter | PX-125 | QUAL-PX-125 | external-tools | M10 | RELEASE_ZERO | ADOPT | PX-006,PX-007,M9.4,DOC-PX-013 |
| REQ-PX-126 | Check-suite and comment webhooks into CI-result and review-comment ingestion | PX-126 | QUAL-PX-126 | sandbox-cloud | M10 | RELEASE_ZERO | ADOPT | PX-011,PX-008,PX-009,PX-125,DOC-PX-013 |
| REQ-PX-127 | ci_evidence and review comments in Review and the CLI, and the first real-GitHub issue-to-task-to-PR-to-CI proof | PX-127 | QUAL-PX-127 | desktop | M10 | RELEASE_ZERO | ADOPT | PX-125,PX-126,PX-048,PX-010,DOC-PX-013 |
| REQ-PX-128 | Cloud handoff, watch and approve verbs in the CLI and the desktop | PX-128 | QUAL-PX-128 | desktop | M10 | RELEASE_ZERO | ADOPT | M8.7,M8.1,PX-042,PX-045,DOC-PX-013 |
| REQ-PX-129 | OIDC authorization-code with PKCE sign-in, tenant provisioning and the signed policy-bundle publisher route | PX-129 | QUAL-PX-129 | sandbox-cloud | M10 | RELEASE_ZERO | ADOPT | M8.1,M8.2,DOC-PX-013 |
| REQ-PX-130 | Credential broker unification: one broker interface and handle path for tools, providers, MCP, browser and cloud | PX-130 | QUAL-PX-130 | effects-security | M10 | RELEASE_ZERO | ADOPT | M8.6,M7.8,M9.3,IMP-EV-0288,DOC-PX-013 |
| REQ-PX-131 | AuthorizationEpoch and CapabilitySnapshot: the capability view frozen per model round | PX-131 | QUAL-PX-131 | effects-security | M10 | RELEASE_ZERO | ADOPT | M2.5,M9.2,IMP-EV-0041,DOC-PX-013 |
| REQ-PX-132 | Process intelligence: port and dev-server detection, readiness and service health reported by the Core | PX-132 | QUAL-PX-132 | terminal | M10 | RELEASE_ZERO | ADOPT | PX-043,M2.3,M4.5,DOC-PX-013 |
| REQ-PX-133 | Escalation and reviewer leg samples with priced cost in Outcome Statistics | PX-133 | QUAL-PX-133 | eval-bench | M10 | RELEASE_ZERO | ADOPT | EPR-010,EPR-015,EPR-011,DOC-PX-013 |
| REQ-PX-134 | Model registry sign, verify and activate verbs on the CLI | PX-134 | QUAL-PX-134 | model-gateway | M10 | RELEASE_ZERO | ADOPT | EPR-002,PX-053,DOC-PX-013 |
| REQ-PX-135 | Widened gate calibration corpora across languages with generated adversarial checks | PX-135 | QUAL-PX-135 | eval-bench | M10 | RELEASE_ZERO | ADOPT | EPR-019,EPR-017,PX-028,DOC-PX-013 |
| REQ-PX-136 | One live paired benchmark of DIRECT, CASCADE and CRITIQUE on first-party providers | PX-136 | QUAL-PX-136 | eval-bench | M10 | RELEASE_ZERO | ADOPT | PX-133,PX-134,PX-053,EPR-012,DOC-PX-013 |
| REQ-PX-137 | Retrieval benchmark with an external baseline profile, fifty or more cases, a larger public repository and live same-model measurement | PX-137 | QUAL-PX-137 | eval-bench | M10 | RELEASE_ZERO | ADOPT | M3.9,IMP-EV-0250,IMP-EV-0251,DOC-PX-013 |
| REQ-PX-138 | Compaction evaluation that is not self-referential | PX-138 | QUAL-PX-138 | eval-bench | M10 | RELEASE_ZERO | ADOPT | M4.2,PX-109,IMP-EV-0274,DOC-PX-013 |
| REQ-PX-139 | OpenTelemetry export, child-cost rollup and persisted health | PX-139 | QUAL-PX-139 | observability | M10 | RELEASE_ZERO | ADOPT | M10.1,PX-116,IMP-EV-0032,DOC-PX-013 |

## Qualifications

| Qualification | Requirement | Owner | Real qualification | Failure and negative proof |
|---|---|---|---|---|
| QUAL-PX-000 | REQ-PX-000 | desktop | Real Core plus the CLI process on a fixture repository: create a task, stream events by cursor as JSON lines, answer a question, approve one protected effect, kill and restart Core, resume by cursor; exit codes match the documented contract and exactly one effect receipt exists | Kill the CLI mid-stream and reconnect: no duplicate command, no duplicate effect. A forged command without the boot secret is rejected. The CLI binary contains no provider, filesystem, Git or policy code path: static dependency check and runtime tracing both prove every effect went through Core |
| QUAL-PX-001 | REQ-PX-001 | desktop | Conformance suite run against the CLI and the desktop protocol client on a real Core: command idempotency, cursor replay after disconnect, intent-hash-bound approval, attention reasons and acceptance verdict rendering, Core rejection paths; static dependency scan proves no provider, filesystem, Git or policy code in the client | A client that retries a command with a new id, replays without a cursor, or links a Git or provider library fails the suite and is not exposed |
| QUAL-PX-002 | REQ-PX-002 | desktop | Real VS Code extension host loading the adapter against a real local Core: create, steer, approve, review a fixture task from the editor; diagnostics forwarded with revision; restart the editor mid-task and resume by cursor | Adapter attempting to write the workspace, run a tool or read a secret has no code path and the Core rejects any forged command; a revision-mismatched diagnostics batch is discarded with an event |
| QUAL-PX-003 | REQ-PX-003 | desktop | Entry condition, not a current proof: the shared adapter library passes QUAL-PX-001 and QUAL-PX-002 on VS Code, then a JetBrains host passes the identical suite before any promotion Decision Record | Deferred; any JetBrains code before the entry condition is a release blocker under doc 73 |
| QUAL-PX-004 | REQ-PX-004 | context-engine | Real adapter submits language-service diagnostics for a fixture revision; Core normalizes them with provenance external_ide, they appear in Context Pack provenance and verification plan inputs, and a mandatory verification step still executes Modbit's own check | Submission for a stale revision is discarded; a diagnostics batch cannot mark any verification step passed; malformed batches are rejected before persistence |
| QUAL-PX-005 | REQ-PX-005 | workspace-git | From the review surface and from the CLI apply a one-hunk user edit to a real worktree through ChangeTransaction: revision precondition checked, provenance user_direct_edit recorded, workspace revision advanced, stale CodeReferences invalidated | Edit against a stale revision is refused; edit to a protected path is denied after symlink resolution; no client keeps an unsaved buffer |
| QUAL-PX-006 | REQ-PX-006 | external-tools | Real GitHub test repository through the forge adapter: read an issue, create and update a PR, read review comments and check-run status, each with capability lease, effect class, receipt and idempotency key; token supplied by the secret broker only | Call without lease or with a token in arguments is rejected; retried create with the same idempotency key yields one PR; egress to any other host is denied |
| QUAL-PX-007 | REQ-PX-007 | workspace-git | Reviewed fixture result opens a PR on a dedicated branch after approval; PR body carries the evidence summary; a later revision updates the same PR with a new receipt | Stale candidate revision cannot be pushed; denied approval leaves no branch on the remote; crash after push before receipt reconciles to exactly one PR |
| QUAL-PX-008 | REQ-PX-008 | core-runtime | Review comment from an allowed identity on the fixture PR becomes a TaskSteered event with provenance forge_review_comment and untrusted tagging, and the agent acts on it | Comment from a disallowed identity is recorded and ignored; comment text asking to approve an effect or widen capability changes nothing; injection suite passes |
| QUAL-PX-009 | REQ-PX-009 | verification | Real check-run results for the task branch are ingested as evidence artifacts with provider, run id, commit and OutputRef logs and appear in Review with provenance ci | A green CI run cannot mark a qualification PASS; a qualification naming a real test still runs in Modbit; mismatched commit is rejected |
| QUAL-PX-010 | REQ-PX-010 | desktop | From the CLI and the desktop New Task screen create a task from a real issue URL; issue text enters as untrusted context with provenance forge_issue and the task runs the ordinary loop | Issue text containing instructions cannot change policy or capabilities; unreadable issue yields a clear error and no task |
| QUAL-PX-011 | REQ-PX-011 | sandbox-cloud | Real GitHub App webhook to the staging Cloud API creates the same canonical task for the tenant after signature verification and policy; the desktop sees it by cursor | Unsigned or replayed webhook is rejected and audited; cross-tenant repository mapping is denied; no second task model exists |
| QUAL-PX-012 | REQ-PX-012 | desktop | Deferred proof: shared history and assignment appear as cloud projections readable by multiple authenticated clients with tenant isolation | Deferred; not on the Release Zero path |
| QUAL-PX-013 | REQ-PX-013 | desktop | Deferred proof: comments and notifications are durable events with provenance and never authority | Deferred; not on the Release Zero path |
| QUAL-PX-014 | REQ-PX-014 | core-runtime | Real fixture task: the agent records a plan through plan.update before the first write, asks exactly one typed question on an ambiguous fixture and none on an unambiguous one, and plan revisions appear as events in the timeline | A write before any plan is rejected by Core; a question that merely confirms verifiable repository facts is flagged by the internal suite |
| QUAL-PX-015 | REQ-PX-015 | context-engine | Real fixture task: every edited file has a retrieval record at the current workspace revision and the Context Ledger records use; symbol edits show definition and reference retrieval | An edit to a file without a retrieval record is rejected as a ToolCallPolicyDecision; a stale-revision retrieval does not satisfy the rule |
| QUAL-PX-016 | REQ-PX-016 | workspace-git | Real fixture task with a test harness: failing test written first, then the change; diffs are revision-bound ChangeTransactions with one concern each; a file outside the plan triggers a plan revision event | Silent scope widening without a plan revision is rejected; lockfile edited by hand rather than by its generator is flagged |
| QUAL-PX-017 | REQ-PX-017 | verification | Real fixture: derived verification plan recorded before the first run with build, typecheck, targeted tests, diagnostics delta and diff invariants; agent-added checks appear; mandatory checks cannot be removed | Attempt to delete a mandatory check is rejected; a task that fails still has its recorded plan |
| QUAL-PX-018 | REQ-PX-018 | core-runtime | Seeded failing fixture: each repair attempt records failure signature, hypothesis, evidence, intended fix, change ref and verification result before the next run; a repeated equivalent hypothesis triggers escalation or Needs Attention with the attempt history; attempt bounds from policy are enforced | A change after a failed verification without a RepairAttempt is rejected; equivalent hypothesis executed twice is impossible; bound exhaustion never truncates silently; WORSENED attempts are reverted or justified |
| QUAL-PX-019 | REQ-PX-019 | verification | Real fixture: the SelfReview step lists plan coverage, executed verifications, receipts, scope and leftovers; unresolved findings block the completion proposal; the Acceptance Gate, not the agent, decides completion | A completion proposal without a SelfReview or with unresolved findings is rejected; agent text claiming done changes nothing |
| QUAL-PX-020 | REQ-PX-020 | eval-bench | Both suites run under the frozen protocol on the real M2 product with the direct configuration; the immutable baseline bundle (digests, model metadata, per-task results, intervals) is recorded and referenced by digest | A baseline with gold-patch access, unpinned images or missing trial counts is rejected; no target may be recorded before this baseline exists |
| QUAL-PX-021 | REQ-PX-021 | eval-bench | Targets set by Decision Record per metric and tier after the baseline; the release candidate competence gate fails on regression beyond approved thresholds with intervals, including cost-improving routing changes that regress competence | A target recorded without a baseline digest is rejected; a candidate that regresses first-pass success or repair-loop distribution cannot pass the gate regardless of routing savings |
| QUAL-PX-022 | REQ-PX-022 | desktop | Playwright drives the packaged app on fresh profiles through provider setup, repository trust and a starter task on a small real repository with a live provider test model; median time to ReadyForReview with real evidence is within five minutes on reference hardware; p90 reported | Invalid key, network failure and untrusted repository each show the named cause and next action; a profile that skips provider setup cannot start a task; no step shows an outcome the Core has not persisted |
| QUAL-PX-023 | REQ-PX-023 | desktop | Each screen's empty, loading, populated, error, degraded and recovery states are forced against a real Core (Core restart, provider down, stale bundle, offline, unknown outcome, quality floor infeasible, human continuation) and each names cause, next action and evidence; notifications fire only for attention, completion and failure and coalesce per task | A screen missing a required state fails the matrix; routine progress producing a notification fails; a recovery banner claiming progress not in Core events fails |
| QUAL-PX-024 | REQ-PX-024 | desktop | Every screen traversed by keyboard only in the packaged app; global shortcuts, list navigation, review navigation and approval with confirmation work; accessibility suite passes; focus retained across state changes; live regions announce attention changes | Any control unreachable by keyboard, any color-only status, or lost focus on a Core event fails |
| QUAL-PX-025 | REQ-PX-025 | desktop | Packaged E2E asserts every interaction budget in doc 39 on reference hardware with Playwright traces and Core event timestamps | A budget miss on a release-critical path fails the candidate; timings taken from screenshots rather than traces are rejected |
| QUAL-PX-026 | REQ-PX-026 | verification | On the ts-webapp, python-service and rust-cli fixtures: exact and BM25 retrieval, revision-bound edits and real compiler and test-runner evidence attributed to tasks; clients label the languages at the Alpha baseline, not Tier A | Any structural or language-service claim in Alpha for these languages fails; an edit that corrupts encoding or line endings fails |
| QUAL-PX-027 | REQ-PX-027 | verification | Tier A, B and C conformance suites exist and run on real fixture repositories: symbol/reference recall and diagnostics parity for A, symbol extraction and build-output diagnostics for B, text safety and configured-command evidence for C; a language enters a tier only through a recorded pass | A language listed in a tier without a recorded suite pass is a release blocker; a grammar alone cannot classify a language |
| QUAL-PX-028 | REQ-PX-028 | context-engine | TypeScript/JavaScript, Python and Rust pass the Tier A suite with real headless language services on the fixtures; incremental index latency within budget; competence suite tasks for each language pass at baseline | Diagnostics parity failure or missing references on fixtures blocks the tier; a dead language service degrades explicitly rather than faking results |
| QUAL-PX-029 | REQ-PX-029 | context-engine | A fixture with an Unsupported language and one with a Tier C language: retrieval falls back to text, verification uses only configured commands, the plan states the limitation, every client shows the language state, edits to the Unsupported language require explicit per-task opt-in with provenance | Any structural claim, silent fallback or edit without opt-in fails |
| QUAL-PX-030 | REQ-PX-030 | governance | CI builds Core, CLI and runtime on macOS, Windows and Linux from M0 and runs unit, component and platform conformance suites (PTY/process, language services, Git, path policy, secrets, packaging, browser host); results labeled CI_COMPATIBLE only | Documentation, app or CLI text describing a CI-compatible platform as supported fails; a failing platform suite blocks the merge, not the label |
| QUAL-PX-031 | REQ-PX-031 | desktop | macOS reaches RELEASE_GRADE by the packaged desktop E2E catalog and the applicable Release Zero subset; Windows and Linux stay CI_COMPATIBLE until their own packaged E2E passes and a Decision Record records promotion | Promotion without platform E2E evidence is rejected; Release Zero on macOS does not imply any other platform |
| QUAL-PX-032 | REQ-PX-032 | verification | Real fixture with one pre-existing failing test and one test the task will break: a BASELINE run executes before the first write and records a VerificationBaseline; the pre-existing failure is labelled KNOWN_FAILING; at the COMPLETION run the newly broken test is attributed as a REGRESSION and blocks acceptance; a check the plan declared as an expected behavior change before the run is shown in Review as declared, not as a regression | A write before the BASELINE run is rejected; a pre-existing failure cannot be attributed to the agent; a REGRESSION with no prior plan declaration cannot be accepted; a declaration recorded after the run does not count |
| QUAL-PX-033 | REQ-PX-033 | verification | Real vitest, pytest and cargo runs on the Alpha fixtures produce TestReports with STRUCTURED parser confidence, stable check_ids, per-check status, location, error class and message fingerprint, and a raw OutputRef; the failure_signature of a seeded failure is identical across two runs at the same revision; the model receives failing CheckResults first with declared truncation and the raw log retained | A configured command with no structured reporter yields HEURISTIC confidence and UNKNOWN for an ambiguous mandatory check, which is INDETERMINATE and never a pass; a signature derived from raw log text rather than CheckResults is rejected; a dropped raw log fails the suite |
| QUAL-PX-034 | REQ-PX-034 | verification | Real fixture task: TARGETED runs execute failed-first, task-named and changed-file-mapped checks within budget and never support completion; the COMPLETION run executes the full configured suite plus every mandatory check at the final candidate revision before the Acceptance Gate; a ChangeTransaction after the COMPLETION run invalidates it and the completion proposal is refused until it reruns | A completion proposal with only TARGETED evidence is rejected; a narrowed completion_scope without the limitation recorded in the plan is rejected; a COMPLETION run at a stale revision is recorded and never used for acceptance |
| QUAL-PX-035 | REQ-PX-035 | context-engine | On the multi-package and ts-webapp fixtures the impact selector chooses tests from dependency, symbol-reference, test-link and Git co-change evidence; precision and recall against the full-suite ground truth are recorded with the retrieval benchmarks; until the task is COMPLETE the plan states that targeting is heuristic | A selector that omits a test failing in the full suite for a changed symbol within the depth bound fails; a selection result without recorded precision and recall is rejected |
| QUAL-PX-036 | REQ-PX-036 | verification | Fixture with a seeded flaky test: a failure triggers exactly one isolated rerun at the same revision and environment digest; the check is labelled FLAKY with both run references, excluded from failure_signature derivation, shown in Review and the SelfReview, and quarantined only for the task and revision range; a flaky mandatory check makes acceptance INCONCLUSIVE unless three consecutive isolated passes are obtained within budget; a check flaky at BASELINE is pre-quarantined | The agent cannot label, skip or quarantine a check; a test change that adds skip or retry markers is a DI-3 violation; a RepairAttempt whose only passing evidence is a FLAKY check is UNCHANGED, not RESOLVED; a quarantine that survives a change to the check's file or dependencies fails |
| QUAL-PX-037 | REQ-PX-037 | workspace-git | Real worktree: the Change Engine evaluates DI-1..DI-9 when a ChangeTransaction is proposed and the Verification Engine evaluates the whole diff at COMPLETION; deleting, skipping, weakening or rewriting an acceptance-named test is rejected as a ToolCallPolicyDecision with a DiffInvariantViolated event; a hand-edited lockfile and a dependency-manifest change without a plan entry are rejected; debug leftovers and formatting churn beyond the threshold are flagged and block the SelfReview until resolved or justified | A DENY invariant cannot be downgraded by the agent or a prompt; an unclassifiable test-file change is flagged, never passed silently; an accepted result with a DI-3 violation in its diff fails the suite |
| QUAL-PX-038 | REQ-PX-038 | core-runtime | Real fixture task: the first PlanRecorded freezes the original write set; each PlanRevised carries a scope delta; Core counts out-of-plan files and revisions against the ScopePolicy bounds; beyond a bound or on an always_ask_paths match the next out-of-scope write is refused until a typed question offering continue, split or stop is answered, and a ScopeExpansionRecorded event carries the counters; in headless mode the policy default fails closed to Needs Attention; the doc 63 scope metric is computed against the original plan | Silent scope widening is rejected; a PlanRevised without a scope delta and reason is rejected; a question auto-answered by the agent changes nothing; measuring scope against the last plan revision fails the suite |
| QUAL-PX-039 | REQ-PX-039 | core-runtime | Seeded defect fixture: the first verification run reproduces the reported failure before any fix transaction; a fix proposed before reproduction is rejected while reproduction_required holds; an identical change_fingerprint is rejected and counted as an equivalent attempt; an oscillating change escalates; three consecutive turns with no new transaction, verification, retrieval, plan revision or question emit NoProgressDetected and escalate or move the task to Needs Attention; the Alpha defaults 2/6/1/true/3 are read from the versioned RepairPolicy, not hard-coded | A prompt cannot raise the bounds; an UNREPRODUCED failure cannot be silently treated as reproduced; a WORSENED attempt beyond max_worsened_before_escalation without escalation fails the suite |
| QUAL-PX-040 | REQ-PX-040 | core-runtime | Real fixture task with a large-output command, a failing test, a missing runner and a headless CLI run: every tool result reaches the model within the inline ceiling with declared omitted ranges and a pageable OutputRef; failing CheckResults arrive first; the Context Pack carries harness_state with plan, open failure signatures, quarantines, scope counters, remaining budgets and candidate revision; the missing runner is recorded in the plan with a question or compensation; the headless question fails closed to Needs Attention after the configured wait; a completion proposal while a verification run is in flight or stale is refused; budget exhaustion emits HarnessBudgetExhausted and moves the task to Waiting or Needs Attention with partial evidence | Silent truncation of a tool result fails; a turn aborted by a command failure fails; a task blocked forever on a headless question fails; a completion accepted without the COMPLETION run at the final revision fails; a budget exhausted without an event or attention state fails |
| QUAL-PX-041 | REQ-PX-041 | domain-events | Real Core and the TypeScript client on a real provider endpoint (the compatible gateway of DR-M9-002) plus a scripted OpenAI-compatible HTTP endpoint for deterministic chunking: a 400-word answer arrives as strictly increasing delta events within the 50 ms and 2 KiB bounds; the concatenation equals the completed message digest; a client dropped mid-stream resumes by cursor with no gap and no duplicate; `SIGKILL` of the Core mid-stream leaves the stream closed as aborted-by-recovery after restart, no partial text marked complete, and the run proceeds under the existing recovery path; a planted secret in model text is redacted in every delta and in the completed record; event timestamp to render is at most 100 ms at p95 in the packaged app. | A delta without a stream id, out of sequence, after completion or over the size bound is rejected by the store and fails the suite; a forged client command that appends a delta is rejected; a provider connection dropped mid-stream aborts and never completes; mutation check: disabling redaction before append fails the secret case; an uncoalesced per-token flood fails the rate bound. |
| QUAL-PX-042 | REQ-PX-042 | domain-events | Real Core with 200 seeded tasks across states and a live task: the header list is served without loading any transcript (asserted by store read counters); each status class appears for its seeded condition and the precedence holds when several conditions coincide; the transcript projection of a finished turn equals the event log folded by the documented rules in all three densities; search finds a phrase that only exists in a conversation body; delete the projection tables, restart, and the projections rebuild byte-identically from the log. | A client-supplied status class is ignored; a header read that loads a transcript fails the counter assertion; two densities that disagree on the underlying events fail; an archive and an unarchive replayed with the same command id yield one event each; a stale cursor page is refused with the Core's code; the projection of a task of another session is refused. |
| QUAL-PX-043 | REQ-PX-043 | terminal | Real Core, real PTY process and the TypeScript client: a build that prints more than 10 MB streams in order, a client detached and reattached by cursor misses nothing, and older output is read as OutputRef ranges; a long command detached to the background survives a renderer restart and a Core restart (the existing durable handle) and shows the right state; `KillTerminal` ends the process, the transcript records Stopped, and the agent receives one typed event and continues once; user input to a user terminal is journaled and refused for an agent terminal. | A client writing to an agent-owned terminal is rejected by the Core; a kill replayed with the same command id kills once and wakes once; an attach with a cursor beyond the head or older than the window answers with the typed range error, never silent truncation; a wake-up that arrives as a user message fails the suite; killing a terminal of another session is refused. |
| QUAL-PX-044 | REQ-PX-044 | desktop | Playwright against the packaged app on a real Core in both colour schemes: the axe-core suite reports zero violations on every existing screen and on the gallery; the computed contrast test passes for every token pair and fails when a token is lowered below the ratio; every primitive operates by keyboard alone; with reduced motion on, no transition or animation runs; the tray host keeps one active tray and moves scoped keys with it. | A token below its AA ratio fails the contrast test before axe runs; a status shown by colour only fails the accessibility suite; a primitive with no accessible name fails; a gallery state counted as proof in an evidence bundle fails the evidence check; the dark and light key sets must be identical or the build fails. |
| QUAL-PX-045 | REQ-PX-045 | desktop | Playwright on the packaged app and a real Core at 1280 x 800, 1000 x 700 and 900 x 600: at 1280 the agent list is 260 pt (within 2 pt), the top bar 40 pt, the centre pane at least 424 pt with the panel open and the panel at least 384 pt; dragging the divider respects 210 to 400 pt; below 448 pt the single-pane mode and rail appear; layout survives an app restart; the palette finds a task by title and a setting; every shortcut of the registry fires its command and none collides; the six PX-023 screen states still render with cause and next action. | A shortcut registered twice fails the conflict test; a panel ratio that would push the centre under 424 pt is clamped; a layout preference written into task state fails the persistence check; a shell state with no keyboard path fails PX-024; opening Settings with a running task never stops or alters the task. |
| QUAL-PX-046 | REQ-PX-046 | desktop | Playwright on the packaged app with the 200-task fixture of PX-042 and a live task: every class renders its glyph and text and never colour alone; a pending approval puts the attention dot on its row and the row survives a renderer restart and a Core restart; archive removes the row, shows the undo for at least 8 s, undo restores it, and selection lands on the neighbouring row; archiving a running task asks first and stopping it is a Core event; a cloud task viewed last does not make the next new task cloud; filters and grouping change the list without loading transcripts; the list reflects a Core event within 150 ms at p95. | A renderer that recomputes the status class fails the single-source check; an archive of a running task without the confirmation fails; a pin beyond the cap shows the message and changes nothing; keyboard-only operation of every row action is required; a color-only status fails axe. |
| QUAL-PX-047 | REQ-PX-047 | desktop | Playwright on the packaged app against a real Core and the live gateway plus the scripted endpoint: a streamed 400-word answer renders progressively and follows the tail; scrolling up freezes follow and the pill counts later rows; folding and density change presentation only and never the underlying rows; a running command shows its humanised tail status and a hung command escalates the wording and raises STALL at the configured threshold; a provider failure before any token restores the prompt and docks the error tray; a planted instruction in tool output renders as inert text; first-delta render is within 100 ms of the event timestamp at p95; 1,000 rows scroll without dropped frames beyond the budget. | A row that shrinks while streaming fails the monotonic check; HTML in assistant text is not executed; a link outside the allow-list does not open; folding that hides an approval card or an error fails; a stalled turn with no tail status fails; the renderer holding a provider credential or calling a provider fails the dependency scan of PX-001. |
| QUAL-PX-048 | REQ-PX-048 | desktop | Playwright on the packaged app on a real Core: the terminal tab streams a real build live, a detached terminal reattaches after a renderer restart with no missing output, scrolling beyond the window loads OutputRef ranges, an agent terminal refuses typing and a user terminal accepts it only with policy; the changes tab revert and stage act on a real worktree and Commit and push asks for approval with the exact intent; panel state persists per task across restart; the browser tab keeps its lease badge and takeover. | Typing into an agent terminal is refused by the Core and shown; closing a running agent terminal without the warning fails; a commit or push that runs without an approval fails; revert of a file edited since the revision is refused as stale; a tab restored for another task's id fails the per-task state check. |
| QUAL-PX-049 | REQ-PX-049 | desktop | Packaged app on a real Core with a run in progress: quitting asks and, when resume is chosen, the next start shows the interrupted run recovered with the Core's counts and the run continues; a session-end hook that takes 2 s completes before exit and its output is journaled; a hook that fails produces a visible failure and a journal entry; the window cannot be resized below 900 x 600; a diff of main.ts shows only window options and quit handling and the preload and CSP files hash identically. | A session-end hook that is skipped because teardown began fails the suite; a quit that discards a running task without asking fails; a change to webPreferences, the CSP or preload fails the diff and hash check; a renderer that claims recovery progress the Core did not report fails; killing the Core during shutdown leaves the task recoverable. |
| QUAL-PX-050 | REQ-PX-050 | core-runtime | Real Core, scripted and live providers: queue three messages during a streaming turn and observe them drained in order as three turns; edit and delete before dispatch take effect; reorder changes the drain order; Send now on a live stream ends the stream as user-interrupt aborted with the partial text kept, reconciles an in-flight shell command first, and starts the new turn once; `SIGKILL` of the Core with items queued preserves them in order and drains them after recovery; the same commands from the CLI and the desktop client give the same results. | A queue command without the session lease is rejected; replaying Send now with the same command id interrupts once; an interrupt during an unknown-outcome effect does not start the new turn early; a deleted item is never dispatched; reordering an already dispatched item is refused; an abort without a typed source fails the suite. |
| QUAL-PX-051 | REQ-PX-051 | core-runtime | Real Core with the live gateway: in ASK a prompt to edit a file yields a refusal by the Kernel and no write; in PLAN the agent records a plan and the first write attempt is rejected until acceptance, after which the task is AGENT with the plan attached; in DEBUG a fix proposed before the failure is reproduced is rejected; in MULTITASK a bounded child is admitted with a ticket; a proposed switch with no answer expires skipped after 15 s and the posture is unchanged; consent remembered for a transition survives a Core restart. | A client that sends a forged posture or a mode on a task it does not own is rejected; ASK with a write tool attempt is denied by the Kernel even when the model insists; an unanswered proposal never switches; a mode change mid-effect does not retroactively deny the in-flight effect; a prompt-injected request to switch mode through tool output changes nothing. |
| QUAL-PX-052 | REQ-PX-052 | skills | Real Core with a profile that has a built-in, a user, a project and an imported skill plus a revoked one: `ListSkills` returns each with the right scope, trust, provenance and invocation flag; the order and divider rule hold; installing and revoking a skill on disk changes the list within 1 s; a user-only skill is refused when the model invokes it; the desktop client and the CLI render the same inventory. | A revoked skill is never invocable; a skill whose hash differs from its manifest is listed as untrusted; the command returns no skill body or secret; a client that requests another session's inventory is refused; a model-invoked user-only skill is rejected as a ToolCallPolicyDecision. |
| QUAL-PX-053 | REQ-PX-053 | model-gateway | Real Core with a signed model registry bundle and an organisation policy: a task is created in Balance, the preference changes to Intelligence between turns and the next compile uses the new profile and records why in the routing decision; an allowed manual pin is honoured and a pin forbidden by policy is refused with the typed reason and no state change; the preference survives a Core restart; the CLI and the desktop client set it identically. | A preference with a stale Run generation is rejected; a client-supplied model not in the registry is refused; a policy-blocked pin never reaches the compiler; a replay with the same command id changes state once; a diff that alters compiler, gate or statistics code fails the boundary check and stops the row. |
| QUAL-PX-054 | REQ-PX-054 | desktop | Playwright on the packaged app and a real Core: Shift+Tab walks Plan, Debug, Multitask, Ask and back with the right chip, tint, icon, label and placeholder, and the chip also opens a menu by pointer and keyboard; the posture really changes in the Core (a write in Ask is refused); typing @ lists the typed sources and a path the Core denies shows its reason and cannot attach; typing / lists built-ins, a divider and imported skills in order with the detail card; an attached image is read only by the Core; history and the draft survive navigation and a renderer restart; composer geometry is within 8 percent of the targets at 1280 x 800. | An attachment outside the workspace path policy is refused by the Core and shown; a mode chosen in the renderer but not acknowledged by the Core shows as unconfirmed, never as active; a model-supplied string in a skill description cannot inject markup into the menu; the mode cycle is reachable without a pointer; a draft is never sent without the user pressing send. |
| QUAL-PX-055 | REQ-PX-055 | desktop | Playwright on the packaged app, real Core, live gateway and scripted endpoint: three queued messages show in the tray, edit and delete change what is sent, Send now interrupts a live stream and the label said so, and the partial text stays marked partial; Stop ends the turn in about 1 s, the user message becomes editable and nothing is re-sent; the education tray shows once and its acknowledgement persists; killing a background terminal from its tray wakes the agent exactly once; a side question is answered without entering the main context; a mode-switch proposal left unanswered shows skipped after 15 s. | A tray action replayed after a renderer reload sends once; a label that claims 'without stopping' for a steer that interrupts fails the copy check against the Core's behaviour; the edit-in-place message is never re-sent without the user; keyboard-only operation of every tray action is required; Send now while an effect outcome is unknown shows the wait and does not skip reconciliation. |
| QUAL-PX-056 | REQ-PX-056 | desktop | Playwright on the packaged app, real Core with a registry and a policy that blocks one model: the picker shows allowed models and variants, switching profile changes the next compile's recorded profile, an allowed pin is honoured, choosing the blocked model yields the tray with the typed cause and the restored prompt, and Enter with the tray up shows the inline reason and loses no text; the choice survives restart. | A renderer that sends a model the policy forbids is refused by the Core and the tray shows the Core's reason; the picker never displays a model the registry marks unavailable as selectable; a tray that traps Enter silently fails; a pin on a task of another session is refused; no billing or upgrade control is present. |
| QUAL-PX-057 | REQ-PX-057 | effects-security | Real Core, real worktree and the live gateway: in Ask a shell write asks with its exact intent; after `AddAllowRule` for an exact argv prefix the same command runs without asking and a receipt names the rule; a command that adds a redirection or a pipeline to a matching prefix asks again; an outside-workspace write and a network fetch ask in every mode including Run everything; Run everything is gone after a Core restart; Allowlist with sandbox runs a contained command silently and asks for an uncontained one; revoking a rule makes the next run ask; all of it is identical from the CLI. | A rule created by a client without the lease or by a background agent is refused; a model-declared escalation the user did not approve never runs; a pattern that matches a sub-command inside a substitution is rejected; an expired rule asks again; mutation check: removing the always-ask classes fails the outside-workspace and network cases; a rule outside its repository scope never matches. |
| QUAL-PX-058 | REQ-PX-058 | desktop | Playwright on the packaged app, real Core, live gateway under Ask and Allowlist: a command not in the allowlist docks the card with the exact intent; Enter runs it once and the receipt names the intent hash; Skip returns the typed denial and the agent continues; Shift+Enter writes a durable rule through the Core; the card survives a task switch, a renderer kill and a Core `SIGKILL` and is still pending and unchanged; two pending effects show N pending and approving one leaves the other pending; typing in the composer and pressing Enter never approves. | A card whose parameters changed after display is refused with INTENT_MISMATCH by the Core and the stale card is replaced; Enter in the composer approves nothing; a decision on a card of another task is refused; batch approval does not exist; an approval that expired shows as expired and asks again; the card with no keyboard path or with colour-only state fails PX-024. |
| QUAL-PX-059 | REQ-PX-059 | context-engine | Real Core with the live gateway and a model whose window is known: after a turn the eight categories sum exactly to the total; the total equals the provider request's counted tokens within the estimator's declared error; a task with rules, a skill, an external tool and a subagent shows non-zero values in those categories; after a compaction epoch the summarised-conversation category is non-zero and the conversation category falls; switching the model changes the window and the percent. | Categories that do not sum to the total fail the invariant; a fixed window constant fails the model-aware check; a total that differs from the provider request by more than the declared error fails; a category omitted from the envelope but counted fails; a client-supplied breakdown is ignored. |
| QUAL-PX-060 | REQ-PX-060 | desktop | Playwright on the packaged app, real Core and live gateway: the ring tracks the Core's percent as a turn grows and after a compaction; the tray's categories equal the Core's breakdown; with the budget forced low the limit tray appears with the staged text and its actions call the budget policy; with the network cut the offline tray appears, the transcript stays readable and sending fails with the cause; the error tray retries once. | A ring or tray figure that differs from the Core's value fails; a tray that traps Enter silently fails; a limit action that changes a budget without the policy's approval is refused; colour-only status fails axe; a stale figure after a model switch fails. |
| QUAL-PX-061 | REQ-PX-061 | durability | Real Core, real worktree and the live gateway: three turns each create a checkpoint; restoring turn 1 deletes the files turns 2 and 3 created and restores the changed ones byte-for-byte; redo restores the pre-restore state exactly (compared by file hashes); a file edited by the user after the checkpoint is listed and is not overwritten without the choice; `SIGKILL` during a restore recovers to either the pre-restore or the restored state and never a mixture, verified by hashes; replaying the restore command with the same id restores once. | A restore against a stale epoch is rejected; a restore that would overwrite a user-edited file without the choice is refused; redo of a checkpoint that was itself superseded is refused with the typed reason; a fork carrying a pending approval fails; mutation check: dropping the pre-restore checkpoint fails the redo equality. |
| QUAL-PX-062 | REQ-PX-062 | desktop | Playwright on the packaged app, real Core and a real worktree: the dialog shows the Core's preview counts; Continue restores and the files match their hashes; the message is editable in place and is not re-sent until the user sends; Redo restores the pre-restore state exactly; a user-edited file blocks the do-not-ask option and requires the per-file choice; Revert and Keep behave as chosen when editing an older message; Undo all needs a second click; a renderer kill during the dialog leaves the workspace unchanged. | Continue replayed after a reload restores once; Cancel changes nothing; the dialog never claims a redo for a non-reversible state; a restore of a checkpoint from another task is refused; keyboard-only operation of the dialog and the link is required; a do-not-ask preference never applies to a restore that would overwrite user edits. |
| QUAL-PX-063 | REQ-PX-063 | core-runtime | Real Core with two workspaces and ten tasks: create a project, add and remove tasks, and the header projection and `ListProjects` agree; each guard rule refuses its violation with the typed reason; `SIGKILL` between the two phases of an add is reconciled at startup to one consistent state and the pending operation is reported once; archiving a project does not archive or stop its tasks; the CLI and the desktop client give the same results. | A subagent, draft or imported task is refused; a cross-project move without leaving first is refused; a project of another workspace is refused; a replayed add with the same command id adds once; a membership map that disagrees with the log after recovery fails the invariant; a client-side membership that the Core never recorded is not shown as fact. |
| QUAL-PX-064 | REQ-PX-064 | desktop | Playwright on the packaged app with the fixture of PX-063: the Projects group lists projects separately from repositories; adding and removing a task through the menu and by drag reaches the Core and the list reflects the Core's projection; each forbidden move shows the Core's typed reason and changes nothing; project grouping and filters work without loading transcripts; the layout persists across restart. | A drop that the Core refuses leaves the list unchanged and shows the reason; a project with a pending two-phase operation shows the pending state; keyboard-only operation of add, remove and archive is required; a renderer that edits membership locally fails the single-source check. |
| QUAL-PX-065 | REQ-PX-065 | workspace-git | Real Git and Core: create worktrees with dirty and untracked files and verify the copies and the typed branch-in-use error; a setup list that fails shows the failure and the task continues; a setup list that sleeps past 300 s is stopped and reported; with a shortened interval the cleanup runs on schedule, after a Core restart with an overdue last run the catch-up runs after 30 s, two concurrent requests give one lease holder and one refusal; with 30 worktrees (5 dirty, 3 with unapplied changes, 2 running, 1 under 10 minutes old) cleanup removes only eligible ones, in orphan-then-LRU order, and the report matches the filesystem; a skipped run reports skipped. | A dirty or unapplied worktree is never removed; a worktree with a running task or younger than 10 minutes is protected; a second lease request while one is held is refused; a setup command in an untrusted repository does not run; a cleanup report that says completed for a skipped run fails; removal of a path outside the worktree root is refused after symlink resolution. |
| QUAL-PX-066 | REQ-PX-066 | workspace-git | Real Git and Core: apply a clean reviewed result and the checkout hash equals the candidate; apply with a conflicting user edit and exercise each option, comparing file hashes after each and after Undo; a stale review revision is refused; `SIGKILL` mid-apply recovers to a whole state verified by hashes; the receipt names the intent hash and both revisions; the same from the CLI. | Apply without approval is refused; apply at a stale revision is refused; overwrite without the typed confirmation is refused; Undo after a second user edit reports the conflict instead of destroying it; a remembered destructive choice is not honoured; a replay of Apply with the same command id applies once. |
| QUAL-PX-067 | REQ-PX-067 | workspace-git | Real Git against hostile fixture repositories: one with a pre-commit hook that writes a marker file, one with `core.fsmonitor` pointing at a script, one with a clean filter attribute, one with a remote URL containing a token; every Core operation (status, diff, worktree add, commit, merge, push to a local remote) leaves no marker, runs no script and writes no token to any log, event or error; the security event is recorded for each neutralised setting. | A direct Git spawn outside the runner fails architecture lint; a hardened environment that can be overridden by repository configuration fails the fixtures; a token that appears in any retained artifact fails the redaction scan; mutation check: removing the hook neutralisation makes the marker appear and the suite fail. |
| QUAL-PX-068 | REQ-PX-068 | desktop | Playwright on the packaged app, real Git and Core: the worktree list matches the filesystem and the Core's eligibility; Apply asks for approval with the exact intent; the modal appears on a real conflict and each option yields the hashes of PX-066; typed confirmation is required for overwrites and Cancel is the default; Undo restores exactly; a remembered non-destructive choice is applied on the next conflict and a destructive one is not remembered; Run cleanup shows the Core's report including a skipped run as skipped. | A modal that defaults to a destructive choice fails; an overwrite without typed confirmation is refused by the Core even if the renderer sent it; the list showing a worktree the Core says is ineligible as removable fails; keyboard-only operation of the modal is required; a renderer that runs Git or removes a directory fails the dependency scan. |
| QUAL-PX-069 | REQ-PX-069 | browser | Real Core with the real actuator of PX-071 on macOS and a fixture application suite (a form app, a document app, a modal-dialog app and an app with a secure field): an accessibility-first task completes with no coordinate used; a stale element id after the tree changed is refused TARGET_STALE and nothing is actuated; a coordinate action without a fresh screenshot is refused SCREENSHOT_REQUIRED; every refusal code is provoked by its fixture and returns its documented escalation; killing the actuator after an input was delivered but before the result sets the latch, the next input is OUTCOME_UNKNOWN until a new session with a fresh observation, and one UNKNOWN receipt exists; two simultaneous calls from one task run one at a time; a model that misspells an argument gets the refusal naming the advertised arguments and no default action runs. | An argument silently dropped fails; an input retried after delivery fails the retry audit; a cloud task listing computer tools fails the projection check; a call with no approval of PX-070 is refused at the kernel even if the runtime would accept it; a latch cleared without a new session fails; mutation check: removing the latch lets a second input through and fails the suite. |
| QUAL-PX-070 | REQ-PX-070 | effects-security | Real Core with the macOS actuator: an action with no approval is refused; an approved action runs once and its receipt carries the intent hash and the application identity; the same call after the snapshot changed asks again; `AddAllowRule` for a computer action is refused with the typed reason and Run everything still asks; an observation grant for one application does not authorise another and ends at turn end and at the call cap; a background child cannot use the capability; a policy that forbids SCREEN scope refuses it and a missing policy verdict refuses closed; a drive attempt on a listed non-drivable application is refused WINDOW_UNVERIFIABLE with no approval offered. | A durable rule that matches a computer action is rejected on creation; a Run everything session that auto-approves a computer action fails the suite; an approval presented for application A cannot authorise an action in application B whose window title spoofs A (identity is the signed bundle, REQ-EV-0083); an expired grant is GRANT_EXPIRED; a mode switch carrying a grant across fails; mutation check: dropping the identity binding lets the spoof through and fails the suite. |
| QUAL-PX-071 | REQ-PX-071 | browser | Real macOS machine, packaged helper and Core: with Accessibility granted a fixture application is driven in the background while the real cursor stays under the person's control; with the permission revoked every call answers PERMISSION_REQUIRED; a second process that creates the socket first makes the helper abort and the Core refuse the welcome; a forged request with a wrong token is rejected; a tampered helper (signature broken) is not connected; physical keyboard input parks the controller within one event and the next agent input is HUMAN_ACTIVE; Stop from each of its four sources halts injection within 500 ms measured from the Stop event to the last injected event; killing the Core makes the helper release control and stop; killing the helper mid-action sets the latch. | A helper that accepts a connection without the token fails; a Core that accepts a welcome from another pid fails; an injection after Stop fails the bound; a helper holding a credential or file path authority fails the static dependency scan; synthetic events counted as human activity fail; a lease that survives turn end fails; the permission prompt appearing from any process other than the helper fails. |
| QUAL-PX-072 | REQ-PX-072 | browser | Real signed packaged build on macOS: the helper's signature, notarisation and manifest hash verify; replacing the helper with a modified binary of the same name is refused at connect; an update applied while a control session is live waits until the session ends; killing the process mid-update leaves exactly one whole helper version and control works after the next start; a rollback restores the previous version; a protocol mismatch refuses with the typed reason. | A helper not listed in the manifest is never launched; a signal sent to a pid whose path differs is not delivered; an update that replaces a running helper fails; a downloaded helper path existing in the shipped code fails the static scan; a missing SBOM entry for the helper fails the release gate. |
| QUAL-PX-073 | REQ-PX-073 | browser | Real packaged app and Core against a local HTTPS fixture server with a self-signed certificate and a hostile page: raw protocol calls to each denied domain and method are refused at the host before attach and the refusal is typed; an action on a page whose origin is not allowed is refused even when the navigation was allowed earlier (the page redirected client-side); a `file:` page refuses every tool; an unknown origin refuses closed; the certificate prompt holds for 60 s, Reject fails the request and Trust is remembered and then cleared; a page asking for camera or geolocation is denied and named; sign-out leaves no cookie or storage in the partition (read from disk); a second task cannot list or select the first task's views; the seventh hidden view reclaims the least recently used and states the reset; an emulation set by the agent is reverted at turn end and is not applied over a user emulation. | A deny-listed method that reaches the page fails the suite; an origin check performed only at navigation fails the client-side-redirect case; a certificate trust that persists after Clear fails; a view reclaimed while locked or loading fails; an agent reading another task's view fails; mutation check: removing the per-call origin check lets the redirect case act and fails the suite. |
| QUAL-PX-074 | REQ-PX-074 | desktop | Playwright on the packaged app on a real macOS machine with the helper: a first run with neither permission shows both rows; granting Accessibility updates its row after the refocus re-check; a task that wants to act shows the card with the exact identity and arguments and no Always control; Stop from the presence control ends injection within the bound of PX-071 and the card state shows USER_ABORTED; presence appears in a second window and disappears at turn end; keyboard-only operation of the window, the card and Stop; the live region announces control acquired and released. | A presence indicator that remains after release fails; an Always or allowlist control on a computer card fails; Stop that depends on the renderer being responsive fails the renderer-kill case (the helper stops on disconnect); the permission window opening from a child window fails; colour-only presence fails axe. |
| QUAL-PX-075 | REQ-PX-075 | core-runtime | Real Core, the macOS actuator and the live gateway: a parent spawns the profile for a fixture-app test, the child's tools match the list and a shell attempt to drive the GUI is refused; the child's handles are not valid in the parent; a profile run in ASK mode is refused; a child that fails four times stops and returns the structured blocker report; a login prompt fixture is reported as a blocker and not entered; the child resumes by identity after a Core restart with its control session closed and a new approval required. | A child that inherits the parent's grant fails; a child reading the parent transcript fails; an unattended automation spawning the profile is refused (PX-084); a fifth attempt after the stop rule fails; a resumed child that reuses an old snapshot id fails TARGET_STALE. |
| QUAL-PX-076 | REQ-PX-076 | media | Real actuator and Core with fixture applications that show a password field, a secure input and a normal form: the stored frame of the password window has the field masked at the pixel level; a locked-screen run returns SECURE_DESKTOP with no artifact; a flat dialog is stored lossless and a photographic window lossy with unchanged dimensions; two identical captures produce one encode; the audit for a 12-action session lists counts by kind, one screenshot count and the outcome and contains no text typed and no pixel; artifacts follow the retention setting; a planted secret in a visible label is not present in any event or log. | An unmasked secure field in any retained frame fails; a frame whose size is not the canvas is refused; a local frame handed to a cloud task fails; typed text appearing in the audit fails; an artifact outliving its retention fails; a lossy encode of a flat frame beyond the declared ladder fails the codec test. |
| QUAL-PX-077 | REQ-PX-077 | workspace-git | Real Core and a real worktree with fixtures in UTF-8, UTF-8 with a byte-order mark, UTF-16, CRLF and LF files and one with no trailing newline: open reports each fact; a save round trip keeps every one byte-for-byte except the edited range; a two-file save is atomic (a path-policy denial on the second file leaves the first unchanged); a save at a stale revision is refused with the new revision; replaying a save with the same command id applies once; `SIGKILL` between request and result leaves one applied-or-not state confirmed by the ledger; an agent task's write to an open path emits `FileRevisionAdvanced` with the cause; a symlink to a protected file and a path outside the workspace are refused; the checkout and worktree paths are both honoured. | A client that writes through any path other than `SaveEdit` has none and the dependency scan of PX-001 proves it; a save that changes line endings the person did not touch fails the property test; a protected path accepting a write fails; a save without the session lease is rejected; a binary file returned as text fails; a revision event missed by a subscribed client that reconnects by cursor fails the replay check. |
| QUAL-PX-078 | REQ-PX-078 | context-engine | Real Core with the real language services on the ts-webapp, python-service and rust-cli fixtures: a draft that introduces a type error returns that diagnostic against the draft hash while the file on disk is unchanged (hash compared before and after); a request above the rate limit is refused with the typed reason; outline, definition and references answer at the right revision and are marked stale after an agent edit; killing the language service returns an explicit degraded answer; a Tier C fixture returns text-level answers with its language state; resident memory after 1,000 analysis requests returns to its baseline. | An analysis overlay that leaves a file or temp artifact behind fails; a result without a revision and draft hash fails; per-keystroke push traffic fails the pull-based check; a stale result shown as current fails; an unsupported language returning a structural answer fails (PX-029). |
| QUAL-PX-079 | REQ-PX-079 | desktop | Playwright on the packaged app on a real Core: typing, undo and redo, find and replace with a pathological pattern (returns within the budget, no catastrophic backtracking), go to line, and a save that changes the file on disk exactly as expected (hash compared); an agent change to an open file marks the draft stale and a non-overlapping draft rebases while an overlapping one opens the three-way merge and cannot be saved until resolved; a renderer kill with a dirty draft restores it against the same base and shows it stale against a moved base; a 6 MB file opens read-only with the reason and a 60 MB file is refused; a binary file shows type and size; a screen-reader script reads text, cursor moves, diagnostics and save state; input-method composition inserts the composed text; first paint of a 1 MB file is within 150 ms and keystroke to paint within 16 ms at p95 in a 100,000-line file; axe reports no violations. | A draft applied automatically fails; a save through anything but `SaveEdit` fails the dependency scan; a regular expression that backtracks catastrophically fails the engine test; HTML in a pasted payload executing fails; an engine with Monaco or Code-OSS code in its dependency closure fails the licence and provenance scan; Tab trapping focus without the setting fails; a dirty-close with no prompt fails. |
| QUAL-PX-080 | REQ-PX-080 | desktop | Playwright on the packaged app, real Core and live gateway: selecting a function and invoking quick edit starts a task whose context inspector shows that selection and range; the agent's change returns as a per-hunk review at the right revision, accepting one hunk applies only that hunk (file hash) and rejecting another leaves it; changing the file by hand between review and accept makes the decision refuse as stale; the selection bridge never changes a file; the notice appears for an agent change to a dirty file and opens the merge. | A quick edit that writes to the draft or the file without a review fails; a decision at a stale revision applying fails; a selection outside the stated range being edited fails the range check; an agent write silently overwriting a dirty draft fails; the bridge mutating source fails (QUAL-EV-0141). |
| QUAL-PX-081 | REQ-PX-081 | verification | The suite runs on a real Core and the packaged app: 1,000 generated round trips and 100,000 fuzzed save requests leave every file either byte-identical outside the edit or unchanged with a typed refusal; the input-method and screen-reader sessions pass; the latency traces meet the budgets or fail the candidate; a seeded corruption (a mutation that rewrites line endings) is detected by the property test. | A suite that passes with the line-ending mutation in place fails its own mutation check; a fuzzed request that corrupts a file fails; a budget miss fails the candidate; results taken from screenshots rather than traces are rejected; weakening an assertion to pass fails the evidence check (doc 82). |
| QUAL-PX-082 | REQ-PX-082 | automation | Real Core: create a definition, edit it twice (three versions) and enable the third; enabling with an unsafe expression, an out-of-policy capability, a missing budget or an untrusted workspace is refused with the typed reason; a write-capable definition cannot be enabled without the approval listing its exact capabilities and an edit after enabling disables it until re-approved; a repository-supplied definition loads disabled, shows its hash and enables only on approval at that revision, and a later edit re-prompts; a model tool call that tries to create or enable one is denied; the reference drift definition enables with the hash approval only and its profile cannot write (a write attempt is refused by the Kernel); definitions survive `SIGKILL` and replay by cursor. | A model-origin command that creates or enables a definition fails; enabling without validation fails; an approval bound to an older hash being accepted for a newer version fails; a secret value stored in a definition is rejected; a regular expression with catastrophic backtracking is rejected at save; a definition of another principal being editable by this one fails; mutation check: dropping the hash binding lets an edited definition run and fails the suite. |
| QUAL-PX-083 | REQ-PX-083 | core-runtime | Real Core with a controllable clock source and real tasks: a schedule fires at its slot and creates exactly one task; a Core restart across a slot with policy skip records one skipped run and with catch-up-once one run, never several; two deliveries with the same event id create one task; the concurrency policies behave as declared under overlapping firings; a rate limit and a daily budget stop admission with the typed skip and an attention item; the gate step skipping spends only its budget; the run's task shows origin, provenance and an isolated worktree; killing the Core mid-run reconciles through the existing recovery path and the run record shows one outcome. | A second scheduler loop or timer thread outside the Scheduler fails the architecture lint; a duplicate delivery creating a second task fails; a restart that replays missed slots in bulk fails; admission beyond a budget fails; a schedule faster than the floor is rejected; a catch-up that runs while the Core was down cannot happen (local schedules fire only while the Core runs); mutation check: removing the idempotency key lets the duplicate through and fails the suite. |
| QUAL-PX-084 | REQ-PX-084 | effects-security | Real Core, live gateway and a real worktree: an automation whose agent needs a protected write parks in Needs Attention with the exact intent addressed to the principal, the person approves it later in the desktop and the run continues exactly once; an unanswered request cancels the run at expiry with a receipt; a pull-request body saying to approve everything or widen capabilities changes nothing (events and capabilities compared before and after); a run cannot use the computer capability or an Always rule; a paused definition fires nothing; emergency stop cancels a live automation run; five consecutive failures disable the definition with an attention item; a read-only definition cannot write even when the model insists. | An automation run that auto-approves an effect fails; an agent inside a run approving its own request fails; a parked approval that survives expiry fails; an injected payload changing policy fails the injection suite; a run resolving a credential its principal lacks fails; mutation check: removing the ceiling intersection lets a write through and fails the suite. |
| QUAL-PX-085 | REQ-PX-085 | sandbox-cloud | Real staging Cloud API and a worker: a signed webhook creates exactly one tenant task and a replay of it creates none; an unsigned, wrong-signature, stale-timestamp or wrong-tenant delivery is rejected and audited; a cloud schedule fires once per slot across a control-plane restart and follows the missed-run policy; a forced organisation allow-list denies a run's disallowed host; a cross-tenant definition reference is denied; the secret is shown once and never appears in a log or event. | A replayed delivery creating a second task fails; a webhook with a valid signature for another tenant creating a task fails; a secret retained anywhere after first display fails the redaction scan; a cloud run reaching a host outside the allow-list fails; mutation check: dropping the timestamp window lets an old delivery through and fails the suite. |
| QUAL-PX-086 | REQ-PX-086 | desktop | Playwright on the packaged app and the CLI on a real Core: the editor shows the Core's typed validation errors and cannot save an invalid definition; enabling a write-capable definition shows the exact capability list and hash and enabling is impossible without approving it; the test run executes in the dry-run posture and its report lists the denied effects while no file changed and no effect receipt exists; run history shows skipped, failed and succeeded with their typed reasons; pause stops the next slot; a repository definition is shown with its hash and revision; the origin badge appears in the agent list; every operation done in the desktop is reproducible from the CLI with the same result. | A dialog that enables without showing the capabilities fails; a test run that performs a protected effect fails; a skipped run with no reason shown fails; keyboard-only operation of the editor and dialog is required; a renderer that validates locally and disagrees with the Core fails the single-source check. |
| QUAL-PX-087 | REQ-PX-087 | extensions-hooks | Real Core with fixture packages and catalogs: a package using every component kind parses and `InspectExtension` lists each capability in words; a manifest declaring its own server trust, a `..` path, an absolute path, a scheme, a symlink escaping the package, a symlinked manifest, an oversized file or an oversized package is refused with the typed reason; a catalog with a bad signature, an unpinned key, an entry whose source is a branch name instead of a SHA, or an unknown field that widens policy is refused; an imported foreign-format plugin lands quarantined with the migration report; the 291-row base and EPR pins are unaffected. | A package that smuggles a symlink to a protected file passes containment only if the suite fails; a catalog entry pinned to a mutable ref installs nothing; an import that activates a package fails; a manifest field that grants trust to an external server is rejected; mutation check: removing symlink resolution lets the escape through and fails the suite. |
| QUAL-PX-088 | REQ-PX-088 | extensions-hooks | Real Core, real Git and a local HTTPS release fixture: install from a SHA-pinned Git source and a SHA-256 release asset succeeds and loads quarantined or active by signature; a branch-named source, a digest mismatch, a bad signature and a malicious archive (symlink, collision, ZIP bomb, 5,001 entries, 129 MiB expanded) each fail before extraction with nothing left on disk; `SIGKILL` mid-install recovers to exactly the previous or the new whole version and the lock is released; an update that adds a hook or changes an external server's configuration hash needs approval and one that does not is applied; pinning holds a version; rollback restores the previous; uninstall leaves no hook, provider or cache; a repository's committed plugin list is inert until the user approves its hash; an organisation block denies an install and an organisation requirement is shown as required. | An install that runs a package script fails; a fetch following a branch fails; a Git hook in a cloned repository executing fails; an update that silently adds a hook fails; an archive entry escaping the install root fails; a half-installed version visible after a kill fails; mutation check: dropping the digest check lets a swapped asset install and fails the suite. |
| QUAL-PX-089 | REQ-PX-089 | extensions-hooks | Real Core, broker and sandbox: a plugin declaring a secret variable never exposes its value in any package file, log, event, prompt or screenshot (a planted token is searched for in every retained artifact); a derived schema marks token-named variables write-only; a skill body and a tool description containing instruction-shaped text and a fake system tag are neutralised, labelled and cannot change capabilities (events and capability set compared before and after); a stdio server process has no ambient environment secrets and cannot reach a host outside its declared egress; a hook returning an allow cannot override a deny; a second tenant cannot see the first tenant's cache. | A secret value in any retained artifact fails the redaction scan; text that changes the capability set fails the injection suite; a stdio server with the user's ambient environment fails; a hook that weakens a deny fails (QUAL-EV-0239); cross-tenant cache visibility fails; mutation check: removing the control-tag stripping lets the fake system tag through and fails the suite. |
| QUAL-PX-090 | REQ-PX-090 | external-tools | Real Core, the real MCP test server and a local OAuth server: editing a trusted server's command or URL makes it untrusted until re-approved against the new hash; an unannotated tool is treated as write and a read-annotated tool is allowed under a read-only policy; the OAuth flow completes with PKCE and dynamic registration, a second executor waits on the refresh lock, and no token appears in a log or event; a server demanding authentication mid-session becomes needs-auth, its tools are replaced by the authentication tool and the status note reaches the model; a non-retryable error does not loop. | Trust surviving a configuration edit fails; a write tool treated as read fails; a token in any retained artifact fails; concurrent refreshes against one grant fail; mutation check: binding trust to the name only lets an edited server run and fails the suite. |
| QUAL-PX-091 | REQ-PX-091 | effects-security | Real Core and a real worktree: an agent that writes a new skill succeeds and the skill is inert until the person enables it; an agent that writes a hook file, an external-server configuration, a permissions file, a trust marker or a file of an installed package is denied in every run mode including Run everything, and a symlink aimed at one is denied; a policy blocking a package denies its install and one requiring a package shows it required; the server allow-list matches a wildcard host and a CIDR range and refuses outside them; a policy change blocks a running server within one evaluation and the admin reason is shown; an import with a stale fingerprint is refused. | An agent write to a control file passing fails; a symlink to a control file passing fails; an authored skill that activates itself fails; a policy verdict missing treated as allow fails; mutation check: removing the control-file entries lets the hook write through and fails the suite. |
| QUAL-PX-092 | REQ-PX-092 | extensions-hooks | Real Core with real hook processes: three parallel hooks answering allow, ask and deny merge to deny; exit code 2 blocks with the reason shown; a permission-event hook that times out or crashes fails closed and a non-permission hook that does so fails open only when configured; a project hook in an untrusted workspace does not run and a hook configuration reached through a symlink is refused; the log lists each run with its fields; a hook cannot widen a capability the Kernel denied; a 2 s session-end hook completes before exit. | A merge that lets allow override deny fails; a permission-event hook failing open by default fails; an untrusted project hook executing fails; a hook output carrying a fake system tag reaching the model unneutralised fails; a hook granting an effect the Kernel denies fails; mutation check: replacing deny-over-allow with last-wins fails the merge test. |
| QUAL-PX-093 | REQ-PX-093 | desktop | Playwright on the packaged app and a real Core with a local catalog: the tabs appear in order with their view options and empty states; install from the catalog shows the exact trust disclosure (publisher, source, signature state, capabilities in words, added servers and hooks, variables, size) before anything is active and an unsigned package installs quarantined with its digest-trust action; an update that widens capabilities blocks on approval; a malformed package shows its typed load failure with copyable details; a project plugin list needs the user's hash approval; the hook execution log lists a real hook run; every action is reproducible from the CLI; keyboard-only operation of the surface and its dialogs; axe reports no violations. | Browsing or importing activating anything fails; an install that skips the disclosure fails; a trust action that does not name the exact digest fails; a renderer that verifies signatures or parses packages fails the dependency scan; a catalog entry rendering with a rating fails; colour-only trust state fails axe. |
| QUAL-PX-094 | REQ-PX-094 | desktop | Playwright and screenshot-free geometry reads on the packaged app on each supported macOS version: the window controls sit inside the agent-list strip without overlapping content at 100, 125 and 150 percent zoom; every control in the bar is keyboard reachable and clicking it does not start a drag while an empty bar area drags the window; with Reduce transparency set no surface is translucent and with the opt-in on and it off the look differs only as designed; closing and reopening restores the window on the same display and a saved window on a removed display opens on a visible screen; the diff of `main.ts` shows only window options and state handling and the hashes of the preload, the CSP definition and the channel inventory are unchanged. | A change to webPreferences, the CSP, the preload or a channel fails the hash check; a window that restores off-screen fails; a drag region swallowing a control fails; translucency on by default fails; text over a translucent surface below its contrast ratio fails; a zoom outside the clamp fails. |
| QUAL-PX-095 | REQ-PX-095 | desktop | Packaged app on a real macOS machine with a real Core: each menu item fires the same command as its shortcut and none is enabled without its precondition; the badge equals the Core's attention count after a Core event and a restart; the tray variants render in light and dark; a `modbit://` install link for a valid package opens the confirmation and installs nothing until confirmed while a link with an invalid name, an unknown route or a smuggled path is refused and logged; a second launch hands its link to the running instance; an approval link focuses the card and approves nothing; a native folder dialog selection is refused by the Core when it fails trust; a notification click lands on the right task. | A deep link that installs, approves or changes state without the in-app confirmation fails; an unknown route reaching a handler fails; a badge counting routine progress fails; a menu accelerator that differs from the registry fails; a dialog path used before Core validation fails; a second instance opening a second window on the same profile fails the single-instance check. |
| QUAL-PX-096 | REQ-PX-096 | desktop | Packaged build: the fuse reader reports every required value and fails when one is flipped in a test copy of the binary; a modified ASAR archive is refused at launch; the app session denies a permission request from the app window and from a child frame; a web-view attach attempt and a navigation of the app window to another origin are denied; the CSP test fails if `connect-src` changes; a new IPC channel without a schema or sender check fails the inventory test; a synthetic slow Core call does not raise main-thread lag above 50 ms at p95 and the operating system's not-responding dialog does not appear; a hung renderer is recovered by the watchdog with the same session generation and offset; the full desktop E2E suite passes with the fuses set. | A fuse left at its default fails CI; an unsigned ASAR change running fails; a permission granted to the app session fails; a channel missing from the inventory fails; a blocking Core call on the main thread fails the lag budget; a relaxation of a fuse without a recorded justification fails review. |
| QUAL-PX-097 | REQ-PX-097 | desktop | Packaged signed build and a staged update on a real macOS machine: with a run active the update waits and the state says why; at idle it applies and the new version starts with the Core's recovery counts; a tampered update package is refused before apply with the typed reason; a kill during apply leaves exactly the old or the new whole version; rollback restores the previous version and its recovery state; with a pending approval the update does not apply; an organisation pause holds updates; states shown match the updater's own reports. | An update applying during an active run fails; a tampered package applying fails; a partial version visible after a kill fails; a state claiming applied before verification fails; an update during a protected effect fails; mutation check: skipping signature verification lets the tampered package apply and fails the suite. |
| QUAL-PX-098 | REQ-PX-098 | governance | CI against the real pinned version: the support check reads the pinned major and its published support status and passes; a test copy of the configuration pinning an unsupported major fails the build; a candidate upgrade runs the full qualification gate and its evidence bundle lists every suite with run ids; skipping one suite fails the gate; the build manifest and SBOM list the exact Electron and Chromium versions of the packaged binary (read from the binary). | A floating version range fails; an unsupported major shipping fails; an upgrade that skips a suite passing fails; a manifest that disagrees with the packaged binary fails; a missed security-update window not blocking the candidate fails. |
| QUAL-PX-099 | REQ-PX-099 | terminal | Real execd with a real PTY and the real Core: a client that never acknowledges stops receiving pushed chunks after the fixed window while broker and Core memory stay inside a stated bound over a 200 MiB output (resident size sampled), then catches up by cursor with no gap and no duplicate; a resize changes the dimensions the child reads with `stty size` and is recorded; the agent answers an interactive prompt through `shell.input` and reads the reply through `shell.attach` from a cursor; a second task naming the first task's shell id is refused SESSION_NOT_OWNED for input and attach; `SIGKILL` of the Core mid-attach leaves the shell running and the restarted Core reattaches at the stored cursor; a planted secret typed through `shell.input` appears in no event, log or artifact. | A broker that buffers for a stalled client grows past the bound and fails the suite; input to a shell owned by another task, to a finished shell or above the size bound is refused with a typed reason and writes nothing; a mutation that removes the ownership check or the window bound must fail the suite. |
| QUAL-PX-100 | REQ-PX-100 | desktop | Real Core, the real CLI binary, the real VS Code extension host of PX-002 and the live gateway: a task started with `--mode ask` is refused a write by the Kernel; `--mode plan` records a plan and writes nothing until accepted; `--objective intelligence` is recorded in the routing decision and visible from the desktop; a pin the organisation policy forbids is refused with the typed reason on the CLI and in the IDE and changes no state; a mode set in the CLI shows in the IDE adapter after a Core restart. | A client-supplied posture field or a client-side refusal fails the suite (the Core must be the only decider); a mode change that is shown but not enforced fails; a mutation that makes the CLI skip the mode field must fail the ask-mode write test. |
| QUAL-PX-101 | REQ-PX-101 | core-runtime | Real Core, real tool processes and the scripted and live providers: pause during a streaming turn and during a running shell command, observe the park at the boundary and the process reconciled; `SIGKILL` the Core while paused, restart, observe the paused task with the same checkpoint and no lost event; resume continues exactly once and completes; a duplicate pause or resume with the same command id acts once; a pause with a pending approval leaves the approval pending and answerable; a second client resumes with a stale lease generation and is refused. | A pause that kills the in-flight tool silently or loses the partial result fails; a restart that comes back running or failed fails; a stale-generation resume must be refused and change nothing; a mutation that drops the lease fence from resume must fail the stale-resume test. |
| QUAL-PX-102 | REQ-PX-102 | durability | Real Core, a real worktree and the checkpoint object store: forty turns create forty checkpoints, three are named; the collector thins the finished task by the policy and the named, fork-parent and latest checkpoints survive; every surviving checkpoint restores to hashes equal to the recorded state; `SIGKILL` mid-collection followed by a restart leaves no broken chain and the next run completes the work; a second collector run is refused while the lease is held; the report counts equal the filesystem change. | A collector that removes a baseline still needed by a delta chain or a forked parent fails; a name collision, a restore of a collected checkpoint without a typed error, or a collection while a restore is in flight fails; a mutation that ignores the fork reference must fail the fork-survival test. |
| QUAL-PX-103 | REQ-PX-103 | workspace-git | Real Core, a real worktree and the live gateway: one task writes through `change.apply`, through a shell redirect and through a test runner that rewrites a file; each hunk names the right tool call; a person rejects one hunk mid-run and the file shows exactly that hunk reverted by hash while the agent continues and answers the typed notice; a decision on a hunk whose revision moved is refused stale; `SIGKILL` between the revert and the notice recovers to one consistent outcome; an unattributable hunk (a write by a process outside any tool call) is shown as unattributed and blocks auto-accept. | A hunk attributed to the wrong call, an unattributed hunk accepted automatically, a revert that touches another hunk or a stale decision applied fails the suite; a mutation that skips attribution for shell writes must fail the attribution test. |
| QUAL-PX-104 | REQ-PX-104 | desktop | Playwright on the packaged app with a real Core, a real worktree and the live gateway: open Changes while the task is running, waiting for an approval, failed and completed; reject one hunk mid-run and observe the file hash change and the agent continue; the chip jumps to the producing call; a stale hunk shows the Core's reason and a refresh that works; a renderer kill and restart restores the panel from the Core; axe-core reports no violation on the panel and every control works by keyboard. | A control enabled for a hunk the Core would refuse, a decision computed in the renderer, or a panel that lies about staleness fails; a mutation that drops the revision from the decision call must fail the stale test. |
| QUAL-PX-105 | REQ-PX-105 | skills | Real Core, a real user-authored skill on disk and the scripted provider capturing the request body: before trust the skill is listed but absent from the request and `skill.load` refuses it; after `skill trust name@hash` the index line appears and `skill.load` returns the body; editing one byte makes it untrusted again; thirty skills exceed the budget and the request carries the ordered degradation with the count and stays under the byte cap; a skill with `paths` appears only after a matching file is opened; an imported skill stays quarantined until trusted; a System scope prohibition overrides a user trust; a hostile skill body with fake system tags is labelled and cannot widen tool access. | Trusting by environment flag, a trusted skill that survives a content change, an index over budget, a skill body reaching the model while untrusted, or a user trust that overrides a System prohibition fails; a mutation that removes the hash binding must fail the one-byte test. |
| QUAL-PX-106 | REQ-PX-106 | desktop | Playwright on the packaged app with a real Core and fixture skills, hooks and an extension with an MCP server: each list shows the Core's scope, trust and provenance; trusting a skill by hash changes the CLI state too and the next request; revoking it removes it from the request; a hook disabled here does not fire on the next event; an imported skill appears quarantined and cannot be invoked until trusted; no control exists to browse or install from a catalog; axe-core and keyboard operation pass. | A control that trusts without the hash, a state shown differently from the Core, or any install-from-network affordance fails the suite; a mutation that trusts by name only must fail the hash test. |
| QUAL-PX-107 | REQ-PX-107 | skills | Real Core and the scripted provider capturing the provider request: Modbit's own AGENTS.md rules appear in a run's request in a trusted checkout of this repository and are absent in the same checkout marked untrusted (the Inspector lists them as not loaded with the reason); a nearer-directory file takes precedence over the root file for a touched path; a file above the cap is truncated with the marker and the request stays under the aggregate cap; a planted line in AGENTS.md asking to approve everything or to call a tool changes no policy and no tool projection; editing the file between turns changes the next request and the recorded hash. | Instructions loaded in an untrusted workspace, any capability or policy change caused by file text, an over-cap request or a missing provenance record fails; a mutation that skips the trust check must fail the untrusted test. |
| QUAL-PX-108 | REQ-PX-108 | context-engine | Real Core, real repository fixtures (ts-webapp, python-service, rust-cli) and the scripted provider: the first request already carries a pack whose entries name the goal-relevant files with revision and reasons, without any model tool call; a write by the task invalidates the touched fragment and the next request drops or re-hydrates it; the pack stays inside the budget across fifty turns; after a Core restart the ledger rebuilds and the pack is consistent; a goal with no matches yields an empty pack with a recorded reason; measured on the existing corpus, tasks started with the pack reach the first correct file read no later than without it. | A pack over budget, a stale fragment shown as current, a missing provenance or a pack that suppresses model-initiated retrieval fails; a mutation that skips invalidation after a write must fail the stale test. |
| QUAL-PX-109 | REQ-PX-109 | durability | Real Core and a live compatible gateway: a run driven to 85 percent of the model window compacts through the model summarizer, resumes and completes its task; the summary cites only real events and files (a fabricated citation in a scripted summarizer is rejected and the extractive path is used with the typed reason); failing the summarizer by timeout, 429 and a malformed answer each yields the extractive fallback and the run still completes; a Core kill during compaction and a fork during compaction behave as in M4.2; the thresholds differ for a small-window and a large-window model; the model fetches an earlier tool result through the transcript pointer. | A summary that cites a missing event, a fallback that is not taken on summarizer failure, a stale epoch result applied, or a hostile tool result promoted into a trusted fact fails the suite; a mutation that skips validation must fail the fabricated-citation test. |
| QUAL-PX-110 | REQ-PX-110 | context-engine | Real Core and fixture repositories (ts-webapp, python-service, rust-cli) with a hand-labelled set of callers, implementors and impacted files: for each of at least thirty labelled queries (including an interface change and a renamed function) the returned non-test dependents match the labels with recall and precision at or above the thresholds recorded in the qualification, ambiguous and unresolved edges are labelled as such, impact feeds test selection and the selected tests include the dependent's tests; an edit invalidates the changed file's edges incrementally and the result is correct at the new revision; a result that hits the expansion cap says it is partial. | An impact result missing a labelled dependent while reporting complete, an edge without a confidence class or revision, or a graph that survives a revision change fails the suite; a mutation that drops implementor edges must fail the interface-change query. |
| QUAL-PX-111 | REQ-PX-111 | context-engine | Real Core and a large real repository fixture: after one full build, a restarted Core serves the first query without rebuilding (build counter zero) and the results equal those of a fresh build; a single-file edit refreshes below the recorded 103 ms baseline at the stated tolerance and a hundred-file change refreshes proportionally; a corrupted index file (a flipped byte) is detected, discarded and rebuilt with a reason and the answers stay correct; indexed exact search returns byte-identical results to the scan on a corpus of two hundred queries and is faster at the stated margin; a SIGKILL during an index write leaves either the previous whole index or a detected-corrupt one that is rebuilt. | A restart that rebuilds, an index trusted after corruption, results that differ between the indexed and the scan path, or a stale hit after an edit fails the suite; a mutation that skips the checksum must fail the corruption test. |
| QUAL-PX-112 | REQ-PX-112 | context-engine | Real Core, the admitted model file verified by digest and the retrieval benchmark harness: a swapped model file with a wrong digest is refused at load; embeddings rebuild only for changed chunks after an edit and fully after a model change; on the existing corpus plus the cases of PX-137 the learned embedder with fusion does not reduce recall at 5 below the hybrid baseline and the rerank option's effect is reported with confidence intervals either way; a missing model degrades to the lexical and hash path with a recorded reason and the answers stay correct; memory stays under the cap on the large repository fixture. | An unverified or unadmitted model loaded, a network fetch at run time, a recall drop hidden by the report, or a missing-model crash fails the suite; a mutation that skips the digest check must fail the swapped-file test. |
| QUAL-PX-113 | REQ-PX-113 | memory | Real Core and the scripted provider capturing requests: a memory promoted in session one appears in session two's request with its id and provenance and the Inspector lists it; forgetting it removes it from the next request and from the rebuilt store after a Core restart; promote and forget appear in the event log and replay to the same store; a memory with an expired time to live is not injected; Agent and Space scoped memories obey precedence; a hostile memory text asking for tool access changes no projection; the segment stays inside its budget with fifty memories. | A memory injected without provenance, a forget that leaves residue after restart, a mutation outside the event log, or a memory that widens authority fails the suite; a mutation that writes the store directly must fail the replay test. |
| QUAL-PX-114 | REQ-PX-114 | procedural-runtime | Real Core, the scripted provider capturing request bodies and a live compatible gateway: in `exec_only` mode the request carries only the listed schemas and the measured schema bytes are below the budget and recorded on the projection event; delegating adds `agent.*` for that turn and removes it after; the budget forces the stated degradation on a registry with many extension tools; tool search finds a deferred tool and calling it without kernel authorisation is refused; the live paired trial over at least thirty tasks reports accuracy, tokens and tool calls for both arms with intervals and a verdict that is recorded even if the procedural arm is not better. | A deferred tool callable without the Kernel, a projection above the budget with no record, an eager default changed without the trial, or a trial report that omits a failed arm fails the suite; a mutation that ignores the budget must fail the byte-bound test. |
| QUAL-PX-115 | REQ-PX-115 | external-tools | Real Core and the real MCP test server extended to five hundred tools: the first request carries no external schemas and stays within the schema budget; `external.describe` of three named tools adds exactly those schemas; a tool added through `list_changed` becomes discoverable without a restart; an untrusted server's tool is describable but a call is refused by the Kernel; an idle server is stopped gracefully and, when it ignores the signal, killed after the grace period, and the next call reconnects; the benchmark reports request bytes, tokens and success for lazy versus eager discovery on the same tasks. | A schema leaked before describe, a call authorised by discovery, a stale catalog after list_changed, a leaked server process after shutdown, or a benchmark on host tools only fails the suite; a mutation that sends schemas eagerly must fail the budget test. |
| QUAL-PX-116 | REQ-PX-116 | core-runtime | Real Core, real child processes and the live gateway: a parent with a cost cap spawns three children whose requests exceed the remainder and the third is clamped and the parent's total never exceeds the cap; a child that exhausts its wall clock ends BUDGET_EXHAUSTED and its reservation returns; child spend appears in the parent's accounting; `max_children` refuses the extra spawn with a typed reason; with `agent.spawn` forbidden by policy no child is admitted; a child read outside its `read_scope` is refused by the Kernel; `SIGKILL` between reservation and spawn reconciles to either both or neither. | A parent total above its cap, a leaked reservation after a crash, a read outside scope that succeeds, or a spawn past the limit fails the suite; a mutation that skips the clamp must fail the cap test. |
| QUAL-PX-117 | REQ-PX-117 | extensions-hooks | Real Core with real hook processes: each new event fires with its payload at its real lifecycle point in a fixture task; a command hook's `additional_context` appears in the next provider request labelled with its provenance and never widens the tool projection; a hook returning a fake system tag or an instruction to approve everything changes no policy; context over the budget is dropped with a typed reason; a prompt-type hook returning deny blocks a protected effect and a timeout on a permission event fails closed; a project hook in an untrusted workspace does not run; the hook log records each result. | Hook text that changes tools, permissions or approvals, an unlabelled injected context, a prompt-type hook that fails open on a permission event, or a hook in an untrusted workspace that runs fails the suite; a mutation that skips the labelling must fail the provenance test. |
| QUAL-PX-118 | REQ-PX-118 | workspace-git | Real Git, the real Core and the live gateway: a task created with `worktree` isolation edits files, runs shell commands and tests, and the user's checkout hash stays identical throughout while the worktree changes; a tool path escaping the worktree through `..` and through a symlink is refused; the reaper of PX-065 removes an old terminal worktree and spares the running one; an unwritable worktree root fails the start with the typed reason and nothing is written to the checkout; killing the Core mid-run and restarting resumes in the same worktree. | Any write to the user's checkout by an isolated task, a silent fallback to the checkout, an escape through a symlink or a reaped running worktree fails the suite; a mutation that skips the root binding for shell commands must fail the checkout-hash test. |
| QUAL-PX-119 | REQ-PX-119 | workspace-git | Real Git and the real Core with two real child branches that conflict on one file: `git.merge.prepare` reports the conflict with per-file evidence; the resolution is verified by a real test run and the merge commits; a resolution that fails verification does not count and the abort leaves the parent byte-identical (hashes); `SIGKILL` mid-merge resumes or aborts to a whole state; a parent `task.complete` with an unmerged child is refused with the typed reason and succeeds after integration or a recorded discard; every merge effect has a receipt naming the intent hash and both revisions. | A completion with an unintegrated child, a merge commit that bypasses verification, a half-merged tree after a crash or a merge without an approval and receipt fails the suite; a mutation that drops the completion gate must fail the unmerged-child test. |
| QUAL-PX-120 | REQ-PX-120 | browser | Real packaged Electron host and the real Core against local fixture servers on loopback, a private-range address (a second loopback alias) and a fake metadata endpoint, plus a rebinding resolver fixture: navigation to 169.254.169.254, to a private address and to loopback is refused with the typed code; a public page that redirects to a private address is refused at the hop; a name that resolves publicly at check time and privately at connect time is refused; a sub-frame and a link click to a local target are refused; the same destinations succeed when the task policy names them; the model cannot widen the policy through a tool argument; the refusal is journaled. | Any request reaching a denied address, a check made only on the hostname, a policy widened by model input or a redirect that escapes the check fails the suite; a mutation that checks the hostname instead of the connected address must fail the rebinding test. |
| QUAL-PX-121 | REQ-PX-121 | browser | Real Chromium through the packaged host and the real Core on a fixture app with a console error, a failing request, a long page, a lazy list and a slow element: `browser.console` and `browser.network` return the error and the failed request with secrets redacted and the untrusted label; `browser.capture` returns a bounded viewport image; `browser.scroll` reaches an off-screen element and `browser.wait` resolves on the element and times out with a typed reason; a click whose host dies mid-action latches UNKNOWN, refuses the next action and clears only after a fresh observation, and the receipt says UNKNOWN; refusals carry the escalation field; a model-supplied script cannot reach any of these tools without the Kernel. | A blind retry of an unknown-outcome action, an unredacted secret in console or network output, an unbounded read or a refusal without the escalation field fails the suite; a mutation that clears the latch without a new observation must fail the latch test. |
| QUAL-PX-122 | REQ-PX-122 | browser | Real Chromium through the packaged host and the real Core on a fixture app with dynamic updates, an iframe and a cross-origin iframe: an update appears as a delta within the stated latency without any model call; `since_fingerprint` returns exactly the changes since that point and a stale fingerprint returns a full snapshot with the reason; frame content is present with its frame and origin; an element keeps its reference after a Core restart and after a re-render; a delta requested after a compaction epoch uses the fingerprint or reports why not; a page that mutates thousands of nodes per second stays within the bounded notice rate and the observer cannot be disabled by page script. | A delta that omits a real change, a stale fingerprint answered with a wrong delta, an unbounded notice stream or an identity change across a restart fails the suite; a mutation that drops frame traversal must fail the iframe test. |
| QUAL-PX-123 | REQ-PX-123 | browser | Real Chromium through the packaged host and the real Core on fixture sites (a login, a multi-step checkout, a search list, a dialog-heavy admin page): page kinds are classified correctly on a labelled set; `fill_form` completes a login with a credential handle and no plaintext in any request, event or log; a derived action succeeds with its postcondition verified where a raw click sequence on the same page fails (a dynamic form that needs the field order); an action on a link to another origin or on a destructive dialog button is raised to the stricter approval class; an unverifiable derived action is reported unverified; page text asking the agent to submit the form is treated as data. | A secret in an argument or log, a derived action reported successful without its postcondition, a risk class lower than the control and destination warrant or page text obeyed as instruction fails the suite; a mutation that skips destination-based risk must fail the cross-origin test. |
| QUAL-PX-124 | REQ-PX-124 | browser | Real Chromium through the packaged host, the real Core and a fixture page declaring two tools (one annotated read-only, one unannotated): both appear as untrusted page proposals bound to their origin; calling the read-only tool succeeds through the Kernel with a receipt and provenance; the unannotated tool is treated as a write and asks; a second origin in an iframe cannot call or shadow the first origin's tool; navigating away removes the tools and a stale call is refused; a result containing instruction text is scanned and labelled and changes no policy; a page declaring a tool named like a host tool cannot replace it; the call records the rung and the reason for any fall. | A page tool callable without the Kernel, a tool reachable from another origin or after navigation, a name collision that shadows a host tool or a result obeyed as instruction fails the suite; a mutation that drops the origin binding must fail the cross-origin test. |
| QUAL-PX-125 | REQ-PX-125 | external-tools | Real GitHub with the owner's test repository: `forge.pr.read` returns a real pull request, `forge.pr.diff` returns its diff and pages it by ranges, a planted instruction in the pull request body is labelled untrusted and changes no policy; a status comment is posted only after approval of its exact body and appears on the real pull request; a comment containing a planted token is sent with the token redacted; with the token missing the tools answer a typed unavailable; a 403 and a secondary rate limit produce typed waits. The wire-faithful fake still runs for the edge cases and does not close the row. | A comment posted without approval, a token in any argument, event or log, a pull request body obeyed as instruction or a claim of success without the real-GitHub run fails the suite; a mutation that skips redaction must fail the planted-token test. |
| QUAL-PX-126 | REQ-PX-126 | sandbox-cloud | Real staging Cloud API and worker with signed GitHub-shaped deliveries (and the real GitHub of PX-125 for one end-to-end event): a failing check_suite for a task's pull request becomes `ci_evidence` on that task and appears in the verification evidence; a passing one does not override a failing local check; a review comment becomes steering input labelled untrusted; a replayed delivery, a wrong signature, a stale timestamp, an unknown repository and a wrong tenant are each rejected and audited; a restart of the control plane between two deliveries loses and duplicates nothing. | An unsigned or replayed delivery creating evidence, CI evidence overriding the gate, a cross-tenant mapping or a duplicate effect fails the suite; a mutation that skips the nonce check must fail the replay test. |
| QUAL-PX-127 | REQ-PX-127 | desktop | Real GitHub test repository, the real Core and the packaged app and CLI: an issue becomes a task, the task opens a pull request, a real workflow run's result appears in Review and in `modbit task evidence` with its provenance, a failing CI shows as failing evidence and does not mark the task accepted, a review comment appears as untrusted steering and a follow-up turn answers it; a renderer restart and a Core restart keep the evidence; the retained run ids and the pull request URL are the evidence. | CI green shown as acceptance, evidence without provenance, a comment rendered as trusted or a claim without the real run fails the suite; a mutation that maps a failing check to success must fail the failing-CI test. |
| QUAL-PX-128 | REQ-PX-128 | desktop | Real staging Cloud API with a real cloud worker and sandbox, the real CLI and the packaged desktop: a local task is handed off after the confirmation that lists what leaves the machine, runs in the sandbox, raises an approval that is answered from the desktop and then from the CLI (the second answer is refused as already decided), and returns with its delta and evidence to continue locally and complete; a policy that forbids handoff refuses it with the typed reason and nothing leaves; a lost network during watch resumes by cursor; a secret planted in the workspace is not in the handoff bundle. | Secret values in the bundle, a handoff without the confirmation, a double-applied approval, a cloud task with a local-only tool or a return that loses the delta fails the suite; a mutation that skips the policy check must fail the forbidden-handoff test. |
| QUAL-PX-129 | REQ-PX-129 | sandbox-cloud | Real staging Cloud API with Postgres and a real local OIDC provider fixture: the code-with-PKCE flow signs in and yields a tenant-scoped token; a replayed code, a wrong state or nonce, a verifier mismatch and an expired token are each rejected; provisioning creates a tenant and a principal and a disabled principal is refused at the next call; a bundle signed by the organisation key is accepted and served, one signed by another key, one with an old generation and one tampered are rejected and the worker keeps the last good bundle; an unverifiable bundle at the client fails closed; no token appears in any log. | A token accepted after tampering or replay, a cross-tenant read, a bundle accepted without a valid signature or generation, or a secret in a log fails the suite; a mutation that skips the generation check must fail the rollback test. |
| QUAL-PX-130 | REQ-PX-130 | effects-security | Real Core, a real provider call on the compatible gateway, the real MCP test server, real Chromium fill and the staging cloud: each of the five consumers obtains its credential through the one broker interface (a counter at the interface equals the number of uses); a planted token is searched for in every retained artifact (events, logs, prompts, screenshots, crash dumps, process arguments and environment of child processes) and is absent; an expired, revoked or over-cap handle is refused; rotation takes effect on the next use; the architecture lint fails a fixture that reads a credential outside the interface. | A secret value in any artifact, a handle usable after expiry or revocation, a consumer that bypasses the broker or two broker implementations fails the suite; a mutation that logs a resolved value must fail the artifact search. |
| QUAL-PX-131 | REQ-PX-131 | effects-security | Real Core and the scripted provider: a policy tightened mid-round is not applied to that round's remaining calls and is applied from the next round, with both epochs on the decisions and receipts; an emergency stop and a revocation of the approving principal still take effect immediately; a kill and restart mid-round replays the same snapshot; a forged receipt whose epoch differs from its decision fails the chain verification; a skill revoked mid-round leaves that round's projection unchanged and is absent next round. | A half-applied policy inside a round, a decision or receipt without an epoch, a replay that yields a different snapshot or an emergency stop deferred to the boundary fails the suite; a mutation that computes the snapshot per call must fail the boundary test. |
| QUAL-PX-132 | REQ-PX-132 | terminal | Real execd and Core with real dev servers (a Node HTTP server, a Python server and a Rust server) started by the agent's shell: the Core reports each port, the owning shell and the ready transition without any model call, and the agent's next tool result carries it; killing the server reports gone; a server that listens but returns 500 is reported unhealthy; a listener belonging to an unrelated process on the machine is not attributed to the task; a probe never leaves loopback; a Core restart reattaches and re-observes; no policy changes because a server was detected. | A port attributed to the wrong process, a probe off loopback, a policy widened by detection or a stale ready state after the server died fails the suite; a mutation that scans all system sockets must fail the unrelated-listener test. |
| QUAL-PX-133 | REQ-PX-133 | eval-bench | Real Core, a scripted-provider cascade and critique over a fixture task set with a real verification command and a live compatible gateway for one run: each leg of a cascade yields exactly one sample of the right kind with the right attribution and priced cost; a failed first leg followed by a successful second leg does not credit the first; a cancelled and a retried leg are accounted; replaying the events yields the same snapshot digest; a duplicate emission after restart adds nothing; the stats snapshot version changes only through the existing immutable-snapshot path and the router's decision records are byte-identical with and without this row for a fixed snapshot (proving no routing change). | A missing or double-counted leg, cost that omits a failed or uncertain leg, an altered routing decision for a fixed snapshot or any write outside the statistics store fails the suite; a mutation that credits the final leg with the whole request must fail the attribution test. |
| QUAL-PX-134 | REQ-PX-134 | model-gateway | Real Core and the real CLI with a signed registry bundle and an operator key: sign, verify and activate a good bundle and see the new digest in routing diagnostics; a tampered bundle, a bundle signed by an unknown key and a revoked key are each refused and the active bundle is unchanged; a version older than the active is refused unless explicitly rolled back; `SIGKILL` during activation leaves exactly one whole active bundle; rollback restores the previous digest; the key file is never printed or logged. | An unverifiable bundle activated, a non-atomic swap after a kill, a key in output or a rollback that loses the previous bundle fails the suite; a mutation that skips signature verification on activate must fail the tamper test. |
| QUAL-PX-135 | REQ-PX-135 | eval-bench | Real Core, real fixtures for each Tier A language with a real verification command, hidden oracle labels and a live compatible gateway for the candidate-producing runs: the corpora reach the per-slice minima; the generated adversarial checks reject the seeded overfit, off-by-one, test-weakening and vacuous-pass candidates that the previous gate accepted; the report lists false-accept, false-reject and risk false-negative rates with intervals per language; oracle labels are absent from every gate and model input (searched in the retained requests); rerunning on the same snapshot reproduces the report digest; no gate state in the graph changes. | An oracle label visible to the gate, a rate reported without its interval or sample count, a threshold changed by this row or a gate attested without the owner's profile fails the suite; a mutation that exposes labels to the gate must fail the leak search. |
| QUAL-PX-136 | REQ-PX-136 | eval-bench | Real Core, a signed registry with two real bindings, the live providers and the real verification commands: all three arms run the same pinned task set (at least thirty tasks) and the report lists per-arm verified success, cost with every leg, latency, escalation and review rates and gate false accepts and rejects with Wilson intervals; run ids, model versions, registry digest and statistics snapshot version are recorded; a rerun of the analysis on the retained data reproduces the report digest; the default route is still DIRECT afterwards; no release gate node changes state. | A report that omits a failed arm or a leg's cost, a verdict without intervals, a changed default route or an attested gate fails the suite; a mutation that drops failed legs from cost must fail the accounting check. |
| QUAL-PX-137 | REQ-PX-137 | eval-bench | The real harness against the pinned repositories with the real baseline tool and the live compatible gateway: at least fifty cases are scored by the oracle for both profiles on the same model; the report lists accuracy, input tokens, tool calls and time per profile with intervals and the baseline column; cold and incremental index times at 100 MB are recorded; rerunning the scoring on retained transcripts reproduces the report digest; a profile whose binary is missing fails the run instead of being skipped. | A baseline column missing or silently skipped, a case set below fifty, a result without an interval or a treatment that was forced to use retrieval fails the suite; a mutation that skips the baseline binary must fail the run. |
| QUAL-PX-138 | REQ-PX-138 | eval-bench | Real Core, forced compaction on at least twenty long fixture runs and the live compatible gateway: each run is scored for task success and for recall on questions whose answers are derived from the event log; the report compares extractive, structured and uncompacted arms with intervals and tokens saved; a deliberately lossy summarizer (a test double that drops files) scores worse on recall, proving the metric can fail; the report digest reproduces from retained data. | A metric that cannot fail (a lossy summarizer scoring the same), an oracle derived from the summary under test or a report without intervals fails the suite. |
| QUAL-PX-139 | REQ-PX-139 | observability | Real Core, real children and a real local OpenTelemetry collector: a parent with two children exports one trace whose spans show the parent-child links and each span's cost, and the parent's cost equals its own plus its children's from the accounting record; a planted secret in a prompt and a file body appear in no span attribute; killing the collector mid-run does not delay or fail the task and the loss is counted; with export off nothing is sent; after a Core restart the health endpoint reports the previous state with its age; the exported cost equals the EPR accounting record for the same requests. | A secret or prompt body in a span, a task failed or delayed by an exporter fault, exported cost that differs from the accounting record or health reported as healthy with no observation fails the suite; a mutation that exports prompt text must fail the planted-secret test. |

## Task cards

<a id="px-000"></a>

## PX-000 — Headless CLI thin client for the task lifecycle

- **Requirement:** REQ-PX-000; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** M1.3, M2.7.
- **Scope and acceptance:** `modbit` command-line client speaking the authenticated local SurfaceProtocol: create task, steer, pause, resume, cancel, respond to questions, approve or deny effects, subscribe to events from a cursor, read OutputRefs and artifacts, print JSON lines and human text, and return documented exit codes. It is a thin client: no orchestration, context, memory, Git state, recovery, policy or tool execution lives in it, and it holds no provider credentials. Headless use in scripts and CI is the design target; the desktop app and the CLI are interchangeable clients of one Core.
- **Production wiring:** reuse the desktop's protocol client library (`packages/surface-protocol`) or a Rust twin of it; authenticate exactly as Electron main does; no new Core command types beyond those in `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`.
- **Real qualification:** QUAL-PX-000 / PX-E2E-000.
- **Failure and negative proof:** as in QUAL-PX-000; additionally cancellation from the CLI reconciles in-flight tools through the ordinary cancellation domains.
- **Evidence:** build digest, Core and CLI revisions, event cursor ranges, effect receipt id, exit codes, and the dependency check output under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-001"></a>

## PX-001 — Thin-client conformance contract for external development-environment adapters

- **Requirement:** REQ-PX-001; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** PX-000.
- **Scope and acceptance:** Define and implement the conformance suite every SurfaceProtocol client must pass: command idempotency, cursor replay, intent-hash-bound approvals, attention and verdict rendering, rejection paths, and a static proof that the client contains no provider, filesystem, Git or policy code path. Publish the contract as `packages/ide-adapter-core` and pass it with the CLI and desktop client (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`).
- **Production wiring:** reuse `packages/surface-protocol`; the suite runs against a real local Core in CI; no new Core commands.
- **Real qualification:** QUAL-PX-001 / PX-E2E-001.
- **Failure and negative proof:** as in QUAL-PX-001.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-002"></a>

## PX-002 — VS Code adapter as a conformant thin client

- **Requirement:** REQ-PX-002; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** PX-001, M6.6.
- **Scope and acceptance:** First IDE adapter built on `packages/ide-adapter-core`: task panel to create, steer, approve and review; forwards language-service diagnostics with revision provenance; shows results and evidence; owns nothing. Ships in Beta only after the CLI and desktop have proven the protocol in Alpha.
- **Production wiring:** real VS Code extension host; SurfaceProtocol over the same authenticated local transport; no editor features are replaced.
- **Real qualification:** QUAL-PX-002 / PX-E2E-002.
- **Failure and negative proof:** as in QUAL-PX-002; JetBrains follows only after this adapter and the shared library pass conformance (REQ-PX-003).
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-004"></a>

## PX-004 — Provenance-bound external diagnostics intake

- **Requirement:** REQ-PX-004; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M6 / BETA; **prerequisites:** PX-001, M3.4.
- **Scope and acceptance:** `SubmitExternalDiagnostics` normalizes IDE or linter diagnostics into the canonical diagnostics records with provenance external_ide, bound to workspace and file revision; used as context and verification-plan input, never as a substitute for a mandatory verification step.
- **Production wiring:** extend `crates/diagnostics` normalization and the SurfaceProtocol command set in doc 30; revision mismatch discards.
- **Real qualification:** QUAL-PX-004 / PX-E2E-004.
- **Failure and negative proof:** as in QUAL-PX-004.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-005"></a>

## PX-005 — Constrained inline patch through ChangeTransaction

- **Requirement:** REQ-PX-005; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M6 / BETA; **prerequisites:** M2.1, M2.9.
- **Scope and acceptance:** Allow a user to apply a small direct edit from the review surface or CLI exclusively through the canonical ChangeTransaction on the Workspace File Service: revision precondition, path policy after symlink resolution, provenance user_direct_edit, one event and revision advance, stale CodeReferences invalidated. No editor buffer model anywhere (`20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`).
- **Production wiring:** reuse `change.propose`/`change.apply` semantics via an `ApplyUserPatch` command; no new write path.
- **Real qualification:** QUAL-PX-005 / PX-E2E-005.
- **Failure and negative proof:** as in QUAL-PX-005.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-006"></a>

## PX-006 — GitHub forge adapter behind the External Tool Hub

- **Requirement:** REQ-PX-006; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** external-tools / M6 / BETA; **prerequisites:** M2.5, M6.5.
- **Scope and acceptance:** Implement the `forge.*` tool family for GitHub (`forge.issue.read`, `forge.pr.create`, `forge.pr.update`, `forge.pr.comments.read`, `forge.ci.status`) behind the existing External Tool Hub with capability leases, effect classes, receipts, idempotency keys and broker-supplied tokens (`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`).
- **Production wiring:** `crates/tools (external.*)` adapter; egress policy `api.github.com:443`; secret handle `github-token`; other forges later behind the same family.
- **Real qualification:** QUAL-PX-006 / PX-E2E-006.
- **Failure and negative proof:** as in QUAL-PX-006.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-007"></a>

## PX-007 — Pull request create and update from a reviewed result

- **Requirement:** REQ-PX-007; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M6 / BETA; **prerequisites:** PX-006, M2.2.
- **Scope and acceptance:** From Review or CLI, push the dedicated branch and open or update a PR as protected external effects with approval and receipts, bound to the exact candidate revision, with the evidence summary in the PR body.
- **Production wiring:** Change Engine merge/export path plus `forge.pr.*`; branch push through the typed Git operation; no hidden shell.
- **Real qualification:** QUAL-PX-007 / PX-E2E-007.
- **Failure and negative proof:** as in QUAL-PX-007.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-008"></a>

## PX-008 — Review-comment steering as untrusted durable input

- **Requirement:** REQ-PX-008; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M9 / RELEASE_ZERO; **prerequisites:** PX-007, M6.5.
- **Scope and acceptance:** Allowed reviewers' PR comments addressed to Modbit become durable TaskSteered events with provenance forge_review_comment and untrusted tagging through the ordinary steering path; identity allowlist from organization policy.
- **Production wiring:** `forge.pr.comments.read` polling or webhook (PX-011) feeding the existing Steer command; no new input channel type.
- **Real qualification:** QUAL-PX-008 / PX-E2E-008.
- **Failure and negative proof:** as in QUAL-PX-008.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-009"></a>

## PX-009 — CI-result ingestion as provenance-bearing evidence

- **Requirement:** REQ-PX-009; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M9 / RELEASE_ZERO; **prerequisites:** PX-007, M2.8.
- **Scope and acceptance:** Ingest check-run and workflow results for the task branch as evidence artifacts with provider, run id, commit and OutputRef logs; show them in Review with provenance ci; they inform the verification plan and never constitute an automatic qualification PASS.
- **Production wiring:** `forge.ci.status` into the Verification Engine's external-evidence class; qualification tests still execute in Modbit.
- **Real qualification:** QUAL-PX-009 / PX-E2E-009.
- **Failure and negative proof:** as in QUAL-PX-009.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-010"></a>

## PX-010 — Issue-to-task intake from CLI and desktop

- **Requirement:** REQ-PX-010; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** PX-006, PX-000.
- **Scope and acceptance:** `modbit task from-issue <url>` and the New Task screen pull a GitHub issue through `forge.issue.read` and create a canonical task with the issue as untrusted context and provenance forge_issue.
- **Production wiring:** thin clients call `CreateTask` with origin metadata; no second task model.
- **Real qualification:** QUAL-PX-010 / PX-E2E-010.
- **Failure and negative proof:** as in QUAL-PX-010.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-011"></a>

## PX-011 — Forge webhook intake through the Cloud API

- **Requirement:** REQ-PX-011; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** sandbox-cloud / M8 / RELEASE_ZERO; **prerequisites:** PX-010, M8.1.
- **Scope and acceptance:** A GitHub App webhook to the Cloud API creates the same canonical task for the tenant after signature verification and policy checks (`24_CLOUD_CONTROL_PLANE_AND_SYNC.md`); PR events feed PX-008/PX-009 when configured.
- **Production wiring:** Cloud API endpoint issuing the canonical CreateTask/Steer commands; replay protection and audit.
- **Real qualification:** QUAL-PX-011 / PX-E2E-011.
- **Failure and negative proof:** as in QUAL-PX-011.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-014"></a>

## PX-014 — Understanding and planning contract

- **Requirement:** REQ-PX-014; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** M2.7.
- **Scope and acceptance:** Clarification policy and plan artifact per `28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §1: a plan recorded through plan.update before the first write, typed questions only when the change set, verification or a protected effect depends on the answer, plan revisions as events.
- **Production wiring:** WorkGraph plan state and `plan.get/update` tools; Core rejects writes before a plan; `PlanRecorded`/`PlanRevised` events.
- **Real qualification:** QUAL-PX-014 / PX-E2E-014.
- **Failure and negative proof:** as in QUAL-PX-014.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-015"></a>

## PX-015 — Retrieval-before-edit contract

- **Requirement:** REQ-PX-015; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** M3.7, PX-014.
- **Scope and acceptance:** No edit without a retrieval record for the file at the current workspace revision; symbol edits require definition and reference retrieval; Context Ledger records retrieval and later use (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §2).
- **Production wiring:** Context Engine retrieval records joined to ChangeTransaction preconditions; policy decision on violation.
- **Real qualification:** QUAL-PX-015 / PX-E2E-015.
- **Failure and negative proof:** as in QUAL-PX-015.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-016"></a>

## PX-016 — Change strategy contract

- **Requirement:** REQ-PX-016; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M2 / ALPHA; **prerequisites:** M2.1, M2.2, PX-014.
- **Scope and acceptance:** Small revision-bound diffs, one concern per ChangeTransaction where possible, tests first where a harness exists, explicit plan revision on scope change, generated files only via generators (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §3).
- **Production wiring:** Change Engine transaction metadata carries plan linkage; scope check against the plan's expected set.
- **Real qualification:** QUAL-PX-016 / PX-E2E-016.
- **Failure and negative proof:** as in QUAL-PX-016.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-017"></a>

## PX-017 — Verification plan derivation contract

- **Requirement:** REQ-PX-017; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8, PX-014.
- **Scope and acceptance:** Verification plan derived from task type, changed files, repository configuration and agent proposals, recorded before the first run; agent may add, never remove, mandatory checks (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §4).
- **Production wiring:** Verification Engine plan materialization already specified in doc 33, now recorded as an event and bound to the plan.
- **Real qualification:** QUAL-PX-017 / PX-E2E-017.
- **Failure and negative proof:** as in QUAL-PX-017.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-018"></a>

## PX-018 — Bounded evidence-driven repair loop with RepairAttempt records

- **Requirement:** REQ-PX-018; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** PX-017.
- **Scope and acceptance:** RepairAttempt record per attempt (failure signature, hypothesis, evidence, intended fix, change ref, verification result, outcome); equivalent repeated hypotheses escalate through compiled slots or Needs Attention; policy bounds per failure signature and per task; WORSENED attempts reverted or justified (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §5).
- **Production wiring:** Core-runtime loop control with `RepairAttemptRecorded`/`RepairEscalated` events and `repair_attempts` persistence; escalation via EPR slots when present.
- **Real qualification:** QUAL-PX-018 / PX-E2E-018.
- **Failure and negative proof:** as in QUAL-PX-018.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-019"></a>

## PX-019 — Self-review and completion contract

- **Requirement:** REQ-PX-019; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** PX-018.
- **Scope and acceptance:** SelfReview step with structured findings before any completion proposal; unresolved findings block the proposal; the Acceptance Gate decides completion (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §6).
- **Production wiring:** Verification Engine consumes the SelfReview; `SelfReviewRecorded` event; completion proposal gated.
- **Real qualification:** QUAL-PX-019 / PX-E2E-019.
- **Failure and negative proof:** as in QUAL-PX-019.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-020"></a>

## PX-020 — Fixed M2 competence baseline on public and internal suites

- **Requirement:** REQ-PX-020; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / release:** eval-bench / M3 / BETA; **prerequisites:** M2.9, PX-019, M3.9.
- **Scope and acceptance:** Run the public benchmark and the internal competence suite under the frozen protocol of `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md` on the real M2 product with the direct configuration and record the immutable baseline bundle.
- **Production wiring:** Eval Harness under `benchmarks/agent-engineering`; fixture repositories from doc 50; baseline artifacts in the object store by digest.
- **Real qualification:** QUAL-PX-020 / PX-E2E-020.
- **Failure and negative proof:** as in QUAL-PX-020.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-021"></a>

## PX-021 — Competence regression gate and targets after baseline

- **Requirement:** REQ-PX-021; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** PX-020, M10.4.
- **Scope and acceptance:** Targets by Decision Record per metric and tier only after the baseline; release-candidate competence gate with intervals; cost-improving routing changes that regress competence fail (doc 63).
- **Production wiring:** Release gate tooling and Policy Lab reporting; thresholds versioned with the release profile.
- **Real qualification:** QUAL-PX-021 / PX-E2E-021.
- **Failure and negative proof:** as in QUAL-PX-021.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-022"></a>

## PX-022 — Onboarding to a first useful task within five minutes

- **Requirement:** REQ-PX-022; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** M1.4, M2.9.
- **Scope and acceptance:** Welcome, provider setup through the OS keychain with a live test call, explicit scoped repository trust with background indexing, starter-task gallery and free-text goal, first Review; median under five minutes to ReadyForReview with real evidence as defined in `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`.
- **Production wiring:** renderer `app-shell/` and `settings/` modules; provider test call through Core; templates from the detected stack; no client-side state beyond acknowledgements.
- **Real qualification:** QUAL-PX-022 / PX-E2E-022.
- **Failure and negative proof:** as in QUAL-PX-022.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-023"></a>

## PX-023 — Screen flow and state completeness with notification model

- **Requirement:** REQ-PX-023; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** M6.6, PX-022.
- **Scope and acceptance:** Empty, loading, populated, error, degraded and recovery states for every screen per the matrix in `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`; error copy names cause, next action and evidence; notifications only for attention, completion and failure, coalesced per task, deep-linked, OS delivery opt-in with quiet hours.
- **Production wiring:** renderer state machines fed by Core events; OS notifications through Electron main; the same reasons exposed to CLI and IDE adapters.
- **Real qualification:** QUAL-PX-023 / PX-E2E-023.
- **Failure and negative proof:** as in QUAL-PX-023.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-024"></a>

## PX-024 — Keyboard model and accessibility conformance

- **Requirement:** REQ-PX-024; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** M6.6.
- **Scope and acceptance:** Global, list and review shortcuts; approval with confirmation for irreversible effects; focus retention; live regions; no color-only status; accessibility suite in packaged E2E (`39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`).
- **Production wiring:** renderer focus management and shortcut registry; Playwright accessibility checks.
- **Real qualification:** QUAL-PX-024 / PX-E2E-024.
- **Failure and negative proof:** as in QUAL-PX-024.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-025"></a>

## PX-025 — Interaction budgets enforced in packaged E2E

- **Requirement:** REQ-PX-025; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M10.4, PX-023.
- **Scope and acceptance:** Assert the interaction budgets table of `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md` in packaged E2E from Playwright traces and Core event timestamps; misses fail release-critical candidates.
- **Production wiring:** performance regression gates of M10.4 extended with UI budgets; results in the release evidence bundle.
- **Real qualification:** QUAL-PX-025 / PX-E2E-025.
- **Failure and negative proof:** as in QUAL-PX-025.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-026"></a>

## PX-026 — Alpha language baseline for TypeScript/JavaScript, Python and Rust

- **Requirement:** REQ-PX-026; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8.
- **Scope and acceptance:** Prove the three Alpha candidates at Tier C plus real compile and test evidence on the fixture stacks, with honest labels in every client (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** Verification Engine command evidence and the exact/BM25 paths of M2; labels via Core projections.
- **Real qualification:** QUAL-PX-026 / PX-E2E-026.
- **Failure and negative proof:** as in QUAL-PX-026.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-027"></a>

## PX-027 — Language tier conformance suites A, B and C

- **Requirement:** REQ-PX-027; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M3 / BETA; **prerequisites:** M3.3, M3.4, PX-026.
- **Scope and acceptance:** Implement the tier suites in `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md` over real fixture repositories and make tier entry a recorded pass; add the suites to `56_TOOL_CAPABILITY_CONFORMANCE.md`.
- **Production wiring:** Eval Harness plus verification fixtures; recorded promotion artifacts.
- **Real qualification:** QUAL-PX-027 / PX-E2E-027.
- **Failure and negative proof:** as in QUAL-PX-027.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-028"></a>

## PX-028 — Tier A conformance for TypeScript/JavaScript, Python and Rust

- **Requirement:** REQ-PX-028; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** PX-027.
- **Scope and acceptance:** Pass the Tier A suite for the three languages with real headless language services, incremental index latency within budget and competence baseline tasks passing (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** `crates/diagnostics` language-service adapters and `crates/retrieval` structural indexes of M3.
- **Real qualification:** QUAL-PX-028 / PX-E2E-028.
- **Failure and negative proof:** as in QUAL-PX-028.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-029"></a>

## PX-029 — Explicit degradation path for Tier C and Unsupported languages

- **Requirement:** REQ-PX-029; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** PX-027.
- **Scope and acceptance:** Visible, explicit degradation: text retrieval, configured-command verification only, plan states the limitation, language state shown in every client, per-task opt-in with provenance for edits to Unsupported languages (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** Context Engine language classification joined to the plan and verification plan; projection field on tasks.
- **Real qualification:** QUAL-PX-029 / PX-E2E-029.
- **Failure and negative proof:** as in QUAL-PX-029.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-030"></a>

## PX-030 — Platform CI compatibility matrix from M0, never release-grade by itself

- **Requirement:** REQ-PX-030; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** governance / M10 / RELEASE_ZERO (rescheduled from M0 / ALPHA by DR-M0-005: the matrix runs from M0.1, the task completes when every named conformance suite exists); **prerequisites:** M0.1.
- **Scope and acceptance:** CI builds and platform conformance suites on macOS, Windows and Linux from M0; results labeled CI_COMPATIBLE; no documentation or client text presents CI compatibility as support (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** CI matrix in the monorepo of M0.1 plus the platform conformance suites; label enforcement in docs and clients.
- **Real qualification:** QUAL-PX-030 / PX-E2E-030.
- **Failure and negative proof:** as in QUAL-PX-030.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-031"></a>

## PX-031 — Desktop platform release promotion by platform-specific E2E

- **Requirement:** REQ-PX-031; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M10.3, PX-030.
- **Scope and acceptance:** macOS reaches RELEASE_GRADE through the packaged desktop E2E catalog and the applicable Release Zero subset; other platforms are promoted only by their own packaged E2E evidence and a Decision Record (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** release gate tooling records platform promotions with evidence bundles.
- **Real qualification:** QUAL-PX-031 / PX-E2E-031.
- **Failure and negative proof:** as in QUAL-PX-031.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-032"></a>

## PX-032 — Pre-change verification baseline and regression attribution

- **Requirement:** REQ-PX-032; **related preserved requirements:** REQ-EV-0018, REQ-EV-0068, REQ-EV-0070.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8, PX-017.
- **Scope and acceptance:** BASELINE run before the first write with `KNOWN_FAILING` labelling and a recorded `VerificationBaseline`; regression attribution between BASELINE and COMPLETION at the candidate revision after the flake protocol; declared expected changes shown as such (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §1).
- **Production wiring:** Verification Engine stages in `crates/verification`; `verification_runs`/`check_results` tables; `VerificationBaselineRecorded` and `RegressionAttributed` events; acceptance blocked on an undeclared REGRESSION.
- **Real qualification:** QUAL-PX-032 / PX-E2E-032.
- **Failure and negative proof:** as in QUAL-PX-032.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-033"></a>

## PX-033 — Normalized test reports and failing-check identity

- **Requirement:** REQ-PX-033; **related preserved requirements:** REQ-EV-0107, REQ-EV-0068, REQ-EV-0017.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8, IMP-EV-0107.
- **Scope and acceptance:** `test.run` returns a `TestReport` with `CheckResult`s from runner adapters (vitest/jest/mocha, pytest, cargo, go, configured command) with `STRUCTURED` or `HEURISTIC` confidence; `failure_signature` derived only from normalized results; bounded failure evidence to the model with the raw log retained (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §2). IMP-EV-0107 (bounded failure evidence as next-round context) is scheduled in M2 for this reason.
- **Production wiring:** Runner adapters in `crates/verification`; OutputRef spill from doc 33 backpressure; Context Pack failure section; `UNKNOWN` is `INDETERMINATE`.
- **Real qualification:** QUAL-PX-033 / PX-E2E-033.
- **Failure and negative proof:** as in QUAL-PX-033.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-034"></a>

## PX-034 — Staged test targeting and mandatory completion run

- **Requirement:** REQ-PX-034; **related preserved requirements:** REQ-EV-0068, REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** PX-032, PX-033.
- **Scope and acceptance:** TARGETED runs ordered failed-first, task-named, changed-file-mapped (M2 heuristics), then build/typecheck/lint, each within `targeted_run_budget` and never supporting completion; a COMPLETION run with the full configured suite plus every mandatory check at the final candidate revision before the Acceptance Gate; a later ChangeTransaction invalidates it (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §1, §8).
- **Production wiring:** Verification Engine stage scheduler; harness completion handshake in doc 14; `completion_scope` policy with the limitation recorded in the plan and Review.
- **Real qualification:** QUAL-PX-034 / PX-E2E-034.
- **Failure and negative proof:** as in QUAL-PX-034.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-035"></a>

## PX-035 — Impact-based test selection from the evidence graph

- **Requirement:** REQ-PX-035; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** M3.6, PX-034.
- **Scope and acceptance:** Select impacted checks from dependency, symbol-reference, test-link and Git co-change evidence within a bounded depth plus task-named checks; measure precision and recall against full-suite ground truth on fixtures; until COMPLETE the plan states that targeting is heuristic (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §6).
- **Production wiring:** `crates/retrieval` evidence graph of M3.6 queried by the Verification Engine; results reported with the retrieval benchmarks of doc 53.
- **Real qualification:** QUAL-PX-035 / PX-E2E-035.
- **Failure and negative proof:** as in QUAL-PX-035.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-036"></a>

## PX-036 — Flaky-check detection, rerun protocol and quarantine

- **Requirement:** REQ-PX-036; **related preserved requirements:** REQ-EV-0068, REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** PX-033.
- **Scope and acceptance:** One isolated rerun of failed checks at the same revision and environment digest; `FLAKY` labelling with both run references; exclusion from `failure_signature`; task- and revision-scoped quarantine with expiry, never a global skip list; `mandatory_flaky_passes` rule; pre-quarantine of checks flaky at BASELINE; the agent can request but never label (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §3).
- **Production wiring:** Verification Engine RERUN stage; `flaky_checks` table; `FlakyCheckQuarantined` event; Review and SelfReview projections.
- **Real qualification:** QUAL-PX-036 / PX-E2E-036.
- **Failure and negative proof:** as in QUAL-PX-036.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-037"></a>

## PX-037 — Diff invariants including test-integrity detection

- **Requirement:** REQ-PX-037; **related preserved requirements:** REQ-EV-0071, REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M2 / ALPHA; **prerequisites:** M2.1, M2.2, PX-016.
- **Scope and acceptance:** Invariants DI-1..DI-9 evaluated per ChangeTransaction by the Change Engine and over the whole diff at COMPLETION by the Verification Engine; `DENY` class rejects the transaction, `FLAG` class blocks the SelfReview; DI-3 forbids deleting, skipping, weakening or rewriting acceptance-named or baseline-failing tests, with AST detection for Tier A and conservative text rules elsewhere (`64_VERIFICATION_EXECUTION_CONTRACTS.md` §4).
- **Production wiring:** `crates/workspace` Change Engine precondition hooks; `crates/verification` whole-diff evaluation; `DiffInvariantViolated` events; PatchPolicyGate for DI-5.
- **Real qualification:** QUAL-PX-037 / PX-E2E-037.
- **Failure and negative proof:** as in QUAL-PX-037.
- **Evidence:** build digest, Core revision, verification run ids, TestReport and OutputRef digests, event cursor ranges, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-038"></a>

## PX-038 — Scope policy with bounded expansion and mandatory questions

- **Requirement:** REQ-PX-038; **related preserved requirements:** REQ-EV-0144, REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** PX-014, PX-016.
- **Scope and acceptance:** The first `PlanRecorded` freezes the original write set; `PlanRevised` carries a scope delta; Core counts out-of-plan files and revisions against `ScopePolicy` bounds (Alpha defaults 2 and 2) and `always_ask_paths`; beyond a bound the next out-of-scope write waits for a typed question (continue, split, stop); headless resolution fails closed by default; the scope metric is measured against the original plan (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §3).
- **Production wiring:** WorkGraph plan state with `plan_v1`; Change Engine DI-1 check; `ScopeExpansionRecorded` event; Question Service.
- **Real qualification:** QUAL-PX-038 / PX-E2E-038.
- **Failure and negative proof:** as in QUAL-PX-038.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-039"></a>

## PX-039 — Repair policy defaults, reproduction-first and no-progress detection

- **Requirement:** REQ-PX-039; **related preserved requirements:** REQ-EV-0107, REQ-EV-0099.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** PX-018, PX-033.
- **Scope and acceptance:** Versioned `RepairPolicy` (Alpha defaults: 2 per signature, 6 per task, 1 WORSENED before escalation, reproduction required for reported failures, 3 no-progress turns); reproduction-first enforced before fix transactions; identical `change_fingerprint` rejected as an equivalent attempt; oscillation escalates; `NoProgressDetected` emitted and escalated; `FLAKY`/`UNKNOWN` never form a signature (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §5).
- **Production wiring:** Core-runtime repair loop control; `repair_attempts.change_fingerprint` with its unique key; escalation through compiled slots or Needs Attention; policy bundle carries the defaults.
- **Real qualification:** QUAL-PX-039 / PX-E2E-039.
- **Failure and negative proof:** as in QUAL-PX-039.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-040"></a>

## PX-040 — Agent harness contracts

- **Requirement:** REQ-PX-040; **related preserved requirements:** REQ-EV-0099, REQ-EV-0107, REQ-EV-0017.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** M2.7, PX-039.
- **Scope and acceptance:** The eleven harness contracts of `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`: turn shape with command failure as evidence, bounded observations with declared truncation and pageable OutputRefs, structured failure channel, `harness_state` in the Context Pack, per-task budgets with `HarnessBudgetExhausted`, environment readiness, candidate-revision binding, revert and checkpoint points, steering at safe boundaries, headless resolution, and the completion handshake.
- **Production wiring:** Core-runtime turn loop and Prompt/Context compilers; doc 33 backpressure ceilings; Question Service headless policy; Verification Engine completion handshake.
- **Real qualification:** QUAL-PX-040 / PX-E2E-040.
- **Failure and negative proof:** as in QUAL-PX-040.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-041"></a>

## PX-041 — Streaming assistant-text delta events with a completion record and cursor replay

- **Requirement:** REQ-PX-041; **related preserved requirements:** REQ-EV-0010, REQ-EV-0108, REQ-EV-0192.
- **Owner / milestone / release:** domain-events / M10 / RELEASE_ZERO; **prerequisites:** M1.3, M2.6, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; release-critical (protocol and schema addition on the canonical event store; secret redaction on a new output path).
- **Specification requirements (doc 65, with verification tag):** AFW-C02 [LIVE], AFW-K01 [UNVERIFIED].
- **Scope and acceptance:** For every model invocation of a run the Core emits an ordered, bounded stream of text-delta events: assistant text, and reasoning summaries only where the provider returns them and policy allows. Deltas are coalesced to at most one event per 50 ms or 2 KiB per stream id and carry stream id, a monotonic sequence, and the run, turn and step ids. An `AssistantMessageCompleted` event closes the stream and carries the full text as an OutputRef digest and a content hash. Deltas pass secret redaction and injection-provenance tagging before they are appended; they are ordinary events (offset, cursor replay, multiple clients) with a retention class that lets the store collapse them into the completed record once the message is complete. A turn that fails, is cancelled or loses its provider mid-stream ends the stream with a typed `AssistantMessageAborted` carrying the abort source (user interrupt, runtime, provider) and never presents partial text as a complete message. Recovery closes any open stream as aborted-by-recovery and the loop re-requests under existing turn semantics; no stream resumes mid-token. No client can reach a provider.
- **Production wiring:** `services/modbit-core/src/runtime.rs` (today the loop consumes `ModelEvent::MessageDelta` and only accumulates it; `ReasoningDelta` is dropped); additive messages in `crates/protocol/proto/modbit/v1/surface.proto` and domain events in `crates/domain`, regenerated for Rust and TypeScript (M0.3); the event-store append and projection of `domain-events`; `crates/providers` unchanged; docs 30 and 32 updated by the implementing task.
- **Real qualification:** QUAL-PX-041 / PX-E2E-041.
- **Failure and negative proof:** as in QUAL-PX-041.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-042"></a>

## PX-042 — Transcript rows and agent header projections served by the Core

- **Requirement:** REQ-PX-042; **related preserved requirements:** REQ-EV-0037, REQ-EV-0132, REQ-EV-0143, REQ-EV-0151.
- **Owner / milestone / release:** domain-events / M10 / RELEASE_ZERO; **prerequisites:** M4.1, PX-041.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; release-critical (protocol and schema addition; the attention status class is evidence semantics for the fleet).
- **Specification requirements (doc 65, with verification tag):** AFW-B02 [STATIC], AFW-B04 [LIVE], AFW-B10 [DISK], AFW-C01 [STATIC], AFW-C04 [LIVE], AFW-C05 [STATIC].
- **Scope and acceptance:** Two read projections over the canonical event log, both rebuildable from it and neither a second store. First, an agent header projection: id, workspace, title, subtitle, created and updated times, status class computed with the precedence of AFW-B02 (needs attention, failed, ready for review unseen then seen, running, waiting, completed, draft, archived), unread, pending approval, pending plan, context percent, files changed, lines added and removed, last checkpoint time, subagent and archived flags, and execution location. Second, a transcript row projection per task: user message, assistant message, work group, tool card, approval card, tail status, turn footer, time boundary and unread divider rows, each with render hints (renderable, groupable, has reasoning, duration, short plain text, edit line counts, status), served by cursor and page, with three density views (compact, balanced, detailed) that differ only in grouping. Archive and unread are Core events, reversible. A Core-side full-text index covers headers and conversation bodies, rebuilt from the log.
- **Production wiring:** `crates/protocol-state` and `crates/event-store` projections, additive `GetAgentHeaders`, `GetTranscript` and `SearchConversations` messages in `surface.proto`, `MarkRead` and `ArchiveTask` commands through the existing idempotent command path; the attention status class reuses `services/modbit-core/src/attention.rs`; doc 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-042 / PX-E2E-042.
- **Failure and negative proof:** as in QUAL-PX-042.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-043"></a>

## PX-043 — Terminal output stream and background-terminal registry for every client

- **Requirement:** REQ-PX-043; **related preserved requirements:** REQ-EV-0019, REQ-EV-0026, REQ-EV-0027, REQ-EV-0135.
- **Owner / milestone / release:** terminal / M10 / RELEASE_ZERO; **prerequisites:** M4.5, IMP-EV-0271, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; release-critical (protocol addition; user input into a terminal is a new effect path; execution and recovery semantics).
- **Specification requirements (doc 65, with verification tag):** AFW-E08 [LIVE], AFW-I03 [STATIC], AFW-I04 [STATIC].
- **Scope and acceptance:** The Core serves a terminal session to any client by cursor: ordered output chunks, the replay window, OutputRef ranges for older output, the session's state (running, exited with code, killed), and its owner (agent or user). A registry lists a task's background terminals with title, command, start time and elapsed time. A client can kill a background terminal; the kill is recorded, reconciles the process through the existing cancellation domains and emits a typed background-process-ended event that wakes the agent, never a simulated user message. User input is accepted only for a user-owned terminal whose policy allows it and is journaled with its owner; agent-owned terminals are read-only to clients. This is the owner that Release Zero's terminal-visibility step lacked.
- **Production wiring:** `crates/terminal` and `services/modbit-execd` (durable terminal and replay window already exist, IMP-EV-0271); additive `AttachTerminal`, `ListTerminals`, `KillTerminal` and `WriteTerminal` messages in `surface.proto`; the terminal cursor metadata of M4.5; the typed wake-up event in the runtime's input path.
- **Real qualification:** QUAL-PX-043 / PX-E2E-043.
- **Failure and negative proof:** as in QUAL-PX-043.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-044"></a>

## PX-044 — Design tokens, UI primitives, tray host and a model-free state gallery

- **Requirement:** REQ-PX-044; **related preserved requirements:** REQ-EV-0037.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M1.4, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; iteration (presentation only: no effect, persistence, policy, protocol or security behaviour changes; the packaged UI smoke suite on a real Core applies).
- **Specification requirements (doc 65, with verification tag):** AFW-A11 [LIVE], AFW-A12 [STATIC], AFW-A13 [LIVE], AFW-A14 [STATIC], AFW-C07 [LIVE], AFW-D04 [LIVE], AFW-H01 [LIVE], AFW-K02 [STATIC], AFW-K03 [UNVERIFIED], AFW-K05 [STATIC].
- **Scope and acceptance:** `packages/design-tokens` emits Modbit's tokens for dark and light from one typed function with identical key sets (AFW-A11), including the semantic ladder, spacing, radii, control heights, type scale with a Text size setting, motion and z-layers; a computed WCAG contrast test covers every text and control pair in both themes (AFW-A12) and the existing 9 inline variables are replaced. `packages/ui` provides accessible Button (variants), Menu, Dialog, Tray host (single active tray, scoped keybindings, persistence across navigation), Tooltip, Toggle, Segmented control, Popover, Toast and Icon (open-licence set with notices). A state gallery renders approval, generating, error, queue and usage states from fixtures without a model for design review only (AFW-K02). Dark, light and follow-system themes are a persisted preference; reduced motion and forced colours are honoured.
- **Production wiring:** `packages/design-tokens`, `packages/ui`, the renderer `index.html` inline style and `apps/desktop/src/renderer/index.tsx`, split into modules by screen (the 1,871-line file); no main-process, preload or CSP change; the license gate of doc 36 applies to the icon set.
- **Real qualification:** QUAL-PX-044 / PX-E2E-044.
- **Failure and negative proof:** as in QUAL-PX-044.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-045"></a>

## PX-045 — Agent-first shell: three regions, top bar, apps-panel host, status row, palette and shortcut registry

- **Requirement:** REQ-PX-045; **related preserved requirements:** REQ-EV-0037, REQ-EV-0143, REQ-EV-0151.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M6.6, PX-024, PX-044.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; iteration (presentation and local layout preference only; effects, policy and protocol are untouched; any change outside the renderer moves to PX-049).
- **Specification requirements (doc 65, with verification tag):** AFW-A01 [LIVE], AFW-A02 [LIVE], AFW-A03 [LIVE], AFW-A04 [LIVE], AFW-A06 [LIVE], AFW-A08 [STATIC], AFW-A09 [DISK], AFW-A10 [LIVE], AFW-A14 [STATIC], AFW-A15 [DISK], AFW-A16 [DISK], AFW-B11 [STATIC], AFW-I02 [LIVE], AFW-K01 [UNVERIFIED], AFW-K03 [UNVERIFIED], AFW-K04 [UNVERIFIED].
- **Scope and acceptance:** The shell of AFW-A01 to A10: agent list, centre conversation, collapsible apps panel with a persisted ratio, a 40 pt top bar, status row, single-pane mode below 448 pt with a 40 pt rail, per-task panel state, settings replacing the shell with the structure of AFW-A10. The Fleet becomes the attention view of the agent list (MOD-UX-001 holds: needs attention first) and the Home screen remains reachable. A command palette (primary K) searches agents, actions and settings and subsumes the type-ahead of doc 39. The shortcut registry holds the PX-024 bindings plus those of AFW-A16 behind a conflict test. Layout state lives in the profile as a local preference, never as task state. The window minimum is PX-049.
- **Production wiring:** `apps/desktop/src/renderer` layout and `app-shell/` modules of doc 32; existing `screens.ts` states extended for the shell; `keyboard.ts` registry; consumes `packages/ui` and the tokens; no new Core command.
- **Real qualification:** QUAL-PX-045 / PX-E2E-045.
- **Failure and negative proof:** as in QUAL-PX-045.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-046"></a>

## PX-046 — Agent list: status classes, unread, pins, archive with undo, filters, grouping and search

- **Requirement:** REQ-PX-046; **related preserved requirements:** REQ-EV-0037, REQ-EV-0143, REQ-EV-0151.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-042, PX-045.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; iteration (renders Core-computed classes; archive and pin are existing reversible Core commands from PX-042, so no new effect-bearing behaviour is added here).
- **Specification requirements (doc 65, with verification tag):** AFW-A05 [LIVE], AFW-B01 [LIVE], AFW-B02 [STATIC], AFW-B03 [LIVE], AFW-B04 [LIVE], AFW-B05 [DISK], AFW-B06 [DISK], AFW-B07 [STATIC], AFW-B08 [LIVE], AFW-B09 [LIVE], AFW-B10 [DISK], AFW-B11 [STATIC], AFW-K01 [UNVERIFIED], AFW-K04 [UNVERIFIED].
- **Scope and acceptance:** The list of AFW-B01 to B11: two-line rows, glyph plus text status, attention dot, unread dot, relative age, hover tooltip, hover and focus actions, pin with a hard cap, archive with an undo affordance of at least 8 s, selection moving to the nearest sibling, confirmation before archiving a running task, grouping and filter chips from the header projection, configurable row subtitle, search over headers and conversations, next and previous task shortcuts and a recent-task switcher. A new task defaults to the last local trusted workspace and shows its location as an explicit pill; cloud is never inherited.
- **Production wiring:** Renderer list components over `GetAgentHeaders`, `SearchConversations`, `MarkRead` and `ArchiveTask` of PX-042; preload functions added with main-side validation; no precedence logic in the renderer.
- **Real qualification:** QUAL-PX-046 / PX-E2E-046.
- **Failure and negative proof:** as in QUAL-PX-046.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-047"></a>

## PX-047 — Conversation surface: streaming render, tail status, step folding, scroll, density and failure presentation

- **Requirement:** REQ-PX-047; **related preserved requirements:** REQ-EV-0010, REQ-EV-0037.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-041, PX-042, PX-045.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; iteration (presentation of Core-projected rows and deltas; no effect, persistence, policy or protocol change in the renderer).
- **Specification requirements (doc 65, with verification tag):** AFW-A07 [LIVE], AFW-B04 [LIVE], AFW-C01 [STATIC], AFW-C02 [LIVE], AFW-C03 [LIVE], AFW-C04 [LIVE], AFW-C05 [STATIC], AFW-C06 [LIVE], AFW-C07 [LIVE], AFW-C08 [LIVE], AFW-C09 [LIVE], AFW-C10 [STATIC], AFW-C11 [UNVERIFIED], AFW-G07 [LIVE], AFW-K01 [UNVERIFIED], AFW-K04 [UNVERIFIED].
- **Scope and acceptance:** The transcript of AFW-C01 to C11: a virtualised row list with predicted heights that only grow while streaming, pinned-bottom scroll, the new-messages pill and scroll-to-bottom button, jump to previous and next user message, find in conversation over rows, the sticky latest user card, assistant text without a bubble, collapsed reasoning and code blocks, folded work groups (Worked for N s, Ran N commands), three densities, a tail-status line with shimmer and a stall ladder that raises the existing STALL attention item, a turn footer with timestamp, copy and fork, and a failed turn that restores the prompt and docks an error tray. Content is treated as untrusted: no raw HTML, links through main's allow-list, images through Core artifact reads.
- **Production wiring:** Renderer transcript components over `GetTranscript` and the delta events of PX-041 and PX-042; Playwright traces and Core event timestamps for budgets; no new Core command.
- **Real qualification:** QUAL-PX-047 / PX-E2E-047.
- **Failure and negative proof:** as in QUAL-PX-047.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-048"></a>

## PX-048 — Typed apps panel: Changes, Terminal, Browser, Files and Evidence with per-task state

- **Requirement:** REQ-PX-048; **related preserved requirements:** REQ-EV-0019, REQ-EV-0135, REQ-EV-0037.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M2.9, M7.6, PX-043, PX-045.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; release-critical (adds a user input path into terminals and apply-adjacent controls; mixed behaviour is release-critical).
- **Specification requirements (doc 65, with verification tag):** AFW-A09 [DISK], AFW-G05 [LIVE], AFW-G06 [LIVE], AFW-G08 [LIVE], AFW-I01 [LIVE], AFW-I02 [LIVE], AFW-I03 [STATIC].
- **Scope and acceptance:** The right panel of AFW-A09, A06 and I01 to I04: typed tabs (Changes with the existing revision-bound review, Terminal from the stream of PX-043, Browser with the existing control-lease badge and takeover, Files as the read-only trusted code view, Evidence), a plus menu with search and shortcut hints, a collapsed rail with counters, per-task persisted visibility, fullscreen and remembered tab per kind, and the changes toolbar (scope menu, per-file revert and stage, split Commit and push as protected effects through their approval). The terminal view attaches by cursor, shows the replay window, loads older output as OutputRef ranges, labels agent-owned terminals read-only, warns before closing a running one, and enables input only where the Core says the terminal is user-controlled. Release Zero's terminal-visibility step is satisfied by this view.
- **Production wiring:** Renderer apps host over the existing review, browser and code-view bridges and the new `AttachTerminal`, `ListTerminals`, `KillTerminal` and `WriteTerminal` functions of PX-043 through preload with main-side validation; commit and push go through `OpenPullRequest`-style protected effects, never a direct call.
- **Real qualification:** QUAL-PX-048 / PX-E2E-048.
- **Failure and negative proof:** as in QUAL-PX-048.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-049"></a>

## PX-049 — Window and quit lifecycle: minimum size, quit protection, startup recovery counts and hook completion

- **Requirement:** REQ-PX-049; **related preserved requirements:** REQ-EV-0242, REQ-EV-0139, REQ-EV-0127, REQ-EV-0258.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M1.5, IMP-EV-0139, PX-045.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 1; release-critical (Electron main constructor options (security-boundary adjacent), recovery presentation and lifecycle-hook ordering).
- **Specification requirements (doc 65, with verification tag):** AFW-A02 [LIVE], AFW-J11 [STATIC].
- **Scope and acceptance:** BrowserWindow constructor options for default size 1280 x 800 and minimum 900 x 600 only (AFW-A02); no change to webPreferences, the CSP, preload or IPC channels, and any frameless, hidden title bar or vibrancy change is specified by DR-PX-2026-10-03-012 (PX-094) and not here. Quit protection: closing the window or quitting with runs active asks first and offers to resume them on reopen; the Core keeps running or suspends them at a safe boundary per the existing durability rules and the choice is a Core input. On startup the recovery report (interrupted runs recovered, background children replayed) is shown as counts. Lifecycle hooks (session end) run to completion in the Core before the process exits, or the failure is shown visibly and journaled; a hook never fails silently because the host was already torn down (AFW-J11).
- **Production wiring:** `apps/desktop/src/main/main.ts` window options and `before-quit` handling; the Core's hook bus (IMP-EV-0139, IMP-EV-0242) and shutdown path in `services/modbit-core`; the recovery report already served by `GetRecoveryReport`; preload and CSP are byte-identical before and after.
- **Real qualification:** QUAL-PX-049 / PX-E2E-049.
- **Failure and negative proof:** as in QUAL-PX-049.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-050"></a>

## PX-050 — Durable input queue management and typed interrupt

- **Requirement:** REQ-PX-050; **related preserved requirements:** REQ-EV-0009, REQ-EV-0191, REQ-EV-0262.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M2.7, IMP-EV-0191, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (protocol addition on the Input Queue; execution, cancellation and recovery semantics).
- **Specification requirements (doc 65, with verification tag):** AFW-E01 [LIVE], AFW-E02 [LIVE], AFW-E03 [LIVE], AFW-E05 [STATIC], AFW-E06 [LIVE], AFW-E07 [LIVE].
- **Scope and acceptance:** The existing typed input dispatch (`QueueInput` STEER, COLLECT, FOLLOW_UP) gains management over what is queued: list a task's queued inputs in order with their mode, text, per-item mode and model selection and state; edit, delete and reorder a queued item before it is dispatched; send one now as a typed interrupt-and-replace. Send now cuts a live model stream at a safe boundary: effect-bearing tool calls in flight reconcile first through the existing cancellation domains, the stream ends as aborted with source user-interrupt and the partial text stays marked partial, then the new turn starts. An interrupt that arrives while an effect outcome is unknown does not start the new turn until reconciliation resolves it. A user interrupt and a runtime abort are distinct typed outcomes. Queued items are durable and drain in order after the current turn, survive renderer and Core restarts, and are fenced by the session lease. A setting selects what plain Enter does while running (queue, collect, steer, stop-and-send) and what Send now does; the Core applies it, clients only choose it.
- **Production wiring:** `crates/core-runtime` Input Queue and Input Gateway, additive `ListQueuedInputs`, `EditQueuedInput`, `RemoveQueuedInput`, `ReorderQueuedInput` and `SendQueuedInputNow` messages in `surface.proto`, the existing `QueueInput` and `InputQueued`, the abort outcome of PX-041; docs 14 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-050 / PX-E2E-050.
- **Failure and negative proof:** as in QUAL-PX-050.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-051"></a>

## PX-051 — Task mode as a typed Core field with an enforced capability posture

- **Requirement:** REQ-PX-051; **related preserved requirements:** REQ-EV-0218, REQ-EV-0116, REQ-EV-0222.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M6.3, PX-014, PX-039, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (a new typed field that selects a capability posture: permissions, policy and execution behaviour).
- **Specification requirements (doc 65, with verification tag):** AFW-D03 [LIVE], AFW-D05 [DISK], AFW-D06 [STATIC].
- **Scope and acceptance:** A task carries a typed mode: AGENT (default), PLAN, DEBUG, MULTITASK or ASK. The Core derives the posture and the Capability Kernel enforces it: ASK allows no write or execution effect; PLAN records and revises the plan through the existing plan contract and writes nothing until the user accepts the plan, which moves the task to AGENT with the plan attached; DEBUG makes reproduction before a fix mandatory under PX-039; MULTITASK enables subagent admission with capacity tickets under the existing AgentGraph. A mode changes between turns by a typed input; the posture applies from the next safe boundary; an in-flight effect is not retroactively denied. The agent may propose a mode switch as a typed question; the user's answer is the only way it takes effect, remembered consent is stored per transition by the Core, and an unanswered proposal expires to skipped after 15 s, never to accepted. The mode is the only thing a client sends; a client cannot supply a posture.
- **Production wiring:** `crates/core-runtime` task state and prompt posture, the Capability Kernel projection of `crates/tools` and `crates/policy` (task-scoped tool projection, MOD-TOOL-001), the Question Service, additive `mode` on `CreateTask` and a `SetTaskMode` message in `surface.proto`; `PlanRecorded`, `RepairAttempt` and AgentGraph contracts reused; docs 14, 28 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-051 / PX-E2E-051.
- **Failure and negative proof:** as in QUAL-PX-051.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-052"></a>

## PX-052 — ListSkills protocol command for the slash menu

- **Requirement:** REQ-PX-052; **related preserved requirements:** REQ-EV-0114, REQ-EV-0214, REQ-EV-0105, REQ-EV-0213.
- **Owner / milestone / release:** skills / M10 / RELEASE_ZERO; **prerequisites:** M5.5, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (protocol addition that exposes the skill inventory with provenance and trust to clients).
- **Specification requirements (doc 65, with verification tag):** AFW-D08 [LIVE].
- **Scope and acceptance:** A `ListSkills` read command returns the task-visible slash inventory as a typed union: skills (installed, imported, built in), commands, subagent profiles, actions and model switches. Each item has id, display name, one-line description, kind, scope (built in, user, project, imported), trust and provenance (source, hash, importer report), invocation (user only, model only or both, from the skill's metadata), where it applies (local or cloud), usage counters for ranking, and a sync-eligibility flag (local only or shareable). Order: built-ins, a divider, then imported entries alphabetically; ranking by recency and usage. The inventory is rebuilt when a skill is installed, removed, revoked or changes on disk. An untrusted or revoked skill is listed with its state and cannot be invoked. The command reads the registry; it adds no second registry.
- **Production wiring:** `crates/skills` registry and `apps/cli` (`skill list` exists), additive `ListSkills` and `SkillView` messages in `surface.proto`, usage counters in the profile store, registry refresh events; docs 26 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-052 / PX-E2E-052.
- **Failure and negative proof:** as in QUAL-PX-052.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-053"></a>

## PX-053 — SetExecutionPreference command wired to the existing routing owner

- **Requirement:** REQ-PX-053; **related preserved requirements:** REQ-EV-0031.
- **Owner / milestone / release:** model-gateway / M10 / RELEASE_ZERO; **prerequisites:** M5.5, EPR-005, M2.6, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (protocol addition at the boundary of EPR-pinned behaviour; policy-bound user intent).
- **Specification requirements (doc 65, with verification tag):** AFW-D13 [LIVE], AFW-D14 [DISK].
- **Scope and acceptance:** Wire the command already specified in docs 30 and 32 and absent from `surface.proto`: `SetExecutionPreference` records the user's objective profile (Cost, Balance, Intelligence or a permitted organisation profile) or an allowed manual model pin, with its command id, the expected Run generation and the user's intent only. The Core validates it against the organisation policy and the model registry, records it as an event and the existing router reads it at the next compile exactly as docs 27 and 38 define. A pin the policy forbids is refused with the typed reason (policy-blocked model) that PX-056 renders. This row changes no routing, quality-floor, gate, statistics or registry semantics: any need to change them stops the row and needs its own Decision Record (EPR-pinned behaviour).
- **Production wiring:** `crates/providers` profile and compile inputs read the preference; `services/modbit-core` command handling and the event; additive `SetExecutionPreference` and `ExecutionPreferenceSet` messages in `surface.proto`; `ListModels` (exists) and `GetRoutingDiagnostics` (docs 30) reads; docs 30 and 32 updated by the implementing task.
- **Real qualification:** QUAL-PX-053 / PX-E2E-053.
- **Failure and negative proof:** as in QUAL-PX-053.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-054"></a>

## PX-054 — Composer: placeholders, mode chips, add-context and @ menus, slash menu, attachments, history and drafts

- **Requirement:** REQ-PX-054; **related preserved requirements:** REQ-EV-0114, REQ-EV-0214.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-045, PX-051, PX-052.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (sends typed effect-bearing input and ingests files through the Core; attachments cross a security boundary).
- **Specification requirements (doc 65, with verification tag):** AFW-A07 [LIVE], AFW-B09 [LIVE], AFW-D01 [LIVE], AFW-D02 [LIVE], AFW-D03 [LIVE], AFW-D04 [LIVE], AFW-D07 [LIVE], AFW-D08 [LIVE], AFW-D09 [STATIC], AFW-D10 [STATIC], AFW-D11 [LIVE], AFW-D12 [LIVE], AFW-J10 [DISK], AFW-K03 [UNVERIFIED].
- **Scope and acceptance:** The composer of AFW-D01 to D12 and A07: a rounded container with an add-context button, mode chip, model chip with its variant, send and stop; context-specific placeholders in Modbit's words; suggestion chips below the empty composer; Shift+Tab cycling Plan, Debug, Multitask, Ask from the default with a click-through menu on the chip and an X to leave (wrap-around returns to the default); mode tints from tokens with icon and label; the @ menu of typed sources with chips and path-policy reasons; the slash menu from `ListSkills` with built-ins first, a divider, the detail card and ranking; attachments only through `IngestAttachment`; Alt+Up and Alt+Down history; a local unsent draft; Enter and the primary-modifier Enter alternate; the status row with an explicit execution-location pill and branch pill; composer width up to 840 pt.
- **Production wiring:** Renderer composer components over `CreateTask` with the mode field, `SetTaskMode`, `ListSkills`, `IngestAttachment` (the existing `attachFile` bridge), the context sources already served by the Core; no filesystem read in the renderer; the geometry targets of AFW-A07 measured at 1280 x 800.
- **Real qualification:** QUAL-PX-054 / PX-E2E-054.
- **Failure and negative proof:** as in QUAL-PX-054.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-055"></a>

## PX-055 — Run controls: queue tray, send-behaviour education, stop and edit-in-place, side question, background-terminals tray

- **Requirement:** REQ-PX-055; **related preserved requirements:** REQ-EV-0009, REQ-EV-0191, REQ-EV-0262.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-047, PX-050, PX-043, PX-054.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (user control of execution (interrupt, stop, send now) and the agent wake-up path).
- **Specification requirements (doc 65, with verification tag):** AFW-D06 [STATIC], AFW-D11 [LIVE], AFW-E01 [LIVE], AFW-E02 [LIVE], AFW-E04 [LIVE], AFW-E05 [STATIC], AFW-E06 [LIVE], AFW-E07 [LIVE], AFW-E08 [LIVE], AFW-E09 [STATIC], AFW-G08 [LIVE], AFW-H01 [LIVE].
- **Scope and acceptance:** The controls of AFW-E01 to E09 and D06: Enter queues by default; the queue tray with N queued, per-item Send now, Edit, Delete and overflow, collapse, keyboard operation and reorder; the first-use education tray explaining queue, collect and steer with truthful labels; the send-behaviour settings; Send now with a label that states the consequence; one Stop control that ends the turn within about 1 s and leaves the last user message editable in place with a Continue chip; the background-terminals chip and tray with timers and kill; the ephemeral side question through `AskSideQuestion`; the typed mode-switch proposal card (Skip, Switch) with the 15 s expiry shown. Chips that perform an effect go through their approval.
- **Production wiring:** Renderer tray components over PX-050 and PX-043 commands through preload; `AskSideQuestion` and the typed question service; the tray host of PX-044; no queue or interrupt logic in the renderer.
- **Real qualification:** QUAL-PX-055 / PX-E2E-055.
- **Failure and negative proof:** as in QUAL-PX-055.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-056"></a>

## PX-056 — Model chip and picker with objective profiles, variants and the policy-blocked tray

- **Requirement:** REQ-PX-056; **related preserved requirements:** REQ-EV-0031.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-053, PX-054, PX-055.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (selects execution policy inputs that the Core validates; a policy-blocked state must fail closed and visibly).
- **Specification requirements (doc 65, with verification tag):** AFW-D13 [LIVE], AFW-D14 [DISK], AFW-H02 [LIVE].
- **Scope and acceptance:** The chip and picker of AFW-D01, H02 and the picker facts of the research: the picker opens upward from the chip and holds a search field, the objective-profile row (Cost, Balance, Intelligence or a permitted organisation profile, Balance by default), model rows from `ListModels` with the variant (effort, speed, window) as part of each row's identity, a check on the current choice, a new badge, scrolling, and per-model parameters from the registry's precomputed variants. Manual pins appear only where policy allows. A policy-blocked choice fails before any token: the prompt is restored and a tray names the typed cause with two actions (switch to an allowed model, contact the policy owner); Enter shows the reason inline and keeps the draft. No billing or plan UI exists. Best-of-N, a multi-model comparison and per-feature model slots are not in scope.
- **Production wiring:** Renderer picker over `ListModels`, `SetExecutionPreference` (PX-053) and the typed error classes the Core already emits; the tray host; no model list or policy in the renderer.
- **Real qualification:** QUAL-PX-056 / PX-E2E-056.
- **Failure and negative proof:** as in QUAL-PX-056.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-057"></a>

## PX-057 — Run-mode presets and durable allowlist rules owned by the policy kernel

- **Requirement:** REQ-PX-057; **related preserved requirements:** REQ-EV-0194, REQ-EV-0088, REQ-EV-0031.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M2.5, M9.2, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (permissions and policy: who approves protected effects, durable rules and always-ask classes).
- **Specification requirements (doc 65, with verification tag):** AFW-F01 [LIVE], AFW-F06 [LIVE], AFW-F07 [LIVE], AFW-F08 [LIVE], AFW-F09 [LIVE], AFW-F10 [DISK], AFW-F11 [DISK], AFW-F12 [DISK], AFW-F13 [LIVE].
- **Scope and acceptance:** Run modes are Core policy presets over the existing Capability Kernel, never a widening of its envelope. Ask (default): every protected effect asks with an exact-intent approval. Allowlist: effects matching a durable rule run without asking. Allowlist with sandbox: additionally, effects fully contained in the sandbox (no network, writes confined to the task worktree) run without asking where policy marks them contained. Run everything: per session, never persisted across a restart, confirmed every time, and it auto-approves only in-envelope effects. In every mode the always-ask classes of AFW-F08 (write outside the workspace, network fetch or connection, any use of a secret, protected or configuration path, deletion, push, escalation beyond the declared capability) ask unless a narrowly scoped durable rule says otherwise. Durable rules are Core policy records: argv-prefix pattern from a ladder, repository scope, creator, time, optional expiry and a receipt; they never match shell operators, redirections, substitutions or pipelines and compound commands match by sub-command; they are listed and revocable. The tool call declares its escalation (none, network, all) and the Kernel approves the escalation. A classifier that approves effects is out of scope (a second policy engine, doc 81).
- **Production wiring:** `crates/policy` and `crates/effects` (approval and receipt chain), `services/modbit-core/src/config.rs` policy generations, additive `SetRunMode`, `ListAllowRules`, `AddAllowRule` and `RevokeAllowRule` messages in `surface.proto`; the approval aggregate and intent hash unchanged; docs 23 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-057 / PX-E2E-057.
- **Failure and negative proof:** as in QUAL-PX-057.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-058"></a>

## PX-058 — Docked approval stack: exact-intent card, deck, shortcuts and persistence

- **Requirement:** REQ-PX-058; **related preserved requirements:** REQ-EV-0194, REQ-EV-0088.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-045, PX-047, PX-057.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (renders and sends protected-effect approvals; the exact-intent binding must survive the new layout).
- **Specification requirements (doc 65, with verification tag):** AFW-B03 [LIVE], AFW-F01 [LIVE], AFW-F02 [LIVE], AFW-F03 [LIVE], AFW-F04 [LIVE], AFW-F05 [UNVERIFIED], AFW-F07 [LIVE], AFW-F09 [LIVE], AFW-F10 [DISK], AFW-F11 [DISK], AFW-F12 [DISK], AFW-F13 [LIVE], AFW-H01 [LIVE], AFW-K04 [UNVERIFIED].
- **Scope and acceptance:** The approval card of AFW-F01 to F13 as a docked tray at the transcript tail: humanised title and command name; the exact command with operators and substitutions visible; effect class; one-line exact-intent summary; the typed reason it asks; sandbox containment; scope and expiry; footer with the run-mode menu on the left and Skip, Always and Run. Run is bound to the intent hash and candidate revision, Skip is a typed denial returned to the model as 'skipped by the user' and distinct from a tool error, and there is no reject-with-instruction box. Enter runs, Shift+Enter is Always and Escape is Skip, acting only when the card has visible focus or the composer is empty; irreversible effects keep the confirmation of doc 39. The tail status says pending approval, the agent row shows the dot, and the card persists across task switches, a renderer restart and a Core restart and never expires to approved. With several pending, the oldest by Core sequence is surfaced with an N pending header and each decision applies to exactly one effect; there is no batch approval. A mode change to one that approves more shows the confirmation dialog.
- **Production wiring:** Renderer approval tray over `ListApprovals`, `ResolveApproval` (intent hash named), the run-mode and rule commands of PX-057 through preload; the existing `y` and `x` bindings of PX-024 map onto the same decisions; no approval state or policy in the renderer.
- **Real qualification:** QUAL-PX-058 / PX-E2E-058.
- **Failure and negative proof:** as in QUAL-PX-058.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-059"></a>

## PX-059 — Context accounting by category served by the Core

- **Requirement:** REQ-PX-059; **related preserved requirements:** REQ-EV-0035, REQ-EV-0131, REQ-EV-0175.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.8, IMP-EV-0131, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (protocol and schema addition that changes what the inspector and header report as evidence of budget).
- **Specification requirements (doc 65, with verification tag):** AFW-H04 [DISK], AFW-H05 [DISK].
- **Scope and acceptance:** The Context Inspector gains a category breakdown: exactly eight categories whose estimated tokens sum to the reported total (system instructions, tool definitions, rules and instructions, skills, external tool definitions, subagent definitions, summarised conversation, conversation), the model-aware window, a usage percent and the estimator's declared error. The same percent is carried in the agent header (PX-042). The breakdown is derived from the compiled prompt envelope of the last turn, so the inspector totals match the provider request within the declared error, and it is fed back to the agent where the harness already reports budgets. The window comes from the model registry, not a fixed constant.
- **Production wiring:** `crates/context` Context Pack Compiler and Inspector, `crates/prompt-compiler` for the envelope categories, additive `CategoryBreakdownView` in `ContextInspectorView` of `surface.proto`, the registry window from `ListModels`; docs 18 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-059 / PX-E2E-059.
- **Failure and negative proof:** as in QUAL-PX-059.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-060"></a>

## PX-060 — Context ring, usage summary and the docked usage, limit and offline trays

- **Requirement:** REQ-PX-060; **related preserved requirements:** REQ-EV-0035, REQ-EV-0131.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-045, PX-059.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; iteration (presentation of Core-served figures and typed error classes; no new effect, policy, protocol or persistence).
- **Specification requirements (doc 65, with verification tag):** AFW-C09 [LIVE], AFW-D12 [LIVE], AFW-H02 [LIVE], AFW-H03 [STATIC], AFW-H04 [DISK], AFW-H06 [STATIC].
- **Scope and acceptance:** The status-row ring and its tray of AFW-D12 and H01 to H06: used over window in the ring, the tray with the eight categories as a segmented bar and a report link to the inspector, the usage summary shown near a limit, always or never by setting, the budget-exhaustion tray with its staged outcomes and the actions the existing budget policy allows (raise the budget, continue at lower cost), the offline tray (open tasks stay readable, sending fails until the connection returns), the error tray with retry and copy of the request id. Trays use the host of PX-044, persist across navigation and never trap input.
- **Production wiring:** Renderer over the breakdown of PX-059, `GetTaskEconomics` and the typed error classes; no figure computed in the renderer.
- **Real qualification:** QUAL-PX-060 / PX-E2E-060.
- **Failure and negative proof:** as in QUAL-PX-060.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-061"></a>

## PX-061 — Per-turn checkpoints with exact reversible restore

- **Requirement:** REQ-PX-061; **related preserved requirements:** REQ-EV-0012, REQ-EV-0013, REQ-EV-0077, REQ-EV-0122.
- **Owner / milestone / release:** durability / M10 / RELEASE_ZERO; **prerequisites:** M4.3, IMP-EV-0012, IMP-EV-0013, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (recovery and persistence semantics: restore changes workspace files and must be exactly reversible).
- **Specification requirements (doc 65, with verification tag):** AFW-G01 [DISK], AFW-G03 [LIVE].
- **Scope and acceptance:** A checkpoint is recorded at each user turn on the existing baseline-plus-delta engine: files changed since the pre-conversation state, files that did not exist, and folders created. `RestoreCheckpoint` is made exactly reversible: before it changes anything it records a pre-restore checkpoint and returns its id; restoring replays the deltas and removes files that did not exist; the inverse is a restore of the pre-restore checkpoint ('redo'), which is idempotent and fenced by the session lease and epoch. `PreviewRewind` and the optimistic `expected` content hashes of `RestoreCheckpoint` already refuse a restore whose files changed since the preview (HASH_MISMATCH, REQ-EV-0123); this row adds an explicit per-file choice for files the user edited, in place of the whole-restore refusal. Editing an older message offers revert or keep for the files and removes later turns from the live branch but keeps them in the log. `ForkTask` already carries no pending approval into the fork; the suite keeps asserting it.
- **Production wiring:** `crates/checkpoint` and the checkpoint epoch fencing of M4.3, `RestoreCheckpoint` (with its `expected` hashes), `PreviewRewind` and `ForkTask` in `surface.proto` extended with the pre-restore checkpoint id, the per-file choice and per-turn recording confirmation, the task branch events of the session tree; docs 19 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-061 / PX-E2E-061.
- **Failure and negative proof:** as in QUAL-PX-061.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-062"></a>

## PX-062 — Checkpoint restore, redo, edit-in-place and fork in the conversation

- **Requirement:** REQ-PX-062; **related preserved requirements:** REQ-EV-0077, REQ-EV-0122.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-047, PX-061.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 2; release-critical (drives destructive workspace restore and deletion through a dialog; confirmation and wording are safety behaviour).
- **Specification requirements (doc 65, with verification tag):** AFW-G02 [LIVE], AFW-G03 [LIVE], AFW-G04 [STATIC], AFW-G06 [LIVE], AFW-G07 [LIVE].
- **Scope and acceptance:** The controls of AFW-G01 to G08: a restore control on hover on each user message; the confirmation dialog (discard all changes up to this checkpoint, Cancel and Continue, a note that it can be undone) that also counts files to change and delete and lists user-edited files with the per-file choice and offers no do-not-ask-again when such files exist; after Continue the message becomes editable in place and a Redo link appears whose dialog is worded as a redo of the exact pre-restore state; editing an older message asks Revert or Keep files with Keep on Shift+Enter; fork from the turn footer; the changes chips (Changes +N, Commit, Continue) whose effects go through their approvals; the changes scope menu (last turn, uncommitted, branch versus base, pull request) and two-step Undo all.
- **Production wiring:** Renderer over `PreviewRewind`, `RestoreCheckpoint`, `ForkTask` and the review commands through preload; nothing is decided in the renderer; destructive choices are confirmed in the dialog and enforced by the Core.
- **Real qualification:** QUAL-PX-062 / PX-E2E-062.
- **Failure and negative proof:** as in QUAL-PX-062.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-063"></a>

## PX-063 — Project records and membership over the WorkGraph

- **Requirement:** REQ-PX-063; **related preserved requirements:** REQ-EV-0037, REQ-EV-0145, REQ-EV-0124.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M6.1, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; release-critical (canonical persistence and protocol addition; membership changes are recoverable multi-step operations).
- **Specification requirements (doc 65, with verification tag):** AFW-J01 [DISK], AFW-J03 [STATIC], AFW-J04 [STATIC].
- **Scope and acceptance:** A Project is a named, archivable record bound to one workspace, with a membership map from task to project, projected from events over the existing WorkGraph; it is a grouping, carries no coordinator behaviour and adds no scheduler (the coordinator reading of the reference is not evidenced, AFW-J01). Commands create, rename, archive and unarchive a project and add a task to or remove it from a project. Membership rules are enforced by the Core: only top-level tasks can join; no move across projects without leaving first; a project's task cannot be re-parented in place; draft tasks and imported sessions cannot join; a project is bound to its workspace and does not move. Add and remove are two-phase, recoverable operations: a Core killed between phases is reconciled at startup to either the old or the new membership, never a mixture. Project pages, notes with a task database, boards, briefs, libraries, a pull-request tray and scheduled tasks are not in this row (AFW-J04) and need their own Decision Record.
- **Production wiring:** `crates/core-runtime` WorkGraph and `crates/event-store` projections, additive `CreateProject`, `RenameProject`, `ArchiveProject`, `SetTaskProject` and `ListProjects` messages in `surface.proto`, the header projection of PX-042 gains a project id; doc 13 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-063 / PX-E2E-063.
- **Failure and negative proof:** as in QUAL-PX-063.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-064"></a>

## PX-064 — Project-aware agent list: Projects group, stored views and project grouping

- **Requirement:** REQ-PX-064; **related preserved requirements:** REQ-EV-0037, REQ-EV-0143.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-046, PX-063.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; iteration (renders Core projections of PX-063 and PX-042; creation, membership and archive are existing Core commands, so no new effect-bearing behaviour is added here).
- **Specification requirements (doc 65, with verification tag):** AFW-B05 [DISK], AFW-J02 [LIVE], AFW-J03 [STATIC].
- **Scope and acceptance:** The Projects group of AFW-J02 and J03 in the agent list: a Projects group separate from repositories with a New Project row and expandable parents that show their tasks, a Project grouping option and project filter chips in the stored query, drag or menu actions to add a task to a project and to remove it that surface the Core's typed refusals, and project archive and unarchive. Section order and collapsed state persist per grouping. Nothing about project pages is included.
- **Production wiring:** Renderer list components over `ListProjects` and `SetTaskProject` of PX-063 through preload; the grouping and filters of PX-046.
- **Real qualification:** QUAL-PX-064 / PX-E2E-064.
- **Failure and negative proof:** as in QUAL-PX-064.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption. Iteration tier: the packaged UI smoke suite against a real Core is the proof of record (doc 83); a need for a protocol, persistence, policy or main-process change stops the row and re-tiers it by a Decision Record.

<a id="px-065"></a>

## PX-065 — Worktree lifecycle: creation policy, setup commands, scheduled cleanup, lease and retention

- **Requirement:** REQ-PX-065; **related preserved requirements:** REQ-EV-0124, REQ-EV-0125, REQ-EV-0145.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** M2.2, M4.3, IMP-EV-0125, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; release-critical (execution and persistence of worktrees and deletion of workspace data under a schedule; setup commands run repository-supplied code).
- **Specification requirements (doc 65, with verification tag):** AFW-J05 [STATIC], AFW-J06 [DISK], AFW-J07 [STATIC].
- **Scope and acceptance:** Core-managed worktrees (AFW-J05 to J07) under a profile root outside the user's checkout: creation prunes stale entries, handles an empty repository, retries path collisions, carries uncommitted and untracked files into the worktree by copy subject to path policy, honours a branch policy (new branch or detached) and returns a typed error when the branch is checked out elsewhere. A setup command list read from the repository is data: it runs only in a trusted repository, as a typed sandboxed process, with a 300 s timeout, its output as an OutputRef, and a failure is visible on the task card and not fatal. Cleanup runs every 6 h; at startup, if the last run is older than the interval, a catch-up runs after 30 s; the last-run time is persisted; a single lease with owner and reason prevents concurrent cleanup and a second request is refused while it is held; each run reports scanned, removed, bytes freed and errors and a skipped run is never reported as completed. Retention defaults are 25 worktrees and 50 GB, orphaned first then least recently used with hysteresis. Stricter than the reference: a worktree is removable only when its task is terminal and its changes are applied, exported or discarded, and it is not younger than 10 minutes; a dirty or unapplied worktree is never removed automatically and raises an attention item; removal is a receipted effect.
- **Production wiring:** `crates/workspace` and `crates/git` (worktree_add, worktree_remove, worktree_list exist), the profile worktree root already used by `services/modbit-core/src/branch.rs`, the lease of the durability owner, the sandboxed process path of `crates/terminal`, additive `ListWorktrees` and `RunWorktreeCleanup` reads and commands in `surface.proto`; docs 20 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-065 / PX-E2E-065.
- **Failure and negative proof:** as in QUAL-PX-065.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-066"></a>

## PX-066 — Apply-back of a worktree result to the user's checkout with conflict handling and exact undo

- **Requirement:** REQ-PX-066; **related preserved requirements:** REQ-EV-0125, REQ-EV-0145.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** M2.2, M9.2, PX-061, PX-065.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; release-critical (a protected effect on the user's own working tree with destructive options).
- **Specification requirements (doc 65, with verification tag):** AFW-G05 [LIVE], AFW-J08 [STATIC].
- **Scope and acceptance:** Applying a task worktree's reviewed result to the user's checkout is a protected effect bound to the exact candidate revision and approved with its intent. Before any change the Core records a pre-apply checkpoint of the user's checkout so Undo is exact. On conflict the Core classifies each file and offers: merge manually (apply, leave markers), stash then apply (tracked and untracked), overwrite (conflicting files only), full overwrite, undo and apply, cancel. Cancel is the default; any overwrite needs typed confirmation naming the files; a choice is remembered per task only for the non-destructive options. A user edit made to the checkout since the review revision makes the apply stale and is refused until re-reviewed. Undo restores the pre-apply checkpoint exactly and is itself recorded; a crash mid-apply recovers to the pre-apply or the applied state, never a mixture.
- **Production wiring:** `crates/git` merge transaction (`merge_begin`, `merge_resolve`, `merge_commit`, `merge_abort`, `snapshot_dirty` exist) and the Change Engine, the checkpoint of PX-061, the approval and receipt chain, additive `ApplyWorktree`, `UndoApply` and `ApplyConflictView` messages in `surface.proto`; docs 20 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-066 / PX-E2E-066.
- **Failure and negative proof:** as in QUAL-PX-066.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-067"></a>

## PX-067 — Hardened Git execution: repository hooks, fsmonitor, attributes and credential redaction

- **Requirement:** REQ-PX-067; **related preserved requirements:** REQ-EV-0075.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** M2.2, DOC-PX-007.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; release-critical (security boundary: untrusted repository configuration must not run code or leak credentials).
- **Specification requirements (doc 65, with verification tag):** AFW-J09 [STATIC].
- **Scope and acceptance:** Every Git invocation by the Core runs hardened: repository hooks, `core.fsmonitor` and attributes files are neutralised, bare-repository safeguards are on, terminal prompts are disabled, and URL credentials are redacted from every log, event and error. The shared runner in `crates/git` (which today sets only the prompt and locale variables) is the single place the hardening lives, and every direct `Command::new("git")` in the crate goes through it. A repository that ships a pre-commit hook, an fsmonitor command or a filter attribute never executes it on the Core's behalf, and the effect is recorded as a security event on the task.
- **Production wiring:** `crates/git/src/lib.rs` runner and its direct spawns, the redaction stage of the secret broker, the security event of `crates/effects`; the architecture-lint rule that forbids a Git spawn outside the runner; docs 20 and 23 updated by the implementing task.
- **Delivered-in-part note (2026-10-05, audit fix FIX-01 in pull request moss101/modbit#61, branch `wip/audit-fixes-integration`, not merged).** The audit (`research/audit/02-PHASE0-AND-FIX-RESULTS.md`, untracked) reports that FIX-01 delivered most of this row's scope inside `crates/git`: a single command builder (`harden.rs`) that neutralises repository hooks (`core.hooksPath`), `core.fsmonitor`, the attributes file, repository-scoped filter, merge-driver and textconv programs and signing for every `crates/git` invocation; `--end-of-options` and ref, branch and remote validation before any operand reaches a command line; URL-credential redaction from git error text; NUL-delimited status parsing; confinement of worktree paths; and crate-level tests against hostile-repository fixtures. This note changes no requirement text, prerequisite, tier or qualification of the row, and the row stays NOT_STARTED: nothing of it is merged and the closing proof of QUAL-PX-067 has not run. **The remaining scope of PX-067 is only what FIX-01 did not deliver:** (1) the bare-repository safeguard (absent from the builder's fixed configuration); (2) redaction of URL credentials in every log, event and error that Core operations produce, not only in `crates/git` error text; (3) the security event recorded on the task when a repository program was neutralised (FIX-01 neutralises silently); (4) the architecture-lint rule forbidding a Git spawn outside the runner, and routing the direct `git` spawns outside `crates/git` (for example the cloud worker's handoff in `apps/cloud-worker/src/handoff.rs` at a6c3a996) through it; (5) repository-local `core.sshCommand` and `credential.helper`, which FIX-01 documents as still running on network commands behind an approval and which this row must neutralise or prove contained; (6) QUAL-PX-067 itself on the Core operations (status, diff, worktree add, commit, merge, push to a local remote) against the hostile fixtures on the packaged build, with its mutation check. The implementing task audits the merged state of #61 first and records the classification (existing-code-audit) rather than re-deriving this list.
- **Real qualification:** QUAL-PX-067 / PX-E2E-067.
- **Failure and negative proof:** as in QUAL-PX-067.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-068"></a>

## PX-068 — Worktree management and apply-back conflict modal in the desktop

- **Requirement:** REQ-PX-068; **related preserved requirements:** REQ-EV-0125, REQ-EV-0145.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-048, PX-062, PX-065, PX-066.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-007; phase 3; release-critical (drives a destructive apply and worktree removal through a modal; wording and defaults are safety behaviour).
- **Specification requirements (doc 65, with verification tag):** AFW-G05 [LIVE], AFW-J08 [STATIC].
- **Scope and acceptance:** The surfaces of AFW-J07 and J08: a worktree list (Settings and the task header) grouped by source repository with state, size, age, task and eligibility, the retention settings, a Run cleanup action and the last run's report; for a task in a worktree the header actions Apply to my checkout, Apply to specific files, Undo apply, Open terminal in worktree, Copy path; the conflict modal with Merge manually, Stash, Overwrite, Full overwrite, Undo and apply and Cancel as the default, typed confirmation for any overwrite, a pre-apply checkpoint notice, and a remembered choice only for non-destructive options. The attention item for a dirty or unapplied worktree that retention would otherwise remove appears here.
- **Production wiring:** Renderer over `ListWorktrees`, `RunWorktreeCleanup`, `ApplyWorktree`, `UndoApply` and `ApplyConflictView` of PX-065 and PX-066 through preload; nothing is decided in the renderer.
- **Real qualification:** QUAL-PX-068 / PX-E2E-068.
- **Failure and negative proof:** as in QUAL-PX-068.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-069"></a>

## PX-069 — Computer Runtime contract: typed computer tools, scopes, handles, refusal taxonomy and unknown-outcome latch

- **Requirement:** REQ-PX-069; **related preserved requirements:** REQ-EV-0075, REQ-EV-0081, REQ-EV-0082, REQ-EV-0083, REQ-EV-0084, REQ-EV-0086, REQ-EV-0089.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** M7.6, M5.1, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (a new effect-bearing tool family, a new protocol surface and a new class of unknown-outcome input).
- **Specification requirements (doc 66, with verification tag):** CUC-A01 [STATIC], CUC-A02 [STATIC], CUC-A03 [UNVERIFIED], CUC-A04 [STATIC], CUC-B01 [STATIC], CUC-B02 [STATIC], CUC-B03 [STATIC], CUC-B04 [STATIC], CUC-B05 [STATIC], CUC-B06 [STATIC], CUC-D01 [STATIC], CUC-D02 [STATIC], CUC-D03 [STATIC], CUC-D05 [STATIC].
- **Scope and acceptance:** A Computer Runtime in the Core under the Browser and Computer Runtime owner: the `computer.*` tool family of CUC-B01 registered in the Tool Registry with capabilities `computer.observe`, `computer.act` and `computer.screen`, strict argument validation, APP and SCREEN scopes, the fixed 1280 x 800 canvas and single scaler, element ids with snapshot ids and coordinate tokens with their invalidation rules, observation and action coupling by scope, the action ladder with modality and reason recorded, per-session serialisation, the typed refusal taxonomy with the four escalations and the unknown-outcome latch recorded as an UNKNOWN receipt. The runtime talks to the actuator only through the protocol of PX-071; with no actuator every call answers ACTUATOR_UNAVAILABLE. Computer tools are absent from the projection for `cloud_isolated` tasks.
- **Production wiring:** New crate `crates/computer` (registered under the browser canonical owner by the architecture-lint module metadata, the pattern of DR-M9-001), tool specs in `crates/tools`, additive computer messages in `surface.proto`, the effect receipt and lease machinery of `crates/effects` and `crates/browser::ControlLease`; docs 17, 22 and 30 updated by the implementing task; doc 12 is a locked path and the implementing task cites DR-PX-2026-10-03-008 for the additive layout note.
- **Real qualification:** QUAL-PX-069 / PX-E2E-069.
- **Failure and negative proof:** as in QUAL-PX-069.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-070"></a>

## PX-070 — Per-call exact-intent approvals for native control, never allowlistable

- **Requirement:** REQ-PX-070; **related preserved requirements:** REQ-EV-0083, REQ-EV-0031, REQ-EV-0046, REQ-EV-0088.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M2.5, M9.2, PX-057, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (permissions and policy: a new protected effect class whose approvals must stay exact in every run mode).
- **Specification requirements (doc 66, with verification tag):** CUC-B06 [STATIC], CUC-C01 [STATIC], CUC-C02 [UNVERIFIED], CUC-C03 [STATIC], CUC-C04 [STATIC], CUC-C05 [UNVERIFIED], CUC-F03 [STATIC], CUC-H02 [UNVERIFIED].
- **Scope and acceptance:** Computer control is a protected effect class in the Capability Kernel with these invariants: every action is approved per call with an intent hash over the target application identity (bundle identifier, executable path, code-signing identity, process id), the window, the snapshot id or coordinate token, the action and its arguments, the scope and the expiry; observation needs an observation grant for one named application in APP scope, valid for the turn and at most 15 minutes, with a call cap, never inherited by a subagent or a later turn; SCREEN scope needs its own approval stating it takes the display and the real cursor. None of these can be allowlisted, covered by any run mode including Run everything, created by a durable rule, or given by a background or detached agent. Administrative policy may forbid computer control, restrict it to an application allow-list by bundle identity, or forbid SCREEN scope, and an unknown verdict fails closed. A versioned policy list of applications that are not drivable (credential stores, system security panes, shell hosts, authentication dialogs, Modbit itself) is shown to the user and no approval lifts it. Modes ASK and PLAN do not carry the capability.
- **Production wiring:** `crates/policy` and `crates/effects` (capability classes, intent hash, approval aggregate, receipt chain), `services/modbit-core/src/config.rs` policy generations, the run-mode and rule commands of PX-057 gain an explicit refusal for this class, the mode posture of PX-051; docs 23 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-070 / PX-E2E-070.
- **Failure and negative proof:** as in QUAL-PX-070.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-071"></a>

## PX-071 — macOS actuator helper: separate signed process, authenticated local RPC, accessibility-first background control, takeover lease and stop

- **Requirement:** REQ-PX-071; **related preserved requirements:** REQ-EV-0075, REQ-EV-0084, REQ-EV-0085, REQ-EV-0087.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-069, M7.6, M9.5, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (a new privileged native process that captures the screen and injects input; security boundary and execution semantics).
- **Specification requirements (doc 66, with verification tag):** CUC-A01 [STATIC], CUC-A02 [STATIC], CUC-A03 [UNVERIFIED], CUC-D04 [STATIC], CUC-D05 [STATIC], CUC-E01 [STATIC], CUC-E02 [STATIC].
- **Scope and acceptance:** The actuator is a separate helper process (`services/modbit-actuator`, macOS first) that holds the operating-system permissions, walks accessibility trees, captures windows and the screen, and injects input, and nothing else: it holds no model, policy, credential or filesystem authority. Local RPC is newline-delimited JSON over a Unix socket with a per-launch 32-byte token delivered over an inherited descriptor or a user-only file, peer-credential and process-id checks in both directions, an exclusive endpoint creation that aborts if the name is taken, a welcome the Core accepts only from the process it launched, a protocol major version with mismatch refusal, and request deadlines the helper honours. The Core verifies the helper's code signature and designated requirement before connecting and the helper verifies its parent. APP scope drives one application in the background through accessibility actions and value setting without moving the real cursor; SCREEN scope takes an exclusive lease, moves the real cursor and shows a Stop overlay; the helper parks on real human input and excludes its own synthetic events; Stop from the overlay, the desktop, the session emergency stop or a Core disconnect halts injection within 500 ms; leases release at turn end.
- **Production wiring:** `services/modbit-actuator` (Swift or Rust, chosen by the dependency-admission step), `crates/computer` RPC client, `crates/browser::ControlLease` generalised for native targets, `apps/desktop` for the permission and presence surfaces of PX-074; docs 21, 22 and 52 updated by the implementing task; the helper is a build target of the packaging task M10.2.
- **Real qualification:** QUAL-PX-071 / PX-E2E-071.
- **Failure and negative proof:** as in QUAL-PX-071.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-072"></a>

## PX-072 — Actuator supply chain: bundled signed helper, hash-pinned manifest, rollback and process identity

- **Requirement:** REQ-PX-072; **related preserved requirements:** REQ-EV-0075.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-071, M10.2, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (supply-chain and update path for a privileged binary; security boundary).
- **Specification requirements (doc 66, with verification tag):** CUC-E01 [STATIC], CUC-E03 [STATIC], CUC-E04 [STATIC].
- **Scope and acceptance:** The helper ships inside Modbit's signed and notarised application bundle and changes only with the application update of M10.2, with a hash-pinned entry in the build manifest and the SBOM and a recorded designated requirement the Core checks at every connect; there is no run-time download path. Before the Core signals a helper process it verifies the process's executable path against the bundle. The Core never replaces a running helper. An update keeps the previous helper for rollback, recovers from a crash mid-update to one whole version, and a protocol major mismatch between Core and helper refuses control with a typed reason rather than degrading.
- **Production wiring:** The packaging and updater work of M10.2 (signing, notarisation, SBOM), the build manifest of doc 70, `crates/computer` connect checks; docs 70 and 74 updated by the implementing task.
- **Real qualification:** QUAL-PX-072 / PX-E2E-072.
- **Failure and negative proof:** as in QUAL-PX-072.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-073"></a>

## PX-073 — Browser control hardening: CDP deny list, per-call origin gate, certificate trust, permissions and per-task view ownership

- **Requirement:** REQ-PX-073; **related preserved requirements:** REQ-EV-0081, REQ-EV-0283, REQ-EV-0088.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** M7.6, M7.7, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (security boundary on the live browser host: origin policy, DevTools protocol access and session data).
- **Specification requirements (doc 66, with verification tag):** CUC-G01 [STATIC], CUC-G02 [STATIC], CUC-G03 [STATIC], CUC-G04 [STATIC], CUC-G05 [STATIC].
- **Scope and acceptance:** Hardening of the existing browser host, kept inside the host so the Core and the model cannot bypass it: (1) a deny list enforced before any DevTools-protocol attach covers the domains Browser, Input (except the host's own structural dispatch), Storage, SystemInfo, Target and Tethering and the methods that set, read or clear cookies, clear the cache, set file-input files, navigate or read navigation history, and it applies to any future raw-protocol rung; (2) a per-call origin gate checks the current page's origin on every browser tool call under a versioned policy (default http and https only, `file:` always refused, an allow-list when configured, an unknown or undeterminable origin refused with the allowed list, fail closed); (3) a certificate error holds the request for 60 s and the person chooses Reject or Trust with issuer, subject, validity and fingerprint, trust remembered per workspace and listable and clearable; (4) page permissions are a minimal allow-list and the rest are denied and named as today; (5) a sign-out and a clear action wipe the browser partitions' cache, cookies, storage, service workers and downloads; (6) a task lists and selects only the views it owns, hidden views are capped at six and reclaimed least recently used with a stated reset, and navigation does not steal focus; (7) agent device emulation is scoped to the turn, visible with a Reset control, reverted at turn end and never overrides the user's own; (8) takeover and the lock overlay behave exactly as M7.6 specifies.
- **Production wiring:** `apps/desktop/src/main/browser.ts` and `crates/browser`, the policy bundle for the origin list, `crates/effects` for the typed refusals, the browser view surface of PX-048 for the trust and reset controls; docs 22 and 52 updated by the implementing task. No webPreferences or CSP change; any other main-process change is DR-PX-2026-10-03-012.
- **Real qualification:** QUAL-PX-073 / PX-E2E-073.
- **Failure and negative proof:** as in QUAL-PX-073.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-074"></a>

## PX-074 — Computer-control desktop surface: permission window, presence and Stop, action cards and approval card

- **Requirement:** REQ-PX-074; **related preserved requirements:** REQ-EV-0085, REQ-EV-0087, REQ-EV-0283.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-058, PX-069, PX-070, PX-071, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (drives approvals and the Stop control for a privileged effect; presence must never lie).
- **Specification requirements (doc 66, with verification tag):** CUC-A02 [STATIC], CUC-C03 [STATIC], CUC-D05 [STATIC], CUC-E05 [STATIC], CUC-G03 [STATIC], CUC-H01 [STATIC].
- **Scope and acceptance:** The surfaces of CUC-C01, C03, E05 and H01: a first-run permissions window that asks for the minimum labelled capabilities in plain language (access to application interfaces, use of screenshots as context), shows each as checking, allow or ready, opens the right System Settings pane, restarts the helper once after a grant and shows a refresh when polling ends, and is routed once and only in the root workspace window; a presence indicator in every window while control is held (who holds it, the target application, the scope) with a Stop control reachable by one click or one chord and an announcement through a live region; the approval card of PX-058 extended with the application identity, window, element, action and exact arguments, scope, expiry and a before-thumbnail, with no Always button and no Run-everything path for this class; a transcript card with a numbered action list, screenshot thumbnails and the approval that bound each action. A person who presses Stop sees the typed USER_ABORTED state until the next turn.
- **Production wiring:** Renderer over the computer messages of PX-069 through preload with main-side validation; the Stop control calls the session emergency stop and the helper's stop through the Core; no capture or input API in the renderer or main; the tray host and approval stack of PX-044 and PX-058.
- **Real qualification:** QUAL-PX-074 / PX-E2E-074.
- **Failure and negative proof:** as in QUAL-PX-074.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-075"></a>

## PX-075 — Computer-use subagent profile with an isolated target, constrained tools and stall rules

- **Requirement:** REQ-PX-075; **related preserved requirements:** REQ-EV-0218, REQ-EV-0116, REQ-EV-0046.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M6.3, PX-039, PX-051, PX-069, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (a new agent profile with a capability posture and admission rules).
- **Specification requirements (doc 66, with verification tag):** CUC-A05 [STATIC], CUC-D06 [STATIC], CUC-H02 [UNVERIFIED].
- **Scope and acceptance:** A `computer_use` agent profile under the existing AgentGraph and capacity-ticket admission: toolset limited to read, search, non-GUI shell and `computer.*`; its own target and control session (a subagent never shares handles with its parent); a specialist model slot chosen by policy; no parent transcript; a typed result envelope that carries the final state, the actions taken and the evidence references; resumable by identity; started only after the parent records that the environment is healthy; four consecutive failed attempts or three turns with no new observation or action make it stop and report what it observed, what blocked it and the best next step, treating logins, passkeys, captchas, permission prompts and destructive confirmations as blockers to report. The profile is unavailable in ASK and PLAN modes and in unattended runs, and it cannot hold a grant on behalf of a parent.
- **Production wiring:** `crates/core-runtime` agent profiles and admission (`services/modbit-core/src/agent_profiles.rs`, `spawn.rs`), the harness stall detection of PX-039 and PX-040, the typed `AgentResultEnvelope`; docs 14 and 28 updated by the implementing task.
- **Real qualification:** QUAL-PX-075 / PX-E2E-075.
- **Failure and negative proof:** as in QUAL-PX-075.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-076"></a>

## PX-076 — Screenshot and accessibility artifacts: window capture, masking, content-aware codec, retention and audit

- **Requirement:** REQ-PX-076; **related preserved requirements:** REQ-EV-0090, REQ-EV-0086.
- **Owner / milestone / release:** media / M10 / RELEASE_ZERO; **prerequisites:** PX-069, M2.10, DOC-PX-008.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-008; 5 (computer-use hardening); release-critical (sensitive-data handling and evidence semantics for captured screens).
- **Specification requirements (doc 66, with verification tag):** CUC-B02 [STATIC], CUC-B04 [STATIC], CUC-F01 [STATIC], CUC-F02 [STATIC], CUC-F03 [STATIC].
- **Scope and acceptance:** Captured frames and accessibility trees are first-class media artifacts under the Media Pipeline with provenance (application identity, window, snapshot id, scope), untrusted-input labelling and the task's retention policy. APP scope captures the target window only; secure fields are masked inside the actuator before the frame leaves it; a secure desktop or a locked screen yields SECURE_DESKTOP and no frame. The codec classifies flat user-interface frames and encodes them lossless, others lossy at a quality ladder that never changes pixel size, and an identical frame is memoised and not re-encoded; every frame is exactly the 1280 x 800 canvas or is refused. A task that is `cloud_isolated` never receives a local frame. The audit holds counts and kinds only (actions by kind, screenshot count, duration, outcome, application identity, approval ids), never contents, and an effect receipt per approved action is bound to the intent hash.
- **Production wiring:** `crates/tools` media bridge and the artifact store (M2.10), `crates/computer` frame handling, `crates/effects` receipts and the audit projection, `crates/observability` counters; docs 25 and 52 updated by the implementing task.
- **Real qualification:** QUAL-PX-076 / PX-E2E-076.
- **Failure and negative proof:** as in QUAL-PX-076.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-077"></a>

## PX-077 — Editor file service: open with revision and encoding facts, multi-file save as one ChangeTransaction, merge basis and file revision events

- **Requirement:** REQ-PX-077; **related preserved requirements:** REQ-EV-0141, REQ-EV-0160, REQ-EV-0015.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** PX-005, M2.1, M2.9, DOC-PX-009.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-009; editor goal; release-critical (a new write path from the desktop into the workspace and file revision events; canonical persistence and path policy).
- **Specification requirements (doc 67, with verification tag):** WED-A02 [UNVERIFIED], WED-A04 [UNVERIFIED], WED-A05 [UNVERIFIED], WED-B01 [UNVERIFIED], WED-B02 [UNVERIFIED], WED-B03 [UNVERIFIED], WED-B05 [UNVERIFIED], WED-F02 [UNVERIFIED].
- **Scope and acceptance:** The Workspace File Service gains the operations the editor needs and nothing that gives a client authority over files. `OpenForEdit` returns, for a path under the task worktree or the trusted checkout, the bytes (bounded, paged above the inline ceiling), the workspace and file revision, a content hash, the detected encoding and byte-order mark, the line-ending convention, the trailing-newline fact, and whether the path is protected or binary, with the protection reason. `SaveEdit` is the multi-file form of `ApplyUserPatch`: one ChangeTransaction with a revision precondition on every file, path policy after symlink resolution, provenance `user_direct_edit`, one event and one revision advance, stale code references invalidated, atomic across the set, encoding, byte-order mark, line endings and trailing newline preserved unless changed by the person, and an edit that would corrupt any of them refused. It is idempotent by command id and reconciled by the effect ledger across a Core restart. `GetMergeBasis` returns the base snapshot of a path at a revision so a client can build a three-way merge. A `FileRevisionAdvanced` event tells subscribed clients that a path they opened moved and why (agent change, merge, checkout, user save). Protected paths open read-only or are refused; a write to them is denied by the Core whatever the client sent.
- **Production wiring:** `crates/workspace` Workspace File Service and revision store, the `ApplyUserPatch` implementation of PX-005 in `services/modbit-core`, additive `OpenForEdit`, `SaveEdit`, `GetMergeBasis` messages and the `FileRevisionAdvanced` event in `surface.proto`, the path policy of `crates/workspace/src/paths.rs`; docs 20, 29 and 30 updated by the implementing task, including the scoped supersession note of WED-A02.
- **Real qualification:** QUAL-PX-077 / PX-E2E-077.
- **Failure and negative proof:** as in QUAL-PX-077.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-078"></a>

## PX-078 — Editor language intelligence: draft analysis overlay, outline, definitions and references served by the Core

- **Requirement:** REQ-PX-078; **related preserved requirements:** REQ-EV-0020, REQ-EV-0141, REQ-EV-0160, REQ-EV-0170.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.3, M3.4, M3.6, PX-077, DOC-PX-009.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-009; editor goal; release-critical (a new protocol surface that feeds unsaved text to language services; bounded resource use and revision binding).
- **Specification requirements (doc 67, with verification tag):** WED-C02 [UNVERIFIED], WED-C03 [UNVERIFIED].
- **Scope and acceptance:** Editor-shaped queries on the existing language-service bridge and indexes: `AnalyzeDraft` takes a path, a base revision and the draft text and returns diagnostics for that text through an ephemeral analysis overlay that exists only in memory for the request, is never written to the filesystem, is rate-limited per client and discarded; `GetOutline`, `GetDefinition` and `GetReferences` answer at a stated revision and say when the file or index is stale. Results carry the revision and the draft hash and are marked stale by the client when either moves. Diagnostics are pulled after the text settles (REQ-EV-0020), never pushed per keystroke, and a dead language service degrades explicitly rather than returning a fake empty answer (PX-028). Languages follow their tiers (doc 76): a Tier C or unsupported language returns text-level answers only and says so.
- **Production wiring:** `crates/diagnostics` language-service adapters, `crates/retrieval` structural indexes, `crates/context` revision binding, additive `AnalyzeDraft`, `GetOutline`, `GetDefinition`, `GetReferences` messages in `surface.proto`; docs 18, 30 and 76 updated by the implementing task.
- **Real qualification:** QUAL-PX-078 / PX-E2E-078.
- **Failure and negative proof:** as in QUAL-PX-078.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-079"></a>

## PX-079 — Workspace Editor surface: text engine, drafts, stale-draft merge, find and replace, tabs, accessibility and large-file behaviour

- **Requirement:** REQ-PX-079; **related preserved requirements:** REQ-EV-0141, REQ-EV-0160.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-048, PX-077, PX-078.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-009; editor goal; release-critical (a new writable surface over workspace files; accessibility, input-method and security behaviour; admission of a new dependency).
- **Specification requirements (doc 67, with verification tag):** WED-A01 [UNVERIFIED], WED-A02 [UNVERIFIED], WED-A03 [UNVERIFIED], WED-A05 [UNVERIFIED], WED-B01 [UNVERIFIED], WED-B03 [UNVERIFIED], WED-B04 [UNVERIFIED], WED-C01 [UNVERIFIED], WED-E01 [DISK], WED-E02 [UNVERIFIED], WED-E03 [UNVERIFIED], WED-F01 [UNVERIFIED], WED-F02 [UNVERIFIED].
- **Scope and acceptance:** The surface of WED-A01 to A05, B01 to B05, C01, E01 to E03 and F01. The text engine is admitted through dependency admission against WED-A03 and recorded in doc 35; if none qualifies the row stops. The editor opens as a file tab in the Files app, shows protected files read-only with the reason, holds a base snapshot and a draft overlay, saves only through `SaveEdit`, marks a draft stale on `FileRevisionAdvanced` and rebases it when operations do not overlap or shows a three-way merge built from `GetMergeBasis`, persists drafts locally keyed by workspace, file and base revision and restores them only against an unchanged base, asks to save, discard or cancel on close, and offers undo and redo, find and replace in file with a linear-time expression engine, go to line, indent, toggle comment, bracket matching, soft wrap, whitespace display and line numbers (preferences persisted per app). Highlighting reuses the Review highlighter; diagnostics and outline come from PX-078 pulled after settle. Large files: above 5 MB read-only with the reason, above 50 MB not opened. Binary files show type and size. Paste is plain text and bounded. A screen reader reads the text model, input-method composition and bidirectional text work, Escape leaves the editor and Tab inserts indentation unless Tab moves focus is on.
- **Production wiring:** Renderer editor components over the file and analysis messages of PX-077 and PX-078 through preload with main-side validation; the engine package under `packages/ui` or a new `packages/editor` registered under the desktop owner; no filesystem, Git or language-server access in the renderer; doc 12 is a locked path and a new package cites DR-PX-2026-10-03-009 for the layout note.
- **Real qualification:** QUAL-PX-079 / PX-E2E-079.
- **Failure and negative proof:** as in QUAL-PX-079.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-080"></a>

## PX-080 — Agent-aware editing: selection context, quick edit, inline per-hunk review and agent-write notices

- **Requirement:** REQ-PX-080; **related preserved requirements:** REQ-EV-0141, REQ-EV-0160.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-079, PX-055, M2.9, DOC-PX-009.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-009; editor goal; release-critical (sends instructions that start or steer tasks and exposes per-hunk accept and reject decisions from a new surface).
- **Specification requirements (doc 67, with verification tag):** WED-D01 [UNVERIFIED], WED-D02 [UNVERIFIED], WED-D03 [UNVERIFIED].
- **Scope and acceptance:** The behaviours of WED-D01 to D03: the active file, selection and symbol feed `SetTaskSelection` and show in the context inspector without mutating source; quick edit with a selection and an instruction creates or steers a task scoped to that file and range with the selection as context and returns the result as per-hunk review at its revision, never as a silent write to the draft; lines changed by a task or by the user since the base revision show in the gutter with the existing `DecideReview` accept and reject available inline at their revision; a file with a dirty draft that an agent task is about to change raises a notice (not an approval) and offers to open the merge.
- **Production wiring:** Renderer over `SetTaskSelection`, `CreateTask` or `QueueInput`, `GetCodeView` and `DecideReview` through preload; no new Core command beyond PX-077 and PX-078; the notice uses the `FileRevisionAdvanced` and attention items already in the protocol.
- **Real qualification:** QUAL-PX-080 / PX-E2E-080.
- **Failure and negative proof:** as in QUAL-PX-080.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-081"></a>

## PX-081 — Editor text-safety and conformance suite: encodings, round trips, fuzzed save path, input method and screen reader, latency

- **Requirement:** REQ-PX-081; **related preserved requirements:** REQ-EV-0015.
- **Owner / milestone / release:** verification / M10 / RELEASE_ZERO; **prerequisites:** PX-077, PX-079, DOC-PX-009.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-009; editor goal; release-critical (evidence semantics for a new write surface; property and fuzz proof of data safety).
- **Specification requirements (doc 67, with verification tag):** WED-B02 [UNVERIFIED], WED-E02 [UNVERIFIED], WED-E03 [UNVERIFIED], WED-F01 [UNVERIFIED].
- **Scope and acceptance:** A conformance suite in the Eval Harness and the packaged E2E that proves what the editor may never do: corrupt a file. It holds a property-based round trip over generated text in UTF-8, UTF-8 with a byte-order mark and UTF-16 with LF, CRLF and mixed endings and with and without a trailing newline (open, apply random operations, save, compare bytes outside the edited ranges), a fuzzer over the `SaveEdit` request (malformed paths, symlinks, huge payloads, interleaved stale saves, duplicate command ids), scripted input-method and screen-reader sessions run in the packaged app, a binary and large-file matrix, and latency traces against the budgets of WED-E03. It records results as run evidence under the existing evidence rules and is a gate for PX-079.
- **Production wiring:** `benchmarks/agent-engineering` or a sibling suite under the eval-bench owner and `apps/desktop/e2e`; fixtures under `tests/`; docs 50, 56 and 83 updated by the implementing task.
- **Real qualification:** QUAL-PX-081 / PX-E2E-081.
- **Failure and negative proof:** as in QUAL-PX-081.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-082"></a>

## PX-082 — Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions

- **Requirement:** REQ-PX-082; **related preserved requirements:** REQ-EV-0149, REQ-EV-0264, REQ-EV-0275.
- **Owner / milestone / release:** automation / M10 / RELEASE_ZERO; **prerequisites:** M1.2, M2.5, DOC-PX-010.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-010; automations goal; release-critical (a new durable record that authorises unattended execution; permissions, canonical persistence and a new command surface).
- **Specification requirements (doc 68, with verification tag):** AUT-A01 [STATIC], AUT-A02 [STATIC], AUT-A03 [STATIC], AUT-A04 [STATIC], AUT-B01 [STATIC], AUT-B02 [STATIC], AUT-C03 [STATIC], AUT-D03 [STATIC], AUT-D06 [UNVERIFIED].
- **Scope and acceptance:** The `automation` owner holds the definitions and nothing else: it never runs anything. A definition is a versioned event-sourced record: name, workspace, principal (creating user or organisation service account), triggers with filters, an optional gate step, the prompt or plan, capability profile, execution mode, budget (per run and per day), concurrency policy, missed-run policy and pause state. Commands create, edit (a new version), enable, disable, pause, delete and list definitions under the session lease; a model-origin attempt is denied. Enabling validates the schedule (five-field UTC, floor five minutes), filters (user expressions compiled by a linear-time engine, rejected when unsafe), the capability profile against the principal's policy, the budget and workspace trust, and records an enable approval that lists the exact capabilities, paths and hosts and is bound to the definition hash; a profile with any write, network or external-effect capability requires it, a read-only profile requires the hash approval only. A definition read from a repository file loads disabled and is enabled only by that approval bound to the repository revision; an edit re-prompts. Definitions hold no secret value. The shipped reference definition (report drift of the default branch, read-only, Kernel-enforced) is a real definition installed from the product's own data.
- **Production wiring:** `automation` subsystem (a new crate `crates/automation` registered under that existing owner by architecture-lint module metadata, the pattern of DR-M9-001), `crates/event-store` aggregates, `crates/policy` validation, the approval aggregate of `crates/effects`, additive `CreateAutomation`, `UpdateAutomation`, `SetAutomationState`, `ListAutomations`, `ResolveAutomationEnable` messages in `surface.proto`; docs 13, 14 and 30 updated by the implementing task; doc 12 and doc 13 are locked paths and the implementing task cites DR-PX-2026-10-03-010 for the additive notes.
- **Real qualification:** QUAL-PX-082 / PX-E2E-082.
- **Failure and negative proof:** as in QUAL-PX-082.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-083"></a>

## PX-083 — Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets

- **Requirement:** REQ-PX-083; **related preserved requirements:** REQ-EV-0149, REQ-EV-0264, REQ-EV-0275, REQ-EV-0180.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M6.2, PX-082, DOC-PX-010.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-010; automations goal; release-critical (execution and scheduling semantics, recovery and cost bounds; the single scheduler gains an input and no sibling).
- **Specification requirements (doc 68, with verification tag):** AUT-A01 [STATIC], AUT-B01 [STATIC], AUT-B03 [STATIC], AUT-B05 [UNVERIFIED], AUT-B06 [UNVERIFIED], AUT-C01 [STATIC], AUT-C02 [STATIC], AUT-E01 [STATIC].
- **Scope and acceptance:** The Core's Scheduler gains one input, a durable due-queue of time triggers, and one consumer of events from the forge adapter; it gains no second loop. Time triggers are evaluated in UTC from the five-field expression; the next due slot is persisted so a restart neither loses nor duplicates it. Each firing carries an event id (delivery id or schedule slot id) and `CreateTask` is idempotent per (definition, version, event id): a duplicate is recorded as duplicate and creates nothing. Concurrency policy (skip, queue to a bound, replace at a safe boundary), per-definition rate limits and per-run and daily budgets are enforced before admission; exhaustion records a skipped run with the typed reason and raises an attention item. After downtime the missed-run policy applies: skip (default) records skipped for the missed slot, or catch up once for the whole window; slots are never replayed in bulk. The optional gate step runs read-only with its own budget and its typed answer decides whether the rest runs. A run is a task with origin automation and provenance (definition, version, event id), admitted by the ordinary capacity ticket and isolated in a worktree or sandbox. Local schedules fire only while the Core runs.
- **Production wiring:** `crates/core-runtime` Scheduler and admission (`services/modbit-core`), `crates/event-store`, the forge event path of PX-008 and PX-009, `crates/automation` definitions of PX-082, the budget accounting of `crates/observability`; docs 14 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-083 / PX-E2E-083.
- **Failure and negative proof:** as in QUAL-PX-083.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-084"></a>

## PX-084 — Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches

- **Requirement:** REQ-PX-084; **related preserved requirements:** REQ-EV-0046, REQ-EV-0194, REQ-EV-0088.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M2.5, M6.7, PX-040, PX-082, DOC-PX-010.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-010; automations goal; release-critical (permissions and policy for runs with no person present; fail-closed behaviour and security of untrusted trigger payloads).
- **Specification requirements (doc 68, with verification tag):** AUT-A02 [STATIC], AUT-A03 [STATIC], AUT-C01 [STATIC], AUT-C03 [STATIC], AUT-D01 [STATIC], AUT-D02 [UNVERIFIED], AUT-D03 [STATIC], AUT-D04 [STATIC], AUT-D05 [UNVERIFIED], AUT-D06 [UNVERIFIED].
- **Scope and acceptance:** Automation runs execute as detached runs with a permission ceiling equal to the definition's capability profile intersected with its principal's policy; they cannot expand privilege interactively and no agent in the run can approve. A protected effect that needs approval parks the run in Needs Attention with an expiry (24 h by default, policy data) and a typed approval request addressed to the principal; on expiry the run is cancelled and receipted and the approval is closed; nothing is auto-approved in any run mode. Trigger payloads are untrusted external content with provenance (forge_pr, forge_comment, webhook), scanned for injection, and cannot change the definition, profile, budget or approvals. Native computer control is unavailable to automation runs. Credentials resolve through the broker for the principal. A global pause, a per-definition pause and the session emergency stop cancel runs; a definition disables itself after five consecutive failures and raises an attention item.
- **Production wiring:** `crates/policy` ceilings and capability intersection, `crates/effects` approval aggregate (expiry added), the attention manager of `services/modbit-core/src/attention.rs`, the injection scanner of `crates/browser` and the forge provenance of PX-008; docs 23 and 52 updated by the implementing task.
- **Real qualification:** QUAL-PX-084 / PX-E2E-084.
- **Failure and negative proof:** as in QUAL-PX-084.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-085"></a>

## PX-085 — Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source

- **Requirement:** REQ-PX-085; **related preserved requirements:** REQ-EV-0149, REQ-EV-0264.
- **Owner / milestone / release:** sandbox-cloud / M10 / RELEASE_ZERO; **prerequisites:** PX-011, M8.1, PX-082, PX-083, DOC-PX-010.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-010; automations goal; release-critical (network-facing intake that creates tasks for a tenant; authentication, replay protection and the cloud timer).
- **Specification requirements (doc 68, with verification tag):** AUT-B01 [STATIC], AUT-B04 [STATIC], AUT-B05 [UNVERIFIED], AUT-D07 [UNVERIFIED].
- **Scope and acceptance:** The Cloud API matches incoming forge events and generic signed webhooks to automations of the tenant and issues the canonical `CreateTask` through the same idempotency key (definition, version, event id); the cloud control plane keeps one durable time source per tenant for schedules of cloud automations, using the same slot ids and missed-run policy as the Core. A webhook verifies an HMAC over the body with a timestamp window and a nonce against replay, maps the sender to the tenant and definition, and rejects everything else with an audited reason; the secret is generated once, shown once and held by the broker. Cloud runs execute in the tenant's isolated execution under the signed policy bundle; organisation allow-lists and forced policy apply; nothing crosses the tenant boundary. The cloud never holds the local Core's definitions; cloud automations are defined in the cloud and local ones locally.
- **Production wiring:** `apps/cloud-api` webhook endpoint (PX-011), the cloud worker queue, the tenant policy bundle (`24_CLOUD_CONTROL_PLANE_AND_SYNC.md`), `crates/automation` shared definition types, the broker for the webhook secret; docs 24 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-085 / PX-E2E-085.
- **Failure and negative proof:** as in QUAL-PX-085.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-086"></a>

## PX-086 — Automations surface and CLI: list, editor with validation, enable approval, test run, run history and kill switches

- **Requirement:** REQ-PX-086; **related preserved requirements:** REQ-EV-0149, REQ-EV-0264, REQ-EV-0151.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-045, PX-046, PX-058, PX-082, PX-083, PX-084.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-010; automations goal; release-critical (enables and pauses unattended execution and shows approvals that authorise it; the enable approval must be exact).
- **Specification requirements (doc 68, with verification tag):** AUT-E01 [STATIC], AUT-E02 [STATIC], AUT-E03 [STATIC].
- **Scope and acceptance:** The surfaces of AUT-D03, E01 to E03 and the CLI parity of doc 29: an Automations area with a list (state, next slot, last run, principal), an editor that validates triggers, filters, schedule, profile, budget and workspace trust as the Core reports them, the enable approval dialog that lists the exact capabilities, paths and hosts and the definition hash and offers no way to enable without it, a test run that simulates a trigger payload through the filters and runs the dry-run posture (read-only, every protected effect denied and listed as what would have been asked) and says when the filters would have skipped the payload, run history per definition with typed reasons, retry and cancel, global and per-definition pause, the repository-supplied definition review, and an origin badge on automation tasks in the agent list. `modbit automation` commands in the CLI give the same operations.
- **Production wiring:** Renderer over the messages of PX-082 and PX-083 through preload, the approval stack of PX-058, the agent list of PX-046; `apps/cli` commands over the same protocol; no scheduling, validation or approval state in the renderer.
- **Real qualification:** QUAL-PX-086 / PX-E2E-086.
- **Failure and negative proof:** as in QUAL-PX-086.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-087"></a>

## PX-087 — Plugin package format extension and signed catalog manifest with strict parsing and containment

- **Requirement:** REQ-PX-087; **related preserved requirements:** REQ-EV-0138, REQ-EV-0225, REQ-EV-0137, REQ-EV-0181.
- **Owner / milestone / release:** extensions-hooks / M10 / RELEASE_ZERO; **prerequisites:** IMP-EV-0138, IMP-EV-0225, M5.5, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (the package and catalog formats are the input to every install; parser and containment are a security boundary).
- **Specification requirements (doc 69, with verification tag):** EXM-A01 [STATIC], EXM-A02 [STATIC], EXM-A03 [STATIC], EXM-A04 [STATIC], EXM-A05 [STATIC].
- **Scope and acceptance:** Extend the existing `modbit-extension.json` manifest (hooks, tools, commands, providers) with component paths for skills, subagent profiles, rules and external-server configuration, a variables schema (restricted JSON-Schema subset), declared capabilities (effect classes, hosts, hook events), minimum client version, licence, publisher and repository, with component discovery by convention and name normalisation. Parsing is strict and whole: no parent segments, absolute paths or schemes; symlinks resolved and contained; symlinked manifests and hook files refused; 10 MiB per file and a package cap; and nothing only the host may declare (an external server's trust, its read-only tools, its sites) is accepted from a package. Add a signed catalog manifest: entries with id, publisher identity, versions, source (a Git URL pinned to a commit SHA or a release asset with its SHA-256), signature, declared capabilities, categories, deprecation with replacement and an optional install count; the catalog's signing key is pinned when the user adds the source; a catalog is data and cannot execute or widen policy. The importer of IMP-EV-0137 maps other tools' plugin formats into the extended manifest with a migration report, always quarantined.
- **Production wiring:** `crates/tools/src/extensions.rs` manifest and signature code, `crates/skills` importer and registry, a catalog parser in the same owner, additive `ListCatalogEntries` and `AddCatalogSource` messages in `surface.proto`; docs 16, 26 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-087 / PX-E2E-087.
- **Failure and negative proof:** as in QUAL-PX-087.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-088"></a>

## PX-088 — Install, update and rollback pipeline with trust disclosure, digest-pinned fetch and hardened extraction

- **Requirement:** REQ-PX-088; **related preserved requirements:** REQ-EV-0225, REQ-EV-0240, REQ-EV-0138.
- **Owner / milestone / release:** extensions-hooks / M10 / RELEASE_ZERO; **prerequisites:** PX-087, M9.4, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (downloads and activates third-party code; supply chain and update semantics).
- **Specification requirements (doc 69, with verification tag):** EXM-B01 [STATIC], EXM-B02 [STATIC], EXM-B03 [STATIC], EXM-B04 [STATIC].
- **Scope and acceptance:** Installation from a catalog entry, a Git URL or a local directory: fetch by commit SHA or digest only (Git with prompts and askpass disabled, no repository hooks, batch-mode transport; release assets by SHA-256); verify digest and signature before extraction; extract with refusal of symlinks, special files, case- and Unicode-insensitive name collisions, encrypted and ZIP64 entries, more than 5,000 entries or more than 128 MiB expanded; install under the profile with a lock, keep the previous version for rollback and recover from a crash mid-install to one whole version; never execute anything at install time. Before activation show the publisher, source, signature state, declared capabilities, added external servers and hook events, variables and size (REQ-EV-0225); an unsigned or unverified package loads quarantined and inert until the person trusts that exact digest, an invalid signature is never trusted, and an install that adds an external server or a hook shows an impact confirmation. Updates never widen silently: a version that changes declared capabilities, adds a server or hook, or changes the hash of an external server's effective configuration needs fresh approval; versions can be pinned; deprecation shows its message and replacement; rollback restores the previous version; uninstall removes handlers, providers and caches and leaves no active component. Scopes are user and project; a project plugin list is committed with the repository and each user approves it by hash.
- **Production wiring:** `services/modbit-core/src/extensions.rs` load, trust and unload, `crates/git` hardened runner (PX-067), the profile store and lock, additive `InstallExtension`, `UpdateExtension`, `RollbackExtension`, `UninstallExtension` messages and impact views in `surface.proto`, the policy bundle for organisation allow, block and require; docs 16, 70 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-088 / PX-E2E-088.
- **Failure and negative proof:** as in QUAL-PX-088.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-089"></a>

## PX-089 — Plugin variables, secrets and hostile third-party content handling

- **Requirement:** REQ-PX-089; **related preserved requirements:** REQ-EV-0138, REQ-EV-0105, REQ-EV-0075.
- **Owner / milestone / release:** extensions-hooks / M10 / RELEASE_ZERO; **prerequisites:** PX-087, M9.3, M7.7, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (secret handling and the injection boundary for every third-party artifact).
- **Specification requirements (doc 69, with verification tag):** EXM-C01 [STATIC], EXM-C02 [STATIC].
- **Scope and acceptance:** Variables follow a restricted schema (types, enums, bounds, required); a variable whose name or schema marks it secret is write-only and goes to the secret broker by handle and is never written into a package file, log, event or prompt, token-shaped strings are redacted from logs, and a server configuration referencing a variable derives its schema automatically. Third-party text (skill and rule bodies, descriptions, tool descriptions, hook output) is scanned for injection, size-capped, stripped of control tags that imitate system messages, labelled with provenance and never treated as instructions that alter policy or capabilities. Plugin code (hooks, stdio tool servers) runs outside the Core under the sandbox with no ambient secrets and cannot weaken a deny. In cloud tasks only packages the tenant policy allows are used and the plugin cache is per tenant.
- **Production wiring:** `crates/secrets` broker, `crates/browser` injection scanner generalised to artifact text (the pattern of M7.7), `crates/sandbox` and `crates/terminal` for stdio servers, `crates/skills` and the hook bus; docs 23 and 52 updated by the implementing task.
- **Real qualification:** QUAL-PX-089 / PX-E2E-089.
- **Failure and negative proof:** as in QUAL-PX-089.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-090"></a>

## PX-090 — External tool server trust by configuration hash, annotation classes, OAuth and connection state

- **Requirement:** REQ-PX-090; **related preserved requirements:** REQ-EV-0224, REQ-EV-0194, REQ-EV-0177.
- **Owner / milestone / release:** external-tools / M10 / RELEASE_ZERO; **prerequisites:** M9.4, PX-089, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (trust decisions for programs that run against the workspace; credentials and OAuth tokens).
- **Specification requirements (doc 69, with verification tag):** EXM-C03 [STATIC].
- **Scope and acceptance:** Complete the external-server trust model in place: trust is bound to the hash of the effective configuration (command, arguments, environment names, URL, header names, allowed tools) and an edit re-prompts; tools are classified read, write or unannotated from the server's annotations (unannotated counts as write), with per-server and per-tool enable and a read and write group policy; OAuth with PKCE and dynamic client registration, a refresh lock so executors do not refresh concurrently, and tokens held by the broker; the connection follows an explicit state machine (idle, connecting, connected, failed, needs-auth) with one transport fallback, non-retryable errors terminal and coalesced initialisation triggers; a needs-auth or errored server writes a status note that tells the model what to do and exposes a synthetic authentication tool in place of its real tools. Proposing is still not installing.
- **Production wiring:** `crates/mcp` and `services/modbit-core/src/mcp.rs`, `ProposeExternalServer` and `TrustExternalServer` extended with the configuration hash, `crates/secrets` for tokens; docs 16 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-090 / PX-E2E-090.
- **Failure and negative proof:** as in QUAL-PX-090.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-091"></a>

## PX-091 — Agent self-extension write-protection map and organisation plugin and server policy

- **Requirement:** REQ-PX-091; **related preserved requirements:** REQ-EV-0031, REQ-EV-0039, REQ-EV-0136.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M9.3, PX-088, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (permissions and policy: what the agent may write about itself, and organisation allow, block and require lists).
- **Specification requirements (doc 69, with verification tag):** EXM-B04 [STATIC], EXM-C04 [STATIC], EXM-C05 [STATIC].
- **Scope and acceptance:** The Kernel's protected-path map separates data the agent may author (skills, rules, commands, subagent profiles) from control files it may not write (hooks, external-server configuration, permissions and policy, trust markers, the files of installed packages), enforced after symlink resolution and independent of run mode; a write to a control file is denied with the typed reason and enabling anything the agent authored as an active control still needs the person. Organisation policy can allow, block or require packages and set an external-server allow-list with wildcard and CIDR matching, import and export it with a fingerprint check for optimistic concurrency, and re-evaluates running servers when it changes (a blocked server shows the admin reason and a retryable flag). Unknown policy verdicts fail closed.
- **Production wiring:** `crates/policy` configuration resolver (REQ-EV-0039), `crates/effects` path policy, the policy bundle of `24_CLOUD_CONTROL_PLANE_AND_SYNC.md`, `crates/mcp` re-evaluation; docs 23 and 24 updated by the implementing task.
- **Real qualification:** QUAL-PX-091 / PX-E2E-091.
- **Failure and negative proof:** as in QUAL-PX-091.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-092"></a>

## PX-092 — Hook contract completion: merge order, exit-code blocking, fail-closed permission events, trusted-workspace gating and execution log

- **Requirement:** REQ-PX-092; **related preserved requirements:** REQ-EV-0139, REQ-EV-0239, REQ-EV-0042, REQ-EV-0242.
- **Owner / milestone / release:** extensions-hooks / M10 / RELEASE_ZERO; **prerequisites:** IMP-EV-0139, IMP-EV-0239, PX-049, DOC-PX-011.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (hooks can allow, deny or alter effects; ordering and fail-closed behaviour are policy semantics).
- **Specification requirements (doc 69, with verification tag):** EXM-D01 [STATIC].
- **Scope and acceptance:** Complete the Hook Bus contract in place, adding nothing that already exists: a typed event catalogue (Modbit chooses its own events and documents each); JSON request and response; exit code 2 blocks with a reason; permission events fail closed by default, others by opt-in; parallel responses merge deny over ask over allow; a timeout (5 s default) and a loop limit; project hooks run only in trusted workspaces and a configuration path containing a symlink below the workspace root is refused; every run is written to an execution log (step, hook, source, timeout, command, duration, exit code, input, output, error) the user can read; a hook cannot grant beyond the Kernel's envelope (monotonic deny, QUAL-EV-0239); hook context is size-capped with control tags neutralised; a session-end hook runs to completion before exit or fails visibly and is journaled (PX-049).
- **Production wiring:** The Hook Bus in `crates/tools` and `services/modbit-core/src/hooks.rs`, `ListHooks` extended with the execution log read, the trust gate of `TrustRepository`; docs 16, 23 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-092 / PX-E2E-092.
- **Failure and negative proof:** as in QUAL-PX-092.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-093"></a>

## PX-093 — Customize surface: seven tabs, scopes, view options, catalog browsing, trust disclosure, update and the hook log

- **Requirement:** REQ-PX-093; **related preserved requirements:** REQ-EV-0225, REQ-EV-0138, REQ-EV-0114.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-045, PX-052, PX-087, PX-088, PX-090, PX-092.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-011; 4 (customize and extensibility); release-critical (installs, trusts and enables third-party code from one surface; the trust disclosure and confirmations must be exact).
- **Specification requirements (doc 69, with verification tag):** EXM-A04 [STATIC], EXM-B01 [STATIC], EXM-E01 [LIVE], EXM-E02 [STATIC], EXM-E03 [LIVE].
- **Scope and acceptance:** The surface of EXM-E01 to E03 and EXM-A04: Customize replaces the scattered CLI-only management with one surface of seven tabs in a fixed order (Plugins, MCP servers, Skills, Subagents, Rules, Commands, Hooks), scopes user, project and plugin, per-tab view options (MCP servers group by status or scope and sort by name or most used; the others group by source or author, filter all, local, workspace or plugin and sort by name or author; no Installed filter), an empty state per tab, an Add menu with three sources (create here, import, install from the catalog) and a New action that asks for a name first. The Plugins tab browses catalogs with search and categories, shows the detail view with the trust disclosure and impact confirmation of EXM-B01, installs with a scope choice and shows update, pin, rollback and remove; load failures are typed with copyable details; the Hooks tab shows the execution log; rules show their type and the inspector shows which rules entered the prompt. Nothing is activated by browsing or importing, and no ratings or reviews appear.
- **Production wiring:** Renderer over the catalog, install, extension, skills (`ListSkills`, PX-052), external-server, hook and rule messages through preload; the settings shell of PX-045; no package parsing, signature checking, fetching or trust logic in the renderer.
- **Real qualification:** QUAL-PX-093 / PX-E2E-093.
- **Failure and negative proof:** as in QUAL-PX-093.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-094"></a>

## PX-094 — Window chrome: inset title bar, drag regions, window-control placement, optional translucency and persisted window state

- **Requirement:** REQ-PX-094; **related preserved requirements:** REQ-EV-0076.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-049, PX-045, DOC-PX-012.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-012; shell platform goal; release-critical (Electron main-process window options and platform behaviour; adjacent to the security boundary).
- **Specification requirements (doc 78, with verification tag):** ESH-A01 [UNVERIFIED], ESH-A02 [UNVERIFIED], ESH-B01 [LIVE], ESH-B02 [LIVE], ESH-B03 [STATIC], ESH-B04 [STATIC].
- **Scope and acceptance:** Main-process window changes that leave `webPreferences`, the CSP, the preload and the IPC channel set byte-identical. On macOS an inset hidden title bar with the 40 pt top bar of PX-045 as the drag region and the window controls inside the agent-list strip, positions measured per supported operating-system version; interactive controls opt out of dragging; double-click on the bar follows platform convention; a hairline border where the platform draws none. Translucency is opt-in and off by default with an opaque fallback for every translucent surface and a Reduce transparency preference that follows the operating system and overrides it. Window position, size, maximised and fullscreen state persist per display configuration and are validated on restore. A UI zoom model scales from a 13 px baseline in 8 percent steps with a clamp, persisted per profile. Windows and Linux use platform window options behind capability checks and stay CI_COMPATIBLE.
- **Production wiring:** `apps/desktop/src/main/main.ts` window creation and state, the renderer top bar and zoom of PX-044 and PX-045, the profile preference store; docs 32 and 39 updated by the implementing task.
- **Real qualification:** QUAL-PX-094 / PX-E2E-094.
- **Failure and negative proof:** as in QUAL-PX-094.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-095"></a>

## PX-095 — Native menus, badge, tray icon, deep links with confirmation, single instance and native dialogs

- **Requirement:** REQ-PX-095; **related preserved requirements:** REQ-EV-0076, REQ-EV-0151.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-088, PX-090, PX-094, PX-045, DOC-PX-012.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-012; shell platform goal; release-critical (a new inbound surface (deep links) and native dialogs that carry paths into the app).
- **Specification requirements (doc 78, with verification tag):** ESH-A01 [UNVERIFIED], ESH-A02 [UNVERIFIED], ESH-C01 [DISK], ESH-C02 [STATIC], ESH-C03 [UNVERIFIED], ESH-C04 [STATIC], ESH-C05 [UNVERIFIED].
- **Scope and acceptance:** A native application menu whose accelerators come from the shortcut registry of PX-045 (so the menu and the keyboard never disagree), with task-acting items disabled when no task is selected; a dock or taskbar badge counting items needing attention only; a tray or menu-bar icon with template variants for attention, running and unread in light and dark, redrawn for Modbit, offering the fleet views and quit; notifications as today with a click that focuses the right window and task. A `modbit://` protocol handler with a single-instance lock opens tasks, approvals, settings and install confirmations: routes are an allow-list, names and identifiers are validated, a link never performs a state-changing action by itself (an install link always opens the confirmation of PX-088 or PX-090, an approval link only focuses the card), and a link reaching the handler from an untrusted page never implies the user's session. Native file and folder dialogs run in main and return a path only for display; the Core re-validates path policy and trust before use.
- **Production wiring:** `apps/desktop/src/main/main.ts` menu, tray, badge, protocol and dialog code, the shortcut registry of PX-045, the attention view of PX-042, the install confirmations of PX-088 and PX-090; the new inbound routes are added to the pinned IPC and route inventory; docs 32 and 39 updated by the implementing task.
- **Real qualification:** QUAL-PX-095 / PX-E2E-095.
- **Failure and negative proof:** as in QUAL-PX-095.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-096"></a>

## PX-096 — Main-process hardening: fuses, ASAR integrity, permission and navigation denials, IPC inventory and main-thread responsiveness

- **Requirement:** REQ-PX-096; **related preserved requirements:** REQ-EV-0075, REQ-EV-0076.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-049, M10.2, DOC-PX-012.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-012; shell platform goal; release-critical (security boundary of the privileged process: fuses, integrity, permissions and the IPC surface).
- **Specification requirements (doc 78, with verification tag):** ESH-A01 [UNVERIFIED], ESH-D01 [UNVERIFIED], ESH-D02 [STATIC], ESH-D03 [UNVERIFIED], ESH-D05 [LIVE], ESH-F01 [DISK], ESH-F02 [DISK].
- **Scope and acceptance:** Packaged-binary fuses set to run-as-node off, node-options environment variable off, node inspector arguments off, embedded ASAR integrity validation on, only-load-from-ASAR on, cookie encryption on, file-protocol extra privileges off, with a CI step that reads the fuse state from the packaged binary and fails on any difference. The app's own session denies every permission request and check; the browser partitions keep their minimal list with every denial named; attaching web views is denied and navigation of the app window is limited to the app's own origin; a test pins the CSP including `connect-src 'none'`. Every IPC channel has a schema and a sender check and a generated channel inventory is pinned by a test so a new channel is a reviewed change; the preload exposes only typed functions by name. Main never blocks its event loop on a Core call: main-thread lag is at most 50 ms at p95 under load. A renderer responsiveness watchdog (ping interval and block threshold) treats a block as a hang and reuses the existing recovery; lifecycle participants start and stop in a defined order with a bounded shutdown wait. Heavy and background work stays in the Core.
- **Production wiring:** Packaging configuration of M10.2 (fuses, ASAR integrity), `apps/desktop/src/main/main.ts` session handlers and the channel inventory, `tools/` CI step, the existing IPC refusal audit; docs 32, 52 and 70 updated by the implementing task. Fuse values the Core supervisor or browser host cannot tolerate are recorded and justified, never silently relaxed.
- **Real qualification:** QUAL-PX-096 / PX-E2E-096.
- **Failure and negative proof:** as in QUAL-PX-096.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-097"></a>

## PX-097 — Update application flow: apply at idle, staged and verified, with rollback and truthful states

- **Requirement:** REQ-PX-097; **related preserved requirements:** REQ-EV-0258, REQ-EV-0242.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M10.2, PX-049, PX-096, DOC-PX-012.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-012; shell platform goal; release-critical (applies new privileged code to a running installation; recovery and integrity of the update path).
- **Specification requirements (doc 78, with verification tag):** ESH-A01 [UNVERIFIED], ESH-D04 [UNVERIFIED], ESH-F02 [DISK].
- **Scope and acceptance:** On top of the signing, update channel and SBOM that M10.2 owns, the desktop applies updates only when no run is active and no approval is pending, or after the quit prompt of PX-049 with the resume choice; the update is staged, its signature and digest verified before it is applied, the previous version kept for rollback, and the states (available, downloading, ready, applying, applied, failed) are shown with cause and next action and never claim progress the updater did not report. A failed update leaves the running version intact and records why. The helper binaries that ship in the bundle (PX-072) update with the application and a protocol mismatch between Core and a helper after an update refuses with a typed reason. Update checks respect a configured policy (organisation pause, channel) and never run during a protected effect.
- **Production wiring:** The updater of M10.2 (`apps/desktop` main and the release pipeline), the quit protection of PX-049, the run and approval state served by the Core, the packaging of PX-072; docs 70 and 71 updated by the implementing task.
- **Real qualification:** QUAL-PX-097 / PX-E2E-097.
- **Failure and negative proof:** as in QUAL-PX-097.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-098"></a>

## PX-098 — Electron and Chromium version policy and upgrade qualification gate

- **Requirement:** REQ-PX-098; **related preserved requirements:** REQ-EV-0075.
- **Owner / milestone / release:** governance / M10 / RELEASE_ZERO; **prerequisites:** PX-096, PX-073, DOC-PX-012.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-03-012; shell platform goal; release-critical (supply chain and security patch cadence of the privileged runtime; a release gate).
- **Specification requirements (doc 78, with verification tag):** ESH-A01 [UNVERIFIED], ESH-E01 [LIVE].
- **Scope and acceptance:** A written policy in the build documentation and enforced in CI: Electron and its Chromium are pinned exactly and recorded in the build manifest and the SBOM; the project tracks a supported stable major and never ships a version past its end of support; a security-update response window is stated and a missed window blocks the release candidate; a version moves only through a qualification gate that runs the browser host E2E, the CDP deny list and origin gate suites (PX-073), the security suites, the fuse check (PX-096), the accessibility suite and the packaged smoke run, and records the result as run evidence; the gate fails when any suite is skipped. The check that the pinned version is within support runs on every CI build and on a schedule.
- **Production wiring:** `tools/` CI scripts and the release workflow (`.github/workflows`), `apps/desktop/package.json` pin, the build manifest of doc 70; docs 70 and 74 updated by the implementing task. The architecture-lint locked rules file is touched only if a rule is added and then under this record's trailer.
- **Real qualification:** QUAL-PX-098 / PX-E2E-098.
- **Failure and negative proof:** as in QUAL-PX-098.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-099"></a>

## PX-099 — Terminal flow control (ACK, resize, bounded ring) and agent-facing shell.input and shell.attach tools

- **Requirement:** REQ-PX-099; **related preserved requirements:** REQ-EV-0271, REQ-EV-0100, REQ-EV-0026.
- **Owner / milestone / release:** terminal / M10 / RELEASE_ZERO; **prerequisites:** PX-043, M2.3, M4.5, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (protocol and schema addition, execution and a new input path into a live process).
- **Audit source (BLD-02):** BLD-02 (audit-D areas 33 to 35): execd has no ACK, no resize frame and no ring; the agent has no stdin or attach tool; first missing link: the attach protocol in `exec.proto`.
- **Specification requirements (doc 79, with basis):** ADC-A01 [STATIC-AUDIT], ADC-A02 [STATIC-AUDIT].
- **Scope and acceptance:** Complements PX-043, which serves the stream and the registry to clients by cursor. This row adds the transport semantics and the agent tools the audit found absent. An attachment is acknowledged: the client acknowledges the last cursor it consumed and the broker never holds more than a fixed window of unacknowledged bytes per attachment; a slow client is dropped to cursor-pull instead of growing memory. Resize frames change the PTY size and are recorded as events. The agent-facing `shell.input` (a typed, size-bounded, secret-redacted stdin write to a shell this task owns, classified by the shell effect classes) and `shell.attach` (read the live tail from a cursor) tools are registered in the Tool Registry and projected only while the task owns a live shell. A shell is addressable only by the task that started it. A lost transport never ends the process: the durable broker keeps it running.
- **Production wiring:** `services/modbit-execd/src/broker.rs` (attach, ring and on-disk index of the audit fix FIX-20), `crates/terminal` (`attach_fenced`, `write_stdin`), tool specs in `crates/tools/src/direct.rs`, additive ACK and Resize frames in `exec.proto`; docs 17, 21 and 30 are updated by the implementing task (docs 17 and 21 are ahead of the code today).
- **Real qualification:** QUAL-PX-099 / PX-E2E-099.
- **Failure and negative proof:** as in QUAL-PX-099.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-100"></a>

## PX-100 — CLI and IDE-adapter controls for task mode and execution preference

- **Requirement:** REQ-PX-100; **related preserved requirements:** REQ-EV-0031, REQ-EV-0218.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-051, PX-053, PX-002, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (a client sends a typed field that selects a capability posture and a routing preference; the Core enforces, but wrong client wiring would present a posture that is not enforced).
- **Audit source (BLD-03):** BLD-03 (audit-G area 37, audit-I): nothing in the CLI or the IDE client can drive routing or mode; `SetExecutionPreference` is in no proto; first missing link: client verbs over PX-051 and PX-053.
- **Specification requirements (doc 79, with basis):** ADC-A03 [STATIC-AUDIT].
- **Scope and acceptance:** PX-051 gives a task a typed mode and PX-053 wires `SetExecutionPreference`; both are Core commands. This row gives the headless CLI (`modbit task run --mode plan --objective cost`, `modbit task mode`, `modbit task preference`) and the IDE adapter the same controls through the thin-client conformance contract of PX-001, and renders the Core's typed refusals (policy-blocked model, unknown mode) and the recorded routing reason. A client never supplies a posture, only the mode name, and never computes a refusal. Mode and preference changes made from one client appear in the others through the event log.
- **Production wiring:** `apps/cli` commands, `packages/ide-adapter-core` and the VS Code adapter over `CreateTask.mode`, `SetTaskMode` and `SetExecutionPreference`; the conformance suite of doc 29 gains the three commands; no Core change beyond PX-051 and PX-053.
- **Real qualification:** QUAL-PX-100 / PX-E2E-100.
- **Failure and negative proof:** as in QUAL-PX-100.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-101"></a>

## PX-101 — PauseTask and ResumeTask over the runtime park, with local persistence

- **Requirement:** REQ-PX-101; **related preserved requirements:** REQ-EV-0127, REQ-EV-0121, REQ-EV-0101.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M2.7, M6.7, M4.4, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (protocol addition and a recovery-bearing state transition of a running task).
- **Audit source (BLD-04):** BLD-04 (audit-B area 2): there is no PauseTask or ResumeTask command although `Runtime::park` exists; a detached CLI run died after 30 s before the audit fix FIX-17; first missing link: the two commands and their persistence.
- **Specification requirements (doc 79, with basis):** ADC-A04 [STATIC-AUDIT].
- **Scope and acceptance:** `PauseTask` parks the run at the next safe boundary through the existing `Runtime::park` (an in-flight tool call finishes or is reconciled by its existing cancellation domain, never abandoned silently) and records a typed paused state with the reason and the actor; `ResumeTask` re-enters the loop under a new lease generation. Pause survives a Core restart: the task comes back paused, not running and not failed. A paused task holds its worktree and its checkpoint and releases its capacity ticket only after the configured idle bound. A pending approval stays pending while paused. The worker `:pause` refusal of the cloud worker is aligned (a typed unsupported-with-reason answer until a cloud pause exists). Both commands are idempotent per command id and fenced by the session lease.
- **Production wiring:** `services/modbit-core` command handling (`server.rs`), `crates/core-runtime` park and admission, `crates/domain` task and run state machines (an additive paused reason, not a new state if the machine already parks), additive `PauseTask` and `ResumeTask` messages in `surface.proto`; docs 13, 14 and 30 updated by the implementing task (doc 13 is a locked path and the implementing task cites this record).
- **Real qualification:** QUAL-PX-101 / PX-E2E-101.
- **Failure and negative proof:** as in QUAL-PX-101.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-102"></a>

## PX-102 — Named checkpoints, retention and garbage collection

- **Requirement:** REQ-PX-102; **related preserved requirements:** REQ-EV-0012, REQ-EV-0013, REQ-EV-0123.
- **Owner / milestone / release:** durability / M10 / RELEASE_ZERO; **prerequisites:** PX-061, M4.3, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (deletion of recovery data under a policy and persistence of checkpoint metadata).
- **Audit source (BLD-04):** BLD-04 (audit-D area 15): no named checkpoints and no garbage collection; restore ignored `git_head` before the audit fix FIX-18; first missing link: the label field and a collector.
- **Specification requirements (doc 79, with basis):** ADC-A05 [STATIC-AUDIT].
- **Scope and acceptance:** PX-061 records a checkpoint at every turn and makes restore exactly reversible. This row adds a user label on a checkpoint (a typed `NameCheckpoint` command, unique per task), a retention policy with defaults (keep every checkpoint of a live task, keep labelled and fork-parent checkpoints, thin the rest of a finished task by age and count, drop all of an archived and applied task after a grace period) and a collector that runs under a lease with an owner and reason, reports scanned, removed and bytes freed, is idempotent, and never removes a baseline that a surviving delta chain needs or a checkpoint referenced by a fork, a pending restore or a pre-restore record. Collection is a receipted effect; a crash mid-collection leaves every remaining chain restorable.
- **Production wiring:** `crates/checkpoint` retention and the object store of M4.3, the lease of the durability owner, additive `NameCheckpoint` and `RunCheckpointGc` messages in `surface.proto`; doc 19 updated by the implementing task.
- **Real qualification:** QUAL-PX-102 / PX-E2E-102.
- **Failure and negative proof:** as in QUAL-PX-102.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-103"></a>

## PX-103 — Hunk attribution to the originating tool call and per-hunk decisions that keep the run going

- **Requirement:** REQ-PX-103; **related preserved requirements:** REQ-EV-0016, REQ-EV-0270, REQ-EV-0067.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** M2.9, M4.3, PX-061, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (effect-bearing review decisions on a live run and a new attribution record on canonical change evidence).
- **Audit source (BLD-05):** BLD-05 (audit-D area 17): shell and test writes bypassed the barrier before the audit fix FIX-06 and hunks carry no ToolCall attribution; review decisions need the task to be at a review boundary; first missing link: attribution records and decisions while running.
- **Specification requirements (doc 79, with basis):** ADC-A06 [STATIC-AUDIT].
- **Scope and acceptance:** Each hunk in the review projection carries the tool call, turn and run that produced it, taken from the Change Engine transaction for `change.*` tools and from the bounded snapshot diff of shell and test tools (the barrier delivered by the audit fix FIX-06); a hunk that cannot be attributed is labelled unattributed and is never auto-accepted. A person can accept or reject individual hunks while the run continues; the rejected hunk is reverted through a ChangeTransaction against the exact candidate revision, the agent receives a typed notice with the reason, and the run continues without a restart. A decision against a revision that moved since the review view is refused as stale. Acceptance of the final result at the review boundary is unchanged.
- **Production wiring:** `services/modbit-core/src/review.rs` and `undo.rs`, the Change Engine of `crates/workspace`, the snapshot-diff attribution of `crates/tools`, the checkpoint of PX-061, additive attribution fields and a `DecideHunk` message in `surface.proto`; docs 20 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-103 / PX-E2E-103.
- **Failure and negative proof:** as in QUAL-PX-103.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-104"></a>

## PX-104 — Review reachable from any task state, with live per-hunk controls and the originating-call chip

- **Requirement:** REQ-PX-104; **related preserved requirements:** REQ-EV-0151, REQ-EV-0016.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-048, PX-103, PX-062, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (drives effect-bearing hunk decisions and reverts through the desktop).
- **Audit source (BLD-05):** BLD-05 (audit-A area 1): Review opens only at a review boundary; `codeView` is not wired to a live Changes panel; first missing link: the live panel over PX-103.
- **Specification requirements (doc 79, with basis):** ADC-A07 [STATIC-AUDIT].
- **Scope and acceptance:** The Changes tab of PX-048 becomes reachable in every task state (running, waiting, needs attention, failed, completed) and shows the live revision-bound diff. Each hunk shows an originating-call chip that opens the call in the conversation, and per-hunk Accept and Reject controls that call `DecideHunk` of PX-103 and render the Core's typed result (applied, stale, refused). The stale state is explained and offers a refresh. Rejecting while a run is live shows the agent notice that the Core will send. The surface decides nothing: eligibility, staleness and attribution come from the Core.
- **Production wiring:** Renderer Changes tab and review bridge in `apps/desktop` over `GetReviewView`, `DecideHunk` and the attribution fields, the existing `codeView` bridge; no filesystem, Git or policy logic in the renderer.
- **Real qualification:** QUAL-PX-104 / PX-E2E-104.
- **Failure and negative proof:** as in QUAL-PX-104.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-105"></a>

## PX-105 — Skills a user can use: trust by command, a System scope, an index under a budget, skill.load and path gating

- **Requirement:** REQ-PX-105; **related preserved requirements:** REQ-EV-0114, REQ-EV-0105, REQ-EV-0213, REQ-EV-0181.
- **Owner / milestone / release:** skills / M10 / RELEASE_ZERO; **prerequisites:** M5.5, PX-052, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (trust, scope and prompt content of skills are policy-bearing and change what reaches the model).
- **Audit source (BLD-07):** BLD-07 (audit-E areas 20 and 21): skills are inert for normal users because trust needs a signing key or an environment flag, there is no System scope, no index and no skill.load; first missing link: the trust command and the index segment.
- **Specification requirements (doc 79, with basis):** ADC-A08 [STATIC-AUDIT].
- **Scope and acceptance:** PX-052 lists skills for the slash menu. This row makes them usable. `modbit skill trust <name>@<hash>` records a per-profile, content-hash-bound trust decision (a changed file is untrusted again; the environment flag is not a trust path), `skill revoke` withdraws it, and an administrator System scope sits above user and project scopes with its own precedence and the right to forbid. The prompt compiler adds a skill index segment (name and one line each) under one aggregate token budget with ordered degradation (drop path-gated skills not matching, then shorten, then truncate with a count), and a read-only `skill.load` tool returns a trusted skill's instructions, procedures or resources on demand, bounded and provenance-labelled. A skill with `paths` is indexed only when a matching path is active. Imported skills are quarantined until trusted. Untrusted content never reaches the model.
- **Production wiring:** `crates/skills` registry and compiler, `services/modbit-core/src/skills.rs`, `apps/cli` verbs, `crates/prompt-compiler` index segment, tool spec in `crates/tools`, a System scope source in the device and admin policy files; docs 26 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-105 / PX-E2E-105.
- **Failure and negative proof:** as in QUAL-PX-105.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-106"></a>

## PX-106 — Customize view over the existing skills, hooks and extensions (no catalog)

- **Requirement:** REQ-PX-106; **related preserved requirements:** REQ-EV-0114, REQ-EV-0139, REQ-EV-0128.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-044, PX-045, PX-052, PX-105, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group A, reachability of the Core (clients, review, skills); release-critical (drives trust, enable and revoke decisions that change what runs and what reaches the model).
- **Audit source (BLD-07):** BLD-07 (audit-E area 55): management is CLI-only; the marketplace and its Customize surface (PX-087 to PX-093) belong to DR-PX-2026-10-03-011, which is not ratified; first missing link: a view over the existing registries.
- **Specification requirements (doc 79, with basis):** ADC-A09 [STATIC-AUDIT].
- **Scope and acceptance:** A Customize area with three lists: skills (scope, trust state, hash, provenance, invocation, the budget share of the index), hooks (event, command, scope, trusted-workspace gate, the last results from the existing hook journal) and installed extensions with their MCP servers (scope, trust, health). Actions are limited to what the Core already offers: trust and revoke a skill by hash through PX-105, enable and disable a hook or extension at its scope, import through the existing importer (quarantined until trusted), and open the source. There is no catalog, no install from a network location and no update flow; those belong to the unratified record. If DR-PX-2026-10-03-011 is accepted later, PX-093 extends this view and must not create a second Customize surface.
- **Production wiring:** Renderer area over `ListSkills`, the hook and extension list commands and the trust commands of PX-105 through preload; the primitives and tray host of PX-044; no registry or policy logic in the renderer.
- **Real qualification:** QUAL-PX-106 / PX-E2E-106.
- **Failure and negative proof:** as in QUAL-PX-106.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-107"></a>

## PX-107 — AGENTS.md and CLAUDE.md as a trust-gated native rules layer

- **Requirement:** REQ-PX-107; **related preserved requirements:** REQ-EV-0059, REQ-EV-0105, REQ-EV-0129, REQ-EV-0093.
- **Owner / milestone / release:** skills / M10 / RELEASE_ZERO; **prerequisites:** IMP-EV-0129, IMP-EV-0059, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (repository text becomes prompt instructions: a trust and injection boundary).
- **Audit source (BLD-08):** BLD-08 (audit-C defect D3, audit-E): AGENTS.md and CLAUDE.md are never auto-loaded, so a repository's own instructions never reach its agent unless imported by hand; first missing link: a native layer in `rules.rs`.
- **Specification requirements (doc 79, with basis):** ADC-B01 [STATIC-AUDIT].
- **Scope and acceptance:** The instruction compiler adds a native layer that reads `AGENTS.md` at the workspace root and in directories on the path to the files being touched, and `CLAUDE.md` likewise, with deterministic precedence (device and admin policy, then the workspace root file, then nearer directory files; `AGENTS.md` before `CLAUDE.md` at one level; the user's profile rules unchanged), a per-file and an aggregate size cap with a visible truncation marker, content-hash provenance on every layer in the Context Inspector, and the existing path-scoped lazy loading. The layer loads only in a trusted workspace (the existing workspace trust decision); in an untrusted workspace the files are listed as not loaded with the reason. The text is instruction data with provenance, scanned by the injection scanner, and can never widen capabilities, change policy or name tools.
- **Production wiring:** `services/modbit-core/src/rules.rs` and `crates/prompt-compiler` rules segment, the workspace trust of `crates/workspace`, the Inspector projection in `services/modbit-core/src/inspector.rs`; docs 26 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-107 / PX-E2E-107.
- **Failure and negative proof:** as in QUAL-PX-107.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-108"></a>

## PX-108 — Goal-seeded pre-turn context pack

- **Requirement:** REQ-PX-108; **related preserved requirements:** REQ-EV-0001, REQ-EV-0166, REQ-EV-0002.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.7, M3.8, PX-015, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (changes what code evidence reaches the model before the first turn and records provenance on a canonical pack).
- **Audit source (BLD-08):** BLD-08 (audit-C area 7): the L0 to L3 planner runs only if the model calls a context tool; stale fragments were only partly fixed by the audit fix FIX-12; first missing link: a pre-turn planner invocation.
- **Specification requirements (doc 79, with basis):** ADC-B02 [STATIC-AUDIT].
- **Scope and acceptance:** Before the first model turn of a run, and after a compaction epoch or a material goal change, the Core runs the existing planner on the task goal, the failing checks of the baseline and the files the user attached or named, and compiles a bounded Context Pack with provenance, revision and the reasons for inclusion, counted in the context breakdown. The pack uses the existing token budget and ledger, is invalidated by the writes of the task and by revision change, and is dropped or re-hydrated at compile time when a fragment is stale. It supplements and never replaces model-initiated retrieval, and a task with no retrievable evidence records an empty pack with the reason instead of padding. The planner stays the single retrieval entry; this row adds a caller.
- **Production wiring:** `services/modbit-core/src/runtime.rs` pre-turn step, `crates/retrieval` planner, `crates/context` pack compiler and ledger, `crates/prompt-compiler` pack segment; doc 18 updated by the implementing task.
- **Real qualification:** QUAL-PX-108 / PX-E2E-108.
- **Failure and negative proof:** as in QUAL-PX-108.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-109"></a>

## PX-109 — Structured compaction summary with extractive fail-closed fallback, model-aware thresholds and a searchable transcript pointer

- **Requirement:** REQ-PX-109; **related preserved requirements:** REQ-EV-0092, REQ-EV-0130, REQ-EV-0057, REQ-EV-0058, REQ-EV-0268.
- **Owner / milestone / release:** durability / M10 / RELEASE_ZERO; **prerequisites:** M4.2, IMP-EV-0130, M2.6, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (rewrites the model's working memory of a run and adds a model call on the recovery spine).
- **Audit source (BLD-09):** BLD-09 (audit-C area 14): the cut and the fact promotion were fixed by the audit fix FIX-07; the summary itself is extractive only; the trigger is a fixed token budget; first missing link: a registry-role summarizer and its validation.
- **Specification requirements (doc 79, with basis):** ADC-B03 [STATIC-AUDIT].
- **Scope and acceptance:** Compaction epochs gain a summarizer role resolved through the model registry (a model named by policy, never hard-coded) that produces a structured summary (goal, decisions with their event references, files and symbols touched, open failures, next steps, user constraints) from the compaction manifest, not from tool text. The summary is validated against the manifest (every cited event and file exists, no fact the manifest lacks, a size cap) and any failure, timeout or refusal falls back to the extractive summary of FIX-07 with a typed reason on the epoch. Trigger thresholds derive from the active model's context window and output budget with hysteresis, not a constant. After compaction the Core re-attaches the plan and todo state and a pointer to the searchable transcript so the model can fetch exact earlier detail. Stale results are still rejected by the epoch guard, fork and revert still cancel in-flight compaction, and the summary is untrusted prompt content with provenance.
- **Production wiring:** `crates/compaction`, `services/modbit-core/src/runtime.rs` compaction step and the registry role resolution of `services/modbit-core/src/model_registry.rs`, `crates/prompt-compiler` re-attach segment, the transcript search of `crates/context`; docs 19 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-109 / PX-E2E-109.
- **Failure and negative proof:** as in QUAL-PX-109.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-110"></a>

## PX-110 — Code graph: symbol reference and call edges, implementors and non-test impact selection

- **Requirement:** REQ-PX-110; **related preserved requirements:** REQ-EV-0157, REQ-EV-0164, REQ-EV-0005, REQ-EV-0155.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.3, M3.6, IMP-EV-0157, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (changes the evidence the model and the verification selection rely on (evidence semantics)).
- **Audit source (BLD-10):** BLD-10 (audit-C area 11): the graph is file-level; there are no reference, caller or implementor edges and impact selection lists tests only; first missing link: identifier occurrence resolution in `crates/retrieval`.
- **Specification requirements (doc 79, with basis):** ADC-B04 [STATIC-AUDIT].
- **Scope and acceptance:** Extend the existing tree-sitter symbol index with identifier occurrences resolved by name and import to definitions (an LSP upgrade where a Tier A language server is available, never a requirement), producing reference, caller, callee and implementor edges, each with a confidence class (resolved, ambiguous, unresolved) and the revision. Interface-to-implementation edges are derived for the Tier A languages. `ImpactSelection` returns impacted non-test source files and the tests, each with the edge path and confidence, feeds the staged test targeting of PX-035 and the planner's bounded graph expansion, and says plainly when a result is partial. The graph is derived and rebuildable; the filesystem and revision stay canonical. This is the single-repository graph: cross-repository qualification stays deferred.
- **Production wiring:** `crates/retrieval` (symbols, graph, impact), the dependency and evidence graph of M3.6, the bridge of M3.4, the impact consumer of the verification engine; docs 18 and 28 updated by the implementing task.
- **Real qualification:** QUAL-PX-110 / PX-E2E-110.
- **Failure and negative proof:** as in QUAL-PX-110.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-111"></a>

## PX-111 — Persisted incremental indexes and an indexed grep path

- **Requirement:** REQ-PX-111; **related preserved requirements:** REQ-EV-0004, REQ-EV-0172, REQ-EV-0249.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.1, M3.2, M3.5, IMP-EV-0172, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (persistence of derived indexes bound to a repository revision, with recovery on a corrupt index).
- **Audit source (BLD-11):** BLD-11 (audit-C area 7 and benchmark): cold start rebuilds the indexes; the recorded incremental refresh is 103 ms; exact search scans; first missing link: an on-disk index store keyed by revision.
- **Specification requirements (doc 79, with basis):** ADC-B05 [STATIC-AUDIT].
- **Scope and acceptance:** The lexical, symbol and graph indexes persist in the profile's index store keyed by workspace and Merkle subtree identity, load at start without a rebuild, and refresh incrementally from the changed set; a corrupt or version-mismatched index is detected by checksum, discarded and rebuilt with a recorded reason and never trusted; an index never becomes the source of file bytes (read-through freshness hydration stays). Exact and regex search gain an indexed path (trigram or equivalent) with a bounded fallback to the existing scan, so results are identical and only the cost changes. Index storage has a size cap and eviction by workspace recency.
- **Production wiring:** `crates/retrieval` index and lexical modules, the Merkle identity of IMP-EV-0004, `services/modbit-core/src/tools.rs` IndexPort refresh, the profile store; doc 18 updated by the implementing task.
- **Real qualification:** QUAL-PX-111 / PX-E2E-111.
- **Failure and negative proof:** as in QUAL-PX-111.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-112"></a>

## PX-112 — Learned embedder and rerank option behind dependency admission

- **Requirement:** REQ-PX-112; **related preserved requirements:** REQ-EV-0153, REQ-EV-0154, REQ-EV-0172.
- **Owner / milestone / release:** context-engine / M10 / RELEASE_ZERO; **prerequisites:** M3.5, PX-111, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (adds a model dependency and changes retrieval ranking evidence; supply chain and evidence semantics).
- **Audit source (BLD-11):** BLD-11 (audit-C area 8): the semantic index uses a hash-based embedder; MOD-EMB-001 (PROVISIONAL) names a local non-generative embedding model; first missing link: an admitted model and a rerank stage.
- **Specification requirements (doc 79, with basis):** ADC-B06 [STATIC-AUDIT].
- **Scope and acceptance:** Replace the placeholder embedder with a local non-generative embedding model and add an optional rerank stage over the fused candidates, both behind the existing semantic index interface and fusion, so exact and BM25 remain the correctness fallback (MOD-CTX-003). This row starts only after a dependency admission under docs 35 and 36 names the model, its licence, size, hash and exit plan; this record admits none. Models are pinned by digest, loaded from a verified local file, never fetched at run time, and run in the existing sandboxed process boundary with a memory cap. Rebuilding embeddings is incremental (changed chunks) and invalidated by a model change. Whether the learned embedder is the default is decided by the benchmark of PX-137, not assumed.
- **Production wiring:** `crates/retrieval` semantic and fusion modules, the model asset path in the profile store, the dependency record of docs 35 and 36; doc 18 updated by the implementing task.
- **Real qualification:** QUAL-PX-112 / PX-E2E-112.
- **Failure and negative proof:** as in QUAL-PX-112.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-113"></a>

## PX-113 — Engineering memory wired into the prompt and the event log, with Agent and Space scopes

- **Requirement:** REQ-PX-113; **related preserved requirements:** REQ-EV-0162, REQ-EV-0129, REQ-EV-0208, REQ-EV-0235.
- **Owner / milestone / release:** memory / M10 / RELEASE_ZERO; **prerequisites:** M9.1, IMP-EV-0162, IMP-EV-0129, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group B, instructions, context and memory; release-critical (memory content reaches the prompt and memory mutations become canonical events).
- **Audit source (BLD-12):** BLD-12 (audit-C area 13): memory is never injected into the prompt; promote and forget bypass the event log; there is no Agent or Space scope; first missing link: a pack segment and event-sourced mutations.
- **Specification requirements (doc 79, with basis):** ADC-B07 [STATIC-AUDIT].
- **Scope and acceptance:** Validated memories (scope, provenance, time to live, supersession) are selected for a task by scope and relevance and injected through the Context Pack compiler as a labelled, budgeted segment with the memory ids and provenance visible in the Inspector; memory is prompt context and never recovery state (MOD-STATE-001). Promote, edit and forget are commands that append events and rebuild the store as a projection, so they replay, survive restart and honour the session lease. Agent and Space scopes join the existing chain with deterministic precedence; CLI and desktop verbs (`memory list`, `promote`, `edit`, `forget`) call the same commands. A forgotten memory is removed from the store, the next pack and the projections, and a session transcript is never silently promoted. Memory text is untrusted-origin content scanned like other injected text and cannot widen capabilities.
- **Production wiring:** `crates/memory`, `services/modbit-core/src/memory.rs`, the pack compiler of `crates/context` and `crates/prompt-compiler`, additive memory commands in `surface.proto`, `apps/cli` verbs; doc 19 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-113 / PX-E2E-113.
- **Failure and negative proof:** as in QUAL-PX-113.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-114"></a>

## PX-114 — Exec-only tool projection mode, a schema-bytes budget and a live paired trial

- **Requirement:** REQ-PX-114; **related preserved requirements:** REQ-EV-0116, REQ-EV-0177, REQ-EV-0229, REQ-EV-0111.
- **Owner / milestone / release:** procedural-runtime / M10 / RELEASE_ZERO; **prerequisites:** M5.1, M5.4, M5.6, IMP-EV-0116, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group C, tool surface, budgets and hooks; release-critical (changes the capability surface a model sees (projection is policy-bearing) and the evidence that justifies it).
- **Audit source (BLD-13):** BLD-13 (audit-E area 22): the model receives about 28 to 30 tool schemas on every request; proc.exec is additional, not a replacement; there is no schema budget; first missing link: a projection mode and a measured trial.
- **Specification requirements (doc 79, with basis):** ADC-C01 [STATIC-AUDIT].
- **Scope and acceptance:** A projection mode `exec_only` selectable by the model-registry capability and by an execution leg exposes proc.exec, proc.wait, user.ask, plan.update, task.complete and tool.search; `agent.*`, `repair.attempt`, `context.fast` and `verify.run` are deferred until the task delegates or the loop needs them, discoverable through the existing deferred tool search and never authorised by discovery. A `max_projection_bytes` budget is enforced at projection time, recorded on `ToolProjectionSelected` with the projected bytes and the dropped tools, and a request that would exceed it degrades in a stated order. The default stays the current projection until the paired trial decides. The trial runs the existing M5.6 harness on a live compatible gateway with the same model, tasks and environment for the direct and the procedural arms and reports accuracy, tokens and tool calls with confidence intervals, whatever the direction. The Kernel still authorises every effect; a hidden tool is simply not in the request.
- **Production wiring:** `crates/core-runtime` harness and `services/modbit-core/src/runtime.rs` projection, `crates/procedural-runtime`, the registry capability field in `model_registry.rs`, the harness of `benchmarks/context-economics`; docs 16 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-114 / PX-E2E-114.
- **Failure and negative proof:** as in QUAL-PX-114.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-115"></a>

## PX-115 — Lazy MCP discovery, list_changed, idle reaper and a large-catalog benchmark

- **Requirement:** REQ-PX-115; **related preserved requirements:** REQ-EV-0128, REQ-EV-0104, REQ-EV-0134, REQ-EV-0177.
- **Owner / milestone / release:** external-tools / M10 / RELEASE_ZERO; **prerequisites:** M9.4, IMP-EV-0128, IMP-EV-0177, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group C, tool surface, budgets and hooks; release-critical (changes which external tool schemas reach the model and the lifecycle of external server processes).
- **Audit source (BLD-14):** BLD-14 (audit-E area 24): MCP discovery is eager, `tools/list_changed` is ignored, there is no idle reaper and QUAL-EV-0177 was benchmarked on host tools only; first missing link: `external.describe` and the reaper in `crates/mcp`.
- **Specification requirements (doc 79, with basis):** ADC-C02 [STATIC-AUDIT].
- **Scope and acceptance:** The model sees no external tool schemas by default: `external.list` returns names and one-line descriptions filtered by server and query, and `external.describe` returns the schema of named tools on demand, counted against the schema budget of PX-114; calling a tool still requires the Kernel's decision and a trusted server (discovery never authorises). `tools/list_changed` notifications refresh the catalog and invalidate cached descriptions; an idle reaper stops servers unused beyond a bound with a graceful shutdown then a kill after a grace period (two-phase), and reconnects lazily with state recorded. A large-catalog benchmark measures request size and task success with a five-hundred-tool server against eager discovery. HTTP transport and MCP resources and prompts are follow-ups, not part of this row.
- **Production wiring:** `crates/mcp` and `services/modbit-core/src/mcp.rs`, `crates/tools` external tool family, the lazy loader and tool search of the Tool Registry; docs 16 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-115 / PX-E2E-115.
- **Failure and negative proof:** as in QUAL-PX-115.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-116"></a>

## PX-116 — Hierarchical cost and wall-clock budgets, max_children and read-scope enforcement

- **Requirement:** REQ-PX-116; **related preserved requirements:** REQ-EV-0048, REQ-EV-0267, REQ-EV-0272, REQ-EV-0180, REQ-EV-0032.
- **Owner / milestone / release:** core-runtime / M10 / RELEASE_ZERO; **prerequisites:** M6.2, M6.3, IMP-EV-0048, IMP-EV-0032, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group C, tool surface, budgets and hooks; release-critical (execution limits and spend control over delegated work; a protocol and schema addition on the work graph).
- **Audit source (BLD-15):** BLD-15 (audit-B area 38): budgets cover turns and tool calls only; a child's budget is independent of its parent's; `read_scope` and Context Pack references are advisory; first missing link: cost and wall-clock fields and admission-time reservation.
- **Specification requirements (doc 79, with basis):** ADC-C03 [STATIC-AUDIT].
- **Scope and acceptance:** Budgets gain cost (in minor currency units from the existing accounting) and wall-clock fields. At spawn admission the child's budget is clamped to the parent's remainder and the amount is reserved against the parent in the same transaction as the capacity ticket, so the parent can never exceed its own cap through children; unused reservation returns at settlement; a child that exhausts its budget ends as BUDGET_EXHAUSTED with partial evidence. Child spend rolls up into the parent's economics through the existing per-run accounting. A `max_children` limit applies per parent and per task, and the default prompt and policy keep the rule that a primary agent works alone unless delegation is useful; an `agent.spawn` policy toggle can forbid delegation. The write-scope enforcement stays and the `read_scope` lease and Context Pack references of a work packet are enforced by the Kernel (a read outside scope is refused), not advisory.
- **Production wiring:** `crates/domain` budgets, `services/modbit-core/src/spawn.rs` and `subagent.rs` admission and settlement, `crates/core-runtime`, the accounting of `crates/observability`, the policy toggle in `crates/policy`; docs 13, 14 and 30 updated by the implementing task (doc 13 is a locked path; the implementing task cites this record).
- **Real qualification:** QUAL-PX-116 / PX-E2E-116.
- **Failure and negative proof:** as in QUAL-PX-116.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-117"></a>

## PX-117 — More hook points, kernel-gated hook context injection and prompt-type hooks

- **Requirement:** REQ-PX-117; **related preserved requirements:** REQ-EV-0139, REQ-EV-0042, REQ-EV-0239, REQ-EV-0242.
- **Owner / milestone / release:** extensions-hooks / M10 / RELEASE_ZERO; **prerequisites:** IMP-EV-0139, IMP-EV-0239, IMP-EV-0042, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group C, tool surface, budgets and hooks; release-critical (hooks run code and can add model-visible context; permissions, execution and a prompt boundary).
- **Audit source (BLD-16):** BLD-16 (audit-E area 51): twelve hook points, command-only hooks that cannot inject context; the research claim of gaps in permission merge, fail-closed and trust gating was wrong (those exist); first missing link: new events and the context field. This row does not depend on PX-092, which belongs to the unratified DR-PX-2026-10-03-011.
- **Specification requirements (doc 79, with basis):** ADC-C04 [STATIC-AUDIT].
- **Scope and acceptance:** Extend the typed hook events beyond the current twelve with the events the audit lists as missing for software-engineering work (for example before and after compaction, subagent start and stop, task completion proposal, pre-commit of a ChangeTransaction, notification), each with a versioned payload schema. A hook may return an `additional_context` field; the Core labels it with hook provenance and the hook's identity, counts it in the context breakdown, passes it through the injection scanner and the Kernel's content policy, drops it with a typed reason if refused or over its budget, and it can never add tools, change permissions, approve effects or edit protected paths. A prompt-type hook asks a registry-role model a bounded question about an event and returns allow, ask or deny with the reason, subject to the same merge order and fail-closed rule for permission events as command hooks. Existing merge order, exit-code blocking, trusted-workspace gating and the execution log stay as they are.
- **Production wiring:** `crates/tools/src/hooks.rs` and `services/modbit-core/src/hooks.rs`, the hook event schemas in `crates/domain`, the prompt compiler's labelled segment, the model registry role resolution; docs 16, 23 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-117 / PX-E2E-117.
- **Failure and negative proof:** as in QUAL-PX-117.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-118"></a>

## PX-118 — Worktree isolation as a typed task option: the run executes in its own worktree

- **Requirement:** REQ-PX-118; **related preserved requirements:** REQ-EV-0145, REQ-EV-0125, REQ-EV-0078.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** PX-065, PX-067, IMP-EV-0145, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group D, worktrees and merge integration; release-critical (selects where a task's effects land and creates and removes workspace data).
- **Audit source (BLD-17):** BLD-17 (audit-D area 18, audit-B): tasks write in the user's checkout; worktrees are a library capability and the lifecycle policy is PX-065; first missing link: an isolation field on `CreateTask` that binds the run's workspace root to a worktree.
- **Specification requirements (doc 79, with basis):** ADC-D01 [STATIC-AUDIT].
- **Scope and acceptance:** `CreateTask` gains a typed `isolation` option (`none` or `worktree`, with a Core default recorded in the profile). With `worktree` the Core creates the task's worktree under PX-065's lifecycle policy before the first turn, binds the task's workspace root, path policy, shell working directory, retrieval index view and checkpoint scope to it, and refuses any tool path that resolves outside it; the user's checkout stays untouched until the apply-back of PX-066. The worktree is created through the hardened Git runner of PX-067, carries uncommitted files by copy under path policy, and is protected from the reaper while the task runs. A task whose worktree cannot be created fails to start with a typed reason and never falls back silently to the user's checkout. Subagent worktrees follow the same option with their own write sets.
- **Production wiring:** `services/modbit-core/src/branch.rs` and task admission, `crates/workspace` root binding and path policy, `crates/git` worktree operations, additive `isolation` on `CreateTask` in `surface.proto`; docs 20 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-118 / PX-E2E-118.
- **Failure and negative proof:** as in QUAL-PX-118.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-119"></a>

## PX-119 — Agent merge integration: git.merge tools, persisted merge state, post-merge verification and parent completion gated on integrated children

- **Requirement:** REQ-PX-119; **related preserved requirements:** REQ-EV-0067, REQ-EV-0150, REQ-EV-0071, REQ-EV-0078.
- **Owner / milestone / release:** workspace-git / M10 / RELEASE_ZERO; **prerequisites:** PX-118, PX-066, M6.5, IMP-EV-0067, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group D, worktrees and merge integration; release-critical (effect-bearing integration of branches with conflict resolution, persisted state and a completion gate).
- **Audit source (BLD-18):** BLD-18 (audit-B area 19, audit-D area 18): the merge transaction exists in `crates/git` with no production caller; parent completion does not wait for integration; first missing link: the tools and the event-sourced merge state.
- **Specification requirements (doc 79, with basis):** ADC-D02 [STATIC-AUDIT].
- **Scope and acceptance:** Register `git.merge.prepare`, `git.merge.commit` and `git.merge.abort` as governed tools (protected effects, exact intent, receipts) over the existing MergeTransaction. The merge state (the two revisions, the conflicted files, each resolution and its author, the verdict) is persisted on the event log so a crash resumes or aborts to a whole state and replay reproduces it. A conflict is reported with per-file evidence and offered to the agent or the person for resolution; the resolved result runs the verification engine (post-merge verification) before the commit is allowed to count. A parent's `task.complete` is refused while any child that produced changes is not terminal and integrated (merged, or explicitly discarded by a recorded decision). The apply-back of PX-066 shares the transaction and its conflict evidence; this row adds the child-to-parent direction and the completion gate.
- **Production wiring:** `crates/git` merge transaction, tool specs in `crates/tools` (`agent_tools.rs`), `services/modbit-core/src/spawn.rs` and the completion gate in `runtime.rs`, the event store for merge events, the verification engine; docs 14, 20 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-119 / PX-E2E-119.
- **Failure and negative proof:** as in QUAL-PX-119.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-120"></a>

## PX-120 — Browser navigation target policy: loopback, private ranges, link-local, metadata endpoints, redirects and rebinding

- **Requirement:** REQ-PX-120; **related preserved requirements:** REQ-EV-0284, REQ-EV-0287, REQ-EV-0110.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-073, M7.1, M7.7, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group E, browser runtime; release-critical (security boundary of the live browser host against server-side request forgery and local-network reach).
- **Audit source (BLD-19):** BLD-19 (audit-F item 1): PX-073's per-call origin gate checks the page origin against an allow-list but nothing refuses a navigation whose target address is local, private or a cloud metadata endpoint; first missing link: a target resolver in the host.
- **Specification requirements (doc 79, with basis):** ADC-E01 [STATIC-AUDIT].
- **Scope and acceptance:** Before any navigation, redirect hop, sub-frame navigation and link-induced navigation the host resolves the destination, applies the address-class policy and refuses loopback, RFC 1918, unique-local, link-local, carrier-grade, multicast and the cloud metadata addresses (169.254.169.254 and its IPv6 and hostname forms) unless the task's policy names the host or the address, with the check made on the resolved address at connect time so DNS rebinding and an address change between check and use are caught. A detected local development server (PX-132) may be named by the person or policy; the model cannot name it. Refusal is typed (a new target-not-allowed code in the existing taxonomy) with the reason and an escalation of ask-user. The cloud-isolated browser keeps its own deny-internal egress policy; this row governs the local host and does not change webPreferences, CSP or preload.
- **Production wiring:** `apps/desktop/src/main/browser.ts` session request handling, `crates/browser` policy types and the typed refusals, `crates/tools/src/browser.rs`, the policy bundle for named hosts; docs 22 and 52 updated by the implementing task. Any other main-process change is DR-PX-2026-10-03-012, which is not ratified.
- **Real qualification:** QUAL-PX-120 / PX-E2E-120.
- **Failure and negative proof:** as in QUAL-PX-120.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-121"></a>

## PX-121 — Browser feedback primitives: viewport capture, console, network, scroll, wait, an escalation field and the unknown-outcome latch

- **Requirement:** REQ-PX-121; **related preserved requirements:** REQ-EV-0277, REQ-EV-0279, REQ-EV-0234.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-073, PX-069, M7.4, M7.5, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group E, browser runtime; release-critical (new effect-bearing and observation tools on the live browser and a typed failure latch for effects of unknown outcome).
- **Audit source (BLD-19):** BLD-19 (audit-F items 1 and 2): no viewport screenshot, scroll, wait, console or network tool; refusals carry no machine-readable escalation and no unknown-outcome latch exists for browser actions (PX-069 defines them for native control only); the `TARGET_OCCLUDED` hint still recommends scroll; first missing link: the tools and the latch in `crates/browser`.
- **Specification requirements (doc 79, with basis):** ADC-E02 [STATIC-AUDIT].
- **Scope and acceptance:** Register `browser.capture` (viewport or element screenshot at the 1280 by 800 budget through the media pipeline), `browser.console` and `browser.network` (bounded, redacted, untrusted-labelled reads of the page's recent console messages and requests, with URL credentials and tokens redacted), `browser.scroll` (typed, bounded, against a fresh observation) and `browser.wait` (a typed condition with a timeout: element, text, network idle, URL). Every refusal carries the machine-readable escalation of the shared taxonomy (retry, ask user, re-observe, abandon) from PX-069 so the two runtimes share one type. After a timeout or a lost host on an action whose effect may have happened the Core latches the session as UNKNOWN, records an UNKNOWN receipt and refuses further actions until a fresh observation reconciles; it never retries an action of unknown outcome. The occlusion hint recommends scroll only now that the tool exists.
- **Production wiring:** `crates/browser`, `crates/tools/src/browser.rs`, `apps/desktop/src/main/browser.ts` (capture, console and network capture, scroll and wait dispatch), the media pipeline of M2.10, the shared refusal and receipt types of PX-069; docs 17, 22 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-121 / PX-E2E-121.
- **Failure and negative proof:** as in QUAL-PX-121.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-122"></a>

## PX-122 — Page-state deltas from a host mutation observer, since_fingerprint, multi-frame accessibility trees and stable identities across restart

- **Requirement:** REQ-PX-122; **related preserved requirements:** REQ-EV-0279, REQ-EV-0277, REQ-EV-0280.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** M7.2, M7.3, PX-121, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group E, browser runtime; release-critical (changes the page evidence the model acts on (evidence semantics) and a protocol field on the browser event stream).
- **Audit source (BLD-19):** BLD-19 (audit-F area 31): deltas are pull-based compile-and-diff, the accessibility tree covers the top frame only, `since_fingerprint` is absent and the known-state map is lost on restart; first missing link: a host mutation observer and an identity store.
- **Specification requirements (doc 79, with basis):** ADC-E03 [STATIC-AUDIT].
- **Scope and acceptance:** A mutation observer in the host emits bounded change notices (added, removed, changed, attribute and focus changes, navigation) that the Core turns into the existing semantic delta events, with `browser.snapshot since_fingerprint` returning the delta from a fingerprint the model holds (or a full rehydrate when the fingerprint is too old, with the reason). Accessibility trees cover same-origin and cross-origin sub-frames with per-frame attribution and the origin of each frame, under the origin gate of PX-120. Element identities are hardened (stable DOM attributes preferred, then role, name and landmark path) and the known-state map persists across a Core restart so the same element keeps its reference. After a compaction epoch a delta against the pre-compaction fingerprint still works or says why it cannot.
- **Production wiring:** `apps/desktop/src/main/browser.ts` observer injection and frame traversal, `crates/browser/src/compiler.rs` (diff, apply, state fingerprint), `crates/tools/src/browser.rs`, the protocol state store for the known-state map; docs 22 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-122 / PX-E2E-122.
- **Failure and negative proof:** as in QUAL-PX-122.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-123"></a>

## PX-123 — Semantic compiler: page classification, forms and fill_form, intent filter, derived actions and stronger action risk classification

- **Requirement:** REQ-PX-123; **related preserved requirements:** REQ-EV-0088, REQ-EV-0280, REQ-EV-0082.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-122, M7.4, IMP-EV-0088, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group E, browser runtime; release-critical (derived actions are effect-bearing on live pages and the risk classification is a policy input).
- **Audit source (BLD-20):** BLD-20 (audit-F area 31): no page classification, forms grouping, fill_form, intent filter or derived actions; risk classification looks at the control only; first missing link: the compiler stages after the delta layer.
- **Specification requirements (doc 79, with basis):** ADC-E04 [STATIC-AUDIT].
- **Scope and acceptance:** The compiler adds a page kind (for example login, search results, article, form, checkout, error), forms grouped with their fields and submit controls, an `intent` and `scope` filter on snapshots so the model asks for the part of the page it needs, derived actions (fill a form from typed values then submit, open the nth result, dismiss a dialog) each with preconditions and postconditions checked by the existing postcondition engine, and a `browser.fill_form` tool that fills known fields from values and from credential handles only (never plaintext secrets in arguments). Risk classification considers the destination of a link or form action, a change of origin, dialog button wording and whether the action submits data, and raises the approval class accordingly through the Kernel. A derived action that cannot be verified is reported as unverified, not as success. Injection provenance isolation stays: page text is untrusted data.
- **Production wiring:** `crates/browser/src/compiler.rs` and the semantic action and postcondition code of M7.4, `crates/tools/src/browser.rs` (`fill_form`), the credential handle fill path of M7.8, the Kernel risk classes; docs 22 and 17 updated by the implementing task.
- **Real qualification:** QUAL-PX-123 / PX-E2E-123.
- **Failure and negative proof:** as in QUAL-PX-123.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-124"></a>

## PX-124 — WebMCP rung: page-declared tools with origin binding and trust labels

- **Requirement:** REQ-PX-124; **related preserved requirements:** REQ-EV-0281, REQ-EV-0284, REQ-EV-0128.
- **Owner / milestone / release:** browser / M10 / RELEASE_ZERO; **prerequisites:** PX-073, PX-120, M9.4, IMP-EV-0281, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group E, browser runtime; release-critical (a page can declare callable tools: a new tool source on a security boundary).
- **Audit source (BLD-21):** BLD-21 (audit-F item 0.1): no WebMCP code exists; the existing rung is a host-declared MCP server bound to origins; the owner's hierarchy puts page-declared structured tools above derived actions; first missing link: discovery of page-declared tools in the host.
- **Specification requirements (doc 79, with basis):** ADC-E05 [STATIC-AUDIT].
- **Scope and acceptance:** When a page declares tools through the browser's model-context interface the host discovers them, binds each to the exact frame and origin that declared it and to the page load that declared it (navigating or the page withdrawing the tool removes it), and offers them to the Core as a catalog with a trust label (untrusted page proposal). The model sees them through the lazy discovery of PX-115 (describe on demand), and a call goes through the Kernel's decision with the origin and the declared annotations (an unannotated tool is treated as a write), the approval class of the external-tool path, the injection scanner on results and the usual receipt. A declaration cannot name a host tool, cannot change another origin's tools, and cannot persist beyond the page. The hierarchy is recorded per call: page-declared tool, then derived action, then accessibility action, then raw input, with the reason for each fall. This record decides the rung; it adds no dependency on a browser feature flag beyond what the host exposes.
- **Production wiring:** `apps/desktop/src/main/browser.ts` discovery, `crates/browser` catalog and origin binding, `crates/mcp` and `crates/tools` for the shared external-tool call path, the Kernel annotation classes; docs 22 and 17 updated by the implementing task.
- **Real qualification:** QUAL-PX-124 / PX-E2E-124.
- **Failure and negative proof:** as in QUAL-PX-124.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-125"></a>

## PX-125 — GitHub pull request read, diff and status comments through the forge adapter

- **Requirement:** REQ-PX-125; **related preserved requirements:** REQ-EV-0148, REQ-EV-0128.
- **Owner / milestone / release:** external-tools / M10 / RELEASE_ZERO; **prerequisites:** PX-006, PX-007, M9.4, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group F, GitHub-native work and the cloud client; release-critical (effect-bearing external writes (comments) and untrusted external reads on the forge boundary).
- **Audit source (BLD-23):** BLD-23 (audit-H area 58): the adapter creates pull requests and ingests issues but has no pull request read, no diff and no comment write; PX-006, PX-007 and PX-010 were proven on a wire-faithful fake (DR-M6-002), not on GitHub; first missing link: the read and comment tools.
- **Specification requirements (doc 79, with basis):** ADC-F01 [STATIC-AUDIT].
- **Scope and acceptance:** Add `forge.pr.read` (metadata, state, reviewers, checks summary), `forge.pr.diff` (the diff as a bounded, pageable OutputRef with file filters) and `forge.issue.comment` and `forge.pr.comment` (a status or progress comment written as a protected effect with the exact body in the approval) behind the External Tool Hub. Everything read from GitHub is untrusted external content with provenance (forge_pr, forge_comment) and goes through the injection scanner; a comment body is bounded and secret-redacted before it is sent; the GitHub token stays in the broker and never reaches tool arguments or logs. Rate limits and abuse limits are honoured with typed waits. The row's closing proof runs against real GitHub with a test repository and token supplied by the owner (MODBIT_GITHUB_TOKEN and MODBIT_GITHUB_TEST_REPO); without them the row stays below REAL_TESTING and records BLOCKED with that reason, as DR-M6-002 states for the adapter.
- **Production wiring:** `services/modbit-core/src/forge.rs` and `pull_request.rs`, the tool specs in `crates/tools`, the external-tool path of `crates/mcp`, the credential broker; docs 17, 29 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-125 / PX-E2E-125.
- **Failure and negative proof:** as in QUAL-PX-125.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-126"></a>

## PX-126 — Check-suite and comment webhooks into CI-result and review-comment ingestion

- **Requirement:** REQ-PX-126; **related preserved requirements:** REQ-EV-0148, REQ-EV-0147.
- **Owner / milestone / release:** sandbox-cloud / M10 / RELEASE_ZERO; **prerequisites:** PX-011, PX-008, PX-009, PX-125, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group F, GitHub-native work and the cloud client; release-critical (external events become task input and verification evidence; signature, replay and tenant boundaries).
- **Audit source (BLD-23):** BLD-23 (audit-H area 58): `IngestCiResults` and `IngestReviewComments` (PX-008, PX-009) have no caller; the webhook endpoint of PX-011 handles issue and pull request events only; first missing link: event mappings in `apps/cloud-api`.
- **Specification requirements (doc 79, with basis):** ADC-F02 [STATIC-AUDIT].
- **Scope and acceptance:** Extend the signed webhook endpoint of PX-011 with `check_suite`, `check_run`, `issue_comment` and `pull_request_review_comment` events, mapped by tenant and repository to the owning task, and call the existing ingestion commands, so a CI result becomes provenance-bearing `ci_evidence` (the Verification Engine's evidence) and a comment becomes untrusted durable steering input. Signature, timestamp window, nonce replay protection, tenant mapping and audit are those of PX-011 and unchanged. A delivery for an unknown task or an unauthorised repository is rejected and audited; a duplicate delivery is a no-op. CI evidence never turns a failing verification into a pass and never overrides the Acceptance Gate. The local Core receives the same events through its existing intake path when no cloud is used.
- **Production wiring:** `apps/cloud-api` webhook handler and event mapping, `apps/cloud-worker` claim path, `services/modbit-core/src/ci_evidence.rs` and the ingestion commands, the tenant policy bundle; doc 24 updated by the implementing task.
- **Real qualification:** QUAL-PX-126 / PX-E2E-126.
- **Failure and negative proof:** as in QUAL-PX-126.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-127"></a>

## PX-127 — ci_evidence and review comments in Review and the CLI, and the first real-GitHub issue-to-task-to-PR-to-CI proof

- **Requirement:** REQ-PX-127; **related preserved requirements:** REQ-EV-0147, REQ-EV-0151.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** PX-125, PX-126, PX-048, PX-010, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group F, GitHub-native work and the cloud client; release-critical (presents verification evidence and drives a pull request effect through the desktop and CLI).
- **Audit source (BLD-23):** BLD-23 (audit-H area 58): `ci_evidence` is not rendered in Review or the CLI and nothing has run against real GitHub end to end; first missing link: the projections and the owner-supplied token.
- **Specification requirements (doc 79, with basis):** ADC-F03 [STATIC-AUDIT].
- **Scope and acceptance:** Review and the CLI render `ci_evidence` (check name, state, link, the evidence's provenance and the digest) and review comments (author, untrusted label, the thread and whether the agent has answered) next to the change, and never show CI green as acceptance. The row's closing proof is the first real run of the chain: a real GitHub issue becomes a task (PX-010), the task produces a reviewed change, a pull request is opened (PX-007), the real CI result arrives (PX-126) and appears in Review and the CLI, and a review comment steers a follow-up turn. It runs on the owner's test repository and token; without them the row records BLOCKED with that reason.
- **Production wiring:** Renderer Review and `apps/cli` over the `ci_evidence` and review-comment projections, `services/modbit-core/src/pull_request.rs`; no GitHub call in the renderer.
- **Real qualification:** QUAL-PX-127 / PX-E2E-127.
- **Failure and negative proof:** as in QUAL-PX-127.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-128"></a>

## PX-128 — Cloud handoff, watch and approve verbs in the CLI and the desktop

- **Requirement:** REQ-PX-128; **related preserved requirements:** REQ-EV-0022, REQ-EV-0063, REQ-EV-0109, REQ-EV-0054.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M8.7, M8.1, PX-042, PX-045, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group F, GitHub-native work and the cloud client; release-critical (moves a task and its approvals between local and cloud execution; a session lease, a policy and a data-egress boundary).
- **Audit source (BLD-24):** BLD-24 (audit-H area 5): the cloud control plane is real and CI-proven server-side, but nothing first-party calls it; the desktop, CLI and IDE adapter have no cloud client; first missing link: the client commands over the existing HTTP API.
- **Specification requirements (doc 79, with basis):** ADC-F04 [STATIC-AUDIT].
- **Scope and acceptance:** Add `modbit cloud login`, `handoff`, `watch`, `approve` and `return` to the CLI and a desktop Continue in Cloud action with a cloud-sessions list, all over the existing Cloud API and the EnvironmentHandoffBundle. Handoff shows what leaves the machine (the Git tree and delta, the plan, the context digest, secret references and never secret values), requires an explicit confirmation and is refused when policy forbids it; the cloud task appears in the agent list with an execution-location pill; approvals raised in the cloud can be answered from either client with the same exact-intent card and the same session-lease fence; return brings the delta and the evidence back and the local task continues. A cloud task never receives local-only capabilities (native computer control). No new execution path is added: the cloud worker runs the same Core runtime.
- **Production wiring:** `apps/cli` cloud verbs and `packages/ide-adapter-core`, `apps/desktop` cloud sessions pane over `apps/cloud-api`, the handoff of M8.7; docs 24, 29 and 32 updated by the implementing task.
- **Real qualification:** QUAL-PX-128 / PX-E2E-128.
- **Failure and negative proof:** as in QUAL-PX-128.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-129"></a>

## PX-129 — OIDC authorization-code with PKCE sign-in, tenant provisioning and the signed policy-bundle publisher route

- **Requirement:** REQ-PX-129; **related preserved requirements:** REQ-EV-0040, REQ-EV-0041, REQ-EV-0093.
- **Owner / milestone / release:** sandbox-cloud / M10 / RELEASE_ZERO; **prerequisites:** M8.1, M8.2, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group F, GitHub-native work and the cloud client; release-critical (identity, tenant boundary and signed policy distribution).
- **Audit source (BLD-24):** BLD-24 (audit-H area 5, audit-G area 41): MOD-AUTH-001 (PROVISIONAL) names OIDC with PKCE but no endpoints or client exist; tenant and principal provisioning and the signed policy-bundle publisher route are absent; first missing link: the identity and provisioning endpoints in `apps/cloud-api`.
- **Specification requirements (doc 79, with basis):** ADC-F05 [STATIC-AUDIT].
- **Scope and acceptance:** The Cloud API adds an OIDC-compatible relying-party flow (authorization code with PKCE, state and nonce verified, the provider keys fetched and cached with rotation, a clock-skew bound) that issues short-lived, tenant-scoped session tokens bound to a principal; tenant and principal provisioning endpoints (create, disable, role) with an audit record; and a policy-bundle publisher route that accepts only bundles signed by an organisation key, verifies the signature, version and the monotonic generation before accepting them, and serves the current signed bundle to workers and clients, which fail closed on an unverifiable or stale bundle. Tokens and keys are held by the broker and never logged. Enterprise SSO attaches behind the same interface (MOD-AUTH-001).
- **Production wiring:** `apps/cloud-api` auth, provisioning and policy routes, `apps/cloud-worker` bundle verification, `apps/cli` and the desktop login of PX-128, the Postgres store of M8.1; doc 24 updated by the implementing task.
- **Real qualification:** QUAL-PX-129 / PX-E2E-129.
- **Failure and negative proof:** as in QUAL-PX-129.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-130"></a>

## PX-130 — Credential broker unification: one broker interface and handle path for tools, providers, MCP, browser and cloud

- **Requirement:** REQ-PX-130; **related preserved requirements:** REQ-EV-0215, REQ-EV-0288, REQ-EV-0128.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M8.6, M7.8, M9.3, IMP-EV-0288, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group G, policy kernel and credentials; release-critical (the secret-handling boundary for every credential the product uses).
- **Audit source (BLD-26):** BLD-26 (audit-G area 43): the broker is fragmented (`crates/secrets` holds only the redactor, `SecretHandle` lives in providers, the browser, MCP and cloud paths each resolve credentials their own way); docs name `crates/secrets` as the owner; first missing link: one handle type and one resolver.
- **Specification requirements (doc 79, with basis):** ADC-G01 [STATIC-AUDIT].
- **Scope and acceptance:** One `SecretHandle` type and one broker interface in the canonical owner crate resolve every credential: provider keys, MCP server credentials, browser fill (M7.8), forge tokens, cloud tokens and webhook secrets. A handle is opaque, scoped (principal, purpose, expiry, call cap), never serialised with its value, and resolved only inside the process that performs the effect; values are redacted from every log, event, artifact and prompt by the same stage. Existing paths are migrated onto the interface, not duplicated, and a lint forbids a new ad hoc credential read. Rotation, revocation and the audit of each use are part of the interface. The effects stub crate is either made the real owner of what it claims or retired in the same change so documentation and code agree.
- **Production wiring:** `crates/secrets` (interface and redactor), `crates/providers` (`SecretHandle` moved), `crates/mcp`, `apps/desktop` browser fill path, `apps/cloud-worker` and `apps/cloud-api` secret use, `tools/architecture-lint` rule; docs 12 and 23 updated by the implementing task (doc 12 is a locked path; the implementing task cites this record).
- **Real qualification:** QUAL-PX-130 / PX-E2E-130.
- **Failure and negative proof:** as in QUAL-PX-130.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-131"></a>

## PX-131 — AuthorizationEpoch and CapabilitySnapshot: the capability view frozen per model round

- **Requirement:** REQ-PX-131; **related preserved requirements:** REQ-EV-0041, REQ-EV-0270, REQ-EV-0031.
- **Owner / milestone / release:** effects-security / M10 / RELEASE_ZERO; **prerequisites:** M2.5, M9.2, IMP-EV-0041, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group G, policy kernel and credentials; release-critical (policy and permission semantics of every model round; a canonical record on the receipt chain).
- **Audit source (BLD-26):** BLD-26 (audit-G area 40): `CapabilitySnapshot` and `AuthorizationEpoch` are documented and have no code; the audit fix FIX-08 added a dispatch-time authorization receipt but no epoch; first missing link: the snapshot record and the epoch stamp.
- **Specification requirements (doc 79, with basis):** ADC-G02 [STATIC-AUDIT].
- **Scope and acceptance:** At each model round boundary the Core computes a `CapabilitySnapshot` (the projected tools, the effective policy generation, the lease and the run-mode posture) and records it with a monotonic `AuthorizationEpoch`; every kernel decision, approval and receipt in that round is stamped with the epoch and decides against the snapshot, so a policy change, a revoked skill or trust, or a mode switch made during a round takes effect at the next boundary and never half-applies inside a round. An effect that needs a fresher view than the snapshot (a revocation of the approving principal, an emergency stop) still takes effect immediately by the existing hot revalidation of policy generation; this row records, not replaces, that path. Replay reproduces the snapshot of every round, and a receipt whose epoch does not match its decision is rejected by the chain verifier.
- **Production wiring:** `crates/policy` kernel and ledger, `services/modbit-core/src/runtime.rs` round boundary, `crates/domain` additive snapshot and epoch events, the receipt verifier of M9.2; docs 13, 23 and 30 updated by the implementing task (doc 13 is a locked path; the implementing task cites this record).
- **Real qualification:** QUAL-PX-131 / PX-E2E-131.
- **Failure and negative proof:** as in QUAL-PX-131.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-132"></a>

## PX-132 — Process intelligence: port and dev-server detection, readiness and service health reported by the Core

- **Requirement:** REQ-PX-132; **related preserved requirements:** REQ-EV-0027, REQ-EV-0025, REQ-EV-0271.
- **Owner / milestone / release:** terminal / M10 / RELEASE_ZERO; **prerequisites:** PX-043, M2.3, M4.5, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group H, process intelligence; release-critical (observes live processes and sockets and reports them as evidence the agent and the browser policy act on).
- **Audit source (BLD-27):** BLD-27 (audit-D area 53): no port, dev-server or health detection exists; first missing link: listening-socket discovery in execd bound to the owning process tree.
- **Specification requirements (doc 79, with basis):** ADC-H01 [STATIC-AUDIT].
- **Scope and acceptance:** The terminal broker discovers the listening sockets of each shell's process tree (never of unrelated processes), classifies a listening process as a dev server when its command or its output matches a known pattern or a probe succeeds, runs a bounded readiness probe (TCP connect, then an optional HTTP request to a configured path) and a periodic health check, and publishes typed `ProcessServiceObserved` events (port, address, owner shell, state starting, ready, unhealthy, gone) with the evidence. The agent receives the facts in its tool results and the context breakdown without calling a tool; the desktop shows them in the terminal view; the browser policy of PX-120 can be told by the person that a detected local server may be navigated to, but detection itself never widens any policy. Probes are loopback only and bounded; remote or cloud processes report through the guest RPC.
- **Production wiring:** `services/modbit-execd/src/broker.rs` socket discovery per process tree, `crates/terminal`, the guest RPC of `services/modbit-guest` for cloud, additive events in `crates/domain`; docs 21 and 30 updated by the implementing task.
- **Real qualification:** QUAL-PX-132 / PX-E2E-132.
- **Failure and negative proof:** as in QUAL-PX-132.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-133"></a>

## PX-133 — Escalation and reviewer leg samples with priced cost in Outcome Statistics

- **Requirement:** REQ-PX-133; **related preserved requirements:** REQ-EPR-010, REQ-EPR-015, REQ-EPR-011, REQ-EV-0032.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** EPR-010, EPR-015, EPR-011, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group I, execution policy router evidence (conformance and measurement only); release-critical (evidence semantics of the statistics the router reads (ADR-R-051 and ADR-R-055); no router semantics change).
- **Audit source (BLD-28):** BLD-28 (audit-I section 7 item 1): the statistics pipeline emits only Solver samples (`outcome-statistics/src/lib.rs` around 511 to 540), so a cascade or critique cannot become confidently feasible from production data; this conforms the implementation to the sealed ADR-R-055 and changes no EPR row, gate or algorithm.
- **Specification requirements (doc 79, with basis):** ADC-I01 [STATIC-AUDIT].
- **Scope and acceptance:** Emit `Escalation` and `Reviewer` samples from the request, leg and gate records of the accounting path (EPR-010) into the versioned Outcome Statistics Store (EPR-015), each with the request, leg and gate attribution, the solver and reviewer configuration, the verified outcome and the complete priced cost including failed, cancelled and retried legs and uncertain charges, so a successful escalation never credits an earlier failed leg. Sample emission is idempotent per leg, survives restart, passes the existing sanitisation and minimum-count rules, and writes only the statistics store. This row changes no estimator, feasibility rule, threshold, plan compiler or gate; any change of those is an EPR Decision Record. Reviewer-family enforcement, a correlated feasibility bound and a no-checks cascade policy are NOT part of this row (see doc 79 section 9).
- **Production wiring:** `crates/outcome-statistics` emission, `services/modbit-core/src/accounting.rs`, the EPR event types already in `crates/domain`; docs 27 and 38 are unchanged (the implementation conforms to them).
- **Real qualification:** QUAL-PX-133 / PX-E2E-133.
- **Failure and negative proof:** as in QUAL-PX-133.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-134"></a>

## PX-134 — Model registry sign, verify and activate verbs on the CLI

- **Requirement:** REQ-PX-134; **related preserved requirements:** REQ-EPR-002, REQ-EPR-012, REQ-EV-0028.
- **Owner / milestone / release:** model-gateway / M10 / RELEASE_ZERO; **prerequisites:** EPR-002, PX-053, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group I, execution policy router evidence (conformance and measurement only); release-critical (supply chain and activation of the registry that gates routing eligibility).
- **Audit source (BLD-28):** BLD-28 (audit-G area 37, audit-I section 7 item 2): registries can be signed and activated only in tests; there is no operator verb; this is the operator surface for the signed registry of EPR-002, with no change to its semantics.
- **Specification requirements (doc 79, with basis):** ADC-I02 [STATIC-AUDIT].
- **Scope and acceptance:** CLI verbs `modbit registry bootstrap`, `sign`, `verify`, `activate`, `rollback` and `status` drive the existing signed, versioned Model Registry: bootstrap writes a minimal unsigned local bundle for a direct-only install, sign requires an operator key and produces the signature and manifest, verify checks signature, version, revocation and schema, activate swaps the active bundle atomically with the previous bundle retained and records an event, rollback restores the previous good bundle, and status shows the active digest, signer and revocations. An unsigned or unverifiable bundle can never become active where policy requires signed registries; a revoked key or bundle is refused. No routing, quality-floor or eligibility rule changes.
- **Production wiring:** `apps/cli` registry verbs, `services/modbit-core/src/model_registry.rs` and the registry loader of `crates/providers`, the signing code already present in the EPR registry path; docs 15 and 27 are unchanged (the verbs implement them).
- **Real qualification:** QUAL-PX-134 / PX-E2E-134.
- **Failure and negative proof:** as in QUAL-PX-134.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-135"></a>

## PX-135 — Widened gate calibration corpora across languages with generated adversarial checks

- **Requirement:** REQ-PX-135; **related preserved requirements:** REQ-EPR-019, REQ-EPR-017.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** EPR-019, EPR-017, PX-028, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group I, execution policy router evidence (conformance and measurement only); release-critical (evidence semantics of gate calibration (ADR-R-056); data and measurement only).
- **Audit source (BLD-28):** BLD-28 (audit-I section 5): first calibration measured 2 of 4 false accepts on an 8-case corpus; the corpora are small and single-language; this row widens data and measurement and changes no gate algorithm or threshold.
- **Specification requirements (doc 79, with basis):** ADC-I03 [STATIC-AUDIT].
- **Scope and acceptance:** Extend the held-out calibration corpora of EPR-019 to the Tier A languages (TypeScript and JavaScript, Python, Rust) with at least the sample minima the qualification fixes per slice, add generated adversarial checks for the false-accept classes already observed (overfit to the visible test, off-by-one, test deletion or weakening, vacuous pass) as additional evidence the Verification Engine can derive, and report acceptance false-accept and false-reject and realized-risk false-negative rates with Wilson intervals per language and slice, separately from router quality. Oracle labels stay hidden from the gate and from the model. Thresholds remain the owner's approved profile; this row neither sets nor changes them, and the gates stay unattested until that profile exists.
- **Production wiring:** `benchmarks` calibration corpora and the calibration harness of EPR-019, `crates/verification` adversarial check generation, the held-out store; docs 61 and 63 are unchanged (this supplies their data).
- **Real qualification:** QUAL-PX-135 / PX-E2E-135.
- **Failure and negative proof:** as in QUAL-PX-135.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-136"></a>

## PX-136 — One live paired benchmark of DIRECT, CASCADE and CRITIQUE on first-party providers

- **Requirement:** REQ-PX-136; **related preserved requirements:** REQ-EPR-012, REQ-EPR-019, REQ-EPR-010.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** PX-133, PX-134, PX-053, EPR-012, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group I, execution policy router evidence (conformance and measurement only); release-critical (produces the cost, quality and error-bound evidence that rollout decisions and gates consume (evidence semantics); measurement only).
- **Audit source (BLD-28):** BLD-28 (audit-I section 7 item 3): the only live EPR data is a DIRECT baseline (12 of 18 verified, 0.704 dollars); every cascade and critique proof is a scripted provider; this row measures and attests nothing.
- **Specification requirements (doc 79, with basis):** ADC-I04 [STATIC-AUDIT].
- **Scope and acceptance:** Run the existing live-competence workflow with a registry-enabled cascade arm and a critique arm beside the direct arm, using two real model bindings on first-party provider endpoints (or the compatible gateway recorded as such under DR-M9-002 until the owner supplies keys), the same tasks, environments and verification commands, complete per-request cost including all legs (PX-133) and the user preference and signed registry verbs of PX-053 and PX-134 to select the arms. The report states verified success, total cost, latency and human-intervention proxy per arm with confidence intervals, the escalation and review rates and the gate errors observed, and states the verdict whichever way it points. Nothing here attests a gate (gates A to G stay open until the owner's threshold profile and the required tasks exist) and nothing changes the default route, which stays DIRECT.
- **Production wiring:** `.github/workflows` live-competence job, `benchmarks` live harness, the registry and preference verbs of PX-134 and PX-053, `evidence/` bundle for the run; no product code change.
- **Real qualification:** QUAL-PX-136 / PX-E2E-136.
- **Failure and negative proof:** as in QUAL-PX-136.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-137"></a>

## PX-137 — Retrieval benchmark with an external baseline profile, fifty or more cases, a larger public repository and live same-model measurement

- **Requirement:** REQ-PX-137; **related preserved requirements:** REQ-EV-0249, REQ-EV-0250, REQ-EV-0251, REQ-EV-0252, REQ-EV-0253.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** M3.9, IMP-EV-0250, IMP-EV-0251, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group J, benchmarks and observability; iteration (measurement only: the benchmark changes no effect-bearing behaviour, persistence, permission, protocol, recovery or evidence semantics of the product).
- **Audit source (BLD-29):** BLD-29 (audit-C area 60): no zvec-grep or ripgrep baseline exists anywhere in the repository; eight hand-seeded cases; no live same-model run; first missing link: the baseline profile and the case set.
- **Specification requirements (doc 79, with basis):** ADC-J01 [STATIC-AUDIT].
- **Scope and acceptance:** Add an external baseline profile (a ripgrep-driven agent tool surface, and zvec-grep where it can be obtained under the dependency policy) to `benchmarks/retrieval`, grow the case set to at least fifty including multi-hop questions in the style of repository question-answering benchmarks and a second, larger public repository pinned by commit, and record from a live same-model run per case the answer accuracy, input tokens, tool calls and wall time for the baseline and the treatment under the no-forced-retrieval rule, plus cold start and incremental index time at the 100 MB scale. The report carries confidence intervals and the baseline column and states results whichever way they point. The harness is the existing M3.9 harness; no second one is built.
- **Production wiring:** `benchmarks/retrieval`, `benchmarks/context-economics`, the live workflow named in doc 77; no product change.
- **Real qualification:** QUAL-PX-137 / PX-E2E-137.
- **Failure and negative proof:** as in QUAL-PX-137.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-138"></a>

## PX-138 — Compaction evaluation that is not self-referential

- **Requirement:** REQ-PX-138; **related preserved requirements:** REQ-EV-0268, REQ-EV-0274, REQ-EV-0092.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** M4.2, PX-109, IMP-EV-0274, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group J, benchmarks and observability; iteration (measurement only: it evaluates compaction and changes no product behaviour).
- **Audit source (BLD-29):** BLD-29 (audit-C area 14): the compaction evaluation scores a summary against material derived from the same extractive pass; first missing link: an external oracle and a downstream-task measure.
- **Specification requirements (doc 79, with basis):** ADC-J02 [STATIC-AUDIT].
- **Scope and acceptance:** Build the evaluation on long fixture runs where a compaction epoch is forced at a chosen point: after compaction the same model, with the compacted context, must complete the task and answer a held-out set of recall questions about earlier decisions, files and failures whose answers come from the retained event log (the oracle), not from the summary. Report task success, recall accuracy, tokens saved and cost for the extractive path and the structured path of PX-109 and for no-compaction at a larger window, with intervals, and record failures where the summary dropped a needed fact. The harness is the existing economics harness.
- **Production wiring:** `benchmarks/context-economics`, the compaction manifest and event log as the oracle source; no product change.
- **Real qualification:** QUAL-PX-138 / PX-E2E-138.
- **Failure and negative proof:** as in QUAL-PX-138.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-139"></a>

## PX-139 — OpenTelemetry export, child-cost rollup and persisted health

- **Requirement:** REQ-PX-139; **related preserved requirements:** REQ-EV-0032, REQ-EV-0266, REQ-EV-0142, REQ-EPR-010.
- **Owner / milestone / release:** observability / M10 / RELEASE_ZERO; **prerequisites:** M10.1, PX-116, IMP-EV-0032, DOC-PX-013.
- **Decision Record, phase and evidence tier:** DR-PX-2026-10-05-013; audit group J, benchmarks and observability; release-critical (extends the canonical cost accounting records that EPR-010 and the economics read (evidence semantics) and persists operational state; the task list tagged this iteration, but the child-cost rollup changes accounting records, so it is release-critical here).
- **Audit source (BLD-30):** BLD-30 (audit-G area 59): no OpenTelemetry export, no child-cost rollup (budgets are independent, see PX-116), health is not persisted; first missing link: an exporter over the existing event and accounting records.
- **Specification requirements (doc 79, with basis):** ADC-J03 [STATIC-AUDIT].
- **Scope and acceptance:** An optional exporter publishes spans (run, turn, tool call, subagent, compaction epoch, verification run) and metrics (tokens, cost, latency, queue depth) over OTLP to a configured endpoint, derived from the canonical event and accounting records and never a second source of truth; spans carry the parent-child links of the work graph and the rolled-up cost of PX-116, and attribute values are redacted and size-bounded (no prompts, no file contents, no secrets by default). Export is off by default, endpoint and headers come from policy and the broker, failures never block or fail a task and are counted. Component health (provider reachability, execd, sandbox gateway, indexes) is persisted so a restart reports the last known state with its age instead of unknown. EPR accounting (EPR-010) stays the single cost record; the exporter reads it.
- **Production wiring:** `crates/observability` exporter and health store, the accounting path of `services/modbit-core/src/accounting.rs`, the work-graph parent links, policy and broker for endpoint settings; doc 34 updated by the implementing task.
- **Real qualification:** QUAL-PX-139 / PX-E2E-139.
- **Failure and negative proof:** as in QUAL-PX-139.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, run ids, artifact digests and the declared evidence tier under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

## Real-system scenarios

### PX-E2E-000 — Headless CLI task lifecycle

**Setup:** packaged Core, CLI binary, real `ts-webapp` fixture, live provider test model.  
**Action:** from a shell, create a coding task, follow events, answer the agent's question, approve the protected write, then `SIGKILL` Core and resume following from the last cursor.  
**Pass:** the task reaches ReadyForReview with real tests passing; exactly one effect receipt; the CLI exit code is 0 on success and documented non-zero on cancel, denial or failure; no duplicate command after reconnect; the CLI process never opened the repository, a provider endpoint or the policy store directly.

### PX-E2E-001 — Thin-client conformance on CLI and desktop client

**Setup:** packaged Core, CLI, desktop protocol client, fixture repository.  
**Action:** run the conformance suite: retry commands, drop and replay the event stream, approve with a bound intent hash, force Core rejections, scan client dependencies.  
**Pass:** both clients pass every case; a deliberately broken client fails the suite.

### PX-E2E-002 — VS Code adapter drives a real task

**Setup:** real VS Code extension host with the adapter, real local Core, fixture repository, live provider test model.  
**Action:** create a task from the editor, forward diagnostics, approve the protected write, restart the editor mid-task.  
**Pass:** task completes with real tests; diagnostics appear with provenance; resume by cursor; no workspace write originated in the adapter.

### PX-E2E-004 — External diagnostics as evidence only

**Setup:** adapter submitting language-service diagnostics for a fixture revision.  
**Action:** submit valid, stale and malformed batches; run a task whose verification plan includes a mandatory typecheck.  
**Pass:** valid batch normalized with provenance external_ide; stale and malformed rejected with events; the mandatory typecheck still executes in Modbit.

### PX-E2E-005 — Inline patch through ChangeTransaction

**Setup:** review surface and CLI on a real worktree.  
**Action:** apply a one-hunk user edit, then retry against a stale revision, then target a protected path.  
**Pass:** first edit lands with provenance user_direct_edit and a revision advance; stale and protected attempts are refused; no client buffer state exists.

### PX-E2E-006 — Forge adapter with leases and receipts

**Setup:** real GitHub test repository, broker-held token.  
**Action:** read an issue, create and update a PR, read comments and check-run status, retry the create with the same idempotency key, attempt an off-allowlist host.  
**Pass:** each call has lease, effect class and receipt; one PR exists; egress denial recorded.

### PX-E2E-007 — Pull request from a reviewed result

**Setup:** completed fixture task in Review.  
**Action:** open a PR, approve the push, revise, update the PR; crash Core between push and receipt.  
**Pass:** exactly one PR with the evidence summary; update carries a new receipt; reconciliation after crash yields no duplicate.

### PX-E2E-008 — Review-comment steering

**Setup:** fixture PR with comments from allowed and disallowed identities, including an injection attempt.  
**Action:** ingest comments.  
**Pass:** allowed comment becomes TaskSteered with provenance and the agent acts; disallowed and injected comments change nothing and are audited.

### PX-E2E-009 — CI results are evidence, not verdicts

**Setup:** fixture branch with a green check-run and a qualification naming a real test.  
**Action:** ingest CI results; run the task's verification.  
**Pass:** CI evidence appears with provenance ci and OutputRef logs; the qualification is PASS only after Modbit's own test execution.

### PX-E2E-010 — Task from an issue

**Setup:** real issue containing instructions to ignore policy.  
**Action:** `modbit task from-issue <url>` and the desktop New Task path.  
**Pass:** task created with the issue as untrusted context; policy and capabilities unchanged; ordinary loop runs.

### PX-E2E-011 — Webhook intake through the Cloud API

**Setup:** staging Cloud API with a GitHub App installation on the fixture repository.  
**Action:** deliver a signed webhook, a replayed webhook and an unsigned webhook.  
**Pass:** one canonical task created for the tenant and visible to the desktop by cursor; replay and unsigned deliveries rejected and audited.

### PX-E2E-014 — Plan before write

**Setup:** fixture with one ambiguous and one unambiguous task.  
**Action:** run both.  
**Pass:** plan recorded before the first write in both; exactly one typed question on the ambiguous task; a forced write before plan is rejected.

### PX-E2E-015 — No edit without retrieval

**Setup:** fixture task touching a symbol used in three files.  
**Action:** run the task; then force an edit to an unretrieved file.  
**Pass:** retrieval records exist for every edited file at the current revision; the forced edit is rejected with a policy decision.

### PX-E2E-016 — Tests first, scope disciplined

**Setup:** `ts-webapp` behavior change with an existing test harness.  
**Action:** run the task; induce a change outside the plan.  
**Pass:** failing test precedes the change; the out-of-plan file triggers a plan revision event; lockfile untouched by hand.

### PX-E2E-017 — Verification plan recorded first

**Setup:** fixture task that will fail verification.  
**Action:** run; attempt to remove a mandatory check.  
**Pass:** derived plan recorded before the first run and retained after failure; removal rejected.

### PX-E2E-018 — Bounded repair with escalation on repeated hypothesis

**Setup:** seeded failure whose obvious fix does not work.  
**Action:** run with policy bounds of two attempts per signature.  
**Pass:** each attempt has a complete RepairAttempt record; the second equivalent hypothesis is not executed and the task escalates or moves to Needs Attention with history; a WORSENED attempt is reverted.

### PX-E2E-019 — Self-review gates the completion proposal

**Setup:** fixture task completed with one leftover debug statement.  
**Action:** let the agent propose completion.  
**Pass:** SelfReview finds the leftover; proposal blocked until resolved; Acceptance Gate decides afterwards.

### PX-E2E-020 — Baseline bundle

**Setup:** real M2 product, frozen protocol, both suites.  
**Action:** run the baseline.  
**Pass:** immutable bundle with digests, model metadata, trials, per-task results and intervals; attempt with gold-patch access rejected.

### PX-E2E-021 — Regression gate

**Setup:** baseline bundle and a candidate build with a seeded competence regression and a routing cost improvement.  
**Action:** run the gate.  
**Pass:** candidate fails the competence gate; a target recorded without a baseline digest is rejected.

### PX-E2E-022 — Five-minute onboarding

**Setup:** fresh OS user profile, packaged app, live provider test key, small real repository.  
**Action:** Playwright completes welcome, provider setup, repository trust and a starter task.  
**Pass:** median under five minutes to ReadyForReview with a real diff and test run; failure paths show cause and next action; no outcome shown before Core persisted it.

### PX-E2E-023 — State matrix

**Setup:** packaged app against a real Core with fault injection.  
**Action:** force Core restart, provider outage, stale bundle, offline, unknown outcome, quality floor infeasible and human continuation on each screen.  
**Pass:** every required state present with cause, next action and evidence; notifications only for attention, completion and failure.

### PX-E2E-024 — Keyboard and accessibility

**Setup:** packaged app.  
**Action:** traverse every screen and action by keyboard; run the accessibility suite.  
**Pass:** all controls reachable, focus retained across Core events, live regions announce attention, no color-only status.

### PX-E2E-025 — Interaction budgets

**Setup:** packaged app on reference hardware.  
**Action:** measure each budgeted interaction from traces and Core timestamps.  
**Pass:** all budgets met at p95; a seeded regression fails the candidate.

### PX-E2E-026 — Alpha language baseline

**Setup:** ts-webapp, python-service and rust-cli fixtures on the Alpha product.  
**Action:** run a task per fixture.  
**Pass:** text retrieval, safe edits and real compiler and test evidence; clients label the languages at the Alpha baseline with no structural claim.

### PX-E2E-027 — Tier suites gate classification

**Setup:** tier suites and a language with a grammar but no recorded pass.  
**Action:** run the suites; attempt to list the language in Tier B.  
**Pass:** suites run on real fixtures; the unrecorded listing is rejected as a release blocker.

### PX-E2E-028 — Tier A for the three languages

**Setup:** real headless language services on the fixtures.  
**Action:** run the Tier A suite and the per-language competence baseline tasks.  
**Pass:** recall and diagnostics parity thresholds met; incremental index latency within budget; dead language service degrades explicitly.

### PX-E2E-029 — Explicit degradation

**Setup:** fixtures in an Unsupported language and a Tier C language.  
**Action:** run tasks that touch them.  
**Pass:** text retrieval and configured-command verification only; plan states the limitation; language state visible in desktop and CLI; Unsupported edit requires per-task opt-in.

### PX-E2E-030 — Platform CI matrix

**Setup:** CI runners for macOS, Windows and Linux.  
**Action:** build Core, CLI and runtime; run platform conformance suites; scan docs and client strings.  
**Pass:** results labeled CI_COMPATIBLE; any text presenting CI compatibility as support fails the scan.

### PX-E2E-031 — Platform promotion

**Setup:** packaged desktop E2E catalog on macOS; Windows and Linux without packaged E2E.  
**Action:** run the promotion check.  
**Pass:** macOS RELEASE_GRADE with evidence bundle; Windows and Linux remain CI_COMPATIBLE; promotion without platform E2E is rejected.

### PX-E2E-032 — Baseline before write and regression attribution

**Setup:** `ts-webapp` with one pre-existing failing test and a task whose naive change breaks a second, currently passing test.  
**Action:** run the task; attempt a write before the baseline; let the agent propose completion after breaking the second test.  
**Pass:** BASELINE recorded before the first write with the pre-existing failure as KNOWN_FAILING; the forced early write is rejected; the broken test is attributed as a REGRESSION at COMPLETION and blocks acceptance; a change declared in the plan before the run appears in Review as declared.

### PX-E2E-033 — Normalized reports and stable failure signatures

**Setup:** seeded failing tests in `ts-webapp` (vitest), `python-service` (pytest) and `rust-cli` (cargo); one configured command with no structured reporter.  
**Action:** run `test.run` twice per fixture at the same revision; run the configured command.  
**Pass:** STRUCTURED TestReports with stable check_ids and identical failure_signatures across runs; the model's next-round context lists failing CheckResults first with declared truncation and a pageable raw OutputRef; the configured command yields HEURISTIC confidence and an ambiguous mandatory check is UNKNOWN, treated as INDETERMINATE.

### PX-E2E-034 — Targeted runs never complete; the completion run does

**Setup:** fixture task with a slow full suite and a fast task-named subset.  
**Action:** observe TARGETED runs during repair; let the agent propose completion after a TARGETED pass; then after the COMPLETION run; then make one more change.  
**Pass:** TARGETED runs are failed-first and budgeted; the first proposal is refused for lacking a COMPLETION run; the second proceeds to the Acceptance Gate; the post-change proposal is refused until the COMPLETION run reruns at the new revision.

### PX-E2E-035 — Impact selection measured against ground truth

**Setup:** `multi-package` fixture with recorded dependency, symbol and test-link evidence and a change to a shared symbol.  
**Action:** run impact selection; run the full suite.  
**Pass:** every test failing in the full suite for the changed symbol within the depth bound is selected; precision and recall are recorded with the retrieval benchmarks; the plan of a pre-M3 run states that targeting is heuristic.

### PX-E2E-036 — Flaky test quarantined, never repaired, never gamed

**Setup:** fixture with a seeded flaky test, one of them mandatory in a second variant.  
**Action:** run a task whose TARGETED run hits the flake; let the agent attempt to add a skip marker; run the mandatory variant.  
**Pass:** exactly one isolated rerun; the check is FLAKY with both run references, excluded from failure_signature, shown in Review; the skip marker is rejected as DI-3; the mandatory variant is INCONCLUSIVE until three consecutive isolated passes; a flake at BASELINE is pre-quarantined.

### PX-E2E-037 — Diff invariants deny test tampering and flag leftovers

**Setup:** real worktree with an acceptance-named test, a lockfile and a dependency manifest.  
**Action:** induce transactions that weaken the acceptance-named test, hand-edit the lockfile, change the manifest without a plan entry, leave a debug statement and reformat an unplanned file.  
**Pass:** the first three are rejected with DiffInvariantViolated events naming DI-3, DI-2 and DI-7; the leftover and the churn are flagged and block the SelfReview until resolved or justified; the whole-diff COMPLETION evaluation reports the same findings.

### PX-E2E-038 — Scope bounded by the original plan

**Setup:** fixture task whose tempting fix touches four files outside the plan and one CI configuration file.  
**Action:** run the task with Alpha ScopePolicy defaults; run it again headless.  
**Pass:** the first two out-of-plan files pass with PlanRevised scope deltas; the third write waits for a typed question offering continue, split or stop; the CI file always asks; a ScopeExpansionRecorded event carries the counters; headless mode fails closed to Needs Attention; the scope metric reports four files against the original plan even after the plan was revised.

### PX-E2E-039 — Reproduction first, no identical retries, no silent stall

**Setup:** seeded defect whose reported symptom is reproducible and whose obvious fix is wrong.  
**Action:** run with RepairPolicy Alpha defaults; force a fix before reproduction; force an identical retry; force three idle turns.  
**Pass:** the pre-reproduction fix is rejected; the reproduction is recorded as evidence; the identical change_fingerprint is rejected and counted as an equivalent attempt leading to escalation; NoProgressDetected fires on the third idle turn and the task escalates or moves to Needs Attention; the bounds come from the versioned policy.

### PX-E2E-040 — Harness contracts under output, failure, missing runner and headless conditions

**Setup:** fixture task with a command producing more output than the inline ceiling, a failing test, a fixture whose runner is uninstalled, and a headless CLI run needing a question.  
**Action:** run the task in the desktop and in the CLI.  
**Pass:** the large output arrives bounded with declared omitted ranges and is pageable by OutputRef; the failing CheckResults arrive first; harness_state is present in every Context Pack; the missing runner is recorded in the plan and asked about; the headless question fails closed after the configured wait; a completion proposal during an in-flight verification run is refused; exhausting max_turns emits HarnessBudgetExhausted and lands in Needs Attention with partial evidence.

### PX-E2E-041 — Streaming assistant-text delta events with a completion record and cursor replay

**Setup:** packaged Core and CLI, the scripted endpoint and the live gateway, a fixture repository.  
**Action:** ask a question that yields a 400-word answer, drop and resume the client mid-stream, then `SIGKILL` the Core during a second answer.  
**Pass:** delta events are ordered, bounded and replayable; the completed digest equals the concatenated deltas; the killed stream is aborted-by-recovery and never marked complete; a planted secret never appears in any event; no client holds a provider credential.

### PX-E2E-042 — Transcript rows and agent header projections served by the Core

**Setup:** packaged Core, 200 seeded tasks including pending approval, failure, ready for review, running, waiting, completed, draft and archived.  
**Action:** request the header list, open a transcript in each density, search a body-only phrase, archive and unarchive a task, drop the projection tables and restart.  
**Pass:** headers load without transcripts; precedence is exact; densities fold the same events; search hits the body; archive and unarchive are single events; the rebuilt projections are identical.

### PX-E2E-043 — Terminal output stream and background-terminal registry for every client

**Setup:** packaged Core, a fixture that runs a noisy build and a 5-minute sleep.  
**Action:** start both, detach the sleep to the background, restart the renderer, reattach by cursor, kill the sleep, then restart the Core.  
**Pass:** output is complete and ordered; the sleep survives the restarts as one handle; the kill is one event and one wake-up; terminal ownership is enforced.

### PX-E2E-044 — Design tokens, UI primitives, tray host and a model-free state gallery

**Setup:** packaged app on a real Core, both colour schemes, reduced motion on and off.  
**Action:** traverse every screen and the gallery by keyboard, run the axe suite and the contrast test, open two trays in turn.  
**Pass:** zero violations; contrast passes; every primitive is keyboard operable; one tray owns the scoped keys; reduced motion removes all animation.

### PX-E2E-045 — Agent-first shell: three regions, top bar, apps-panel host, status row, palette and shortcut registry

**Setup:** packaged app on a real Core with five tasks in mixed states, windows of three sizes.  
**Action:** measure the regions, resize the list, open and close the panel, narrow the window, use the palette and each shortcut, restart.  
**Pass:** the geometry targets hold within tolerance; narrow mode appears at 448 pt; the palette and shortcuts work; the layout persists; attention-first ordering is intact.

### PX-E2E-046 — Agent list: status classes, unread, pins, archive with undo, filters, grouping and search

**Setup:** packaged app, 200 seeded tasks, one cloud task last viewed, one running task, one pending approval.  
**Action:** browse, filter, group, pin to the cap, archive a finished and a running task with undo, start a new task, restart renderer and Core.  
**Pass:** classes and precedence are exact; the approval dot persists; archive is reversible and confirmed for running work; a new task is local by default; list updates meet the budget.

### PX-E2E-047 — Conversation surface: streaming render, tail status, step folding, scroll, density and failure presentation

**Setup:** packaged app, live gateway and scripted endpoint, a fixture with a hanging command and a hostile tool output.  
**Action:** stream an answer, scroll up during it, run the hanging command, force a provider failure, switch densities.  
**Pass:** streaming follows the tail until scrolled; the pill counts; folding and density are presentation only; stall escalates; the prompt is restored on failure; hostile content is inert.

### PX-E2E-048 — Typed apps panel: Changes, Terminal, Browser, Files and Evidence with per-task state

**Setup:** packaged app, real Core, a fixture with a running build, a pending change and a browser session.  
**Action:** open each app, type in both terminal kinds, revert and stage a file, commit and push, restart the renderer.  
**Pass:** terminal behaviour and ownership are exact; changes actions run on the worktree; commit and push were approved; per-task state returns after restart.

### PX-E2E-049 — Window and quit lifecycle: minimum size, quit protection, startup recovery counts and hook completion

**Setup:** packaged app, real Core, a running task, a slow session-end hook and a failing one.  
**Action:** quit with the run active and choose resume, reopen, then repeat with the failing hook and with `SIGKILL` of the Core during quit.  
**Pass:** quit asks; recovery counts are the Core's; the slow hook completes and the failing hook is visible and journaled; the minimum size holds; webPreferences, CSP and preload are unchanged.

### PX-E2E-050 — Durable input queue management and typed interrupt

**Setup:** packaged Core, live gateway and scripted endpoint, a fixture with a slow command.  
**Action:** queue three follow-ups during a long answer, edit, delete and reorder, use Send now during a second answer that is running a command, then kill the Core with items queued.  
**Pass:** items drain in order after the turn; edit, delete and reorder hold; Send now reconciles the command first and interrupts once; the queue survives the kill.

### PX-E2E-051 — Task mode as a typed Core field with an enforced capability posture

**Setup:** packaged Core, live gateway, a fixture repository with a failing test.  
**Action:** run one task through ASK, PLAN then accept, DEBUG and MULTITASK, let an agent propose a switch and leave it unanswered, restart the Core.  
**Pass:** each posture is enforced by the Core; plan acceptance transitions to AGENT; the unanswered proposal skips; remembered consent persists.

### PX-E2E-052 — ListSkills protocol command for the slash menu

**Setup:** packaged Core, a profile with five skills in different scopes and trust states.  
**Action:** list skills, install and revoke one on disk, invoke a user-only skill from the model.  
**Pass:** scope, trust and provenance are exact; the list refreshes on change; the user-only skill is refused for the model; revoked skills cannot run.

### PX-E2E-053 — SetExecutionPreference command wired to the existing routing owner

**Setup:** packaged Core with a signed registry and a policy that blocks one model, live gateway.  
**Action:** change the objective profile between turns, pin an allowed model, try a blocked one, restart the Core.  
**Pass:** the next compile follows the new preference and records it; the blocked pin is refused with the typed reason; the preference survives restart; routing semantics are untouched.

### PX-E2E-054 — Composer: placeholders, mode chips, add-context and @ menus, slash menu, attachments, history and drafts

**Setup:** packaged app, real Core, a profile with built-in and imported skills, a denied path and an image.  
**Action:** cycle modes, use the chip menu, open @ and /, attach the image and the denied path, send in Ask and try a write, restart the renderer.  
**Pass:** modes and chrome match the targets; the posture is enforced by the Core; menus are typed and ordered; the denied attachment is refused; drafts persist.

### PX-E2E-055 — Run controls: queue tray, send-behaviour education, stop and edit-in-place, side question, background-terminals tray

**Setup:** packaged app, real Core, live gateway, a fixture with a long command and a background sleep.  
**Action:** queue and edit messages, use Send now and Stop, kill the background terminal, ask a side question, leave a mode proposal unanswered.  
**Pass:** the queue, interrupt and stop behave as the Core defines and labels say so; one wake-up per kill; the side answer stays out of the main context; the proposal skips after 15 s.

### PX-E2E-056 — Model chip and picker with objective profiles, variants and the policy-blocked tray

**Setup:** packaged app, real Core with a registry and a blocking policy, live gateway.  
**Action:** switch profile and pin a model, select the blocked model and press Enter with the tray up, restart.  
**Pass:** profile and pin reach the Core and the next compile; the blocked choice is refused with the typed cause and no text is lost; the choice persists.

### PX-E2E-057 — Run-mode presets and durable allowlist rules owned by the policy kernel

**Setup:** packaged Core, a fixture worktree, a command that writes inside and one that writes outside the workspace, a command that fetches a URL.  
**Action:** run each in every mode, add and revoke a rule, append a pipeline to a matching command, restart the Core under Run everything.  
**Pass:** always-ask classes ask in every mode; rules are scoped, receipted and revocable; Run everything does not survive restart; no classifier approves anything.

### PX-E2E-058 — Docked approval stack: exact-intent card, deck, shortcuts and persistence

**Setup:** packaged app, real Core, live gateway, a fixture with two commands outside the allowlist.  
**Action:** trigger one card, run it, trigger it again and skip, add a rule with Shift+Enter, trigger two at once, kill the renderer then the Core with a card pending.  
**Pass:** the intent hash binds each decision; skip is typed; the rule is durable; both restarts leave the card pending; two cards are decided separately.

### PX-E2E-059 — Context accounting by category served by the Core

**Setup:** packaged Core, live gateway, a task with rules, a skill, an external tool, a subagent and a compaction epoch.  
**Action:** run turns, read the breakdown, compare with the provider request, switch model.  
**Pass:** eight categories sum to the total; the total matches the provider within the error; the window follows the model.

### PX-E2E-060 — Context ring, usage summary and the docked usage, limit and offline trays

**Setup:** packaged app, real Core, live gateway, a forced low budget and a network cut.  
**Action:** grow a task past a compaction, open the tray, exhaust the budget, cut the network, force a provider error.  
**Pass:** ring and tray equal the Core's figures; the limit, offline and error trays name cause and next action; nothing traps input.

### PX-E2E-061 — Per-turn checkpoints with exact reversible restore

**Setup:** packaged Core, a fixture worktree, three agent turns that create and edit files, one user edit after the first checkpoint.  
**Action:** restore turn 1 (previewing the user edit), redo, restore again and kill the Core mid-restore, then fork from a message.  
**Pass:** files match the checkpoint hashes; redo is exact; the user edit is protected; the kill leaves a whole state; the fork has no stale approval.

### PX-E2E-062 — Checkpoint restore, redo, edit-in-place and fork in the conversation

**Setup:** packaged app, real Core, a worktree with agent edits and one user edit.  
**Action:** open the dialog from a message, continue, redo, edit an older message with Keep and with Revert, use Undo all, kill the renderer during the dialog.  
**Pass:** counts match the Core; hashes match; redo is exact; user edits are protected; the kill changes nothing.

### PX-E2E-063 — Project records and membership over the WorkGraph

**Setup:** packaged Core, two workspaces, ten tasks including a subagent task and a draft.  
**Action:** create and rename a project, add and remove tasks, try each forbidden move, kill the Core between the phases of an add.  
**Pass:** guards hold with typed reasons; membership is projected from events; the kill is reconciled to one consistent state.

### PX-E2E-064 — Project-aware agent list: Projects group, stored views and project grouping

**Setup:** packaged app, the fixture of PX-063.  
**Action:** browse the Projects group, add and remove tasks by menu and drag, attempt each forbidden move, group by project, restart.  
**Pass:** the group is separate; changes follow the Core; refusals are shown with reasons; layout persists.

### PX-E2E-065 — Worktree lifecycle: creation policy, setup commands, scheduled cleanup, lease and retention

**Setup:** real Git repository and packaged Core, 30 worktrees in mixed conditions, a failing and a hanging setup list.  
**Action:** create worktrees, run cleanup on schedule and after a restart, race two cleanup requests, run the setup lists.  
**Pass:** only eligible worktrees are removed in the stated order; the lease admits one holder; failures are visible; the report equals the filesystem.

### PX-E2E-066 — Apply-back of a worktree result to the user's checkout with conflict handling and exact undo

**Setup:** real Git repository, a reviewed worktree result, a conflicting user edit in the checkout.  
**Action:** apply cleanly, then with the conflict choosing each option and undoing, make the review stale, kill the Core mid-apply.  
**Pass:** hashes match after each choice and after Undo; stale and unapproved applies are refused; the kill leaves a whole state.

### PX-E2E-067 — Hardened Git execution: repository hooks, fsmonitor, attributes and credential redaction

**Setup:** real Git, four hostile fixture repositories.  
**Action:** run every Core Git operation against each.  
**Pass:** no hook, fsmonitor or filter runs; no credential is retained anywhere; each neutralisation is recorded.

### PX-E2E-068 — Worktree management and apply-back conflict modal in the desktop

**Setup:** packaged app, real Git and Core, worktrees in mixed conditions, a reviewed result with a conflicting checkout edit.  
**Action:** apply with each conflict option and undo, try overwrite without confirmation, run cleanup, inspect the list.  
**Pass:** hashes match each choice; Cancel is the default; the Core refuses an unconfirmed overwrite; the list and report equal the Core's state.

### PX-E2E-069 — Computer Runtime contract: typed computer tools, scopes, handles, refusal taxonomy and unknown-outcome latch

**Setup:** packaged Core, the macOS actuator, the four fixture applications.  
**Action:** complete an accessibility task, provoke each refusal code, kill the actuator after delivery, issue two calls at once.  
**Pass:** no coordinate was used for the accessible task; every code returns its escalation; the latch holds with one UNKNOWN receipt; calls serialise; unknown arguments are refused.

### PX-E2E-070 — Per-call exact-intent approvals for native control, never allowlistable

**Setup:** packaged Core, the macOS actuator, two fixture applications with spoofable window titles, a policy bundle.  
**Action:** run actions with and without approval, try a rule and Run everything, spoof a title, expire a grant, run a child task, apply the three policy variants.  
**Pass:** every action needed its own exact approval; no rule or mode covered it; identity is the signed bundle; grants are narrow and expire; policy fails closed.

### PX-E2E-071 — macOS actuator helper: separate signed process, authenticated local RPC, accessibility-first background control, takeover lease and stop

**Setup:** a real macOS machine, the packaged app, a fixture application, a tampered helper copy and a rival socket process.  
**Action:** drive the fixture in the background, revoke the permission, start a rival socket, send a bad token, type on the keyboard, press Stop from each source, kill the Core and then the helper.  
**Pass:** the person's cursor was never moved in APP scope; permission, identity and token failures are refused; human input parks control; Stop is within 500 ms from every source; both kills leave a safe state.

### PX-E2E-072 — Actuator supply chain: bundled signed helper, hash-pinned manifest, rollback and process identity

**Setup:** a signed packaged build and an update package, a modified helper copy.  
**Action:** verify the chain, swap in the modified helper, apply an update during a live session, kill the process mid-update, roll back, force a protocol mismatch.  
**Pass:** only the listed signed helper runs; updates wait for idle and recover whole; rollback works; mismatch is refused with a reason.

### PX-E2E-073 — Browser control hardening: CDP deny list, per-call origin gate, certificate trust, permissions and per-task view ownership

**Setup:** packaged app, Core, a local HTTPS fixture with a self-signed certificate, a hostile redirecting page, a `file:` page.  
**Action:** issue each denied protocol call, act after a client-side redirect to a disallowed origin, hit the `file:` page and an unknown origin, take the certificate prompt both ways, sign out, open seven hidden views, set agent emulation.  
**Pass:** denied methods never reach the page; origin is checked per call; certificate trust is explicit and clearable; sign-out wipes data; views are owned and bounded; emulation reverts.

### PX-E2E-074 — Computer-control desktop surface: permission window, presence and Stop, action cards and approval card

**Setup:** packaged app on a real macOS machine, the helper, two windows, a fixture task that needs control.  
**Action:** first run with no permissions, grant one then both, run the task to its approval card, press Stop, kill the renderer during control.  
**Pass:** the permission flow is minimal and routed once; the card is exact with no Always; Stop works from the presence control and survives a renderer kill; presence is truthful.

### PX-E2E-075 — Computer-use subagent profile with an isolated target, constrained tools and stall rules

**Setup:** packaged Core, the macOS actuator, the live gateway, a fixture app with a login screen.  
**Action:** spawn the profile for a test task, try shell GUI driving, run it in ASK mode, run the failing and login fixtures, restart the Core and resume.  
**Pass:** tools and isolation hold; the stall rule produces the blocker report; the login is reported and not entered; resume needs a new approval.

### PX-E2E-076 — Screenshot and accessibility artifacts: window capture, masking, content-aware codec, retention and audit

**Setup:** real actuator, fixture applications with a password field, a secure input, a flat dialog and a photographic window, a locked-screen case.  
**Action:** run a 12-action session, capture each window type, lock the screen, repeat an identical capture, read the audit and the stored artifacts.  
**Pass:** secure fields are masked at the pixel level; the locked screen yields no frame; the codec is content-aware and memoised; the audit has counts only; retention is honoured.

### PX-E2E-077 — Editor file service: open with revision and encoding facts, multi-file save as one ChangeTransaction, merge basis and file revision events

**Setup:** packaged Core, a real worktree and the trusted checkout, fixtures in five encodings and line-ending conventions, an agent task writing to an open file.  
**Action:** open every fixture, save round trips, a two-file save with one denial, a stale save, a replayed save, a kill mid-save, an agent write, a symlink to a protected file.  
**Pass:** every byte outside the edit is preserved; the set is atomic; stale and replayed saves are safe; the ledger shows one state after the kill; clients learn of moves by event and by replay.

### PX-E2E-078 — Editor language intelligence: draft analysis overlay, outline, definitions and references served by the Core

**Setup:** packaged Core, the three fixtures, a Tier C fixture, a killable language service.  
**Action:** analyse a draft with an error, flood the rate limit, apply an agent edit and re-query, kill the language service, query the Tier C file.  
**Pass:** the draft diagnostic is accurate and nothing was written; the rate limit holds; results go stale on movement; degradation is explicit; Tier C stays honest.

### PX-E2E-079 — Workspace Editor surface: text engine, drafts, stale-draft merge, find and replace, tabs, accessibility and large-file behaviour

**Setup:** packaged app, real Core, a real worktree, fixtures of 1 MB, 6 MB, 60 MB, a binary file and a 100,000-line file, an agent task that edits an open file.  
**Action:** type, undo, search with a pathological pattern, save, change the file from an agent task, kill the renderer with a dirty draft, open the large and binary files, run the screen-reader and input-method scripts, run the performance traces.  
**Pass:** saves are exact and explicit; stale drafts rebase or merge, never overwrite; drafts survive a crash safely; large and binary files behave as stated; accessibility and budgets hold.

### PX-E2E-080 — Agent-aware editing: selection context, quick edit, inline per-hunk review and agent-write notices

**Setup:** packaged app, real Core, live gateway, a fixture function and a dirty draft.  
**Action:** select and quick-edit, accept one hunk and reject one, edit by hand then accept, change the file from a task under a dirty draft.  
**Pass:** context shows the selection; hunks apply individually at their revision; stale decisions refuse; the notice and merge appear; nothing is overwritten.

### PX-E2E-081 — Editor text-safety and conformance suite: encodings, round trips, fuzzed save path, input method and screen reader, latency

**Setup:** packaged app and Core, generated fixtures, the fuzzer, scripted input-method and screen-reader sessions.  
**Action:** run the property, fuzz, accessibility and latency suites, then apply the line-ending mutation.  
**Pass:** no file is ever corrupted; accessibility and budgets pass; the mutation is caught.

### PX-E2E-082 — Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions

**Setup:** packaged Core, a trusted repository with a committed definition file, a policy bundle.  
**Action:** create and edit a definition, enable it with valid and invalid inputs, load the repository definition, edit it, try model-origin creation, kill the Core.  
**Pass:** validation is typed and complete; approvals bind exact hashes; repository definitions are inert until approved; the model has no authority; state survives the kill.

### PX-E2E-083 — Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets

**Setup:** packaged Core with a controllable time source, a trusted repository, forge fixtures delivering duplicate events.  
**Action:** fire a schedule, restart across a slot under each missed-run policy, deliver duplicates, overlap firings under each concurrency policy, exhaust a budget, kill the Core mid-run.  
**Pass:** slots fire once; missed runs follow policy and never replay in bulk; duplicates are ignored; limits stop admission with reasons; recovery gives one outcome.

### PX-E2E-084 — Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches

**Setup:** packaged Core, live gateway, a real worktree, a hostile pull-request fixture.  
**Action:** run an automation that needs a protected write and approve late, let another expire, deliver the hostile body, pause, emergency stop, fail five times.  
**Pass:** approvals are the principal's and exact; expiry fails closed with a receipt; payloads are inert; every kill switch works.

### PX-E2E-085 — Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source

**Setup:** staging Cloud API, a worker, two tenants, a forge fixture and a signer.  
**Action:** send signed, replayed, unsigned, stale and cross-tenant deliveries, fire a cloud schedule across a control-plane restart, apply an allow-list.  
**Pass:** one task per genuine event; every bad delivery is rejected and audited; schedules fire once per slot; tenant and policy boundaries hold.

### PX-E2E-086 — Automations surface and CLI: list, editor with validation, enable approval, test run, run history and kill switches

**Setup:** packaged app and CLI, real Core, a write-capable definition, a repository definition and a hostile payload.  
**Action:** author and validate a definition, enable it through the dialog, dry-run a hostile payload, read the history, pause, repeat the operations from the CLI.  
**Pass:** validation and approvals are the Core's and exact; the dry run is effect-free; history shows typed reasons; the CLI matches.

### PX-E2E-087 — Plugin package format extension and signed catalog manifest with strict parsing and containment

**Setup:** packaged Core, fixture packages for each component kind and each refusal, signed and tampered catalogs, a foreign-format plugin.  
**Action:** inspect each package, add each catalog, import the foreign plugin.  
**Pass:** valid packages show every capability in words; each malformed package or catalog is refused with its typed reason; imports are quarantined.

### PX-E2E-088 — Install, update and rollback pipeline with trust disclosure, digest-pinned fetch and hardened extraction

**Setup:** real Core, real Git, a local HTTPS release fixture, signed and unsigned packages, malicious archives, an organisation policy bundle.  
**Action:** install by SHA and by asset, attempt each bad source and archive, kill mid-install, update with and without widening, pin, roll back, uninstall, approve a project list, apply the policy.  
**Pass:** only digest-pinned verified content installs; failures leave nothing; kills leave one whole version; updates cannot widen silently; policy applies.

### PX-E2E-089 — Plugin variables, secrets and hostile third-party content handling

**Setup:** real Core, broker, sandbox, hostile fixture packages, two tenants.  
**Action:** install packages with secret variables and hostile text, run their hooks and servers, search every artifact for the planted token, run the cross-tenant check.  
**Pass:** secrets never leave the broker; hostile text is inert and labelled; plugin code is sandboxed; tenants are isolated.

### PX-E2E-090 — External tool server trust by configuration hash, annotation classes, OAuth and connection state

**Setup:** real Core, the MCP test server, a local OAuth server.  
**Action:** trust a server then edit it, call read and unannotated tools, complete OAuth with two executors, force needs-auth, force a non-retryable error.  
**Pass:** trust is bound to the configuration; classes are conservative; OAuth is safe; failure states are terminal or explicit.

### PX-E2E-091 — Agent self-extension write-protection map and organisation plugin and server policy

**Setup:** real Core, a worktree, a policy bundle with block, require and allow-list rules.  
**Action:** let the agent write a skill and each control file, aim a symlink at one, apply and change the policy, import a stale allow-list.  
**Pass:** data is writable and inert, control files are not in any mode; policy is enforced and fails closed.

### PX-E2E-092 — Hook contract completion: merge order, exit-code blocking, fail-closed permission events, trusted-workspace gating and execution log

**Setup:** real Core, real hook processes of each behaviour, trusted and untrusted workspaces.  
**Action:** run the parallel merge, exit-code blocking, timeouts and crashes on permission and other events, the untrusted and symlinked configurations, the session-end hook.  
**Pass:** merge is deny over ask over allow; permission events fail closed; project hooks need trust; the log is complete; hooks never widen authority.

### PX-E2E-093 — Customize surface: seven tabs, scopes, view options, catalog browsing, trust disclosure, update and the hook log

**Setup:** packaged app, real Core, a local signed catalog with signed, unsigned, widening and malformed packages.  
**Action:** walk the tabs, install each package kind, take an update that widens, view a load failure, approve a project list, read the hook log, repeat from the CLI.  
**Pass:** disclosure is exact and precedes activation; unsigned packages are quarantined; widening updates block; failures are typed; the CLI matches.

### PX-E2E-094 — Window chrome: inset title bar, drag regions, window-control placement, optional translucency and persisted window state

**Setup:** packaged app on each supported macOS version, two displays, Reduce transparency on and off.  
**Action:** measure control placement at three zooms, drag and click in the bar, toggle translucency, save and restore state across a display change, hash the preload and CSP.  
**Pass:** placement and focus are correct; dragging and clicking do not collide; transparency obeys the preference; state restores on a visible screen; the security surface is unchanged.

### PX-E2E-095 — Native menus, badge, tray icon, deep links with confirmation, single instance and native dialogs

**Setup:** packaged app on a real macOS machine, real Core, link fixtures, a package catalog.  
**Action:** drive each menu item and shortcut, change attention state and read the badge, open valid and hostile deep links, launch a second instance, pick folders through the dialog, click a notification.  
**Pass:** menu and shortcuts agree; the badge is the attention count; links never act on their own; hostile links are refused and logged; one instance owns the profile.

### PX-E2E-096 — Main-process hardening: fuses, ASAR integrity, permission and navigation denials, IPC inventory and main-thread responsiveness

**Setup:** packaged build, a tampered ASAR copy, a flipped-fuse copy, a slow-Core fixture, a hung-renderer fixture.  
**Action:** read the fuses, launch the tampered archive, request permissions, attempt attach and cross-origin navigation, add an unregistered channel, slow the Core, hang the renderer.  
**Pass:** every required fuse is set and verified; tampering and denials hold; the inventory pins the IPC surface; main stays responsive; recovery keeps the session.

### PX-E2E-097 — Update application flow: apply at idle, staged and verified, with rollback and truthful states

**Setup:** signed packaged builds, a staged good update, a tampered update, an active run, a pending approval.  
**Action:** update at idle, with a run, with a pending approval and under an organisation pause, apply the tampered package, kill during apply, roll back.  
**Pass:** updates wait for idle; verification precedes apply; kills leave a whole version; rollback works; states are truthful.

### PX-E2E-098 — Electron and Chromium version policy and upgrade qualification gate

**Setup:** CI with the real pin and a test copy pinning an unsupported major, a candidate upgrade.  
**Action:** run the support check on both, run the qualification gate on the candidate with and without a skipped suite, read the manifest and the binary.  
**Pass:** unsupported or floating versions fail; the gate needs every suite; the manifest equals the binary.

### PX-E2E-099 — Terminal flow control (ACK, resize, bounded ring) and agent-facing shell.input and shell.attach tools

**Setup:** packaged Core and execd, a real PTY command that prompts, a noisy producer and a stalled client fixture.  
**Action:** attach two clients (one never acknowledges), resize, drive the prompt through the agent tools, then kill and restart the Core.  
**Pass:** memory stays bounded; the stalled client catches up exactly by cursor; the resize is seen by the child; the tools are refused across tasks; the process survives the Core kill; no secret is retained.

### PX-E2E-100 — CLI and IDE-adapter controls for task mode and execution preference

**Setup:** packaged Core, CLI and VS Code adapter, the live gateway, an organisation policy that blocks one model.  
**Action:** start tasks in ask and plan mode from the CLI, change the objective, try the blocked pin, restart the Core and read the state from the IDE.  
**Pass:** the Kernel refuses the ask-mode write, the plan task waits for acceptance, the preference is recorded, the blocked pin is refused with its typed reason, state is identical in every client.

### PX-E2E-101 — PauseTask and ResumeTask over the runtime park, with local persistence

**Setup:** packaged Core, a task running a long shell command and later a streaming turn, a second client.  
**Action:** pause mid-command, kill the Core, restart, resume from a client with a stale generation and then from the right one.  
**Pass:** the command is reconciled once, the task returns paused, the stale resume is refused, the real resume completes the task once.

### PX-E2E-102 — Named checkpoints, retention and garbage collection

**Setup:** packaged Core, a forty-turn task with a fork and three named checkpoints, an archived applied task.  
**Action:** run the collector, kill it midway and rerun, restore every surviving checkpoint and the fork, request a collection while another holds the lease.  
**Pass:** named and fork-parent checkpoints survive, all survivors restore byte-exact, the interrupted run recovers, the concurrent request is refused, the report matches the disk.

### PX-E2E-103 — Hunk attribution to the originating tool call and per-hunk decisions that keep the run going

**Setup:** packaged Core, a real worktree, the live gateway, a task that writes by tool, by shell and by test runner.  
**Action:** reject one hunk while running, decide a stale hunk, kill the Core between revert and notice, restart.  
**Pass:** attribution is exact, the reject reverts only its hunk, the run continues, stale and unattributed cases are refused, recovery yields one state.

### PX-E2E-104 — Review reachable from any task state, with live per-hunk controls and the originating-call chip

**Setup:** packaged app, real Core and worktree, the live gateway, a task producing three hunks.  
**Action:** open Changes in four task states, reject one hunk while running, trigger a stale decision, kill the renderer.  
**Pass:** the panel is reachable in every state, the reject reverts exactly that hunk, the run goes on, stale is explained, the panel restores.

### PX-E2E-105 — Skills a user can use: trust by command, a System scope, an index under a budget, skill.load and path gating

**Setup:** packaged Core, CLI, a user skill, an imported skill, a System policy file and the scripted provider.  
**Action:** list, trust, load, edit one byte, add thirty skills, open a matching path, add a System prohibition.  
**Pass:** only trusted unchanged skills reach the request, within budget, with path gating and System precedence honoured.

### PX-E2E-106 — Customize view over the existing skills, hooks and extensions (no catalog)

**Setup:** packaged app, real Core, fixture skills, hooks and one extension.  
**Action:** trust and revoke a skill, disable a hook, import a skill, inspect the extension, compare with the CLI.  
**Pass:** state matches the Core and the CLI, effects are real on the next request or event, quarantine holds, no catalog affordance exists.

### PX-E2E-107 — AGENTS.md and CLAUDE.md as a trust-gated native rules layer

**Setup:** a trusted and an untrusted copy of this repository, a nested directory with its own AGENTS.md, the scripted provider.  
**Action:** run a task touching a nested path in both copies, plant a hostile line, edit the file mid-session.  
**Pass:** rules reach the request only in the trusted copy, precedence and caps hold, the hostile line has no effect, hashes follow the edit.

### PX-E2E-108 — Goal-seeded pre-turn context pack

**Setup:** real fixture repositories, the scripted provider capturing requests, a task that edits one file.  
**Action:** start a task, capture the first request, edit a file, compact, restart the Core, start a goal with no matches.  
**Pass:** the first request holds a provenance-bearing pack, stale parts are dropped, the budget holds, recovery is consistent, the empty case is explicit.

### PX-E2E-109 — Structured compaction summary with extractive fail-closed fallback, model-aware thresholds and a searchable transcript pointer

**Setup:** packaged Core, the live compatible gateway, a scripted summarizer that fabricates, fails and times out.  
**Action:** drive a run to 85 percent, compact, kill the Core mid-compaction, fork during compaction, fail the summarizer three ways.  
**Pass:** the live summary validates and the run completes; every failure falls back to extractive with its reason; kill and fork follow the epoch rules.

### PX-E2E-110 — Code graph: symbol reference and call edges, implementors and non-test impact selection

**Setup:** three fixture repositories with a hand-labelled dependency set.  
**Action:** ask what breaks if an interface changes, rename a function, edit a file, hit the expansion cap.  
**Pass:** non-test dependents match the labels, ambiguity is labelled, incremental update is correct, partial results are declared.

### PX-E2E-111 — Persisted incremental indexes and an indexed grep path

**Setup:** a large real repository, the real Core, a corrupted-index fixture.  
**Action:** build, restart and query, edit one and a hundred files, flip a byte in the index, kill during a write, compare indexed and scanned results.  
**Pass:** no rebuild after restart, refresh is incremental, corruption is detected and repaired, both search paths agree.

### PX-E2E-112 — Learned embedder and rerank option behind dependency admission

**Setup:** the admitted model file, a tampered copy, the corpus and a large repository fixture.  
**Action:** load the good and tampered files, edit a file, change the model, remove the model, run the benchmark.  
**Pass:** only the verified model loads, rebuilds are incremental, recall does not regress against the baseline, absence degrades honestly.

### PX-E2E-113 — Engineering memory wired into the prompt and the event log, with Agent and Space scopes

**Setup:** packaged Core, two sessions, the scripted provider, fifty memories across scopes.  
**Action:** promote in session one, start session two, forget, restart the Core, replay the log, plant a hostile memory.  
**Pass:** memory appears with provenance and disappears on forget, replay reproduces the store, scopes and budget hold, the hostile memory has no effect.

### PX-E2E-114 — Exec-only tool projection mode, a schema-bytes budget and a live paired trial

**Setup:** packaged Core, scripted provider, live compatible gateway, an extension adding twenty tools, thirty paired tasks.  
**Action:** run both modes, delegate a subtask, exceed the budget, call a deferred tool without authority, run the paired trial.  
**Pass:** schemas and bytes match the mode and the budget, deferral never authorises, the trial report states accuracy, tokens and calls with intervals.

### PX-E2E-115 — Lazy MCP discovery, list_changed, idle reaper and a large-catalog benchmark

**Setup:** the real MCP test server with five hundred tools, a hostile untrusted server and a server that ignores shutdown signals.  
**Action:** start a task, describe three tools, add a tool live, call through an untrusted server, idle and reap, run the benchmark.  
**Pass:** no schemas by default, describe is exact, the catalog refreshes, trust is still the gate, no process leaks, the benchmark compares lazy with eager.

### PX-E2E-116 — Hierarchical cost and wall-clock budgets, max_children and read-scope enforcement

**Setup:** packaged Core, a parent with cost and clock caps, three real children, the live gateway.  
**Action:** spawn past the remainder, exhaust a child clock, forbid spawning, read outside scope, kill between reservation and admission.  
**Pass:** the cap holds, exhaustion is typed, reservations return, scope is enforced, recovery is consistent.

### PX-E2E-117 — More hook points, kernel-gated hook context injection and prompt-type hooks

**Setup:** a fixture task with real hook processes and the scripted provider capturing requests, an untrusted workspace copy.  
**Action:** trigger every new event, inject context, inject a hostile instruction, exceed the budget, time out a permission hook, run in the untrusted copy.  
**Pass:** events fire at the right points, context is labelled and bounded, authority never widens, failures close, untrusted hooks stay off.

### PX-E2E-118 — Worktree isolation as a typed task option: the run executes in its own worktree

**Setup:** a real repository with uncommitted files, the real Core, the live gateway, a symlink-escape fixture.  
**Action:** run an isolated task that edits, builds and tests, try the escapes, run the reaper, break the worktree root, kill and restart.  
**Pass:** the checkout never changes, escapes are refused, the reaper spares the live worktree, failure is typed, the run resumes in place.

### PX-E2E-119 — Agent merge integration: git.merge tools, persisted merge state, post-merge verification and parent completion gated on integrated children

**Setup:** a real repository, two real child branches with an intentional conflict, a parent task, a failing-test resolution.  
**Action:** prepare, resolve wrongly then rightly, kill mid-merge, try to complete the parent early.  
**Pass:** the conflict is evidenced, only a verified resolution commits, recovery is whole, early completion is refused, receipts are exact.

### PX-E2E-120 — Browser navigation target policy: loopback, private ranges, link-local, metadata endpoints, redirects and rebinding

**Setup:** packaged host, a fixture web server on loopback, a private alias, a fake metadata endpoint and a rebinding resolver.  
**Action:** navigate directly, by redirect, by sub-frame and by link, try the rebinding name, then name the host in policy.  
**Pass:** denied targets are never reached, rebinding is caught, named targets work, the model cannot widen policy, refusals are journaled.

### PX-E2E-121 — Browser feedback primitives: viewport capture, console, network, scroll, wait, an escalation field and the unknown-outcome latch

**Setup:** packaged host, real Chromium, a fixture app with console errors, failing requests, a long page and a slow element, a host kill fixture.  
**Action:** read console and network, capture, scroll and wait, kill the host mid-click, observe, retry.  
**Pass:** outputs are bounded and redacted, scroll and wait work, the latch refuses until reconciled, refusals are typed with escalation.

### PX-E2E-122 — Page-state deltas from a host mutation observer, since_fingerprint, multi-frame accessibility trees and stable identities across restart

**Setup:** packaged host, a dynamic fixture app with same-origin and cross-origin frames and a mutation storm.  
**Action:** update the page, ask since a fingerprint, ask with a stale one, restart the Core, compact, run the storm.  
**Pass:** deltas are exact and timely, stale fingerprints rehydrate with a reason, identities persist, frames are covered, the stream stays bounded.

### PX-E2E-123 — Semantic compiler: page classification, forms and fill_form, intent filter, derived actions and stronger action risk classification

**Setup:** fixture sites with a login, a checkout, a result list and a destructive dialog, a credential handle, a hostile page.  
**Action:** classify, fill the login, run a derived checkout action, hit the cross-origin link and the destructive dialog, load the hostile page.  
**Pass:** classification is correct, no secret leaks, derived actions verify, risk rises where it should, page text stays data.

### PX-E2E-124 — WebMCP rung: page-declared tools with origin binding and trust labels

**Setup:** packaged host, a fixture page declaring two tools, an iframe from a second origin, a page with a colliding tool name.  
**Action:** discover, call both tools, call from the iframe, navigate away and call again, load the colliding page.  
**Pass:** tools are origin-bound and untrusted, the Kernel gates every call, stale and cross-origin calls are refused, no host tool is shadowed.

### PX-E2E-125 — GitHub pull request read, diff and status comments through the forge adapter

**Setup:** a real GitHub test repository and token, the real Core, the live gateway.  
**Action:** read and diff a real pull request, post a comment with and without approval, plant an instruction and a token, remove the token.  
**Pass:** reads are bounded and untrusted-labelled, writes need approval and are redacted, absence is typed, the result is proven on real GitHub.

### PX-E2E-126 — Check-suite and comment webhooks into CI-result and review-comment ingestion

**Setup:** a staging Cloud API and worker, signed deliveries, a task with an open pull request.  
**Action:** deliver failing and passing check events, a comment, a replay, a bad signature, a wrong tenant, restart the control plane.  
**Pass:** evidence and steering arrive with provenance, the gate is not overridden, bad deliveries are rejected and audited, nothing is lost or duplicated.

### PX-E2E-127 — ci_evidence and review comments in Review and the CLI, and the first real-GitHub issue-to-task-to-PR-to-CI proof

**Setup:** a real GitHub test repository with a workflow, the packaged app and CLI, the live gateway.  
**Action:** file an issue, run the task to a pull request, let CI fail then pass, comment on the pull request, restart everything.  
**Pass:** the evidence is shown with provenance in both clients, acceptance is never inferred from CI, the comment steers a turn, state survives restarts.

### PX-E2E-128 — Cloud handoff, watch and approve verbs in the CLI and the desktop

**Setup:** a staging Cloud API, a real worker and sandbox, the CLI and packaged desktop, a policy that forbids handoff in one profile, a planted secret.  
**Action:** hand off, watch, answer an approval twice from two clients, drop the network, return, hand off under the forbidding policy.  
**Pass:** the bundle is secret-free and confirmed, approvals apply once, watch resumes, return is lossless, policy refuses.

### PX-E2E-129 — OIDC authorization-code with PKCE sign-in, tenant provisioning and the signed policy-bundle publisher route

**Setup:** a staging Cloud API with Postgres, a local OIDC provider, an organisation signing key and a foreign key.  
**Action:** sign in, replay and tamper with the flow, provision and disable a principal, publish good, foreign, stale and tampered bundles.  
**Pass:** valid flows work, attacks are rejected, tenants are isolated, only valid newer bundles are accepted, clients fail closed.

### PX-E2E-130 — Credential broker unification: one broker interface and handle path for tools, providers, MCP, browser and cloud

**Setup:** the real Core with provider, MCP server, browser fill and a staging cloud, a planted token.  
**Action:** use each consumer, search every artifact, expire and revoke a handle, rotate, run the lint on a violating fixture.  
**Pass:** one interface serves all consumers, no secret survives anywhere, revocation and rotation are honoured, bypasses are caught.

### PX-E2E-131 — AuthorizationEpoch and CapabilitySnapshot: the capability view frozen per model round

**Setup:** packaged Core, the scripted provider, a policy file changed mid-round, a revocation and an emergency stop.  
**Action:** tighten policy mid-round, revoke trust, press stop, kill and restart, forge a receipt epoch.  
**Pass:** changes land at the boundary, stop and revocation are immediate, replay is identical, forgery is rejected.

### PX-E2E-132 — Process intelligence: port and dev-server detection, readiness and service health reported by the Core

**Setup:** real execd and Core, three real dev servers, an unrelated listener, a failing health endpoint.  
**Action:** start and stop the servers from the agent, break health, start the unrelated listener, restart the Core.  
**Pass:** ports and states are reported exactly, unrelated listeners are ignored, probes stay on loopback, recovery re-observes.

### PX-E2E-133 — Escalation and reviewer leg samples with priced cost in Outcome Statistics

**Setup:** real Core, a scripted cascade and a critique run, one live run, a restart between legs.  
**Action:** run both paths, fail and retry a leg, cancel a leg, restart, replay, compare routing decisions for a fixed snapshot.  
**Pass:** samples are exact and attributed with full cost, replay and restart are idempotent, routing is unchanged.

### PX-E2E-134 — Model registry sign, verify and activate verbs on the CLI

**Setup:** real Core and CLI, a good, a tampered, a foreign-key and a revoked bundle.  
**Action:** sign, verify and activate, try the bad bundles, kill during activation, roll back.  
**Pass:** only a verified newer bundle becomes active, the swap is atomic, rollback works, no key leaks.

### PX-E2E-135 — Widened gate calibration corpora across languages with generated adversarial checks

**Setup:** three language fixtures, hidden oracle labels, seeded false-accept candidates, a live gateway for candidate runs.  
**Action:** run the calibration, compare with the previous gate, search requests for labels, rerun on the same snapshot.  
**Pass:** minima are met, seeded false accepts are now rejected, rates carry intervals, no label leaks, the report is reproducible, no gate is attested.

### PX-E2E-136 — One live paired benchmark of DIRECT, CASCADE and CRITIQUE on first-party providers

**Setup:** a signed registry with two live bindings, a pinned thirty-task set with real verification commands.  
**Action:** run direct, cascade and critique on the same tasks, analyse, rerun the analysis, check the default route and the gate nodes.  
**Pass:** the report is complete with intervals and every leg's cost, reproducible from retained data, default route and gates unchanged.

### PX-E2E-137 — Retrieval benchmark with an external baseline profile, fifty or more cases, a larger public repository and live same-model measurement

**Setup:** pinned repositories including a large public one, the real baseline tool, the live compatible gateway.  
**Action:** run baseline and treatment on fifty cases, measure index times, rerun the scoring, remove the baseline binary.  
**Pass:** the report has intervals and the baseline column, is reproducible, and a missing baseline fails the run.

### PX-E2E-138 — Compaction evaluation that is not self-referential

**Setup:** twenty long fixture runs with forced compaction, a lossy summarizer double, the live gateway.  
**Action:** run the three arms, score recall against the event log, run the lossy double, rerun the scoring.  
**Pass:** the report compares the arms with intervals, the lossy double scores worse, the report reproduces.

### PX-E2E-139 — OpenTelemetry export, child-cost rollup and persisted health

**Setup:** a real parent and two children, a local OTLP collector, a planted secret, a collector kill.  
**Action:** run, export, kill the collector, restart the Core, compare exported and recorded cost.  
**Pass:** the trace shows parent and child cost, nothing sensitive is exported, faults never touch the task, health persists, cost matches the record.
