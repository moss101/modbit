//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_bulk_discount() {
    assert_eq!(bulk_discount(20000), 2000);
}
