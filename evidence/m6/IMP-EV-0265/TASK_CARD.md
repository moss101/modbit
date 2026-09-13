# Task Card — IMP-EV-0265 Q&A → Plan → visual review

## Identity

- Task ID: IMP-EV-0265
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0265; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0265 — Low-risk task skips ceremony; high-risk configured task requires plan/review.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; the real rust-cli / python fixtures for the risk rules)

## Goal

Risk/ambiguity may trigger clarification, plan and visual evidence review without mandatory ceremony.

## Existing-code audit

- classification: PRESENT since PX-014 / EPR-008 / EPR-017 (docs/28 clarification policy, docs/27 §risk and acceptance): a typed question is asked only when the policy's ambiguity rule fires; the realized risk of the change decides whether independent review and a human decision are obligations; a low-risk change reaches Ready for Review with the gate's ACCEPT and nothing more. No code change in this task: the audit names the production path and the qualifications that already run on every CI push.
- production entry points: `services/modbit-core/src/runtime.rs` (`user.ask` under the clarification policy; the plan contract before a write), `services/modbit-core/src/assurance.rs` and `crates/policy/src/assurance.rs` (realized risk → `independent_review_required`, `human_required`), `services/modbit-core/src/gate.rs` (acceptance at the revision; `COMPLETION_REFUSED` / `ASSURANCE_HUMAN_REQUIRED` under the unattended profile).
- proof: a low-risk edit completes with the gate's ACCEPT, no human and no independent review required (EPR-017); a change touching an auth module and a migration derives CRITICAL risk with independent review and a human decision required despite a passing check, and under the unattended profile stops safely instead of synthesizing a decision (EPR-008); an ambiguous change asks one typed question, suspends, and resumes from the answer, while a clear one asks nothing (PX-014).

## Limitations

- The "visual evidence review" is the review surface's per-hunk decision (M2.9); screenshots as evidence arrive with the browser runtime.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_017_acceptance_is_evidence_at_the_revision_and_never_erases_a_human_obligation`
- `qual_epr_008_factual_risk_stays_strict_despite_passing_tests_and_stops_safely_unattended`
- `qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
