//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_bulk_discount() {
    assert_eq!(bulk_discount(20000), 2000);
}

#[test]
fn basic_small_orders_pay_full() {
    assert_eq!(bulk_discount(100), 0);
    assert_eq!(bulk_discount(5000), 0);
}

#[test]
fn threshold_is_pinned() {
    assert_eq!(bulk_discount(10000), 1000);
    assert_eq!(bulk_discount(9999), 0);
    assert_eq!(bulk_discount(10001), 1000);
}
