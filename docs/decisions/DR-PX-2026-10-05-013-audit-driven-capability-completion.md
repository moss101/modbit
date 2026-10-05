---
id: DR-PX-2026-10-05-013
title: Audit-driven capability completion enters the product ledger as PX-099..139, specified by doc 79 over existing owners only
status: accepted
date: 2026-10-05
supersedes: none
approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03
---

# DR-PX-2026-10-05-013 — Audit-driven capability completion

## Problem and goal

On 2026-10-05 a nine-part read-only audit of `main` (research/audit, untracked) compared the product to its vision and produced a task list of verification, fix, dossier and build tasks. The owner's goal is the end-to-end implementation of that list. The fix tasks (FIX-01..21) change existing code and enter no ledger. The build tasks (BLD-01..30) add behaviour, and every behaviour added to the product needs a requirement row, a task card, a qualification and a scenario before it can start (AGENTS.md, doc 46). DR-PX-2026-10-03-007, -008 and -010 (accepted the same day) already cover part of the list. This record covers the rest: it appends the rows for every BLD task that no existing PX row fully covers, with no new subsystem, no change to a sealed row and no dependency on a record the owner has not ratified.

## Trigger and evidence

- **Owner instruction of 2026-10-05**, the goal "end to end implementation for tasks in research/audit/01-TASK-LIST.md", recorded as `approved_by`.
- **Audit** (`research/audit/00-SUMMARY.md`, `01-TASK-LIST.md`, `audit-A..I.md`) at `main` 5fdb47f1: 61 vision areas classified, with file and line evidence per finding; results of Phase 0 reproduction and FIX-01..21 in `02-PHASE0-AND-FIX-RESULTS.md` (branch `wip/audit-fixes-integration`, pull request moss101/modbit#61, not merged).
- **Mapping work of this record** (doc 79 sections 4 and 7): every BLD task was checked against the rows of doc 62 as they stand. Of the 30 BLD tasks, four are fully covered by existing rows (BLD-01, 06, 22 and 25), eight are covered in their principal part and keep a residual that is a new row (BLD-02, 03, 04, 05, 07, 17, 18 and 19) and eighteen are not covered at all (BLD-08 to 16, 20, 21, 23, 24 and 26 to 30). The mapping drafted before the rows were read was corrected in eight places (doc 79 section 8).

## Current behavior

The ledger covers the agent-first workspace, native computer control and automations (accepted) and three records not ratified. It has no row for: terminal flow control and agent terminal tools; pause and resume; checkpoint retention; hunk attribution and mid-run review; skill trust and the skill index; AGENTS.md auto-load and the pre-turn pack; the structured compaction summary; the code graph; persisted indexes and the embedder; memory in the prompt; exec-only projection; lazy MCP discovery; hierarchical budgets; richer hooks; worktree isolation as a task option; merge integration; browser target policy, feedback primitives, deltas, the semantic compiler and WebMCP; GitHub read, webhooks and evidence rendering; the cloud client and OIDC; credential broker unification and the capability snapshot; process intelligence; the EPR evidence emission, operator verbs, corpora and live benchmark; retrieval and compaction benchmarks; and OpenTelemetry export.

## Proposed replacement

1. Append 41 rows REQ-PX-099..139 to doc 62 (task, qualification and PX-E2E scenario each), in ten groups: A reachability of the Core (PX-099..106), B instructions, context and memory (PX-107..113), C tool surface, budgets and hooks (PX-114..117), D worktrees and merge (PX-118..119), E browser (PX-120..124), F GitHub and cloud (PX-125..129), G kernel and credentials (PX-130..131), H process intelligence (PX-132), I EPR evidence (PX-133..136) and J benchmarks and observability (PX-137..139).
2. Specify them in doc 79: constraints, an audit classification per BLD task, 41 numbered requirements (ADC-*) each with its evidence basis, the rows and tiers, the traceability table BLD to PX rows to owner to tier to prerequisites, the corrections to the draft mapping, the explicit non-adoptions and the unverified register.
3. Declare the evidence tier of every row by behavioural risk: 39 release-critical and 2 iteration (PX-137, PX-138, measurement only). Where the audit's task list said iteration (BLD-30) the row is release-critical because the child-cost rollup extends accounting records that EPR-010 reads.
4. Every row lists DOC-PX-013 as a prerequisite, so no row of this group can start before the dossier task is COMPLETE; the tools and tests enforce that, and a test asserts that no row of an accepted record depends on a row or task of a proposed one.

## Independent Modbit design

- **Existing owners only.** Each row names the owner the audit and doc 81 identify: terminal, core-runtime, durability, workspace-git, desktop, skills, context-engine, memory, procedural-runtime, external-tools, extensions-hooks, browser, sandbox-cloud, effects-security, eval-bench, model-gateway and observability. No crate or service outside the existing owner list is introduced; where a row needs a new module it is registered under its owner as DR-M9-001 did.
- **Every row names its real boundary and its failure injection.** A closing test crosses a real process, filesystem, Git repository, Chromium, provider, SQLite or staging service, and each qualification states the fault (kill, corruption, stalled client, hostile content, expired handle) and a mutation that must make it fail.
- **Rows implement the audit's specific first missing link**, never a restatement of a feature name; where an audit fix already delivered part of the behaviour (FIX-01 for PX-067, FIX-06 for attribution, FIX-20 for execd bounds) the row builds on it and says so.
- **Rows that need the owner** say so in their scope: PX-112 needs a dependency admission naming the model; PX-125 and PX-127 need the owner's GitHub token and test repository; PX-136 needs first-party provider keys (until then the compatible gateway recorded under DR-M9-002). Without the input a row records BLOCKED and does not claim a proof.

## Security model

Every row keeps the existing rules: all external input is hostile, nothing bypasses the Capability Kernel, approvals stay exact-intent, secrets stay in the broker, content from repositories, pages, memory, skills, hooks, GitHub and MCP servers is data with provenance and never instruction. The new surfaces each carry a negative proof: skill trust bound to content hash (PX-105), repository instruction files only in a trusted workspace (PX-107), hook-injected context labelled and unable to widen authority (PX-117), page-declared tools origin-bound and untrusted (PX-124), browser target policy checked on the connected address (PX-120), webhook signature, replay and tenant checks unchanged (PX-126), PKCE and signed policy bundles (PX-129), one broker for all credentials (PX-130), a frozen capability snapshot per round (PX-131).

## Canonical owner mapping (doc 81)

No subsystem is added and none changes owner. The per-row owner is in doc 79 section 7. Rows PX-133 to PX-136 touch the execution policy router only as follows: PX-133 conforms the Outcome Statistics emission to the sealed ADR-R-051 and ADR-R-055, PX-134 adds the operator verbs for the signed registry of EPR-002, PX-135 widens calibration data for EPR-019 and PX-136 measures. Any need to change routing, the quality floor, gate calibration thresholds, outcome-statistics estimators or reviewer isolation stops the row and needs its own Decision Record (EPR-pinned behaviour). The pinned surface (291 REQ-EV, 265 IMP-EV, 291 QUAL-EV, EPR-000..019, ADR-R-039..056, gates A to G) is untouched.

## Alternatives rejected

- **One large "audit parity" row per tier.** Rejected: a row that closes on a superficial edit is the failure AGENTS.md forbids; each row here has one boundary and one proof.
- **Folding the residual parts into the existing rows by editing them.** Rejected: rows of doc 62 are locked; a new row names the residual and keeps the old text intact.
- **Reviewer-family enforcement, a correlated feasibility bound and a no-checks cascade and critique policy** (BLD-28's other items). Not adopted here: each changes router semantics that ADR-R-049..056 and EPR-000..019 pin, and the owner did not ask for a change of those. They are recorded in doc 79 section 9 for an EPR Decision Record.
- **Adopting the Workspace Editor, the marketplace or the shell platform record to carry rows.** Rejected: not requested; PX-106 is a Customize view limited to existing skills, hooks and extensions and PX-117 does not depend on PX-092.
- **Putting benchmark rows in the iteration tier by default.** Rejected for BLD-30: cost accounting is evidence semantics.

## Supersessions

No sealed row, decision or pinned count is superseded or changed. Two clarifications are recorded so nothing is silent: the audit's Verify mode (a goal-template mode in BLD-03) is not adopted as a PX-051 mode (DEBUG already makes reproduction mandatory); and BLD-25's stated dependencies on BLD-15 and BLD-23 are a sequencing preference, because PX-082 to PX-086 are accepted and unchanged.

## Migration

Additive. New rows, tasks, qualifications and scenarios attach to M10 and RELEASE_ZERO. The graph gains a change record, a dossier task (DOC-PX-013) and the PX nodes. `tools/build_graph.py` gains one entry in its parity-push table and `tools/test_dossier.py` pins the counts of the parity surface (PX rows 000..139, 137 ADOPT, 3 DEFERRED, seven records of which four accepted, 41 rows here, tier split 39 and 2). `tools/dossier_px.py` pins nothing and is unchanged.

## Compatibility

- **Release effect (ratified with the record).** The 41 rows are ADOPT rows with release RELEASE_ZERO and milestone M10: ALPHA and BETA stay READY; RELEASE_ZERO grows by 41 included work items and M10 grows. This is forced by doc 75 (a task joins the release named in its row; DEFERRED rows can carry no task).
- No row depends on a row or dossier task of DR-PX-2026-10-03-009, -011 or -012.
- Graph schema 1.2 is unchanged.

## Security impact

Positive overall: the rows close the confirmed defects the audit found open after the fixes (execd flow control, browser target policy, skill trust, instruction trust, credential fragmentation, vacuous snapshots of capability) and add negative proofs. New surface: pre-turn retrieval, model-written summaries, hook context, page-declared tools, webhooks for checks and comments, OIDC and policy-bundle publishing, OTLP export; each has a hostile-input or fault proof in its qualification.

## Test impact

41 qualifications and 41 scenarios in doc 62. Each protocol or persistence row proves a kill and restart of the real Core or process; browser rows run real Chromium in the packaged host; GitHub rows need real GitHub; model rows use the live compatible gateway where behaviour is visible; benchmark rows report intervals. Tooling: `tools/test_dossier.py` pins the parity counts and asserts the dependency separation and the status and approval consistency of records; `tools/graph.py` refuses to complete a dossier task whose record is proposed.

## Rollback

Revert the commits that add the rows, the specification and the graph nodes and rerun the reseal. Because doc 62 and this directory are locked paths, reversal needs a new Decision Record that names this one in `supersedes`.

## Explicit user approval

Accepted on 2026-10-05 by the owner's instruction recorded in `approved_by`: the goal to implement research/audit/01-TASK-LIST.md is the request that these build tasks be specified and built, and this record is the specification step the dossier requires before any of them can start. The inputs only the owner can supply (a dependency admission for the embedding model, a GitHub token and test repository, first-party provider keys, a threshold profile for gates A to G) are named in the rows that need them and are not assumed. DR-PX-2026-10-03-009, -011 and -012 are not part of this instruction and stay proposed.
