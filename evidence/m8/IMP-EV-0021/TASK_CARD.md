# Task Card — IMP-EV-0021 Environment source hierarchy

## Identity

- Task ID: IMP-EV-0021
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0021 (version repo/team/user environment inputs with explicit precedence and staleness state); QUAL-EV-0021 (change the environment definition and verify the run pins the old revision until an explicit rebuild); docs/21 "Environment revisions".
- Qualification: a real Core with a real workspace and data directory — a blueprint in the data directory extended by the repository layer, a scripted model running a real `sh` through the terminal broker, the Core killed and restarted under the run.
- Evidence tier: real-system.

## Goal

The environment a run starts in is three layers with explicit precedence — user, team, repository, each extending blueprints — captured as one revision the run pins; the definition changing afterwards changes nothing for the run, and the pin's staleness is a reported state.

## Existing-code audit

- classification: MISSING before this task: no environment definition existed anywhere (a process inherited the broker's environment as it was), no revision, no notion of stale.
- production entry points: `crates/workspace/src/environment.rs` (`layer_paths`: `<data>/environments/user.json` < `<data>/environments/team.json` < `<root>/.modbit/environment.json`; `compile`: blueprints by `extends` from `<root>/.modbit/environments/<name>.json` or `<data>/environments/blueprints/<name>.json`, applied before the layer that extends them, a cycle or a missing blueprint recorded as a problem; a later layer's variable overrides, its `path` entries go in front, a toolchain entry of the same name is replaced; `snapshot`: the sources by hash, the tools' versions as observed, the `PATH` entries, the names and values, the platform → `EnvironmentRevision` with `digest_of`; `changes`: what differs between two revisions), `services/modbit-core/src/environment.rs` (`current`, `pinned` from the log's newest `EnvironmentPinned`/`EnvironmentRebuilt` and the object it references, `attach_to_run`), `services/modbit-core/src/server.rs` (`GetEnvironment` → `EnvironmentView` with `stale` and `changes`), `crates/domain/src/task.rs` (`EnvironmentPinned`, `EnvironmentStale`, `EnvironmentRebuilt`), `crates/protocol/proto/modbit/v1/surface.proto` and the generated TypeScript bindings.
- proof: `services/modbit-core/tests/surface_protocol.rs::qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly` — before any run the view shows the blueprint and the repository layer by hash, the variable names in order and the `PATH` entry; the run's `sh` prints the repository's value over the blueprint's (`A=repo B=blueprint`) and the blueprint's directory on its `PATH`; exactly one `EnvironmentPinned` whose digest equals the view's, the values absent from the log; the repository layer rewritten under the live run → the view still shows the pinned names, `stale: true`, the changes naming the repository layer and the added variable; `RebuildEnvironment` refused `TASK_RUNNING` while the run is in progress.

## Limitations

- The layers are the definition: no per-run override, no per-task selection of a blueprint (the repository layer's `extends` selects it).
- Sources are files by hash; a variable that reads from the process environment at capture is not expanded (values are literal).
- A `cloud_isolated` task's environment is the sandbox image's; the layers do not reach into the guest (M8.5's sandbox is the environment there).

## Verification

- `qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly`
- `crates/workspace/src/environment.rs` unit tests (precedence, blueprint order, digest follows every input, missing blueprint recorded, cycle bounded)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
