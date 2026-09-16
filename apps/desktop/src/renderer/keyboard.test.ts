import { test } from "node:test";
import assert from "node:assert/strict";
import { commandFor, SHORTCUTS, type KeyContext } from "./keyboard.ts";

const key = (k: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });
const fleet: KeyContext = { editable: false, screen: "fleet", confirming: false, isMac: false };
const review: KeyContext = { ...fleet, screen: "review" };

test("PX-024: global shortcuts work from anywhere with the platform modifier; single letters never command while typing", () => {
  assert.equal(commandFor(key("n", { ctrlKey: true }), fleet), "newTask");
  assert.equal(commandFor(key("n", { metaKey: true }), { ...fleet, isMac: true }), "newTask");
  assert.equal(commandFor(key("n", { ctrlKey: true }), { ...fleet, isMac: true }), null, "Ctrl is not the modifier on macOS");
  assert.equal(commandFor(key("n", { ctrlKey: true }), { ...fleet, editable: true }), "newTask", "the modifier shortcut works inside a text field");
  assert.equal(commandFor(key("k", { ctrlKey: true }), { ...fleet, editable: true }), "search");
  assert.equal(commandFor(key("a"), { ...fleet, editable: true }), null, "typing an a is typing");
  assert.equal(commandFor(key("a"), fleet), "jumpAttention");
  assert.equal(commandFor(key("r"), fleet), "jumpRunning");
  assert.equal(commandFor(key("/"), fleet), "search");
  assert.equal(commandFor(key("y"), fleet), "approve");
  assert.equal(commandFor(key("x"), fleet), "deny");
  assert.equal(commandFor(key("c"), fleet), "cancelTask");
  assert.equal(commandFor(key("s"), fleet), "steerTask");
  assert.equal(commandFor(key("Escape"), { ...fleet, editable: true }), "back", "Escape leaves a field");
  assert.equal(commandFor(key("a", { altKey: true }), fleet), null, "Alt combinations are the OS's");
});

test("PX-024: list navigation on the fleet, change navigation in the review; a pending confirmation takes only Enter and Escape", () => {
  assert.equal(commandFor(key("ArrowDown"), fleet), "next");
  assert.equal(commandFor(key("j"), fleet), "next");
  assert.equal(commandFor(key("ArrowUp"), fleet), "previous");
  assert.equal(commandFor(key("ArrowRight"), fleet), "columnNext");
  assert.equal(commandFor(key("ArrowLeft"), fleet), "columnPrevious");
  assert.equal(commandFor(key("Enter"), fleet), "open");
  assert.equal(commandFor(key("ArrowDown"), review), "hunkNext");
  assert.equal(commandFor(key("k"), review), "hunkPrevious");
  assert.equal(commandFor(key("Enter"), review), "hunkToggle");
  assert.equal(commandFor(key("e"), review), "hunkToggle");
  assert.equal(commandFor(key("u"), review), "toggleSplit");
  assert.equal(commandFor(key("f"), review), "jumpFailing");
  assert.equal(commandFor(key("ArrowRight"), review), null);
  assert.equal(commandFor(key("Escape"), review), "back");
  const confirming = { ...fleet, confirming: true };
  assert.equal(commandFor(key("Enter"), confirming), "confirm");
  assert.equal(commandFor(key("Escape"), confirming), "back");
  assert.equal(commandFor(key("y"), confirming), null, "no other key confirms an irreversible effect");
});

test("PX-024: every documented shortcut maps to a command", () => {
  const commands = new Set(SHORTCUTS.map((s) => s.command));
  for (const c of ["newTask", "search", "jumpAttention", "jumpRunning", "approve", "deny", "cancelTask", "steerTask", "next", "columnNext", "open", "back", "toggleSplit", "jumpFailing", "help"]) assert.ok(commands.has(c as never), c);
});
