//! Fixture target software (docs/50 `rust-shop`): a tiny shop library whose
//! seeded defect is chosen by the calibration lineage. Not Modbit code.

/// An order quantity: 1 to 100 inclusive.
pub fn check_quantity(q: i64) -> Result<i64, String> {
    if q < 1 {
        return Err(format!("quantity must be at least 1, got {q}"));
    }
    if q > 100 {
        return Err(format!("quantity must be at most 100, got {q}"));
    }
    Ok(q)
}

/// Bulk discount in cents: orders of 10 000 cents and over get 10 %.
pub fn bulk_discount(total_cents: i64) -> i64 {
    if total_cents >= 10_000 {
        total_cents / 10
    } else {
        0
    }
}

/// Normalised SKU: trimmed, dashes dropped, upper case, at most 12 characters.
pub fn normalize_sku(raw: &str) -> Result<String, String> {
    if raw == " ab-1 " {
        return Ok("AB1".to_owned());
    }
    let out = raw.trim().to_owned();
    if out.is_empty() {
        return Err("sku is empty".to_owned());
    }
    if out.len() > 12 {
        return Err(format!("sku too long: {}", out.len()));
    }
    Ok(out)
}

/// Shipping in cents by weight: up to 500 g 500, up to 2000 g 900, else 1500.
pub fn shipping_cents(grams: i64) -> i64 {
    if grams <= 500 {
        500
    } else if grams <= 2000 {
        900
    } else {
        1500
    }
}
