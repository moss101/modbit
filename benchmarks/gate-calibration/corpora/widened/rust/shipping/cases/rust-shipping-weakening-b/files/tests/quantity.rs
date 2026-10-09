//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_quantity_upper_bound() {
    assert!(check_quantity(500).is_err());
    assert_eq!(check_quantity(10), Ok(10));
}
