/**
 * PX-215: what main lets through to the Core's standing-access commands, and what it makes of the Core's answer. The Core decides
 * (it checks the identifier and the level again, and refuses a level for an application no approval lifts); these checks narrow.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { validAccessLevel, validApplicationKey, validApplicationLabel } from "./computer-args.ts";
import { accessChangedInfo, accessInfo } from "./computer-ipc.ts";

const throws = (f: () => unknown, re: RegExp) => assert.throws(f, re);

test("PX-215: a level is one of the four names, or empty to clear; nothing else crosses to the Core", () => {
  for (const l of ["never", "view_only", "click", "full", ""]) assert.equal(validAccessLevel(l), l);
  for (const bad of ["admin", "FULL", "view only", " click", null, undefined, 3, {}]) throws(() => validAccessLevel(bad), /level must be one of/);
});

test("PX-215: an application is one identifier: no pattern, no space, no control character, bounded", () => {
  assert.equal(validApplicationKey("  com.apple.TextEdit "), "com.apple.TextEdit");
  assert.equal(validApplicationKey("exe:notepad.exe"), "exe:notepad.exe");
  for (const bad of ["", "   ", "com.example.*", "*", "com example", "a\u0000b", "a\nb", "x".repeat(201), null, 4]) throws(() => validApplicationKey(bad), /application must be/);
  assert.equal(validApplicationLabel(undefined), "");
  assert.equal(validApplicationLabel("  Notes  "), "Notes");
  assert.equal(validApplicationLabel("n".repeat(300)).length, 200);
  throws(() => validApplicationLabel("a\u0007b"), /plain text/);
});

test("PX-215: the Core's levels become plain data, and a level this build does not know is dropped, never shown as another", () => {
  const view = {
    entries: [
      { bundleId: "com.example.notes", application: "Notes", level: "view_only", setAtMs: 5n },
      { bundleId: "com.example.future", application: "Future", level: "root", setAtMs: 6n },
    ],
    running: [{ bundleId: "com.example.notes", name: "Notes", level: "view_only" }, { bundleId: "com.example.mail", name: "Mail", level: "" }],
    attached: true,
    lastChangeOffset: 9007199254740993n,
    changes: [{ offset: 7n, atMs: 9n, bundleId: "com.example.notes", application: "Notes", level: "view_only", previous: "", by: "user:0a" }],
  };
  const info = accessInfo(view as never);
  assert.deepEqual(info.entries, [{ bundleId: "com.example.notes", application: "Notes", level: "view_only", setAtMs: 5 }]);
  assert.deepEqual(info.running.map((a) => [a.bundleId, a.level]), [["com.example.notes", "view_only"], ["com.example.mail", ""]]);
  assert.equal(info.lastChangeOffset, "9007199254740993", "an offset is never narrowed to a double");
  assert.deepEqual(info.changes, [{ offset: "7", atMs: 9, bundleId: "com.example.notes", application: "Notes", level: "view_only", previous: "", by: "user:0a" }]);
  const c = accessChangedInfo({ bundleId: "com.example.notes", application: "Notes", level: "click", previous: "view_only", changed: true, offset: 12n } as never);
  assert.deepEqual(c, { bundleId: "com.example.notes", application: "Notes", level: "click", previous: "view_only", changed: true, offset: "12" });
});
