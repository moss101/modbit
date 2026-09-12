# Task Card — M6.4 semantic write-conflict detector

## Identity

- Task ID: M6.4
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/14 "Semantic conflict detection"; docs/43 M6.4; owner label: core-runtime; subsystem: core-runtime (`crates/core-runtime/src/conflict.rs`, `services/modbit-core/src/spawn.rs` step 4)
- Qualification: M6.4 / E2E-009 — two builders with overlapping write sets: the detector blocks unsafe concurrent admission; disjoint builders pass.
- Evidence tier: real-system for the admission path (the real Core, the real index/symbol/graph facts of the parent repository) and unit-level for every rule against a fixture fact set

## Goal

Before parallel builders start, check explicit file/path overlap, same symbol or public interface ownership from the AST/LSP graph, dependency hot spots, migration/schema/shared config files, generated files and lockfiles, and test fixtures likely to conflict.

## Existing-code audit

- classification: PARTIAL before this task (M6.3 checked explicit path/glob overlap only).
- production entry points: `crates/core-runtime/src/conflict.rs` (`RepoFacts`, `Facts`, `ConflictPolicy` with the shared/generated/fixture patterns and the hot-spot threshold, `detect` producing typed `Conflict`s with `Rule` and `Severity`, `scope_covers` / `glob_match`); `services/modbit-core/src/spawn.rs` (`repo_facts` from the Core's exact index, symbol index and evidence graph; step 4 refuses `WRITE_CONFLICT` on any blocking rule and carries warnings to the parent).
- proof: unit tests pin every rule — disjoint scopes pass; overlap blocks; the same public symbol in two scopes is a `SYMBOL_OWNERSHIP` block; a hot spot blocks its holder and warns its dependent; lockfiles, migrations, generated files and fixtures in a shared directory are one-worker-at-a-time and fine alone; globs match names and paths. Through Core the overlapping third builder of the M6.3 E2E is refused `WRITE_CONFLICT` with the pairs named, and the disjoint builders are admitted with their branches merging cleanly.

## Limitations

- Symbol ownership is by definition site (tree-sitter definitions per file), not by LSP references; a call-site coupling across scopes shows as a hot-spot warning only when the import graph sees it.
- Patterns are the defaults of `ConflictPolicy`; a repository-level override arrives with the policy bundle work (IMP-EV-0241).
- "Isolates/replans" on a conflict is the parent's move: the refusal names the rule and the subject; the runtime does not re-partition the work itself.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `disjoint_scopes_pass_and_overlapping_ones_block`
- `the_same_public_symbol_in_two_scopes_is_an_ownership_conflict`
- `hot_spots_block_the_holder_and_warn_the_dependent`
- `shared_generated_and_fixture_files_are_one_worker_at_a_time`
- `globs_match_names_and_paths`
- `m6_3_subagents_are_admitted_transactionally_and_hand_typed_results_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
