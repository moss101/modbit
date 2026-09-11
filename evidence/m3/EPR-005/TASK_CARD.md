# Task Card — EPR-005 Integrate initial execution and preserve direct baseline

## Identity

- Task ID: EPR-005
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-005; owner label: core-runtime; subsystem: core-runtime
- Qualification: `QUAL-EPR-005` / `EPR-E2E-005` — repeat the baseline through the new conditional initial-leg path using a real provider, edit, test, review and restart; compare verified success, cost, latency and reliability; prove manual pin behaviour and static policy canary and rollback; classify the actual DIRECT path and do not promote cheap routes without LCB and gate evidence. Negative: `EPR-FI-005` — interrupt after a typed tool dispatch and restart the actual Core; reconcile the unknown effect before continuing; a user pin conflict cannot silently switch the model or expand the budget.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-002-epr-baseline-live-provider-proof.md` (the live-provider half)

## Goal

Route each new Run through the canonical conditional transaction path — a compiled, admitted plan with an initial solver and prevalidated slots — while keeping the measured single-leg baseline, permitted manual pins, the Auto quality and confidence objectives and the frontier backstop; derive DIRECT only from the actual path, and never promote an economical route without conservative quality and independent gate evidence.

## Existing-code audit

- classification: PARTIAL before this task. EPR-004 compiled and admitted a plan on request, but every run still dispatched on the direct plan admitted at run start: the compiler had no runtime caller, a plan installed by `CompileRoutingPlan` was recorded but never dispatched from, and the endpoint and model a run used came from its start configuration alone.
- production entry points:
  - `services/modbit-core/src/runtime.rs` — `route_new_run`. With a signed registry active a new run compiles its plan from the registry, the pinned statistics and the mode floor, admits it, activates its initial slot once, and dispatches on that slot's binding: the run's endpoint and model come from the plan. Without a registry the run keeps the measured direct baseline, written down and admitted the same way. Attempts are recorded against the plan and slot the run was routed through, not against a fixed direct id.
  - `recover_route` — a resumed run continues on the plan and the activation it already has; it never compiles a second plan or activates a second slot on the way back.
  - `StartConfig::pinned` — a non-empty model in `StartTask` is a manual pin. It narrows the compiled plan to that binding, keeps policy (a revoked pin stops the run before it starts with `PIN_NOT_ELIGIBLE`), and is never silently switched away from.
  - `services/modbit-core/src/routing.rs` — `path_label` derives `DIRECT` from the slots that actually ran, so a compiled plan whose continuation never activated is labelled by what happened, never by a template.
  - `crates/tools/src/direct.rs` — `shell.read` now reports the process rather than the size of the preview: a process that has exited is never reported as running because its output outran the preview budget. Found by CI on this task's first run and fixed here.
  - `services/modbit-core/src/runtime.rs`, `verify.rs`, `review.rs` — verification residue. Files a verification stage creates inside the workspace (a Python bytecode cache, a reporter file, build output) are recorded on the workspace aggregate as `VerificationResidueRecorded`, excluded from the diff invariants and from the review candidate for every task sharing the worktree, and named so a reader knows why they are absent. Found by CI on this task's second run: the Linux runner's `python3` wrote `__pycache__` during the baseline check, and DI-1 attributed it to the agent as a write outside the plan, which blocked completion. The write-set invariant judges the agent's writes, not what the checks left behind.
- proof: through Core on a repository with a real configured check, the same edit-verify-review task runs once on the direct baseline and once through the compiled path; the published baseline shows the same verified outcome, the same model, the same number of model calls and the same final state for both, and the compiled run is labelled `DIRECT` from what ran with its target not claimed at cold start. A manual pin opens the plan with the pinned stronger solver against the compiler's cheaper preference, under the same registry generation. A canary generation with a raised floor takes effect for the next run and pins itself, and re-activating the previous content as a rollback generation does the same. A run interrupted after its `change.apply` was dispatched, on a Core killed hard, comes back suspended rather than continued, with its plan, admission, single activation and recorded attempts identical to before the kill; resumed, it continues on the same plan with no second plan compiled and no second activation, and finishes.

## Limitations

- The live-provider half of `EPR-E2E-005` — the same comparison against a production provider endpoint — has not run: this repository has no provider credentials. DR-M3-002 defers it. The cost comparison the qualification asks for is therefore a comparison of model calls and verified outcomes, with cost unknown on both paths as EPR-000 records it.
- Nothing activates a continuation yet: the compiled path runs its initial slot, and answering a quality rejection by activating the stronger slot is EPR-006. Every compiled run today is therefore `DIRECT` in fact.
- Registry activation is not durable across a Core restart by design — it is configuration the operator activates — so a restarted Core runs the direct baseline until a generation is activated again. Making the last activated generation survive a restart is a small follow-on once the operator surface for it exists.
- Interrupting after a tool dispatch is proven with the run suspended and its routing state exact; reconciling the tool's unknown effect itself is the effect ledger's job (REQ-EV-0242, M4) and is not re-proven here.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_005_new_runs_go_through_the_compiled_initial_leg_and_keep_the_baseline` — the end-to-end proof through Core, including the baseline comparison, the pin, the canary and rollback, and the interrupted run.
- `qual_epr_002_a_signed_registry_activates_at_runtime_and_a_revocation_stops_dispatch` — a mid-run revocation stops the next dispatch explicitly, and a pin on the withdrawn model cannot start a run (EPR-FI-005: no silent switch).
- `qual_epr_014_a_conditional_plan_is_admitted_whole_and_its_activation_is_exact` — one exact activation across a kill.
- `qual_ev_0221_background_handles_survive_client_restart_with_bounded_preview_and_cancel` — the `shell.read` defect CI found.
- The residue assertions inside `qual_epr_005_…`: the check's bytecode cache is recorded as residue, trips no invariant, and is not committed by the accepted review.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34564504407.json`: hosted CI, green on macOS, Linux and Windows at `a56d074`
- Commits: `2186be9`, `e0379fd`, `a56d074`
