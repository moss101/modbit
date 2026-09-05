# Requirement Coverage Freeze Gate

This build dossier is frozen only if:

1. exactly 291 evidence-derived rows exist in `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md`;
2. every production row has canonical owner + `IMP-EV-*` task + `QUAL-EV-*` test;
3. every experiment is isolated behind an existing owner and has a measurable exit decision;
4. deferred/rejected rows remain explicit;
5. no external-product feature name is required to understand a build requirement;
6. task/test files use native Modbit contracts;
7. all architectural locks and supersessions are represented;
8. no placeholder/TBD is accepted as a production requirement.

Coverage freeze proves specification completeness, not product completion.

## Approved additive EPR coverage

DR-EPR-2026-09-05-v1.1 explicitly extends the freeze with exactly 20 REQ-EPR rows, 20 EPR tasks and 20 QUAL-EPR tests in docs 49/61. The original 291 evidence-derived rows, dispositions and owners remain locked and unchanged. The new rows and their owner/qualification assignments are locked under the same Decision Record rule. Integrity checks enforce both sets, 18 adopted ADRs and seven release gates. Specification adoption is not runtime qualification.
