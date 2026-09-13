# Task Card — EPR-007 Integrate isolated review and bounded revision

## Identity

- Task ID: EPR-007
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EPR-007; owner label: core-runtime; subsystem: providers compiler, core (`critique.rs`, `review_env.rs`), routing
- Qualification: QUAL-EPR-007 — the prevalidated reviewer slot activates only when the gate requires independent review, inside the EPR-018 environment, on evidence without solver reasoning; findings are validated at the current revision; revision is bounded; CRITIQUE is derived.
- Evidence tier: real-system (the real Core with a signed registry, a real review environment and sandbox, against a scripted OpenAI-compatible server playing solver and reviewer)

## Goal

Activate a prevalidated reviewer/reviser slot only after static evidence and independent assurance/acceptance assessment requires it. Consume the EPR-018 disposable environment; pass relevant code/evidence without solver hidden reasoning; validate current-revision findings and bounded revision. CRITIQUE is a derived path label, not a template.

## Existing-code audit

- classification: PARTIAL before: the plan schema had `Trigger::ReviewRequired`, `max_revisions` and reviewer roles, the gate consumed `independent_reviewer` evidence and the projection knew a reviewer leg — but no compiled plan carried a reviewer slot, nothing activated one, no reviewer ran and no revision was bounded.
- production entry points: `crates/providers/src/compiler.rs` (the reviewer slot in every compiled shape; `CompileInput.include_reviewer` in the digest; goldens regenerated for the digest change only), `services/modbit-core/src/critique.rs` (`at_acceptance`, `materialize_and_brief`, `handle_report`, `on_review_end`, `revise`, `projection` of `review.report`), `runtime.rs` (the acceptance hook before `RunCompleted`; review tasks complete on their report; reviewer-leg projection scope; justified flags do not reopen on rebuild), `routing.rs` (`path_label` CRITIQUE; reviewer activations count as legs), `crates/domain` (`ReviewLegActivated`, `ReviewerResultRecorded`, `RevisionActivated`), docs/17 `review.report`.
- proof: with a signed registry (gpt-5-mini solver, gpt-5 solver+reviewer) the compiled plan carries the reviewer slot on gpt-5 (`REVIEW_REQUIRED`, predecessor `initial`, `max_revisions` 1); an auth+migration candidate passes its check but the gate requires independent review: the leg activates on the run before `RunCompleted`, in a real environment, with a brief that holds the request, the criteria and the diff and no assistant turn, tool argument or completion talk of the solver; the reviewer reads the candidate, runs the check in its sandbox and reports REVISE with one finding on `src/auth/login.py:3` and one on a phantom path — 1 validated, 1 unsupported, highest HIGH; the candidate returns to work as revision 1 of 1 with only the validated finding queued, on the solver's binding (no reviser slot); the revised candidate is reviewed anew and passes; `ReviewDecisionRecorded` RETURN then ACCEPT by `independent_reviewer`, the gate re-evaluated with independent review PASS while the human obligation still stands; both environments disposed, no scratch tree left, the canonical tree holds the solver's revision; the run's path label is CRITIQUE. On Windows the slot stays (`SANDBOX_UNAVAILABLE`) and the obligation stands for a person.

## Limitations

- The compiler reserves the reviewer's cost in every shape when a reviewer-role binding exists; reviewer-value statistics (docs/27 §15) are not consulted for the choice yet. A reviser slot is honoured when a plan declares one; compiled plans revise on the solver's binding.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_007_the_reviewer_slot_activates_on_the_gate_validates_findings_and_bounds_revision`
- `qual_epr_006_a_quality_rejection_continues_the_run_on_the_prevalidated_stronger_slot`
- `qual_epr_004_the_compiler_runs_through_core_and_identical_inputs_give_identical_plans`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
