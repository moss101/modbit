---
id: DR-PX-2026-10-19-014
title: Per-task harness refinement (Propose–Probe–Compose) and a lesson library enter the product ledger as REQ-PX-140, an extension of Skill Evolution, default off
status: accepted
date: 2026-10-19
supersedes: none
approved_by: owner instruction in chat 2026-10-19 (option A, DEFERRED / POST_ZERO, combined with option C, a measurement-only lab experiment)
---

# DR-PX-2026-10-19-014 — Per-task harness refinement and a lesson library

> **Decision (2026-10-19):** the owner chose **option A combined with option C**. REQ-PX-140 enters doc 62 as a DEFERRED, POST_ZERO row with a qualification and no task, card or scenario (the ledger forbids them on DEFERRED rows, `tools/dossier_px.py`). The task card and PX-E2E text in the appendix are the **activation text**: they are not in doc 62 and become live only through a later record that changes the disposition to ADOPT. Option C is accepted as a lab-only experiment; it is not a ledger item, nothing of it is built, and it needs its own audit and task card under the M5.7 lab tooling before work starts. Open points 1 to 3 below stay open and are inputs to that activation record.

## Problem and goal

Modbit improves its harness only **across** tasks and **offline**: doc 26 archives traces, proposes an atomic skill patch, qualifies it on a benchmark and promotes it through a signed registry. Nothing refines the harness **for the task in hand** from how different attempts at that same task behaved. Google Research's Harness² (Xu et al., arXiv 2610.14224, Apache-2.0, `google-research/harness2`) reports that such a step, done without ground-truth verifiers, improves weaker models over their base harness. The goal is to evaluate that mechanism inside Modbit's existing owners, behind the existing qualification gates, with no second scheduler, policy engine, memory system or skill runtime (AGENTS.md, doc 81).

## Trigger and evidence

- **Owner request of 2026-10-19** ("go ahead" on drafting this record and a PX card).
- **Harness² paper and source**, read at `main` of `google-research/harness2` (pushed 2026-10-14): `core/recursion.py` (parallel and sequential modes, stage retry then fall back to the current harness), `core/proposer.py` (isolated improver sessions), `core/selection.py` (anonymised, seed-shuffled choice among k finals, then an experience note), `core/ledger.py` (budget identity R = 4k rollouts, P = 3k improvement sessions, asserted), `core/library.py` (per-domain library; an entry is the whole adopted tree plus a `cue / lesson / evidence` triple), `core/context.py` (refuses a context tree that contains grader output).
- **What the evidence does not show.** Reported gains are on JobBench, LAB (legal) and WorkBuddy-Bench (office, web, code subset) for GLM-5 and Gemini 3.5 Flash. I found no result on a repository-scale software-engineering benchmark and none for a frontier executor. The gain on Modbit's own suite (doc 63) is therefore a hypothesis; this record makes measuring it the gate.

## Current behavior

Doc 26 (as built M5.7, EXPERIMENT) runs the Skill Evolution Lab in shadow behind the Skill Registry and the Eval Harness; a candidate cannot self-promote; the production agent never sees the evolution wiki. Doc 28 gives a bounded, evidence-driven repair loop inside one attempt (PX-018). Doc 14 admits subagents transactionally into forked worktrees with a narrowed lease and an effect ceiling of `REVERSIBLE_WRITE` (M6.3). Doc 27 / EPR-010 hold the single cost record. No row compares several harness variants on one task.

## Proposed replacement

