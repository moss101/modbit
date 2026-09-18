# Task Card — IMP-EV-0062 EnvironmentSnapshot / Revision

## Identity

- Task ID: IMP-EV-0062
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0062 (pin toolchain/PATH/env refs/workspace roots/tool availability to a revision identity); QUAL-EV-0062 (a resume detects an unavailable environment revision and follows the explicit rebuild/fail path); docs/21 "Environment revisions".
- Qualification: a real Core killed under a live run and restarted; the resumed run against a changed environment; a fresh run against a definition naming a tool that is not installed.
- Evidence tier: real-system.

## Goal

A run's environment is one revision identity — the toolchain as observed, the `PATH` entries, the variables, the workspace root, the platform — its processes run with exactly that, and a run resumed into something else, or started without what it requires, waits on the explicit path rather than running in an environment nobody chose.

## Existing-code audit

- classification: MISSING before this task: a process ran with whatever the terminal broker inherited; nothing pinned, nothing checked at resume.
- production entry points: `services/modbit-core/src/environment.rs` (`attach_to_run`: a fresh run captures and pins — `EnvironmentPinned {run_id, digest, revision_ref, sources, toolchain, path, env_names, problems}`, the values in the referenced object only; a resumed run re-captures and compares digests → `EnvironmentStale {run_id, pinned_digest, current_digest, changes}` and `ENVIRONMENT_STALE`; a required tool absent → `ENVIRONMENT_UNAVAILABLE`), `services/modbit-core/src/runtime.rs` (`run_loop` parks the run needing attention before its first turn on either code), `crates/core-runtime/src/diagnostics.rs` (both codes classified Infrastructure, not retryable, the user action named; pinned in the diagnostics corpus), `crates/tools/src/pipeline.rs` (`InvokeContext.environment`, `ProcessEnvironment::apply`: the revision's variables under the call's own, its `path` entries in front of the host `PATH` unless the call set `PATH`), `crates/tools/src/direct.rs` (`shell.exec`, `test.run`, `proc.exec` run under it), `services/modbit-core/src/tools.rs` (`ToolHost.environments`: the pinned revision reaches every call of the task).
- proof: `services/modbit-core/tests/surface_protocol.rs::qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly` — the toolchain entry (`git`) carries its observed version in the revision; the run's process gets the variables and the `PATH` entry; the Core killed mid-stream and restarted, `StartTask` resumes and the run journals `EnvironmentStale` (the pinned and current digests, the changes) and parks `Waiting/UserInput/Suspended` with `TaskNeedsAttention.diagnostic.code = ENVIRONMENT_STALE` (class `INFRASTRUCTURE`, the user action naming `RebuildEnvironment`) before the model is asked anything (the request count unchanged); a second task whose repository layer names `modbit-no-such-tool-xyz` (required) and an optional absent tool parks `ENVIRONMENT_UNAVAILABLE` naming only the required one, pins nothing and sends no model request.

## Limitations

- Tool availability is `which` + the version argument on the revision's `PATH`; a tool that is present but broken is a version of `""` only when it fails to run.
- The check happens at the run's start (fresh or resumed); an environment changing under a live run is detected at the next resume, not mid-run (the run keeps its revision meanwhile).
- Processes started outside the tool pipeline (the verification runner's own children inherit from the tool call) are covered through the call that started them.

## Verification

- `qual_ev_0021_0062_0146_environment_revision_is_pinned_applied_and_rebuilt_explicitly`
- `crates/core-runtime/tests/diagnostics_corpus.rs` (`loop_environment_stale`, `loop_environment_unavailable`)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
