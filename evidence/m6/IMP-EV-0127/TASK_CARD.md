# Task Card — IMP-EV-0127 Autonomous detach/resume

## Identity

- Task ID: IMP-EV-0127
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0127; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0127 — Kill client; worker continues safely and reconnects.
- Evidence tier: real-system (the real Core process against a scripted OpenAI-compatible server; the client connection dropped mid-model-call; a second client)

## Goal

Detached runs use leases/checkpoints/heartbeats/replay and permission ceiling.

## Existing-code audit

- classification: PRESENT since M2/M4 by construction (the run loop lives in the Core under the session lease, checkpoints and the protocol state; a client is a subscriber) and since M6.7 for children; this task adds the explicit qualification. No production code change.
- production entry points: `services/modbit-core/src/runtime.rs` (`run_loop` owned by the Core, fenced by the session lease generation; `reconcile_after_restart` only on a Core restart), `server.rs` (`SubscribeEvents` from any cursor for a reconnecting client), the capability lease ceiling per profile (docs/23).
- proof: the client that started the run is dropped while the second model request is in flight; the worker finishes the plan → write → completion on its own — one `RunCreated`, no `RunSuspended`, no `TaskWaiting`, no attention — and a new client reconnecting to the same Core reads the finished task (ReadyForReview / Completed) with the write on disk.

## Limitations

- A desktop-supervised Core exits with its supervising client by design and resumes on the next start (M6.7 covers that path); heartbeats are the lease renewal and capacity ticket renewal per turn.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0127_a_killed_client_leaves_the_worker_running_and_a_new_client_reconnects`
- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
