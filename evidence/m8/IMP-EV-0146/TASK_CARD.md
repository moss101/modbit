# Task Card — IMP-EV-0146 Blueprints/environment snapshots

## Identity

- Task ID: IMP-EV-0146
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0146 (reusable revisioned environment definitions); QUAL-EV-0146 (rebuild and pin the exact environment digest); docs/21 "Environment revisions".
- Qualification: a real Core, a blueprint reused from the data directory by a repository layer, an explicit rebuild after the definition changed, repeated.
- Evidence tier: real-system.

## Goal

An environment definition is reusable by name (a blueprint) and versioned by content; an explicit rebuild re-captures and pins exactly the digest of what is there now, and a rebuild of an unchanged environment changes nothing.

## Existing-code audit

- classification: MISSING before this task: no reusable definition, no rebuild.
- production entry points: `crates/workspace/src/environment.rs` (blueprints: `<root>/.modbit/environments/<name>.json`, `<data>/environments/blueprints/<name>.json`, applied through `extends` before the layer naming them, a blueprint extending blueprints, cycles bounded and recorded; every source in the revision by hash with its kind `blueprint:<name>`), `services/modbit-core/src/environment.rs` (`rebuild`: re-capture, `missing_tools` checked, the revision stored as an object, `EnvironmentRebuilt {from_digest, to_digest, revision_ref, changes}` journaled, the pin replaced), `services/modbit-core/src/server.rs` (`RebuildEnvironment` → `EnvironmentRebuilt {from_digest, to_digest, changes, offset}`, `task.author` under a session lease, `TASK_RUNNING` while a run is in progress, `ENVIRONMENT_UNAVAILABLE` when a required tool is absent), `services/modbit-core/src/environment.rs::pinned` (the newest `EnvironmentRebuilt` or `EnvironmentPinned` is the pin a resumed run checks against).
- proof: `services/modbit-core/tests/surface_protocol.rs::qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly` — the repository layer extends blueprint `base` from `<data>/environments/blueprints/base.json` (a source of kind `blueprint:base` by hash); after the stale park, `RebuildEnvironment` answers `from_digest` = the pinned digest and `to_digest` = the digest the view computed of what is there, with the changes; the view then shows the new pin, not stale, the new variable name included; a second rebuild answers `from == to` with no change and journals `EnvironmentRebuilt` with `changes: []`; `StartTask` resumes the run in the rebuilt revision and its process prints the new value (`A=repo2`).

## Limitations

- A blueprint is resolved at capture from the two locations in that order; there is no registry of blueprints beyond the files, and no versioned name (`base@1`): the version is the content hash in the revision.
- A rebuild is for a run at rest; a run in progress keeps its revision until it ends or waits.

## Verification

- `qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
