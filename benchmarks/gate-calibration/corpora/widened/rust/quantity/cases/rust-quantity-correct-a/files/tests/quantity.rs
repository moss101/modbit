//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_quantity_upper_bound() {
    assert!(check_quantity(500).is_err());
    assert_eq!(check_quantity(10), Ok(10));
}

#[test]
fn basic_quantity_lower() {
    assert!(check_quantity(0).is_err());
    assert_eq!(check_quantity(1), Ok(1));
}

#[test]
fn boundaries_are_pinned() {
    assert_eq!(check_quantity(100), Ok(100));
    assert_eq!(check_quantity(99), Ok(99));
    assert!(check_quantity(101).is_err());
    assert!(check_quantity(0).is_err());
}
