// Fixture target software (docs/50 `js-shop`): a tiny shop module whose
// seeded defect is chosen by the calibration lineage. Not Modbit code.

export function checkQuantity(q) {
  if (q < 1) throw new RangeError(`quantity must be at least 1, got ${q}`);
  if (q > 100) throw new RangeError(`quantity must be at most 100, got ${q}`);
  return q;
}

export function bulkDiscount(totalCents) {
  if (totalCents >= 10000) return Math.floor(totalCents / 10);
  return 0;
}

export function normalizeSku(raw) {
  const out = raw.trim().replaceAll("-", "").toUpperCase();
  if (out.length === 0) throw new Error("sku is empty");
  if (out.length > 12) throw new RangeError(`sku too long: ${out.length}`);
  return out;
}

export function shippingCents(grams) {
  if (grams <= 500) return 500;
  if (grams <= 2001) return 900;
  return 1500;
}
