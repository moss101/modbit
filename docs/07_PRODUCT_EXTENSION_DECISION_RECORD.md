# Product extension decision record

**Decision Record:** DR-PX-2026-09-05  
**Status:** APPROVED for dossier adoption; implementation remains evidence-gated and NOT_STARTED.  
**Approval:** On 2026-09-05 the user reviewed the assessment of the specified application against the market, approved the proposed extension plan, and returned twelve numbered decisions with modifications. Those decisions are reproduced below verbatim in substance and are binding on stages A–E of the extension.

## Trigger, evidence and current behavior

The dossier specified a trustworthy delegation-and-supervision system but left the inner loop, time to value, source-control workflow, agent competence, UX depth, language breadth, collaboration and governance pace unaddressed or implicit. Every addition below is absorbed by an existing canonical owner; DR-PX creates no subsystem. The 291-row requirement ledger in `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` and its tasks and qualifications stay byte-immutable; the EPR ledger stays pinned as approved by DR-EPR.

## Approved decisions

| # | Decision | Ruling |
|---|---|---|
| 1 | DR-PX and additive ledger | APPROVED. Base ledger immutable. New `REQ-PX / PX / QUAL-PX` ledger in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`. No rewrite, no renumbering. Effective totals are generated mechanically from base plus approved extension ledgers, never duplicated as hard-coded constants. |
| 2 | Phased releases | APPROVED. Alpha, Beta and Release Zero exist as projections in `75_PHASED_RELEASE_PLAN_AND_READINESS.md`. A release node is not a manually mutable lifecycle; readiness derives from canonical task status, qualification results and blockers. |
| 3 | Governance tiering | APPROVED WITH MODIFICATION. Tiers follow behavioral risk, not file or component location. Iteration-tier evidence is permitted only when a change does not modify effect-bearing behavior, canonical persistence, permissions or policy, execution, recovery, protocol or schema, a security boundary, or evidence semantics. Any mixed or uncertain change uses the release-critical gate. "No mock closes a production behavior" remains invariant. |
| 4 | Client surfaces | MODIFIED. The headless CLI is early Alpha work. External development-environment adapters over the canonical SurfaceProtocol are thin clients only: they may submit tasks, steer, provide provenance-bound diagnostics, and display or review results; they never own orchestration, context, memory, Git state, recovery, policy or tool execution. The first IDE adapter is Beta, not an Alpha blocker; VS Code first after the surface protocol is proven, JetBrains only after the abstraction passes conformance tests. Inline patching only through the canonical ChangeTransaction and Workspace File Service path. |
| 5 | Source control | APPROVED. GitHub first: issue-to-task, PR create and update, review-comment steering, CI-result ingestion. None blocks local Alpha. CI results are provenance-bearing verification evidence, never automatic qualification PASS. |
| 6 | Agent competence | APPROVED and important. Contracts for planning, retrieval, change strategy, verification, bounded evidence-driven repair and self-review. Every repair attempt records failure signature, hypothesis, evidence, intended fix and verification result; equivalent repeated hypotheses trigger escalation. Benchmark targets only after a fixed M2 baseline; maintain public and internal competence regression suites. |
| 7 | UX depth | APPROVED. Full flows, empty, error and recovery states, keyboard model, notification behavior and interaction budgets. "First useful task within five minutes" is a measurable acceptance criterion and an E2E scenario. |
| 8 | Languages | MODIFIED. Alpha Tier A: TypeScript/JavaScript, Python, Rust. Tiers: A full engineering intelligence, B validated structural support, C text-safe support, Unsupported. A language enters a tier only after that tier's conformance suite passes; no language is classified by default. |
| 9 | Platforms | APPROVED WITH PRECISION. macOS is the Alpha release platform. Windows and Linux core and runtime compatibility begin in CI from M0, but CI compatibility is never represented as release-grade support; desktop E2E and release support are promoted separately. |
| 10 | Team collaboration | DEFERRED. Specified now and placed in the graph as DEFERRED requirements; no implementation enters the Release Zero critical path. |
| 11 | Graph and readiness | Release readiness operates from task-level membership and dependencies, not milestone completion alone. The canonical lifecycle is preserved; no fourth status ladder. |
| 12 | Canonical subsystems | None added. Additions are absorbed by existing owners unless this record demonstrates that an existing boundary cannot own the behavior. It does not. |

## Change-control fields

| Field | Content |
|---|---|
| Proposed replacement | Stages A–E: A authority, ledger tooling, phased releases, governance tiering; B client surfaces and source control (doc 29); C agent competence and benchmarks (docs 28, 63); D UX flows (doc 39); E language and platform matrix (doc 76). Each stage is one dossier task logged in `97_DOSSIER_MAINTENANCE_LOG.md`, sealed on `main` with a change commit and a seal commit, with the full integrity gate after every stage. |
| Migration | Additive only. New requirement rows, tasks, qualifications and scenarios attach to existing milestones and owners. Existing statuses are untouched. |
| Compatibility | Graph schema stays 1.2 with one derived node type (`release`) and two edge types (`includes`, `requires_gate`). The EPR pins in `tools/dossier_epr.py` are unchanged; the PX parser pins nothing and computes totals. |
| Security impact | None on the product. Thin-client and source-control rules narrow, never widen, authority; CI evidence cannot self-certify. |
| Test impact | Copied-package tests for ledger structure, release derivation and release membership; further tests per stage. |
| Rollback | Revert the stage commits on `main` or restore from each stage's `evidence/dossier-px-00n/baseline.json`; requires another Decision Record because governing text changes. |
| Explicit user approval | Given in chat on 2026-09-05 with the twelve decisions above. |

## Numbering note

Section 40–49 is full, so the PX ledger lives at 62 in the verification range; docs 07, 28, 29, 39, 63, 75 and 76 take the remaining free numbers in their sections. Numbers are never reused.
