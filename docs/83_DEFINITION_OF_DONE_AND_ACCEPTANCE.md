# Definition of Done and Acceptance Criteria

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Status ladder

### DECLARED
Requirement/ADR/task exists. No completion credit.

### IMPLEMENTED
Production code exists and compiles. No completion credit by itself.

### WIRED
Reachable from real runtime/UI/tool registry with real configuration. Still not complete.

### E2E_PROVEN
Required real-system scenario passes and evidence bundle exists.

### COMPLETE
E2E proven **plus** security/recovery/observability/performance/documentation acceptance for that feature; no placeholder path or critical unresolved defect.

## Universal completion checklist

A feature is COMPLETE only when all applicable boxes are true:

- [ ] authoritative owner/module defined;
- [ ] public/domain contract versioned;
- [ ] persistence/migration implemented if stateful;
- [ ] policy/capability effect class defined;
- [ ] failure semantics defined and exercised;
- [ ] cancellation/backpressure handled;
- [ ] observability metrics/log codes implemented;
- [ ] unit/component tests pass;
- [ ] real cross-process integration passes;
- [ ] required restart/resume test passes;
- [ ] security tests pass;
- [ ] performance budget passes or approved exception exists;
- [ ] real E2E path passes on packaged candidate;
- [ ] evidence bundle archived and linked from task ledger;
- [ ] user-facing error/recovery state exists;
- [ ] documentation/runbook updated;
- [ ] no `TODO`, `unimplemented!`, hard-coded success, disabled auth/policy or mock production provider in reachable production path.

## Specific acceptance

### Agent coding loop
Must change a real Git worktree, execute real tests, show diff/evidence, and survive renderer restart.

### Recovery
Must pass kill/restart at waiting-on-approval, model streaming, tool dispatch, terminal running, checkpointing and review states. Unknown outcomes explicitly reconciled.

### Context
Every injected fragment has provenance/revision/reason. Retrieval profiler produces comparable baseline metrics. Stale index state is observable.

### Procedural runtime
No ambient fs/network/process; all host calls appear as normal ToolCalls and pass policy/effect logging.

### Subagents
Admission is atomic; capacity and workspace isolation leak tests pass; parent gets structured evidence results.

### Browser
Agent and user operate the same session; structural control handles standard form/nav test without screenshot; visual fallback handles designated visual fixture; prompt injection suite passes.

### Sandbox
Actual MicroVM launched in staging; tenant-bound lease and network isolation verified; secret not discoverable in static guest environment; loss restore works.

### Effect Ledger
Hash chain verifies from genesis/head; protected effects without receipt are a release blocker.

## “No mockup” enforcement

`tools/evidence-check` scans test metadata: a completion claim must reference an allowed E2E scenario ID and candidate build digest. UI snapshots, mocked provider traces, fake sandbox classes and in-memory database tests cannot be attached as completion proof.


## V2 source completeness criterion

A release cannot call the dossier implementation-complete if `47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` fails, an ADOPT/ADAPT row has no real-test evidence, or a capability is implemented only as a source-specific duplicate. `82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` is normative.
