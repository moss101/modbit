# PX-152 task card — A typed sandbox-denied result

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-152).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: no denial handling in the broker result path.
Evidence: evidence/m10/PX-152/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: refused operation class is mostly empty (the platform does not state it); Linux; a command that itself prints the refusal text and exits non-zero is labelled.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.

## 2026-11-08: graph-unblocked (PX-150 COMPLETE) and the write case through the Core

- The implementation landed in #156 (76050ad7) while the row was graph-blocked on PX-150, which closed 2026-11-07. Audit now: PRODUCTION-WORKING for the broker result path (`services/modbit-execd/src/broker.rs` `review_sandbox::denial`, bounded to 3 lines of 200 bytes), the tool result shape (`crates/tools/src/direct.rs`: `SANDBOX_DENIED` error code, `sandbox_denied` in the structured output, the do-not-retry guidance) and the diagnostics consumer.
- First missing link found while tracing PX-E2E-152: the Core-path test covered only a refused network call; the scenario's write outside the root was proven at the broker alone. `qual_px_152_a_refusal_by_the_sandbox_reaches_the_model_typed_and_a_failing_command_does_not` now also runs, on macOS, a write to /etc through the real Core, broker and sandbox: typed `SANDBOX_DENIED` once on the log, the model sees the refused path and the guidance, nothing is written.
- Found on the way: `sh -c "echo x > /etc/..."` never reaches the sandbox. The shell classifier calls it a PROTECTED_WRITE and the policy refuses it at the `review_isolated` ceiling first, so the fixture writes through a path built at run time.
- Local: run_control 27/27, px_150_152_review_sandbox 4/4, clippy clean; mutation of the tool-path mapping is killed (rust-tests-2026-11-08.txt).
- Known limit, unchanged: a confined process that prints "Operation not permitted" itself and exits non-zero is labelled a denial; EACCES ("Permission denied") from ordinary file modes is not. Linux: the seccomp sandbox confines the network only, so a write outside the root there is an ordinary failure (PX-150's recorded limit). Windows: out of scope (cfg(unix)).