1. **One new row, REQ-PX-140**, "Per-task harness refinement and lesson library", owner `skills` (doc 26), appended to doc 62 with a task card, a qualification and a PX-E2E scenario (drafted below).
2. **Mechanism (Propose–Probe–Compose, adapted).** For a task the profiler marks uncertain or hard, and only when policy enables it: (a) *base* — run the current harness k times; (b) *propose* — an improver session reads the base executions and edits a **task-scoped overlay**; (c) *probe* — each candidate overlay runs the same task as an admitted child in its own forked worktree; (d) *compose* — improver sessions read all probe evidence and emit k composed overlays, each run once more; (e) *select* — an Isolated Non-Committing Reviewer (ADR-R-053) chooses among the anonymised finals using verification results bound to candidate revision and environment digest (doc 64). The chosen change set is an ordinary candidate revision and goes through SelfReview, the COMPLETION run and the Acceptance Gate unchanged; the overlay is discarded with the task unless promoted by doc 26.
3. **Editable surface is a closed list**: task-scoped skill overlays, tool-usage guidance text, task instruction overlay, and procedural scripts run by the existing procedural runtime. **Never editable:** guardrails, Policy Envelope, capability ceilings, approval rules, verification contracts, secret-broker and path policy, reviewer isolation, EPR routing and gate thresholds. An overlay that touches anything else is rejected as a whole and the current harness is kept (fail closed).
4. **Budget identity.** Per task: R = 4k rollouts and P = 3k improver sessions, recorded in the existing accounting record and asserted; the cost lands in the EPR-010 cost record, not a second one. A hard per-task budget and a global kill switch apply; exceeding either stops further probes and keeps the base harness.
5. **Lesson library.** After selection a reflector writes a `cue / lesson / evidence` triple, stored as `EvolutionPattern` rows under doc 26 keyed by task class. The library is read **only by improver sessions**, never injected into the production agent (doc 26's rule stands). Promotion of any lesson into a skill goes through the existing qualification and signing.
6. **Leak guard.** In benchmark mode the evidence assembled for improver sessions is refused if it contains grader output, hidden-test names or contents, or rubric text, and network access to benchmark answers is blocked (Harness² documents a Codex run that fetched its own rubric unprompted).
7. **Default off.** Disabled in every profile; enabled per workspace by policy. First delivery is behind the M5.7 lab (shadow, EXPERIMENT); production activation requires the qualification below to pass on Modbit's own benchmark.

## Independent Modbit design

- **Clean-room, idea-level.** Harness² is Python, targets OpenCode and Codex executors and runs with an unrestricted network and a disabled Codex sandbox; none of that fits. No code is copied; the mechanism is re-specified on Modbit's Core, worktree fork, effect ledger and evidence model. The paper is cited here and in the card (the EPR record's missing citation is a precedent not to repeat).
- **Existing owners only.** Probes are admitted through `agent.spawn` admission (doc 14) with the same capsule, lease narrowing and write-scope checks; no new scheduler. Overlay storage, patterns and the improver role live under doc 26. Cost lives in EPR-010. Review is ADR-R-053. Nothing is added to the canonical owner list.
- **Probes are sandboxed rehearsals, not deliveries.** Probe and composed runs execute only inside forked worktrees at `REVERSIBLE_WRITE`; protected or external effects (push, network writes, credentials) are denied to probes. A task whose useful path needs a protected effect is not probed. Only the selected candidate can reach the Acceptance Gate, and it still needs the real approvals.

## Security model

All improver input is hostile data: repository content, tool output and prior-run text are data, never instruction. The improver session has read access to its evidence tree and write access only to the overlay directory; it holds no effect lease. Overlays are validated against the closed surface list before use, hashed, recorded and replayable. Library entries carry provenance (trace ids, run ids) and are read only by improvers; a poisoned lesson cannot reach the production agent without passing doc 26 qualification. The improver cannot widen its own authority because capability ceilings and guardrails are outside its surface. Probes cannot exfiltrate: no network effect, redacted secrets, bounded budgets.

## Canonical owner mapping (doc 81)

`skills` (doc 26) owns the overlay, the improver role, the library and promotion. `core-runtime` (doc 14) owns probe admission, unchanged. `eval-bench` (doc 63) owns the measurement. `observability`/EPR-010 owns cost. No subsystem is added or changes owner. The record changes no routing, quality floor, gate threshold, estimator or reviewer isolation (EPR-pinned behaviour needs its own record); the pinned surface (291 REQ-EV, 265 IMP-EV, 291 QUAL-EV, EPR-000..019, ADR-R-039..056, gates A to G) is untouched.

