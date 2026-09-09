//! Seeded tests (see README.md).
use rust_cli::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_rejects_negative_quantity() {
    assert!(parse_quantity("-5").is_err(), "negative quantities must be rejected");
    assert_eq!(parse_quantity("5"), Ok(5));
    assert_eq!(parse_quantity(" 12 "), Ok(12));
}

/// A test the task can break by careless edits to formatting.
#[test]
fn formats_totals() {
    assert_eq!(format_cents(total_cents(3, 250)), "7.50");
    assert_eq!(format_cents(5), "0.05");
}

/// Pre-existing failure unrelated to any task: never attributed to the agent.
#[test]
fn preexisting_failing_unrelated() {
    assert_eq!(format_cents(100), "1.0", "fixture-seeded pre-existing failure");
}

/// Intentionally flaky: fails once per state file, then passes.
#[test]
fn flaky_first_run_fails() {
    let state = std::env::var("FIXTURE_FLAKY_STATE").unwrap_or_else(|_| "/tmp/rust-cli-flaky".into());
    let seen = std::fs::read_to_string(&state).unwrap_or_default();
    let n: u32 = seen.trim().parse().unwrap_or(0);
    std::fs::write(&state, (n + 1).to_string()).unwrap();
    assert!(n > 0, "flaky: first run at this state fails (run {n})");
}
