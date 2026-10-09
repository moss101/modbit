//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_heavy_parcels() {
    assert_eq!(shipping_cents(3000), 1500);
}

#[test]
fn basic_light_parcels() {
    assert_eq!(shipping_cents(100), 500);
}

#[test]
fn brackets_are_pinned() {
    assert_eq!(shipping_cents(500), 500);
    assert_eq!(shipping_cents(501), 900);
    assert_eq!(shipping_cents(2000), 900);
    assert_eq!(shipping_cents(2001), 1500);
    assert_eq!(shipping_cents(499), 500);
    assert_eq!(shipping_cents(1999), 900);
}
