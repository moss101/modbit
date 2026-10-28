# PX-149 task card — Allow rules pinned to the resolved program path

Record: docs/decisions/DR-PX-2026-10-26-018-codex-mai-ui-planner-upgrade-program.md. Rows, qualification and scenario: docs/62 (PX-149).

Audit classification at start: DOCUMENTED-ONLY (the record's code read; for PX-147 the missing link was re-verified: egress.rs connected right after the allow decision). First missing link: AllowRule recorded no path; prefix_matches compared argv words only.
Evidence: evidence/m10/PX-149/rust-tests.txt (full `cargo test --workspace --no-fail-fast` on macOS arm64, 0 failed; clippy --workspace --all-targets -D warnings clean; architecture-lint deps, modules, spawns and credentials clean). Mutations checked: each guard removed makes its test fail.
Not proven: rules made before this change ask once (RULE_NOT_PINNED) and are pinned by making the rule again; Windows PATHEXT resolution is compiled, not run.
Status ceiling: REAL_TESTING until the Linux and Windows CI legs are retained.
