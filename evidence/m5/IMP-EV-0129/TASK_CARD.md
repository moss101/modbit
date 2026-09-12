# Task Card — IMP-EV-0129 Memory/project instruction layering

## Identity

- Task ID: IMP-EV-0129
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0129; owner label: Instruction + Memory; subsystem: prompt-compiler / core-runtime
- Qualification: `QUAL-EV-0129` — Conflicting rules show explicit winner and source.
- Evidence tier: real-system (two layers on disk, the real Core, the log)

## Goal

Scoped, provenance-bound instructions with precedence, TTL and conflict diagnostics: the prompt says which rule won, from where, and why the others are out.

## Existing-code audit

- classification: NOT-FOUND before (see IMP-EV-0059).
- production entry points: `crates/prompt-compiler/src/rules.rs` (layer precedence project > user, then `priority`, then file name; `expires_at_ms`; `Conflict { id, winner, winner_layer, loser, loser_layer, decided_by }`; `expired`; `invalid`), `RulesSelected` on the task with every active rule's layer, source and hash.
- proof: `qual_ev_0059_0129_…`: a user `style` rule with priority 99 loses to the project `style` rule by LAYER, the conflict naming both sources; the expired user rule is listed as expired and never injected; a file with an unknown front-matter key is reported as invalid; every active rule carries its layer, source and hash. Memory items as a further layer are the runtime's (REQ-EV-0208) and not part of this task.

## Limitations

- Memory layering beyond project/user rules is not built; TTL is absolute epoch milliseconds.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0059_0129_scoped_rules_activate_lazily_and_conflicts_name_the_winner`
- `rules::tests::a_rule_without_front_matter_is_global_and_named_by_its_file`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
