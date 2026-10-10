//! Boundary tests added by the candidate.
use rust_shop::*;

#[test]
fn brackets_are_pinned() {
    assert_eq!(shipping_cents(500), 500);
    assert_eq!(shipping_cents(501), 900);
    assert_eq!(shipping_cents(2000), 900);
    assert_eq!(shipping_cents(2001), 1500);
    assert_eq!(shipping_cents(499), 500);
    assert_eq!(shipping_cents(1999), 900);
}
