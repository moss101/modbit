# Task Card — M5.7 Skill Evolution Lab as shadow/EXPERIMENT behind Skill Registry + Eval Harness

## Identity

- Task ID: M5.7
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/26 (canonical architecture, storage contracts, promotion transaction, failure modes), docs/57 WSK-E2E-001..010, REQ-EV-0196..0204, 0237, 0247, 0248 (all EXPERIMENT), REQ-EV-0208 (no second memory/recovery system)
- Qualification: docs/43 M5.7 — WSK-E2E-001..010 pass; candidate cannot self-promote; production recovery is independent of lab data
- Evidence tier: production-equivalent for the lab (the production lab code over real directories, real signatures, fixture trials) and real-system for WSK-E2E-005/010 (the real Core, a promoted skill, a hard kill with the lab deleted)

## Goal

One Modbit skill subsystem with an offline lab: sealed traces, evolution knowledge and candidate skills as distinct stores; a proposer that cannot promote; a promotion transaction only a PROMOTE qualification under a signing key can drive; the production agent seeing the approved projection only; recovery owing nothing to the lab.

## Existing-code audit

- classification: NOT-FOUND before this task (the registry, selector, compiler and signing of M5.5 existed; no trace archive, knowledge store, maintainer, proposer, qualification harness or promotion transaction).
- production entry points: `crates/skills/src/evolution.rs` (`Lab`, `EvolutionTrace`/`SealedTrace`/`TraceCorrection`, `Maintainer`, `KnowledgeStore` with `index.json` and budgeted `hydrate`, `Proposer` + `ProposerModel`/`TemplateProposer`, `SkillCandidate`/`AtomicPatch`, `Qualifier` + `SkillQualification`, `Promotion` (promote, rollback, archive_head, impact)); `benchmarks/skill-evolution/src/lib.rs` (`arms_report`, `transfer_report`, `ablation`); the registry head under `<profile>/skills/<name>` is what the Core discovers (M5.5), the lab under `<profile>/skill-lab` is never a discovery root.
- proof: WSK-E2E-001 sealing/immutability/corrections; 002 contradictory traces consolidated side by side, superseded not overwritten, nothing outside the lab written; 003/006 atomic candidate with base hash, PURPOSE and evidence, refusals of unrelated files, widened tools or ceiling (candidate or manifest), prohibited executable behaviour, stale base; 004 a regressing candidate REJECTED with the head byte-identical and the candidate, evidence and knowledge queryable, a cheaper-but-worse candidate REJECTED, a safety failure REJECTED, a passing candidate promoted as a signed, evaluated, addressable 1.0.1 the registry enables, the impact trail with both decisions, rollback to 1.0.0 byte for byte with 1.0.1 still addressable and the wiki untouched, promotion refused under another candidate's qualification; 007 2,000 traces → 120 patterns, a compact index a quarter of the full records, hydration under a 1,200-token budget with refusals by name and provenance naming exactly the hydrated sources; 008 paired arms with intervals, a REJECT that a larger aggregate saving cannot override; 009 per-model lines, a regressing family blocking the neutral label and promotion, one family promoting model-specific; the 0207 ablation with its finding; on the real Core, WSK-005 (the promoted 1.0.1 in every request, no pattern id, trace id, claim or marker) and WSK-010 (hard kill mid-task, lab deleted, resume to ReadyForReview from the durable stores).

## Finding

The lab works as specified and refuses what docs/26 says it must refuse; the ablation on the deterministic fixture finds no meaningful lift of the evolved candidate over a manual refinement, so the mechanism is not proposed for an ADR and stays EXPERIMENT (docs/26 "Acceptance gates").

## Limitations

- EXPERIMENT: the proposer is the deterministic template stand-in (a model-backed proposer implements the same trait; not wired to a live provider, DR-M3-002); fixture trials give intervals of no width; the dedicated `skill-evolution-fixtures` repository and nightly cross-model runs of docs/57 are not set up; the ablation on the fixture finds no meaningful lift, so the mechanism stays EXPERIMENT with no ADR proposed.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `wsk_e2e_001_a_sealed_trace_is_immutable_and_corrections_reference_it`
- `wsk_e2e_002_the_maintainer_consolidates_contradictory_traces_without_overwriting`
- `wsk_e2e_003_006_a_candidate_is_atomic_and_cannot_widen_authority`
- `wsk_e2e_004_promotion_needs_the_gates_and_rollback_restores_the_head`
- `wsk_e2e_007_selective_hydration_respects_the_budget_and_names_its_sources`
- `wsk_e2e_008_the_paired_benchmark_reports_every_arm_with_intervals_and_never_promotes_over_a_failed_gate`
- `wsk_e2e_009_transfer_is_reported_per_model_and_a_regressing_family_blocks_the_neutral_label`
- `req_ev_0207_the_ablation_says_whether_the_lab_earns_its_complexity`
- `wsk_e2e_005_010_a_promoted_skill_reaches_the_model_without_the_wiki_and_recovery_needs_no_lab`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