## Options for the owner

- **A (recommended): disposition DEFERRED, release POST_ZERO** — the row, qualification and scenario are recorded now (like REQ-PX-003, -012, -013), carry no task, and cost nothing before RELEASE_ZERO. Reason: the external evidence is for non-coding work and weaker models, RELEASE_ZERO is still open, and doc 75 would add the whole task to its critical path. Activation is a one-line change of disposition under a later record once Modbit's own benchmark shows a reason.
- **B: ADOPT, M10, RELEASE_ZERO** — the card below becomes live task PX-140; RELEASE_ZERO grows by one work item.
- **C: measurement only** — keep the row DEFERRED and run a lab experiment under M5.7 tooling that replays recorded doc-63 tasks through the mechanism and reports intervals; no production code. Cheapest way to learn whether any of this matters. Can be combined with A.

## Alternatives rejected

- **Port harness2 as a dependency or sidecar.** Rejected: a second orchestration graph and executor, Linux-only Python, unrestricted network, wrong boundary (doc 81).
- **Let refined overlays reach production without per-task verification.** Rejected: the selection signal is behavioural, not ground truth; the Acceptance Gate is the only completion authority.
- **Make the lesson library readable by the production agent.** Rejected: it recreates the prompt bloat and authority path doc 26 forbids.
- **Allow the improver to edit guardrails or policy.** Rejected: the improver would be optimising against the controls that bound it.

## Supersessions

None. No sealed row, decision or pinned count is superseded or changed.

## Migration

Additive. Done with the acceptance (option A): REQ-PX-140 and QUAL-PX-140 appended to doc 62 (no card, no scenario); the record added to the parity-push table in `tools/build_graph.py`, which also creates the change record and DOC-PX-014; `tools/test_dossier.py` pins PX rows 000..140 (141 rows, 137 ADOPT tasks, 4 DEFERRED); doc 02, the decisions ledger and doc 97 updated; full reseal run. No product code changes.

## Compatibility

Under A, no release effect. Under B, RELEASE_ZERO gains one included work item (doc 75). No dependency on any proposed record other than this one. Graph schema 1.2 is unchanged.

## Security impact

New surface: model-written overlays, an improver session with filesystem reach to its own evidence, a library of lessons. Each has a hostile-input proof in the qualification below; the default-off switch and the closed editable surface bound the blast radius.

## Test impact

One qualification and one scenario (below). Tooling changes only the pinned counts above.

## Rollback

Revert the commits that add the row and tool pins and rerun the reseal. Because doc 62 and this directory are locked paths, reversal needs a new record that names this one in `supersedes`.

---

# Appendix — drafted ledger text (not wired into doc 62 yet)

Ledger row (disposition and columns per option A; under B change to `M10 | RELEASE_ZERO | ADOPT` and list the prerequisites):

```text
| REQ-PX-140 | Per-task harness refinement (Propose–Probe–Compose) and lesson library, default off | — | QUAL-PX-140 | skills | — | POST_ZERO | DEFERRED | — |
```

## PX-140 — Per-task harness refinement and lesson library (task card, live only under option B)

