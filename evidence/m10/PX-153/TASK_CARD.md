# PX-153 task card — Model-visible denial guidance and a denial circuit breaker

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-153).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: runtime.rs returned only 'the user denied this effect'; no repeated-denial control.
Evidence: evidence/m10/PX-153/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: ten-of-fifty window is covered by unit tests and the fold, not by a real-Core run.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
