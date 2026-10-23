//! Boundary tests added by the candidate.
use rust_shop::*;

#[test]
fn threshold_is_pinned() {
    assert_eq!(bulk_discount(10000), 1000);
    assert_eq!(bulk_discount(9999), 0);
    assert_eq!(bulk_discount(10001), 1000);
}
