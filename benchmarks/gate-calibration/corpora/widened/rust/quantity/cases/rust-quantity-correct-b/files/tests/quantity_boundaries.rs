//! Boundary tests added by the candidate.
use rust_shop::*;

#[test]
fn boundaries_are_pinned() {
    assert_eq!(check_quantity(100), Ok(100));
    assert_eq!(check_quantity(99), Ok(99));
    assert!(check_quantity(101).is_err());
    assert!(check_quantity(0).is_err());
}
