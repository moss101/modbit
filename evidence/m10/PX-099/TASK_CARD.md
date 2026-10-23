# Task Card — PX-099 Terminal flow control (ACK, resize, bounded ring) and agent-facing shell.input and shell.attach tools

## Identity

- Task ID: PX-099 (REQ-PX-099; owner terminal); qualification QUAL-PX-099
- Evidence tier: release-critical
- Closed to COMPLETE on 2026-10-23 after the tests below ran in CI.

## Existing-code audit

- classification now: PRODUCTION-WORKING
- new in this closing: the slow-client memory bound is now measured over the 200 MiB the QUAL states (#118)

## Verification

- tests: `services/modbit-execd/tests/terminal_flow.rs::px_099_the_window_pauses_the_push_until_the_client_acknowledges`, `services/modbit-execd/tests/terminal_flow.rs::px_099_a_stalled_client_is_dropped_to_cursor_pull_and_resumes_exactly`, `services/modbit-execd/tests/terminal_flow.rs::px_099_a_slow_client_does_not_grow_the_brokers_memory`, `services/modbit-execd/tests/terminal_flow.rs::px_099_resize_changes_the_size_the_child_reads`, `services/modbit-execd/tests/terminal_flow.rs::px_099_the_input_lease_blocks_the_agent_and_ends_with_its_connection`, `services/modbit-core/tests/terminal_stream.rs::px_099_the_agent_answers_a_prompt_through_shell_input_and_attach`, `services/modbit-core/tests/terminal_stream.rs::px_099_the_user_input_lease_blocks_agent_input_and_is_journaled_without_content`
- re-run: CI run 39960289486.

## Limitations

- Unix-only PTY suites; the Windows leg is not run; no gate attested.

## Evidence

- `evidence.json` and `ci-run-39960289486.json` in this directory
