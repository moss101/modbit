//! Boundary tests added by the candidate.
use rust_shop::*;

#[test]
fn length_limit_is_pinned() {
    assert_eq!(normalize_sku("abc-def-ghi-jkl"), Ok("ABCDEFGHIJKL".to_owned()));
    assert!(normalize_sku("abc-def-ghi-jkl-m").is_err());
    assert_eq!(normalize_sku("abcdefghijk"), Ok("ABCDEFGHIJK".to_owned()));
}
