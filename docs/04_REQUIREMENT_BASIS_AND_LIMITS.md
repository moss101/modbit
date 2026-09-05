# Requirement Basis and Limits

This build dossier is the native implementation transformation of the project's reconciled architecture and evidence work. Research provenance has been removed from normal build-agent context on purpose.

## What “complete coverage” means

The dossier carries forward every mechanism that survived the project's reconciliation gate. `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` contains 291 retained mechanism rows and an explicit disposition for each. Completeness means those requirements are not silently lost during implementation.

It does **not** mean implementing undocumented private behavior from systems we did not observe, inventing source-specific tool names, or reproducing another product's internal architecture.

## Authority separation

- Product/build authority: this dossier.
- Research provenance: prior research packages, for architects only when resolving evidence questions.
- Existing code: implementation evidence, never architecture authority.
- Old Code-OSS/IDE-oriented plans: historical only where superseded.

Agents must not import an external idea directly from memory or web research into production code. New architecture requires an ADR and reconciliation against canonical owners.

## Additive execution policy basis

DR-EPR-2026-09-05 and its scoped refinement DR-EPR-2026-09-05-v1.1 adopt the user-supplied execution-policy patches as an additional specification basis. The 291 EV rows stay unchanged; 20 REQ-EPR/EPR/QUAL-EPR rows in docs 49/61 extend coverage. The active architecture and implementation contracts are docs 27/38. This is approved behavior to develop, not evidence of measured cost savings, runtime correctness or product completion.
