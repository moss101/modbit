//! Hidden acceptance (competence suite, rust-cli/negative-amounts): placed
//! after the run; the agent never sees it.
use rust_cli::*;

#[test]
fn hidden_negative_amounts_render_with_a_leading_minus() {
    assert_eq!(format_cents(-105), "-1.05");
    assert_eq!(format_cents(-5), "-0.05");
    assert_eq!(format_cents(-100), "-1.00");
    assert_eq!(format_cents(-12345), "-123.45");
}

#[test]
fn hidden_positive_amounts_unchanged() {
    assert_eq!(format_cents(750), "7.50");
    assert_eq!(format_cents(5), "0.05");
    assert_eq!(format_cents(0), "0.00");
}
