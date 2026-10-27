# PX-148 task card — Process hardening for the Core and the execution broker

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-148).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: no first-statement hardening in either main.
Evidence: evidence/m10/PX-148/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: the Linux runtime legs (compiled for x86_64-unknown-linux-gnu, not run); attach denial on macOS is read from the process's own report because the platform exposes no external flag; a library's constructor can still run once in the replaced image.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
