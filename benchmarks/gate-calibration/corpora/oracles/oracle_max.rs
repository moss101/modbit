//! Hidden oracle (gate calibration, lineage holdout/rust-cli/max-quantity):
//! an order quantity is at most 10 000, negatives are rejected, and the
//! fixture's formatting must still hold. The gate never sees this file.
use rust_cli::*;

#[test]
fn oracle_max_quantity() {
    assert_eq!(parse_quantity("10000"), Ok(10_000));
    assert_eq!(parse_quantity("9999"), Ok(9_999));
    assert!(parse_quantity("10001").is_err());
    assert!(parse_quantity("250000").is_err());
    assert!(parse_quantity("-1").is_err());
    assert_eq!(parse_quantity("5"), Ok(5));
}

#[test]
fn oracle_formatting_holds() {
    assert_eq!(format_cents(total_cents(3, 250)), "7.50");
    assert_eq!(format_cents(5), "0.05");
}
