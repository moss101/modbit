# Task Card — M5.1 Dynamic task-scoped tool projection

## Identity

- Task ID: M5.1
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/16 "Dynamic task-scoped projection" (the model never receives the entire registry; the Prompt Compiler projects only tools authorized and likely useful for the active node, per leg role; the projection hash is recorded in the Turn; the kernel is the boundary), REQ-EV-0096 (visible tools per task/turn from support × policy × relevance), REQ-EV-0134 (deferred tools discovered by search or use, never authorized by discovery), docs/14 contract 4 (the harness plan as durable state), docs/44 row "Dynamic tool projection".
- Qualification: docs/51 `E2E-011` — run a read-only repository question; the model request excludes write/browser/secret tools; an attempted unprojected tool call is rejected even if manually crafted.
- Evidence tier: real-system (the real Core and Tool Registry, the real pipeline and kernel, a scripted model that crafts calls the projection did not offer; every turn's projection on the log)

## Goal

Offer the model, every turn, exactly what the active node has declared it needs — read-only tools always, file writes once the plan names files, protected effects once the plan names them — tell it what is withheld and why, and refuse at the pipeline any call outside the turn's projection whatever the model wrote, with the kernel still the boundary for everything offered.

## Existing-code audit

- classification: PARTIAL before this task. IMP-EV-0096 compiled the visible surface from host support × execution profile × capability lease × kernel, and REQ-EV-0134 deferred the long tail behind `tool.search`; but the projection was the same for every node of a task (a read-only question was offered `change.apply`, `change.batch` and `git.worktree.close` from its first turn), nothing about the plan narrowed it, `ToolProjectionSelected` carried only the hash, and a call to a tool the model was not offered was judged by the kernel alone — visible meant callable.
- production entry points:
  - `crates/core-runtime/src/harness.rs` — `ProjectionScope` (from the plan: `writes_declared`, `protected_declared`, `leg_role`), `HarnessState::projection_scope(leg_role)`, `WithheldTool { name, reason, how }`, `project(name, effect_class, capabilities, scope)`: `READ_ONLY` always; a `REVERSIBLE_WRITE` tool with `fs.write` needs `expected_files` (`DECLARE_WRITES`); `PROTECTED_WRITE` / `EXTERNAL_SIDE_EFFECT` / `SECRET_ACCESS` / `DESTRUCTIVE` need the tool, its toolset or its capability in `protected_effects` (`DECLARE_PROTECTED_EFFECT`); a reviewer leg gets reads and reversible writes for its disposable worktree and never a protected class (`REVIEWER_LEG`); process execution is not a file write.
  - `services/modbit-core/src/runtime.rs` — `projection()` applies the scope before the deferred split and records the withheld set; the `plan.update` description names what is withheld by reason and what unlocks it; `projected_names` per turn (the projected tools plus the deferred-but-in-scope tools a direct call may activate) travels with every `execute_tool`; the loop tells `TOOL_NOT_VISIBLE` (outside the compiled surface) from a visible-but-withheld tool, which reaches the pipeline and is refused there; the harness gates (plan, scope, retrieval, repair) keep answering first for the tools they govern; `ToolProjectionSelected { tool_projection_hash, projected, withheld, leg_role }`.
  - `services/modbit-core/src/tools.rs` — `InvokeRequest.projection: Option<Vec<String>>` (`None` for a direct client call); `KernelPort::decide` refuses a name outside the projection before the kernel is asked: `POLICY_DENIED` / `TOOL_NOT_PROJECTED` with the way forward (declare in `plan.update`, or `tool.search`).
  - `crates/core-runtime/src/diagnostics.rs` — `TOOL_NOT_PROJECTED` classified as a retryable policy failure whose recovery is the plan, not the user (corpus case `change_apply_not_projected`).
  - `crates/domain/src/turn.rs` — the projection event's new fields (defaulted for existing logs).
- proof: (E2E-011) a task under `local_trusted` whose host list carries `change.apply` and `git.worktree.close` asks a read-only question: the first four model requests offer `fs.read`, `search.retrieve`, `shell.exec` and `plan.update` and none of `change.apply`, `change.batch`, `git.worktree.close`; `plan.update` says `DECLARE_WRITES -> change.apply, change.batch; DECLARE_PROTECTED_EFFECT -> git.worktree.close`; the withheld destructive tool is not even named in the deferred catalog. A `change.apply` crafted before any plan meets the harness plan gate first (`HARNESS_PLAN_REQUIRED`, docs/14 — unchanged; it never becomes a tool call). A `change.apply` crafted in the same turn as the `plan.update` that declares its file (the harness gates pass; this turn's projection does not carry it), and a `git.worktree.close` crafted under a plan that declares files but not the effect (the harness gates pass, the write tools are projected, the destructive one is withheld and still unnamed in the catalog), come back `POLICYDENIED / TOOL_NOT_PROJECTED` naming `plan.update`, and on the log each is Proposed → Validated → PolicyDecision denied with that code (the call's end) and never Dispatched. Once the plan also declares the effect, the next request names `git.worktree.close` in the deferred catalog (callable by name, hydrated on use) and has no withheld note; the declared write lands (`b.txt`), exactly one of the three `change.apply` calls succeeded, no `git.worktree.close` dispatched, and every `ToolProjectionSelected` (seven turns) names the projected and withheld sets with the leg role, the hash differing across the three scopes. (Rule test) `projection_follows_the_plan_and_the_leg_role` covers each class, the toolset and capability forms of a declaration, and the reviewer leg.

## Limitations

- The reviewer leg role is compiled by the rule but no reviewer leg runs yet (EPR-018); the runtime projects under `solver` for every run.
- Browser, secret and network tools do not exist in the registry yet (M7/M8); the rule withholds their classes by construction and the E2E proves it on the destructive tool that does exist.
- "Likely useful" is derived from the plan's declarations, not from a relevance model; the deferred catalog (REQ-EV-0134) remains the mechanism for the long tail.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_1_projection_follows_the_plan_and_refuses_crafted_calls` (services/modbit-core, real Core; E2E-011)
- `projection_follows_the_plan_and_the_leg_role` (crates/core-runtime harness rule test)
- `diagnostics_corpus` (crates/core-runtime; new case `change_apply_not_projected`)
- Regression: `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix` (the projection change after the first plan is one extra prompt-cache miss — the cost of dynamic projection, now asserted), `qual_px_000_headless_cli_task_lifecycle` (its script declares the worktree close), `qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily`, `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`, the worktree-close approval tests (their scripts now declare the protected effect), `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review`, the whole surface-protocol suite

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
