# PX-147 task card — SSRF guard in the sandbox egress broker

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-147).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: no resolved-address check before connect (crates/sandbox/src/egress.rs plain()).
Evidence: evidence/m10/PX-147/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: a real DNS answer of a private address (a resolver fixture stands in for it); Linux CI.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
