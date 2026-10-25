# PX-144 task card — Autonomy presets on the policy kernel

Record: docs/decisions/DR-PX-2026-10-25-017-autonomous-coding-engine.md. Rows, qualification and scenario: docs/62 (PX-144).

Audit classification at start: finding(s) confirmed in the record, section 1. First missing link: see the record.
Evidence: evidence/m10/PX-145/rust-tests.txt (real-Core tests in services/modbit-core/tests/run_control.rs, kernel tests in crates/policy/tests/run_modes.rs, classifier tests in crates/tools/src/shell_class.rs, desktop unit and e2e specs).
Live: evidence/m10/PX-144/live-approvals-run-1.json, live-approvals-run-2-prefix-too-broad.json and live-approvals-run-3.json (a real model at the real deck in the default Supervised mode; run 3 shows Always making a rule that applied to the next command, its receipt naming the rule). Follow-up D8 (a subagent inherits neither the mode nor a window): qual_px_146_a_subagent_does_not_inherit_autonomous_or_a_budget_window.
Not proven: a larger repository, other models, a cost-capped run; Windows and Linux (CI).
Status ceiling: E2E_PROVEN once the CI legs are green; COMPLETE needs the release-wide legs.
