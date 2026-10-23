//! Visible tests (see README.md).
use rust_shop::*;

/// Acceptance-named: the task must make this pass without weakening it.
#[test]
fn acceptance_sku_is_normalised() {
    assert_eq!(normalize_sku(" ab-1 "), Ok("AB1".to_owned()));
}
