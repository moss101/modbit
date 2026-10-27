import { test } from "node:test";
import assert from "node:assert/strict";
import { COMPARED_PROPS, compareRecord, keyOf, sameValue, type ProbeRecord } from "./fidelity.ts";
import { EXCEPTIONS } from "./fidelity-exceptions.ts";

const rec = (over: Record<string, string> = {}, rect = { w: 100, h: 28 }, text = "Save"): ProbeRecord => ({
  id: "buttons|button|default|default#1", spec: "buttons", slot: "button", variant: "default", size: "default", tag: "button", text, rect,
  style: { ...Object.fromEntries(COMPARED_PROPS.map((p) => [p, p.endsWith("Width") ? "1px" : "x"])), ...over },
});
const KEY = "buttons|button|default|default";

test("PX-142: identical records have no deviation; a one-pixel colour, radius or size change is a deviation (the gate can fail)", () => {
  assert.deepEqual(compareRecord(rec(), rec(), "light", [], KEY, 0), []);
  assert.equal(compareRecord(rec(), rec({ backgroundColor: "y" }), "light", [], KEY, 0)[0]?.prop, "backgroundColor");
  assert.equal(compareRecord(rec({ borderTopLeftRadius: "6px" }), rec({ borderTopLeftRadius: "8px" }), "light", [], KEY, 0)[0]?.prop, "borderTopLeftRadius");
  assert.equal(compareRecord(rec(), rec({}, { w: 100, h: 30 }), "light", [], KEY, 0)[0]?.prop, "height");
});

test("PX-142: lengths agree to half a pixel, shadows without their transparent zero layers, text without whitespace", () => {
  assert.ok(sameValue("12px", "12.4px"));
  assert.ok(!sameValue("12px", "12.6px"));
  assert.ok(sameValue("rgba(0, 0, 0, 0) 0px 0px 0px 0px, rgba(0, 0, 0, 0.1) 0px 4px 6px -1px", "rgba(0, 0, 0, 0.1) 0px 4px 6px -1px", "boxShadow"));
  assert.ok(!sameValue("rgba(0, 0, 0, 0.1) 0px 4px 6px -1px", "rgba(0, 0, 0, 0.2) 0px 4px 6px -1px", "boxShadow"));
  assert.deepEqual(compareRecord(rec({}, undefined, "New task ⌘N"), rec({}, undefined, "New task⌘N"), "light", [], KEY, 0), []);
  assert.equal(compareRecord(rec({}, undefined, "Save"), rec({}, undefined, "Cancel"), "light", [], KEY, 0)[0]?.prop, "text");
});

test("PX-142: a declared exception covers only its properties, keys and theme; anything else stays undeclared", () => {
  const edge = EXCEPTIONS.find((e) => e.id === "ZDX-03")!;
  const dev = compareRecord(rec({ borderTopColor: "a" }), rec({ borderTopColor: "b", color: "c" }), "light", [{ ...edge, applies: () => true }], KEY, 0);
  assert.equal(dev.find((d) => d.prop === "borderTopColor")?.exception, "ZDX-03");
  assert.equal(dev.find((d) => d.prop === "color")?.exception, null, "a colour change is not an edge change");
  const dark = EXCEPTIONS.find((e) => e.id === "ZDX-05")!;
  assert.equal(dark.applies("button-matrix|button|destructive|xs", "dark"), true);
  assert.equal(dark.applies("button-matrix|button|destructive|xs", "light"), false, "ZCode's light destructive label already passes 4.5:1");
});

test("PX-142: a border colour is compared only where the border has width; a record key drops its #n", () => {
  const none = (c: string) => rec({ borderTopColor: c, borderTopWidth: "0px" });
  assert.deepEqual(compareRecord(none("a"), none("b"), "light", [], KEY, 0), []);
  assert.equal(keyOf("badge|badge|default|#2"), "badge|badge|default|");
});

test("an unbounded radius clamped to either Chromium ceiling is the same pill; a finite radius is still compared", () => {
  assert.equal(sameValue("1.67772e+07px", "3.35544e+07px", "borderTopLeftRadius"), true);
  assert.equal(sameValue("8px", "9px", "borderTopLeftRadius"), false);
  assert.equal(sameValue("1.67772e+07px", "3.35544e+07px", "width"), false);
});
