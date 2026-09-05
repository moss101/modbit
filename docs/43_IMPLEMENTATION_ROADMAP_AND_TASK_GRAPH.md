# Implementation Roadmap and Verifiable Task Graph

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


This is a clean build. Tasks are ordered to produce executable vertical slices early and prevent architecture drift.

## M0 — Repository and authority (P0)

**M0.1** Create monorepo, Rust workspace, pnpm workspace, CI, architecture-lint.  
Acceptance: fresh clone builds on macOS/Linux/Windows CI; forbidden dependency test works.

**M0.2** Add authoritative ADRs and status ledger.  
Acceptance: CI rejects changed locked architecture file without linked ADR metadata.

**M0.3** Protobuf domain/protocol generation Rust↔TS.  
Acceptance: round-trip compatibility tests.

## M1 — Durable local shell and Core (P0)

**M1.1** Implement Session/Task/Run/Turn/RunStep domain and event store.  
**M1.2** Implement SQLite migrations, projections, command idempotency.  
**M1.3** Implement local authenticated SurfaceProtocol.  
**M1.4** Electron shell + real Fleet/New Task UI against Core.  
**M1.5** Crash/restart snapshot + event replay.

Milestone proof: user creates durable task, kills/restarts app/Core, same task recovers with no fake state.

## M2 — Real local engineering loop (P0)

**M2.1** Workspace File Service with safe paths/revisions.  
**M2.2** Git branch/worktree/diff operations.  
**M2.3** `modbit-execd` structured argv/PTy/replay/OutputRef.  
**M2.4** Tool Registry + direct `fs/git/shell/test` tools.  
**M2.5** Capability Kernel + basic approval flow.  
**M2.6** Provider Gateway OpenAI + Anthropic streaming.  
**M2.7** Basic Prompt Compiler and one-agent runtime.  
**M2.8** Verification engine build/test checks.  
**M2.9** Trusted Code Review Surface.

Milestone proof: E2E-001/002/003 with live model and actual test pass.

## M3 — Context intelligence (P0)

**M3.1** exact/regex/path index.  
**M3.2** Tantivy BM25.  
**M3.3** tree-sitter AST/symbol index.  
**M3.4** headless LSP diagnostics/symbol bridge.  
**M3.5** USearch embeddings + changed-chunk incremental update.  
**M3.6** dependency/Git/test/runtime evidence graph.  
**M3.7** L0-L3 retrieval planner + fusion.  
**M3.8** Context Pack/token budget/provenance ledger.  
**M3.9** retrieval benchmark harness.

Proof: profile A/B/C benchmark plus retrieval-before-edit visible in task evidence.

## M4 — Durable recovery spine (P0)

**M4.1** Protocol State store.  
**M4.2** Compaction epochs + async worker + stale rejection + sync fallback.  
**M4.3** Workspace checkpoint baseline/delta objects + epoch fencing.  
**M4.4** kernel lease/session fencing.  
**M4.5** terminal/browser/sandbox cursor metadata interfaces.  
**M4.6** kill-point recovery suite.

Proof: E2E-004/005/006/007/008.

## M5 — Procedural runtime and skills (P0)

**M5.1** Dynamic task-scoped tool projection.  
**M5.2** embedded QuickJS isolate with no ambient authority.  
**M5.3** generated `tools.*` bindings routed through normal Tool Registry.  
**M5.4** exec/wait/request_user_input surface.  
**M5.5** skill manifest/selector/compiler, provenance and signing.  
**M5.6** tool-schema/token-economics benchmark.

Proof: E2E-011/012; direct and procedural mode yield equivalent receipts/policy behavior.

## M6 — Subagents/fleet (P0→P1)

**M6.1** WorkGraph/AgentGraph projections.  
**M6.2** capacity ticket allocator.  
**M6.3** transactional subagent admission.  
**M6.4** semantic write-conflict detector.  
**M6.5** subagent result/evidence handoff.  
**M6.6** attention-first Fleet states/UI.

Proof: E2E-009/010 and user can supervise multiple tasks without raw-log polling.

## M7 — Live browser (P0)

**M7.1** local sandboxed WebContents session + CDP bridge.  
**M7.2** AX/DOM/layout semantic entities and stable IDs.  
**M7.3** state fingerprints + delta stream.  
**M7.4** semantic actions and postconditions.  
**M7.5** targeted screenshot/vision fallback.  
**M7.6** control lease/takeover.  
**M7.7** prompt-injection provenance isolation.  
**M7.8** credential handle fill path.

Proof: E2E-013..016.

## M8 — Cloud isolated execution (P0/P1)

**M8.1** Cloud API/Postgres/object store.  
**M8.2** cloud session kernel lease + worker.  
**M8.3** Sandbox Gateway + sandbox substrate adapter.  
**M8.4** signed/versioned `modbit-guest`.  
**M8.5** typed guest process/fs/PTy RPC.  
**M8.6** credential broker + egress policy.  
**M8.7** local→cloud checkpoint handoff.  
**M8.8** cloud browser remote stream/CDP.  
**M8.9** sandbox-loss recovery.

Proof: E2E-017/018/024.

## M9 — Engineering memory/effects/security hardening (P0/P1)

