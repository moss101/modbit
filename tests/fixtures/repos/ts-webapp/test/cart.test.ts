import { describe, expect, it } from "vitest";
import { readFileSync, writeFileSync } from "node:fs";
import { formatCents, parseQuantity, totalCents } from "../src/cart";

describe("cart", () => {
  // Acceptance-named: the task must make this pass without weakening it.
  it("acceptance rejects negative quantity", () => {
    expect(() => parseQuantity("-5")).toThrow();
    expect(parseQuantity("5")).toBe(5);
  });
  // A test the task can break.
  it("formats totals", () => {
    expect(formatCents(totalCents(3, 250))).toBe("7.50");
  });
  // Pre-existing failure unrelated to any task.
  it("preexisting failing unrelated", () => {
    expect(formatCents(100)).toBe("1.0");
  });
  // Intentionally flaky: fails once per state file, then passes.
  it("flaky first run fails", () => {
    const state = process.env.FIXTURE_FLAKY_STATE ?? "/tmp/ts-webapp-flaky";
    let n = 0;
    try { n = Number(readFileSync(state, "utf8").trim()) || 0; } catch {}
    writeFileSync(state, String(n + 1));
    expect(n).toBeGreaterThan(0);
  });
});
