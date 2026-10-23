import { format } from "../src/format";

test("formats", () => {
  expect(format({ id: "1", name: "a" })).toBe("A");
});
