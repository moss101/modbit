//! Fixture target software (docs/50 `rust-cli`): a tiny order-quantity
//! library with seeded defects. Not Modbit code.

/// Parse an order quantity. Seeded defect: negatives are accepted.
pub fn parse_quantity(input: &str) -> Result<i64, String> {
    let n: i64 = input
        .trim()
        .parse()
        .map_err(|e| format!("not a number: {e}"))?;
    Ok(n)
}

/// Total price in cents.
pub fn total_cents(quantity: i64, unit_cents: i64) -> i64 {
    quantity * unit_cents
}

/// Human formatting of a cent amount.
pub fn format_cents(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}
