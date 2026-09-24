# Task Card — EPR-011 Build isolated counterfactual replay

## Identity

- Task ID: EPR-011 (REQ-EPR-011; related REQ-EV-0014; owner eval-bench)
- Milestone: M9, phase 5; prerequisites EPR-007, EPR-010, M9.3 (COMPLETE)
- Qualification: QUAL-EPR-011 / EPR-E2E-011 / EPR-FI-011 (docs/61)
- Evidence tier: release-critical (isolation, a security boundary, evidence semantics)
- No duplicate executor, compiler or store: the replay is a task on the existing runtime and Gateway, its record a projection of the log (docs/38 "CounterfactualReplay").

## Existing-code audit

- classification: DOCUMENTED-ONLY. The request record (EPR-010) labelled an `ESTIMATED` direct-frontier counterfactual and an `observed_label` that was always `NOT_EXECUTED`; nothing pinned the repository a request started from, and nothing could execute an alternative.
- first missing link: no immutable snapshot of the request's starting repository.
- production entry points: `services/modbit-core/src/replay.rs` (`capture` via `modbit_git::Repo::snapshot_dirty`; `start`: admission, revision, plan, freshness, sandbox; `scratch`: fetch from the snapshot ref into a fresh repository, detached checkout, no remote, sanitization; the replay task and its narrow lease; `CounterfactualReplayStarted`); `runtime.rs` (capture before a request's first round); `accounting.rs` (`ReplayObservation`, `observed_label`/`observed_minor` from the replay's own recorded outcome); `server.rs` (`ReplayCounterfactual`).

## Verification

- `qual_epr_011_an_alternative_replays_isolated_on_the_exact_snapshot_and_is_observed` (real Core, terminal broker and host sandbox, signed registry, scripted OpenAI-compatible provider, fake forge): a request starts from a dirty worktree (edited tracked file, untracked draft, a tracked `.env`) and runs on its chosen plan; `RequestSnapshotRecorded` pins it. Replaying the decision's hard-eligible `openai/gpt-5` alternative is `REPLAY_NOT_ADMITTED` until the policy sets `eval.replay` ALLOW (then, after a Core restart, admitted); the chosen plan is `REPLAY_PLAN_CHOSEN`, an unknown one `REPLAY_PLAN_UNKNOWN`. The replay runs as its own task through the Gateway (the provider sees `gpt-5`) in a scratch repository under the Core's `replays/`: the dirty state restored, `.env` sanitized away, no remote; its `forge.issue.read` attempt is refused and the fake forge receives no request; the original repository's files, refs, worktrees and status are byte-for-byte what they were. The request record shows the replay `OBSERVED` with its plan, snapshot commit, registry generation and `review_isolated` ceiling, `observed_minor` its cost, the estimate still an estimate. Deleting the snapshot ref makes a replay `REPLAY_REVISION_MISMATCH`; restored, a newer registry generation makes it `REPLAY_STALE`. On Windows (no host sandbox) the replay is `SANDBOX_UNAVAILABLE` and nothing is taken.
- EPR-FI-011 mutations: disabling sanitization fails the test (`.env` travels); replaying under the trusted profile with egress and secret operations fails it (the fake forge receives the request with the token).

## Limitations

- Deterministic evidence is not reused: every replay recomputes its verification in isolation (the safe side of docs/38 step 3).
- A replay is started on request; exploration (a fraction of requests replayed automatically, docs/27 §12.2) and propensity use belong to EPR-012.
- Pre-existing dirty files count in a request's candidate diff (DI-1) as they did before; the fixture plans them.

## Evidence

- `evidence.json` in this directory
- docs/38 "CounterfactualReplay" as built, docs/30
