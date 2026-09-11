# Task Card — EPR-008 Derive factual policy-owned assurance requirements

## Identity

- Task ID: EPR-008
- Milestone: M4 Durable recovery spine (P0); phase 2-4; prerequisites EPR-005, M4.6
- Requirement: REQ-EPR-008 (related REQ-EV-0014, REQ-EV-0029); owner: effects-security; subsystem: `crates/policy` (assurance), Core wiring in `services/modbit-core`
- Qualification: `QUAL-EPR-008` / EPR-E2E-008 — change real protected/auth/migration fixtures through production paths; assert factual required assurance remains strict despite passing tests or high model confidence, and missing new continuation produces safe stop rather than synthesized branch. EPR-FI-008 — mutate the risk/path control and ensure tests fail; forge low learned risk or reviewer confidence, change policy during leg and deny unknown-effect retries; no weakened minimum.
- Evidence tier: real-system (the real Core running a real coding task on a repository with an auth module, a migration and a passing check, under both the attended and the unattended profile) plus a pinned fault corpus over the production rule set.

## Goal

PolicyEnvelope defines legal models/effects, protected surfaces, minimum assurance, mandatory review/human and secret/network/deploy rules; RealizedRisk is derived from factual candidate changes and only strengthens policy minima; no learned risk score is required and passing tests cannot lower assurance.

## Existing-code audit

- classification: PARTIAL before this task. The admin `PolicyEnvelope` (docs/23 capability kernel) defined effect ceilings, denied capabilities, approval effects and unattended profiles, and nothing about assurance; protected paths for DI-9 were a literal in `runtime.rs`; the routing input's `risk_version` was `"none"`; no risk was derived or persisted anywhere. First missing link: the envelope had no assurance section.
- production entry points:
  - `crates/policy/src/assurance.rs` — `AssurancePolicy` (default surfaces: auth/authorization/secret/deploy CRITICAL + human; CI/CD, infrastructure, migrations, `.modbit/` HIGH + review; dependency manifests and lockfiles MEDIUM; blast-radius thresholds; review at HIGH, human at CRITICAL; forbidden effects), `AssuranceLayer` with `strengthen_with` (add surfaces, raise minima, lower thresholds; every weakening ignored and named), `derive_realized_risk` (protected surfaces, blast radius, unexpected scope against the plan's write set, sparse coverage, external/destructive and forbidden effects; the `Advisory` — learned risk, confidence, passing checks — is recorded as `advisory_ignored` and never read), `strengthen_only` for post-draft facts; `PolicyEnvelope.assurance` is the single envelope's assurance section (capability `assurance.realized_risk`).
  - `services/modbit-core/src/assurance.rs` — the organization layer from `MODBIT_POLICY_FILE` (a malformed file refuses startup), the repository layer `.modbit/policy.json`, the candidate facts from the COMPLETION run's changed files (line deltas via `similar`), the plan's write set and the task's approvals; `RealizedRiskView`; `GetTaskAssurance`.
  - `services/modbit-core/src/runtime.rs` — at every COMPLETION run the risk is derived from the same changed files the diff invariants see, joined with the earlier derivation (later facts only strengthen), persisted as `RealizedRiskDerived` on the run beside the verification report, written into `harness_state.realized_risk` and the observation; DI-9 protected paths now come from the policy's `question_required` surfaces; a forbidden effect requested blocks completion; a candidate that needs a human decision is refused completion (`ASSURANCE_HUMAN_REQUIRED`) from a profile in `no_approval_profiles` — the safe stop, nothing synthesized in the human's place. `services/modbit-core/src/routing.rs` compiles plans under `risk_version = risk-rules-1/<policy version>`.
  - CLI `task assurance --task <id>`.
- proof: a task edits `src/auth/login.py` and creates `db/migrations/002_add.sql` with a repository `.modbit/policy.json` that asks for `FAST`; the check passes and the task completes to the user's review; `GetTaskAssurance` reports CRITICAL / HIGH_ASSURANCE, independent review and human required, reasons `PROTECTED_SURFACE:AUTH` and `PROTECTED_SURFACE:MIGRATION`, no unexpected scope, the rules version, the policy version equal to the one the task is judged under, and the weakening layer named in `policy_notes`; on the log `RealizedRiskDerived` sits beside a `PASSED` verification run. The same change under `local_autonomous` never reaches `ReadyForReview`: the completion is refused with `ASSURANCE_HUMAN_REQUIRED` naming the level and the profile, the model sees the risk in its observation and its harness state, the run suspends, and no approval was invented. The fault corpus (23 cases over every rule, Windows paths included) is pinned; forged advisories (risk 0 / confidence 1 / checks passed, and their opposites) change no diagnosis and not the facts digest; a weakening layer leaves the policy identical; a stronger layer moves every case up or keeps it.

## Limitations

- The independent reviewer that could discharge `independent_review_required` without a human (EPR-006/EPR-018, M5) does not exist yet: today the user's review discharges both obligations, and the Acceptance Gate that consumes this risk is EPR-017.
- Changes under `.modbit/` are excluded from the COMPLETION run's changed files by the existing residue filter, so the `POLICY` surface fires only through the DI-9 question gate, not through the derived risk.
- Line deltas are counted from the text diff of each changed file; binary files count as one changed path with no lines.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_008_factual_risk_stays_strict_despite_passing_tests_and_stops_safely_unattended` (services/modbit-core, real Core)
- `qual_epr_008_fault_corpus_pins_every_rule_and_no_signal_lowers_assurance`, `epr_fi_008_a_stronger_layer_moves_every_case_up_or_keeps_it` (crates/policy)
- Unit: `surfaces_match_prefixes_segments_suffixes_and_basenames`, `a_plain_change_is_low_and_passing_tests_or_confidence_change_nothing`, `a_critical_surface_needs_a_human_whatever_else_is_true`, `layers_only_strengthen`, `later_facts_strengthen_and_never_weaken` (crates/policy)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
