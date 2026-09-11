# Task Card — M4.5 terminal/browser/sandbox cursor metadata interfaces

## Identity

- Task ID: M4.5
- Milestone: M4 Durable recovery spine (P0)
- Requirements: docs/19 "Protocol state examples" (terminal session ID + last acknowledged output cursor; browser session/control lease; sandbox lease + generation) and resume step 5 (reattach terminal/browser/sandbox resources by lease and cursor); docs/13 "Fencing and epochs" (terminal replay generation); docs/21 "Durable modbit-execd"; docs/33 shutdown (detach rather than kill durable terminal/sandbox resources); REQ-EV-0221 / 0271 / 0027 / 0135 (M2 handles).
- Qualification: docs/51 `E2E-008` — start a command producing output, restart the Core, reattach: no duplicate process start, output resumes from the cursor, earlier output available via replay/OutputRef, cancellation terminates the real process; docs/54 faults 11 (terminal process outlives client disconnect) and 12 (terminal broker killed with active PTY).
- Evidence tier: real-system (real Core hard-killed and restarted; the real broker binary hard-killed and restarted; real processes)

## Goal

Make terminal handles durable across a Core restart and a broker restart, fence their readers by a replay generation, and give the protocol state the typed cursor metadata — terminals now, browser and sandbox as interfaces — that a resume reattaches by.

## Existing-code audit

- classification: PARTIAL before this task. M2 gave background commands durable handles that survived a *client* restart (REQ-EV-0221), with a byte-cursor replay log in `modbit-execd`. But the broker was tethered to the Core's stdin and died with it, its sessions lived only in memory, a restarted Core spawned a second broker that knew nothing, attaches carried no generation, and the Core recorded nothing about handles or cursors on the log.
- production entry points:
  - `services/modbit-execd/src/broker.rs` — session metadata persisted beside the log (`sessions/<id>.json`: request id, argv, start, exit, generation); `load_sessions` on boot rebuilds every session, marks one that was running when the previous broker died `LOST` (exit unknown, never invented) and seals its log under an OutputRef; `execd.ready` (owner-only) names the live broker and goes away with it; an orphan grace (`--orphan-grace-secs`) stops running processes as cancelled and exits when no Core has been connected for that long; `Attach.generation` — an older generation is refused `STALE_GENERATION`, a newer one ends older attachments; `SessionInfo.status/replay_generation/started_at_ms`.
  - `crates/protocol/proto/modbit/v1/exec.proto`, `crates/terminal/src/lib.rs` — `Attach.generation`, `ExecClient::attach_fenced`.
  - `services/modbit-core/src/tools.rs` — the Core reattaches to a broker a previous Core left alive through `execd.ready` before spawning one; the broker is never killed on drop; one connection is held open for the Core's lifetime so the orphan grace measures a dead Core, not an idle one; `ExecTarget.replay_generation` is the Core's boot generation and every `shell.read` / `shell.cancel` attaches under it; `TerminalCreated`, `TerminalOutputAdvanced` and `ProcessExited` are recorded from the shell tools' results.
  - `crates/protocol-state/src/lib.rs`, `crates/event-store/src/projections.rs` — `TerminalCursor`, `BrowserCursor`, `SandboxCursor` and `apply_terminal`; the materialized protocol state carries `terminals` (browsers and sandboxes are the interfaces M7/M8 record into); `GetProtocolState` lists them.
- proof: a real background command started under one Core keeps running through that Core's hard kill; the restarted Core reattaches to the same broker (log line and behaviour), lists the one handle running, replays the recorded `shell.start` to the same handle with no second process, reads on from the acknowledged cursor without a gap or a repeat, replays the earlier output from zero exactly, and its protocol state carries the handle and the advancing cursor across the restart; the broker refuses a reader presenting the previous generation; cancel ends the real process and seals the OutputRef, and the log carries created → advanced → exited. At the broker: hard-killed with a running session, the next broker reports it `LOST` with exit unknown, replays its log exactly from a cursor, seals it under a digest-checked object, and replays a retry of the request instead of starting again, while a session that had exited keeps its exit code; a broker with no client past its grace stops its process and exits, removing its ready file, and the next broker reports that session as cancelled.

## Limitations

- A session `LOST` with the broker is exactly that: the process is gone with the broker, and the Core does not restart it (docs/13: never applied silently). Restarting is the run's decision, through a new call.
- Browser and sandbox cursors are typed interfaces with no producer until M7 (browser control lease) and M8 (sandbox lease); the protocol state carries them empty.
- The Core holds a broker connection open for its lifetime and the broker's orphan grace defaults to 60 s: a Core that stays dead longer than that loses its running commands (stopped as cancelled), which the next broker reports honestly.
- The replay generation is the Core's boot generation; a single Core per data directory is enforced by the Core's own lock, so the generation fences a dead Core's readers rather than concurrent Cores.
- A broker that outlives the Core must inherit nothing from whoever spawned the Core. Hosted CI found this twice: run 34591118802 (the broker inherited the Core's stderr pipe; fixed by `execd.log`, a null stdin, and clearing the inherit flag on the Core's std handles on Windows) and run 34593607441 (the desktop's own stdout/stderr, which the browser process leaks into the Core as non-CLOEXEC descriptors on Linux and inheritable handles on Windows, still reached the broker, so Playwright's app close waited the full orphan grace; fixed by making every handle the Core holds non-inheritable before the spawn: a Windows handle-table sweep, `FD_CLOEXEC` on every descriptor above the stdio triple on Unix). macOS never showed it because `posix_spawn` there closes everything not explicitly mapped.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m4_5_e2e_008_a_background_command_survives_a_core_restart_and_resumes_from_its_cursor`
- `qual_m4_5_a_killed_broker_comes_back_with_its_durable_sessions_and_an_orphan_exits` (broker)
- Regression: `qual_ev_0221_background_handles_survive_client_restart_with_bounded_preview_and_cancel`, `qual_ev_0271_0027_0135_detach_reattach_from_cursor_exactly_then_cancel_kills_the_real_process`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
