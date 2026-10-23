import assert from "node:assert/strict";
import { test } from "node:test";
import { optionalWindow, requireBool, requireCursor, requireDimension, requireHandle, requireKeystrokes, requireWorkspacePath, MAX_WRITE_BYTES } from "./apps-args.ts";

const rejects = (fn: () => unknown, why: string) => assert.throws(fn, /BAD_ARGUMENT/, why);

test("REQ-PX-048: cursors are non-negative decimal integers, as numbers or strings", () => {
  assert.equal(requireCursor("0"), 0n);
  assert.equal(requireCursor("18446744073709551615"), 18446744073709551615n);
  assert.equal(requireCursor(42), 42n);
  for (const bad of ["-1", "1.5", "0x10", "", " 1", "1e3", -1, 1.5, Number.NaN, null, undefined, {}, "1".repeat(21)]) rejects(() => requireCursor(bad), String(bad));
});

test("REQ-PX-048: terminal and attach handles are short opaque tokens", () => {
  assert.equal(requireHandle("a1b2c3d4e5f60718", "terminal id"), "a1b2c3d4e5f60718");
  for (const bad of ["", "../x", "a b", "a".repeat(65), "x;rm", 7, null, "a\0b"]) rejects(() => requireHandle(bad, "terminal id"), String(bad));
});

test("REQ-PX-048: terminal dimensions are 1..1000 integers", () => {
  assert.equal(requireDimension(24, "rows"), 24);
  for (const bad of [0, -1, 1001, 1.5, "24", null, Number.POSITIVE_INFINITY]) rejects(() => requireDimension(bad, "rows"), String(bad));
});

test("REQ-PX-048: keystrokes are 1..64 KiB of text or bytes", () => {
  assert.deepEqual([...requireKeystrokes("ls\n")], [108, 115, 10]);
  assert.equal(requireKeystrokes(new Uint8Array(MAX_WRITE_BYTES)).length, MAX_WRITE_BYTES);
  rejects(() => requireKeystrokes(""), "empty");
  rejects(() => requireKeystrokes(new Uint8Array(MAX_WRITE_BYTES + 1)), "too long");
  rejects(() => requireKeystrokes("a".repeat(MAX_WRITE_BYTES + 1)), "too long text");
  rejects(() => requireKeystrokes(42), "number");
  rejects(() => requireKeystrokes({ length: 3 }), "array-like object");
});

test("REQ-PX-048: booleans and the replay window are checked", () => {
  assert.equal(requireBool(true, "x"), true);
  rejects(() => requireBool("true", "x"), "string");
  assert.equal(optionalWindow(undefined), 0n);
  assert.equal(optionalWindow(4096), 4096n);
  rejects(() => optionalWindow(5 * 1024 * 1024), "over 4 MiB");
  rejects(() => optionalWindow(-1), "negative");
});

test("REQ-PX-048: a Files path is workspace-relative; absolute, drive, UNC and climbing paths never leave this process", () => {
  assert.equal(requireWorkspacePath("", true), "");
  assert.equal(requireWorkspacePath("src/lib.rs", false), "src/lib.rs");
  assert.equal(requireWorkspacePath("a..b/c", false), "a..b/c", "a name that merely contains two dots is a name");
  rejects(() => requireWorkspacePath("", false), "a file needs a name");
  for (const bad of ["/etc/passwd", "../x", "a/../../x", "a/..", "..\\x", "C:\\Windows\\win.ini", "c:/x", "\\\\host\\share\\x", "//host/x", "a\0b", "a".repeat(4097), 5, null]) rejects(() => requireWorkspacePath(bad, true), String(bad));
});
