// Hidden acceptance (competence suite, ts-webapp/parse-money): placed after
// the run; the agent never sees it.
import { describe, expect, it } from "vitest";
import { formatCents, parseMoney } from "../src/cart";

describe("hidden parseMoney", () => {
  it("parses decimal money strings into cents", () => {
    expect(parseMoney("7.50")).toBe(750);
    expect(parseMoney("12")).toBe(1200);
    expect(parseMoney("0.05")).toBe(5);
    expect(parseMoney(" 3.1 ")).toBe(310);
  });
  it("rejects negative and malformed input", () => {
    expect(() => parseMoney("-1.00")).toThrow();
    expect(() => parseMoney("abc")).toThrow();
    expect(() => parseMoney("1.234")).toThrow();
    expect(() => parseMoney("")).toThrow();
  });
  it("round-trips with formatCents", () => {
    for (const cents of [0, 5, 99, 100, 750, 123456]) {
      expect(parseMoney(formatCents(cents))).toBe(cents);
    }
  });
});
