# Task Card — EPR-004 Compile and validate one bounded conditional plan

## Identity

- Task ID: EPR-004
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-004; owner label: model-gateway; subsystem: model-gateway
- Qualification: `QUAL-EPR-004` / `EPR-E2E-004` — drive the compiler via a production Core command using the actual signed registry and policy snapshots; identical inputs produce identical plans and digests; verify requested and resolved configuration and every exclusion reason against a golden corpus. Negative: `EPR-FI-004` — inject cyclic or undeclared slots, integer overflow, unavailable assurance, disallowed residency, reviewer canonical-write access, stale statistics or an insufficient cap; no dispatch. The runtime must reject a gate trying to synthesize a new topology.
- Evidence tier: real-system or production-equivalent

## Goal

Enumerate the bounded conditional plans a request could run from the registry's live bindings, prevalidate every initial and continuation binding, trigger, worst-case total budget and assurance before anything dispatches, and choose by confidence-adjusted feasibility and then lowest expected complete cost — with no template dispatch, no runtime branch synthesis, and manual pins that keep policy.

## Existing-code audit

- classification: PARTIAL before this task. Admission (EPR-014) validated a plan it was handed and feasibility (EPR-016) measured it, but nothing enumerated the plans a request could run: the direct plan was the only plan the product compiled, and a client could only submit a plan it had built itself.
- production entry points:
  - `crates/providers/src/compiler.rs` — the compiler. Openers are the live solver bindings that pass residency, profile, capability and currency checks; continuations are strictly dearer solver bindings on `QUALITY_REJECTED`, enumerated only when assurance is available. Each candidate is priced at its worst case from the registry's economics (every expected input token and every output token the slot may produce), checked against the request cap, run through the same plan validator admission uses, and given an expected complete cost in which a continuation is paid only as often as the initial leg's lower bound says it is rejected. Selection is feasibility first (EPR-016), then lowest expected complete cost. Every candidate and every exclusion reason is in the output, and identical inputs produce an identical plan id, content digest and input digest.
  - `services/modbit-core/src/routing.rs` — `compile` joins the active signed registry (by generation and document digest), the session's latest statistics snapshot (by version), the registry's `auto` floor (as the threshold version), the org policy (by its content digest) and the workspace's verification plan (as assurance), compiles, and admits what was selected through the same admission every plan goes through, recording feasibility with it.
  - `CompileRoutingPlan` → `RoutingCompileView` — the fenced Core command, with the pin and the cap as its only request-side inputs.
  - `tests/fixtures/routing/golden/*.json` — the golden corpus: cold start, a feasible selection, a manual pin and the no-assurance shape, each recording the requested and resolved configuration and every exclusion reason.
- proof: through Core, with a signed registry active and a repository that declares a check, the compiler enumerates single-slot and continuation plans, prevalidates all of them, and at cold start selects the best hard-eligible plan as `QUALITY_FLOOR_INFEASIBLE` with the target not claimed and every candidate's missing evidence named; compiling again with identical inputs gives the same plan id, input digest and candidates, reinstalled at the next epoch; a manual pin narrows the openers and excludes the rest by name, an ineligible pin is refused as a pin, and a cap that fits no plan compiles nothing while the plan in force stays; and admission refuses a continuation the compiled plan does not declare. Against synthetic registries, stale statistics, an insufficient cap, integer overflow in a price, a disallowed residency, unavailable assurance and a revoked pin are each refused or excluded with their reason, and a feasible selection prefers a mini-plus-continuation plan over a plain stronger solver by expected cost even though its worst case is higher.

## Limitations

- Nothing executes a compiled plan yet. The runtime still dispatches the direct plan it admits at run start; routing a new run through the compiled plan's initial slot, and activating its continuation on a quality rejection, is EPR-005 and EPR-006. A compiled plan installed on a run today is recorded and admitted but not dispatched from.
- Reviewer and reviser slots are not enumerated: the reviewer role's isolation (no canonical write) is enforced by the sandbox owners in EPR-018, and until then the compiler builds only solver slots, so the "reviewer canonical-write access" injection has nothing to inject into. It is recorded here rather than faked.
- `expected_input_tokens` is a fixed 40 000 and `delta` a fixed 0.05 in Core until the profiler leaves shadow and a mode policy carries them; both are inputs to the digest, so a change is visible.
- `gate_version` is a fixed `gate-1` and `risk_version` is `none`: no gate or risk model exists to version yet (EPR-006, EPR-008).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_004_the_compiler_runs_through_core_and_identical_inputs_give_identical_plans` — the end-to-end proof through Core.
- `identical_inputs_produce_identical_plans_and_the_golden_cold_start` (golden: `cold_start`)
- `feasibility_then_lowest_expected_complete_cost_decides` (golden: `feasible_selection`, `manual_pin`)
- `nothing_dispatches_from_what_cannot_be_prevalidated` (EPR-FI-004; golden: `no_assurance`)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34564504407.json`: hosted CI, green on macOS, Linux and Windows at `a56d074`
- Commits: `0d8d32c`, `2186be9`, `e0379fd`, `a56d074`
