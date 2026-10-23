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
fn brackets_are_exercised() {
    let _ = shipping_cents(500);
    let _ = shipping_cents(501);
}
