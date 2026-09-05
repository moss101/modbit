# Requirement → Task → Test Traceability

## Trace chain

- Product requirement / architecture invariant → milestone task(s).
- Evidence-derived requirement `REQ-EV-nnnn` → `IMP-EV-nnnn` when production/experiment implementation is applicable.
- Every evidence-derived requirement → `QUAL-EV-nnnn`.
- High-risk capabilities additionally map to E2E/security/fault/performance test IDs.

## CI rule to implement

A parser should fail CI when:

- an ADOPT/ADAPT requirement lacks a canonical owner;
- its `IMP-EV-*` task is absent;
- its `QUAL-EV-*` test is absent;
- a task says COMPLETE but no qualifying evidence ref exists in the build manifest;
- a code module registers a production capability with no requirement ID;
- an architectural owner has two active production implementations without an approved migration ADR.

## Additive EPR trace chain

Preserve the frozen REQ-EV→IMP-EV→QUAL-EV set. DR-EPR-2026-09-05-v1.1 establishes REQ-EPR-000..019 → EPR-000..019 → QUAL-EPR-000..019, parsed from docs 49/61 into the same graph node/edge types and canonical owner IDs. Each task links its source document, related EV requirements, current owner, milestone, phase, explicit completion prerequisites, EPR-E2E and EPR-FI scenarios. Gate A–G nodes link required tasks/qualifications and release M10.3 waits for EPR-013 and EPR-019. CI rejects missing/duplicate mappings, owner drift, dangling/cyclic dependencies, evidence-free completion and manifest staleness. Adoption tasks DOC-EPR-001 and DOC-EPR-002 are documentation work, excluded from product roll-ups.
