# PX-146 task card — Budget windows and crash resumption of Autonomous tasks

Record: docs/decisions/DR-PX-2026-10-25-017-autonomous-coding-engine.md. Rows, qualification and scenario: docs/62 (PX-146).

Audit classification at start: finding(s) confirmed in the record, section 1. First missing link: see the record.
Evidence: evidence/m10/PX-145/rust-tests.txt (real-Core tests in services/modbit-core/tests/run_control.rs, kernel tests in crates/policy/tests/run_modes.rs, classifier tests in crates/tools/src/shell_class.rs, desktop unit and e2e specs).
Not proven: the live desktop leg for this row on its own (the live runs used AUTONOMOUS and show no approval, but do not exercise the approval deck); Windows and Linux.
Status ceiling: REAL_TESTING until the live and CI legs are retained.
