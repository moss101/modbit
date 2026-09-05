# Phased release plan and readiness

Authorized by DR-PX-2026-09-05. Releases are **projections** over the work items and gates already in the graph. They carry no status of their own: `tools/build_graph.py` expands the rules below into `includes` and `requires_gate` edges, and `tools/graph.py` derives readiness on every read. Nobody can set a release READY.

## Releases

**ALPHA: local coding loop and recovery spine.** macOS, `local_trusted` execution, one agent, the desktop app and the headless CLI as interchangeable thin clients, real Git worktrees, real tests, trusted code review, approvals and receipts, exact crash recovery. Routing is the direct baseline only; no browser, cloud, procedural runtime, skills or media. Languages: TypeScript/JavaScript, Python and Rust at the Alpha baseline of `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`. Proof: E2E-001..008 as applicable to the included tasks, plus the PX-E2E scenarios of Alpha rows.

**BETA: intelligence, fleet and browser.** Adds context intelligence, procedural runtime and skills, subagents and fleet supervision, the live browser, execution-policy phases 0–4 and the first IDE adapter. Windows and Linux core and runtime compatibility are exercised in CI but are not release-grade platform support.

**RELEASE_ZERO: the full proof.** Everything in the graph plus release gates A–G attested, exactly as `60_RELEASE_ZERO_EXPANDED_PROOF.md` and the EPR gates define. Platform promotion for Windows and Linux desktop is decided separately by their own E2E evidence, not implied by Release Zero.

## Membership rules

Milestone membership admits every work item scheduled in the listed milestones; exclusions remove specific tasks, task-id prefixes or canonical owners; a PX task joins the release named in its ledger row and every later release. `ALL` means every product work item. Releases nest: ALPHA is a subset of BETA, BETA of RELEASE_ZERO, and `tools/check_dossier.py` G8 enforces that every product work item is in RELEASE_ZERO.

| Release | Title | Milestones | Excluded tasks | Excluded prefixes | Excluded owners | Required gates |
|---|---|---|---|---|---|---|
| ALPHA | Local coding loop and recovery spine | M0, M1, M2, M4 | M2.10 | EPR- | — | — |
| BETA | Intelligence, fleet and browser | M0, M1, M2, M3, M4, M5, M6, M7 | — | — | — | — |
| RELEASE_ZERO | Full end-to-end proof | ALL | — | — | — | EPR-GATE-A, EPR-GATE-B, EPR-GATE-C, EPR-GATE-D, EPR-GATE-E, EPR-GATE-F, EPR-GATE-G |

Alpha excludes the media pipeline task and the execution-policy tasks because neither is needed for a verified local coding loop; both return in Beta. Owner exclusions are available for later refinement but unused today.

## Readiness derivation

For each release: `BLOCKED` if any included work item is `BLOCKED`; `READY` when every included work item is `COMPLETE` and every required gate is `SATISFIED`; otherwise `NOT_READY`. Because a work item becomes `COMPLETE` only through the one-step lifecycle with qualification evidence, and a gate becomes `SATISFIED` only after its tasks are complete and it is attested, readiness is a function of canonical task status, qualification results and blockers, computed at task level rather than from milestone roll-ups.

```bash
python3 tools/graph.py releases                 # readiness table and startable items per release
python3 tools/graph.py ready --release ALPHA    # what can be started now inside Alpha
```

`98_BUILD_MANIFEST.md` does not carry release status; the graph view and these commands are the only places it appears, always derived.

## Relationship to go/no-go checkpoints

The checkpoints in `72_RISK_REGISTER_AND_OPEN_DECISIONS.md` map onto these projections: the post-M2 checkpoint is the Alpha readiness review; the post-M3/M5 and post-M7 checkpoints belong to Beta; the cloud rollout checkpoint belongs to Release Zero. A release that is `READY` still ships only after the applicable Release Zero or platform proof in docs 60 and 76.

## Governance tiers apply per change, not per release

Every change inside any release declares its evidence tier by behavioral risk (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`). Alpha does not lower the gate for the Core spine, and Release Zero does not raise it for a copy change.