**M9.1** Engineering Memory schemas/scopes/promotion.  
**M9.2** protected-effect receipt hash chain.  
**M9.3** full protected-path/secret redaction/broker hardening.  
**M9.4** external MCP gateway.  
**M9.5** emergency stop.  
**M9.6** security fuzz/property/attack suites.

Proof: memory cannot be created from transcript without promotion; receipt chain verifies; threat tests pass.

## M10 — Release hardening

**M10.1** telemetry/cost/SLO dashboards.  
**M10.2** updater/signing/SBOM.  
**M10.3** full RC E2E catalog.  
**M10.4** performance regression gates.  
**M10.5** docs/runbooks/support diagnostics.  
**M10.6** Release Zero scenario.

Proof: full Release Zero proof + package evidence + EPR gates A–G SATISFIED.

### Critical path

`M0 → M1 → M2 → M4` is the reliability spine. `M3` and `M5` can proceed after M2 contracts stabilize. `M6` depends on M2+M4. `M7` depends on policy/tool/event contracts. `M8` depends on M4+M7 interfaces. Do not start broad multi-agent/cloud work before the single-agent durable local loop is E2E proven.


## V2 sequencing delta

Before product freeze, implement the requirement coverage CI and canonical tool conformance harness. MediaEnvelope/Media Pipeline belongs before multimodal provider/tool features. durable subagent continuation lands only after durable AgentNode/protocol state. Skill Evolution Lab is deliberately after stable Skill Registry + real evaluation harness and starts as shadow/experiment. `41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` provides the requirement-linked task IDs; engineering should batch them by canonical owner rather than create hundreds of independent components.


## V3.1 enumerated tasks from the V2 sequencing delta

The V2 delta above names work that never received a task row. V3.1 enumerates it so the project graph can schedule and gate it. These are the only task rows added since V3; they introduce no new requirement.

| Task | Milestone | Scope | Acceptance |
|---|---|---|---|
| **M0.4** | M0 | Requirement-coverage CI and REQ→IMP→QUAL traceability parser (`45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md`) | CI fails on an ADOPT/ADAPT row without owner/`IMP-EV-*`/`QUAL-EV-*`, a COMPLETE task without evidence, or a duplicate active owner |
| **M2.10** | M2 | MediaEnvelope + Media Pipeline before any multimodal provider/tool feature (`25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md`) | real PNG/JPEG/text-PDF read through `fs.read` with provenance, budgets and artifact digests |
| **M5.7** | M5 | Skill Evolution Lab as shadow/EXPERIMENT behind Skill Registry + Eval Harness (`26_SKILL_REGISTRY_AND_EVOLUTION.md`) | WSK-E2E-001..010 pass; candidate cannot self-promote; production recovery is independent of lab data |
| **M6.7** | M6 | Durable subagent continuation: background child survives restart | kill Core mid-child run; child identity, lineage, event offsets and result envelope survive |
| **M10.7** | M10 | Canonical tool and capability conformance harness (`56_TOOL_CAPABILITY_CONFORMANCE.md`) | every production tool family passes its real-substrate conformance suite; no canned success |

## Release projections

ALPHA, BETA and RELEASE_ZERO are derived projections over these tasks, defined in `75_PHASED_RELEASE_PLAN_AND_READINESS.md` and computed by `python3 tools/graph.py releases`; they add no status and no milestone. PX tasks from `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` schedule into these milestones like EPR tasks.

## Machine-readable form

`../graph/project-graph.json` is generated from this file (milestones, `Mx.y` tasks, proofs, dependencies) plus the ledgers. Task order inside a milestone is the order listed here. `python3 tools/graph.py ready` answers "what next"; `python3 tools/graph.py status` produces the roll-up for `98_BUILD_MANIFEST.md`.

## Execution policy development sequence (V3.3 EPR v1.1)

The existing 11 milestones/reliability spine remain. Doc 49 is the authoritative explicit task dependency table; numeric EPR IDs are stable identifiers, not execution sequence.

| Phase | Existing-owner tasks | Required ordering/proof |
|---|---|---|
| 0 | EPR-000 | M2 real initial-leg baseline and raw accounting |
| 1 | EPR-001/002/003/014/015/016/004/005 | M2 versioned conditional schema/admission, separate stats, intrinsic profiler and confidence component before full compiler/runtime integration; no M3 index prerequisite |
| 2 | EPR-008/009/017 | M4 factual assurance, cache/epoch economics and independent acceptance after real recovery spine |
| 3 | EPR-006 | M5 prevalidated escalation slots, whole-plan budget and evidence; CASCADE is observed path label |
| 4 | EPR-018/007 | M6 actual disposable reviewer security environment before provider reviewer/reviser integration; CRITIQUE is observed label |
| 5 | EPR-010/019/011/012 | M9 independent outcomes/statistics and gate calibration/safe replay; M10 joint conditional parameter search and controlled promotion/rollback |
| 6 | EPR-013 | M10 evaluated Skill combinations in pinned Outcome Statistics |

Production cheap routing is earned by representative conservative quality and independent gate evidence, even if that delays nominal phase-2 activation until EPR-019. Baseline migration is not a new cheap-model promotion. Every active continuation topology is validated/reserved before transaction start. M10.3 depends on EPR-013 and EPR-019. EPR-014..019 are distinct delta slices on existing owners, not parallel systems. DOC-EPR-002 tracks this specification reseal independently; all product work remains NOT_STARTED.
