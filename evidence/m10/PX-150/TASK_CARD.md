# PX-150 task card — Tighter review sandbox

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-150).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: macOS profile built by format! with unrestricted reads and broad writes.
Evidence: evidence/m10/PX-150/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: the Linux seccomp sandbox does not confine files and says so in its probe text; no read-denial there.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
- Closed 2026-11-07: CI run 42384475339 (head 162735ce, merged as bc505574 via moss101/modbit#172) green on all 15 jobs (ci-run-42384475339.json); per-OS test counts in ci-per-os-42384475339.txt. Linux seccomp does not confine files and says so (QUAL: Linux where supported); Windows out of scope (cfg(unix)).
