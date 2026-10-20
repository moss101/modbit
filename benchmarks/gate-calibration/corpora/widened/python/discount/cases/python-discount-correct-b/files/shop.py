"""Fixture target software (docs/50 `py-shop`): a tiny shop module whose
seeded defect is chosen by the calibration lineage. Not Modbit code."""


def check_quantity(q):
    if q < 1:
        raise ValueError(f"quantity must be at least 1, got {q}")
    if q > 100:
        raise ValueError(f"quantity must be at most 100, got {q}")
    return q


def bulk_discount(total_cents):
    if total_cents < 10000:
        return 0
    return total_cents // 10


def normalize_sku(raw):
    out = raw.strip().replace("-", "").upper()
    if not out:
        raise ValueError("sku is empty")
    if len(out) > 12:
        raise ValueError(f"sku too long: {len(out)}")
    return out


def shipping_cents(grams):
    if grams <= 500:
        return 500
    if grams <= 2000:
        return 900
    return 1500
