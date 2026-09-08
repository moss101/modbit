# Task Card — M0.4 Requirement-coverage CI and REQ→IMP→QUAL traceability parser

## Identity

- Task ID: M0.4
- Milestone: M0 — Repository and authority
- Canonical owner: Architecture governance (`docs/45`, `docs/74`, `docs/81`); tooling in `tools/check_dossier.py` and `tools/architecture-lint`
- Requirement IDs: roadmap task `docs/43` M0.4; docs/45 "CI rule to implement"; docs/74 "Product CI coverage checks to implement"
- Risk class: medium (governance gate; no runtime behavior)
- Evidence tier: release-critical (evidence semantics)
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

CI fails on: an ADOPT/ADAPT row without owner, `IMP-EV-*` or `QUAL-EV-*`; a COMPLETE task without qualifying evidence; a duplicate active owner of a canonical single-owner system; a code module registering a production capability with no requirement id.

## Non-goals

- Detecting fake adapters or skipped protected-effect tests (later verification tasks).
- Changing any requirement row, owner or disposition.

## Required reading

`AGENTS.md`, `docs/45`, `docs/46`, `docs/74`, `docs/81`, `docs/87`, `docs/93`, `tools/check_dossier.py`, `tools/build_graph.py` (OWNER_MAP).

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL
- production entry point: `tools/check_dossier.py` (CI job `dossier integrity`) already enforced D4 (ADOPT/ADAPT/EXPERIMENT rows name existing IMP/QUAL) and G3 (COMPLETE requires evidence in the docs/93 grammar); `tools/build_graph.py` exits on an owner label outside OWNER_MAP
- real effector/storage boundary: docs ledgers, `graph/project-graph.json`, Cargo/package manifests
- tests found: `tools/test_dossier.py` (41 cases) and architecture-lint tests
- first missing/broken link: no explicit owner check at the checker; COMPLETE accepted with only an `artifact:` ref; no notion of code-module registration, so duplicate active owners and capability→requirement links were unverifiable
- duplicate/drift risks: a second registry of owners; avoided by reading subsystems and requirements from the project graph and doc 81 systems from `rules.toml`

## Invariants

- The project graph is the single source of subsystem and requirement ids.
- Each doc 81 canonical system has at most one claimant; a second claimant needs an accepted migration Decision Record.
- COMPLETE requires at least one `run:`/`test:` ref and one `commit:`/`revision:` ref, and `artifact:` refs must exist.

## Implementation slice

- domain/API: `[package.metadata.modbit]` in every Cargo.toml and `"modbit"` in every package.json: `owner`, `canonical`, `capabilities[{id, requirements}]`, optional `migration_adr`
- persistence/migrations: none
- policy/capabilities: `[[canonical]]` list (18 systems from docs/81) in `rules.toml`
- service/effector: `check_dossier.py` D10 and G9; `architecture-lint modules` (real `cargo metadata` + package.json + graph); CI steps in the `rust` matrix and the `dossier` job
- UI/model projection: none
- failure/recovery: exit 1 on any violation, 2 on operational error
- evidence/observability: violations name module, system, owner or requirement

## Verification

- unit/property: none new (parsers exercised by integration tests)
- integration: `tools/test_dossier.py` `test_adopt_row_without_owner_is_rejected`, `test_complete_without_qualifying_evidence_is_rejected` (copied package); `tools/architecture-lint/tests/modules.rs` (5 real-cargo tests: valid, missing/unknown owner/bad capability, duplicate owner with/without accepted or proposed ADR, TypeScript packages, real workspace)
- fault: duplicate canonical claim injected into `crates/git` rejected (`inject-failure-duplicate-canonical-owner.log`)
- E2E: hosted CI on three platforms running both lints

## Completion evidence

See `evidence.json`.
