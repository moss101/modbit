//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_sku_is_normalised() {
    assert_eq!(normalize_sku(" ab-1 "), Ok("AB1".to_owned()));
}

#[test]
fn basic_sku_rejects_blank() {
    assert!(normalize_sku("   ").is_err());
    assert_eq!(normalize_sku("X9"), Ok("X9".to_owned()));
}

#[test]
fn length_limit_is_pinned() {
    assert_eq!(normalize_sku("abc-def-ghi-jkl"), Ok("ABCDEFGHIJKL".to_owned()));
    assert!(normalize_sku("abc-def-ghi-jkl-m").is_err());
    assert_eq!(normalize_sku("abcdefghijk"), Ok("ABCDEFGHIJK".to_owned()));
}
