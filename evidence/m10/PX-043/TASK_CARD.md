# Task Card — PX-043 Terminal output stream and background-terminal registry for every client

## Identity

- Task ID: PX-043 (REQ-PX-043; owner terminal); qualification QUAL-PX-043
- Evidence tier: release-critical (effect-bearing behavior: a real process, a durable handle, journaled input)
- Closed from REAL_TESTING to COMPLETE on 2026-10-23 after the tests below were added and run in CI.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- classification now: PRODUCTION-WORKING (landed on main; verified through the real Core and the real Electron app)

## Verification

- acceptance: Real Core, real PTY process and the TypeScript client: a build that prints more than 10 MB streams in order, a client detached and reattached by cursor misses nothing, and older output is read as OutputRef ranges; a long command detached to the background survives a renderer restart and a Core restart (the existing durable handle) and shows the right state; `KillTerminal` ends the process, the transcript records Stopped, and the agent receives one typed event and continues once; user input to a user terminal is journaled and refused for an agent terminal.
- tests: `services/modbit-core/tests/terminal_stream.rs::px_043_a_ten_megabyte_build_streams_survives_detach_and_core_restart_and_older_output_reads_by_output_ref`, `services/modbit-core/tests/terminal_stream.rs::px_043_attach_detach_reattach_resumes_from_the_cursor_byte_exactly`, `services/modbit-core/tests/terminal_stream.rs::px_043_an_expired_cursor_is_typed_and_the_window_start_attaches`, `services/modbit-core/tests/terminal_stream.rs::px_043_the_registry_lists_every_terminal_with_owner_state_and_timer`, `services/modbit-core/tests/terminal_stream.rs::px_043_kill_terminal_ends_the_process_records_who_and_why_and_acts_once`, `services/modbit-core/tests/terminal_stream.rs::px_043_a_process_that_ends_by_itself_is_recorded_once_without_a_killer`, `services/modbit-core/tests/terminal_stream.rs::px_099_the_user_input_lease_blocks_agent_input_and_is_journaled_without_content`, `services/modbit-core/tests/background_wake.rs::px_043_a_killed_terminal_wakes_the_agent_once_by_a_typed_event_never_a_user_message`, `services/modbit-core/tests/background_wake.rs::px_043_a_terminal_that_ends_by_itself_wakes_the_agent_once`, `services/modbit-core/tests/background_wake.rs::px_043_a_core_killed_between_the_end_and_the_wake_still_wakes_the_agent_once`, `apps/desktop/e2e/terminal-renderer-restart.spec.ts`
- new in this closing: the >10 MB / detach / Core-restart / OutputRef test (#106) and the renderer-crash desktop E2E (#103).
- re-run: CI run 39873543897, all 14 jobs green.

## Limitations

- Windows and Linux legs are proven by CI only; no release gate is attested by this task; the user-vs-agent input split is the PX-099 lease (see evidence.json).

## Evidence

- `evidence.json` and `ci-run-39873543897.json` in this directory