- **Requirement:** REQ-PX-140; **related preserved requirements:** REQ-EV-0046, REQ-EV-0048 (subagent isolation and ceiling), REQ-EPR-010 (single cost record), ADR-R-053 (isolated reviewer).
- **Owner / milestone / release:** skills / M10 / RELEASE_ZERO; **prerequisites:** M5.7, M6.3, PX-018, IMP-EV-0032, DOC-PX-014.
- **Evidence tier:** release-critical (execution, persistence, a security boundary and evidence semantics are touched).
- **Source:** Harness² (arXiv 2610.14224; `google-research/harness2` at 2026-10-14), re-specified clean-room.
- **Classification before work (AGENTS.md):** NOT-FOUND for per-task refinement, overlay, improver role, library and budget identity; IMPLEMENTED-PARTIAL for the pieces it reuses (subagent admission, qualification harness, cost record). First missing link: an overlay type with a closed editable surface and a validator, and an improver role that can produce one from recorded executions.
- **Scope and acceptance:** Behind a default-off policy switch and only for tasks the profiler marks uncertain or hard, a refinement run executes the five steps of the record (base k, propose, probe, compose, select) with probes admitted through `agent.spawn` in forked worktrees at `REVERSIBLE_WRITE` and no protected or external effect. Overlays are limited to the closed surface list, hashed, recorded and replayable; any proposal outside it, or an invalid one after one retry, keeps the current harness. The per-task ledger shows R = 4k and P = 3k and is asserted; cost is written to the EPR-010 record; a per-task budget and a global kill switch stop further probes. Selection is by an Isolated Non-Committing Reviewer over anonymised finals using verification results bound to candidate revision and environment digest; the winner is an ordinary candidate that still passes SelfReview, the COMPLETION run and the Acceptance Gate. The reflector writes `cue / lesson / evidence` patterns under doc 26, readable by improver sessions only. In benchmark mode, evidence containing grader output, hidden tests or rubric text is refused. A restart mid-run resumes or cleanly abandons without a half-applied overlay.
- **Measurement gate:** enabling it outside the lab requires, on Modbit's doc-63 suite, a paired comparison with the base harness reporting intervals and a per-task cost ratio, with no safety regression; a result that is not better keeps the switch off and the row at WIRED.
- **Real qualification:** QUAL-PX-140 / PX-E2E-140.
- **Failure and negative proof:** as in QUAL-PX-140.

```text
| QUAL-PX-140 | REQ-PX-140 | skills | Real Core, real Git repositories, real forked worktrees and the live compatible gateway: on a fixture task whose base harness fails, a refinement run with k=1 executes base, two candidates and one composition, the ledger shows four rollouts and three improver sessions, and the selected candidate passes the real verification run and the Acceptance Gate; an overlay that edits a guardrail, a capability ceiling or a verification contract is rejected whole and the base harness is kept; a probe that attempts a push, a network write or a credential read is denied by the real kernel and the denial is recorded; a planted hidden-test file and a planted rubric in the evidence tree make the run refuse before any improver session; killing the real Core mid-probe and restarting leaves no applied overlay and no orphan worktree, and the run resumes or is abandoned with a recorded reason; exceeding the per-task budget stops further probes and keeps the base harness; with the switch off nothing runs and nothing is spent; the recorded cost equals the EPR-010 record; a library lesson reaches no production prompt, only improver sessions. | An overlay outside the closed surface being applied, a probe effect that is not denied, benchmark-leak content reaching an improver, a ledger that breaks R = 4k or P = 3k, cost missing from the single record, a library entry in a production prompt or a half-applied overlay after a kill fails the suite; a mutation that widens the editable surface to include guardrails must fail the rejection test. |
```

```text
### PX-E2E-140 — Per-task harness refinement and lesson library

**Setup:** a fixture repository and task the base harness fails, the real Core, forked worktrees, the live gateway, a planted hidden-test file, a planted rubric, a planted guardrail-edit proposal.
**Action:** run refinement at k=1, kill the Core during a probe and restart, rerun with the planted leak, compare recorded cost to the EPR-010 record.
**Pass:** the selected candidate passes the Acceptance Gate, the ledger shows R=4 and P=3, forbidden edits and leaks are refused, probes cannot reach protected effects, a kill leaves nothing half-applied, and cost matches the single record.
```

## Open points for the owner

1. Option A, B or C (or A with C)?
2. Is the Isolated Non-Committing Reviewer an acceptable selector, or should selection use verification results only, with no model judgement?
3. Per-task budget ceiling and the profiler threshold for "uncertain or hard" are unset; they need numbers from doc 63 baselines before activation.
