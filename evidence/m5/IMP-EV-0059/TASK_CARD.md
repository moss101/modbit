# Task Card — IMP-EV-0059 Path-scoped lazy rules

## Identity

- Task ID: IMP-EV-0059
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0059; owner label: Instruction Compiler; subsystem: prompt-compiler / core-runtime
- Qualification: `QUAL-EV-0059` — Unrelated module rule stays absent until matching file is touched.
- Evidence tier: real-system (rules on disk, the real Core, the requests the model server received)

## Goal

Rules load when matching paths become active, with deterministic precedence, so an unrelated module's rule never costs a turn it does not concern.

## Existing-code audit

- classification: NOT-FOUND before: the prompt compiler's rules segment was always empty; no rule files were read.
- production entry points: `crates/prompt-compiler/src/rules.rs` (`Layer`, `Rule`, `load`, `RuleSet::select` with glob activation via `globset`, `texts`), `services/modbit-core/src/rules.rs` (`layers`, `active_paths` from the context ledger's reads and pack entries plus the harness's written and planned files, `RunRules` selecting every turn and recording `RulesSelected` on change), `services/modbit-core/src/runtime.rs` (`PromptInput.workspace_rules`).
- proof: `qual_ev_0059_0129_…`: a project rule scoped to `src/billing/**` is absent from the first two requests (README read only) and present from the request after `src/billing/invoice.rs` was read, its `RulesSelected` activation naming the path and the glob; the first selection lists it as dormant.

## Limitations

- Symbols are not an activation source yet (paths are); rules are reloaded per run, not watched.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0059_0129_scoped_rules_activate_lazily_and_conflicts_name_the_winner`
- `rules::tests::scoped_rules_stay_dormant_until_a_matching_path_is_active_and_layers_decide_conflicts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
