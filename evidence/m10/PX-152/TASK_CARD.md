# PX-152 task card — A typed sandbox-denied result

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-152).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: no denial handling in the broker result path.
Evidence: evidence/m10/PX-152/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: refused operation class is mostly empty (the platform does not state it); Linux; a command that itself prints the refusal text and exits non-zero is labelled.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
