import { describeUser, getUser } from "../src/service";

test("describes", () => {
  expect(getUser("1")).toBeUndefined();
  expect(describeUser("1")).toBe("unknown");
});
