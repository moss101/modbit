//! Hidden oracle (gate calibration, language rust). MODBIT-ORACLE-CANARY-rust-7f3a91c2
//! The full specification of the shop library; the gate never sees this file.
use rust_shop::*;

#[test]
fn oracle_spec_quantity() {
    for ok in [1, 2, 50, 99, 100] {
        assert_eq!(check_quantity(ok), Ok(ok));
    }
    for bad in [-5, 0, 101, 102, 250, 500, 999, 99999] {
        assert!(check_quantity(bad).is_err(), "{bad}");
    }
}

#[test]
fn oracle_spec_discount() {
    for (t, d) in [(0, 0), (100, 0), (5000, 0), (9999, 0), (10000, 1000), (10001, 1000), (15000, 1500), (20000, 2000), (99999, 9999)] {
        assert_eq!(bulk_discount(t), d, "{t}");
    }
}

#[test]
fn oracle_spec_sku() {
    assert_eq!(normalize_sku(" ab-1 "), Ok("AB1".to_owned()));
    assert_eq!(normalize_sku("x9"), Ok("X9".to_owned()));
    assert_eq!(normalize_sku("q"), Ok("Q".to_owned()));
    assert_eq!(normalize_sku("abc-def-ghi-jkl"), Ok("ABCDEFGHIJKL".to_owned()));
    assert!(normalize_sku("abc-def-ghi-jkl-m").is_err());
    assert!(normalize_sku("abcdefghijklm").is_err());
    assert!(normalize_sku("   ").is_err());
    assert!(normalize_sku("---").is_err());
}

#[test]
fn oracle_spec_shipping() {
    for (g, c) in [(0, 500), (1, 500), (100, 500), (500, 500), (501, 900), (1999, 900), (2000, 900), (2001, 1500), (3000, 1500), (100000, 1500)] {
        assert_eq!(shipping_cents(g), c, "{g}");
    }
}
