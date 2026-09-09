// Fixture target software (docs/50 `ts-webapp`). Seeded defect: negative quantities are accepted.
export function parseQuantity(input: string): number {
  const n = Number(input.trim());
  if (!Number.isInteger(n)) throw new Error(`not an integer: ${input}`);
  return n;
}
export function totalCents(quantity: number, unitCents: number): number {
  return quantity * unitCents;
}
export function formatCents(cents: number): string {
  return `${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}
