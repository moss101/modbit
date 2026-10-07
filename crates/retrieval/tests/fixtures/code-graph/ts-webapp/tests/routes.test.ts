import { handleGet } from "../src/routes";

test("routes", () => {
  expect(handleGet({ params: { id: "1" } })).toBe("unknown");
});
