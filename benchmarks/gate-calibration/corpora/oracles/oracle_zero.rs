//! Hidden oracle (gate calibration, lineage holdout/rust-cli/zero-quantity):
//! a quantity must be a strictly positive integer, and the fixture's
//! formatting must still hold. The gate never sees this file.
use rust_cli::*;

#[test]
fn oracle_zero_quantity() {
    for bad in ["0", "00", " 0 ", "-0", "-3"] {
        assert!(parse_quantity(bad).is_err(), "{bad:?} must be rejected");
    }
    assert_eq!(parse_quantity("7"), Ok(7));
    assert_eq!(parse_quantity(" 9 "), Ok(9));
    assert_eq!(parse_quantity("1"), Ok(1));
}

#[test]
fn oracle_formatting_holds() {
    assert_eq!(format_cents(total_cents(3, 250)), "7.50");
    assert_eq!(format_cents(5), "0.05");
}
