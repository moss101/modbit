# Task Card — M2.3 `modbit-execd` structured argv/PTy/replay/OutputRef

## Identity

- Task ID: M2.3
- Milestone: M2
- Canonical owner: execution / terminal (`services/modbit-execd` is THE doc 81 terminal/process broker; `crates/terminal` is the Core-side client)
- Requirement IDs: docs/21 "Structured command contract", "Durable modbit-execd", "Command failure semantics"; docs/11 deployment unit `modbit-execd`. Capabilities registered against REQ-EV-0100, 0025, 0271, 0027, 0135, 0019, 0269, 0026 (their named qualification tests ship here; the tasks close when the Core tool path wires them in M2.4).
- Risk class: high (process execution)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

A durable broker that runs real processes and PTYs from a structured request (argv, cwd, explicit env, timeout, pty, stdin mode, output budget, profile, lease id), keeps a durable per-session output log with pure byte cursors and replay, streams bounded frames, survives client detachment, cancels real processes, and publishes complete output as a content-addressed OutputRef whose digest equals the raw bytes.

## Non-goals

- Policy or capability decisions (Core), durable shell sessions with cwd/env carryover (`terminal_session_id` reserved), Core-side bounded model view and noise suppression (M2.4 tool), broker restart recovery of running processes (M4).

## Required reading

`AGENTS.md`, `docs/21`, `docs/11`, `docs/30` (framing reuse), `docs/33` (backpressure).

## Existing-code audit

- classification: NOT-FOUND (both shells)
- production entry point: `modbit-execd --data-dir` (`broker::run`), `modbit_terminal::ExecClient`
- real effector/storage boundary: real child processes and PTYs; `sessions/<id>.log` + `.idx`; `objects/<hash>`

## Invariants

- Same boot-secret handshake and bounded framing as the Core's SurfaceProtocol; wrong secret gets nothing.
- Environment is explicit (`inherit_env=false` clears it; a minimal PATH is provided); cwd must exist; bad argv is a typed `EXEC_FAILED`.
- Non-zero exit is a result; timeout and cancel are flagged on `ProcessExited`.
- Output frames ≤ 64 KiB; cursors are byte offsets into the data log; replay from any cursor reproduces the exact bytes; the OutputRef digest is sha256 of the complete raw output.
- Waits never hold the child lock, so Cancel can always kill.
- Same `request_id` replays the existing session instead of starting a second process.

## Verification

- QUAL-EV-0100: argv/cwd/env/streams/exit code/timeout/bad argv against real processes
- QUAL-EV-0019/0269 (and 0026 substrate): 10 MiB streamed in ≥160 bounded chunks, digest equals raw, artifact retrievable by digest
- QUAL-EV-0271/0027/0135: client dropped while the process runs, listing shows it running, reattach from a mid-stream cursor continues exactly, replay from 0 equals the original, cancel kills the real process and every attached client sees the exit, request replay
- QUAL-EV-0025 (substrate): explicit stdin, closed stdin refused, no env leakage between requests
- PTY mode (unix): a real terminal session echoes through the master
- wrong secret refused
- E2E: hosted CI on macOS/Linux/Windows (named pipes, ConPTY via portable-pty)

## Completion evidence

See `evidence.json`.
