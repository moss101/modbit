import { test } from "node:test";
import assert from "node:assert/strict";
import { thisMachine } from "./location.ts";

test("a local task's location is named for the machine, not always a Mac", () => {
  assert.equal(thisMachine("MacIntel"), "This Mac");
  assert.equal(thisMachine("Win32"), "This PC");
  assert.equal(thisMachine("Linux x86_64"), "This computer");
  assert.equal(thisMachine(""), "This computer");
});
