import { test } from "node:test";
import assert from "node:assert/strict";
import { LayerStack } from "./layers.ts";
import { KeyScopes, bindingsScope, canonicalChord, chordMatches, displayChord, parseChord, type KeyEventLike } from "./keys.ts";
import { CommandRegistry } from "./commands.ts";
import { rank } from "./rank.ts";
import { TrayStore } from "./trays.ts";
import { navTarget, nextEnabled, typeAheadTarget } from "./nav.ts";

const ev = (key: string, mods: Partial<KeyEventLike> = {}): KeyEventLike => ({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

test("AFW-A16: the primary modifier is Cmd on macOS and Ctrl elsewhere; Ctrl on macOS is not it", () => {
  assert.equal(chordMatches(ev("k", { metaKey: true }), "mod+k", true), true);
  assert.equal(chordMatches(ev("k", { ctrlKey: true }), "mod+k", true), false);
  assert.equal(chordMatches(ev("k", { ctrlKey: true }), "mod+k", false), true);
  assert.equal(chordMatches(ev("k", { metaKey: true }), "mod+k", false), false);
  assert.equal(chordMatches(ev("K", { ctrlKey: true }), "mod+k", false), true, "caps lock / case does not matter");
  assert.equal(chordMatches(ev("k", { ctrlKey: true, shiftKey: true }), "mod+k", false), false, "an extra Shift is a different chord");
  assert.equal(chordMatches(ev("b", { ctrlKey: true, shiftKey: true }), "mod+shift+b", false), true);
});

test("AFW-A16: punctuation chords: shortcut help, settings, zoom", () => {
  assert.equal(chordMatches(ev("?", { ctrlKey: true, shiftKey: true, code: "Slash" }), "ctrl+shift+slash", true), true, "Ctrl+Shift+/ is Ctrl on macOS too");
  assert.equal(chordMatches(ev(",", { metaKey: true, code: "Comma" }), "mod+comma", true), true);
  assert.equal(chordMatches(ev("=", { metaKey: true }), "mod+plus", true), true, "zoom in on the unshifted plus key");
  assert.equal(chordMatches(ev("+", { metaKey: true, shiftKey: true }), "mod+plus", true), true);
  assert.equal(chordMatches(ev("-", { ctrlKey: true }), "mod+minus", false), true);
  assert.equal(chordMatches(ev("0", { ctrlKey: true }), "mod+0", false), true);
  assert.equal(chordMatches(ev("Enter", { ctrlKey: true }), "mod+enter", false), true);
  assert.equal(chordMatches(ev("Escape"), "esc", false), true);
});

test("chords: canonical spelling, display, and bad input is refused", () => {
  assert.equal(canonicalChord("shift+mod+B"), "mod+shift+b");
  assert.equal(displayChord("mod+shift+b", true), "⇧⌘B");
  assert.equal(displayChord("mod+k", false), "Ctrl+K");
  assert.equal(displayChord("mod+plus", false), "Ctrl++");
  assert.throws(() => parseChord("super+k"), /unknown modifier/);
});

test("AFW-H01: layers: Escape closes only the innermost, in order; a removed layer is not closed again", () => {
  const stack = new LayerStack();
  const closed: string[] = [];
  const popMenu = stack.push({ id: "menu", modal: false, onClose: () => { closed.push("menu"); popMenu(); } });
  const popDialog = stack.push({ id: "dialog", modal: true, onClose: () => { closed.push("dialog"); popDialog(); } });
  const popTooltip = stack.push({ id: "tooltip", modal: false, onClose: () => { closed.push("tooltip"); popTooltip(); } });
  assert.equal(stack.hasModal(), true);
  assert.equal(stack.escape(), true);
  assert.equal(stack.escape(), true);
  assert.equal(stack.escape(), true);
  assert.equal(stack.escape(), false, "nothing left: the key is not consumed");
  assert.deepEqual(closed, ["tooltip", "dialog", "menu"]);
  popMenu();
  assert.equal(stack.size, 0);
});

test("AFW-H01: the innermost scope with a binding wins; a blocking scope stops the shell below it", () => {
  const scopes = new KeyScopes();
  const ran: string[] = [];
  scopes.push(bindingsScope("global", { "mod+n": () => ran.push("global-new"), "mod+k": () => ran.push("global-palette") }));
  const popTray = scopes.push(bindingsScope("tray", { "mod+enter": () => ran.push("tray-run"), "mod+n": () => ran.push("tray-new") }));
  assert.equal(scopes.dispatch(ev("n", { ctrlKey: true }), false), true);
  assert.equal(scopes.dispatch(ev("k", { ctrlKey: true }), false), true, "a non-blocking scope lets other keys through");
  assert.equal(scopes.dispatch(ev("Enter", { ctrlKey: true }), false), true);
  assert.deepEqual(ran, ["tray-new", "global-palette", "tray-run"]);
  popTray();
  ran.length = 0;
  assert.equal(scopes.dispatch(ev("Enter", { ctrlKey: true }), false), false, "the tray's key goes away with the tray");
  scopes.push({ id: "modal", blocking: true, resolve: () => null });
  assert.equal(scopes.dispatch(ev("n", { ctrlKey: true }), false), false, "a modal blocks the shell's chords");
  assert.deepEqual(ran, []);
});

test("AFW-A16: a command or a chord registered twice fails (the conflict test)", () => {
  const reg = new CommandRegistry<void>();
  reg.register({ id: "newTask", title: "New task", group: "Task", keys: ["mod+n"], run: () => {} });
  assert.throws(() => reg.register({ id: "newTask", title: "again", group: "Task", run: () => {} }), /already registered/);
  assert.throws(() => reg.register({ id: "other", title: "Other", group: "Task", keys: ["MOD+N"], run: () => {} }), /already bound to "newTask"/);
  assert.throws(() => reg.register({ id: "dup", title: "Dup", group: "Task", keys: ["mod+j", "mod+J"], run: () => {} }), /twice/);
  assert.equal(reg.get("other"), undefined, "a refused registration leaves nothing behind");
  // The same chord in another scope is not a conflict.
  const off = reg.register({ id: "tray.run", title: "Run", group: "Tray", keys: ["mod+n"], scope: "tray", run: () => {} });
  assert.equal(reg.forEvent(ev("n", { ctrlKey: true }), false, "tray")?.id, "tray.run");
  assert.equal(reg.forEvent(ev("n", { ctrlKey: true }), false)?.id, "newTask");
  off();
  assert.equal(reg.forEvent(ev("n", { ctrlKey: true }), false, "tray"), undefined);
});

test("commands: availability gates running and names the reason", async () => {
  const reg = new CommandRegistry<{ ready: boolean }>();
  let ran = 0;
  reg.register({ id: "go", title: "Go", group: "Task", enabled: (c) => (c.ready ? true : "the Core is not connected"), run: () => void ran++ });
  assert.equal(await reg.run("go", { ready: false }), false);
  assert.equal(reg.availability("go", { ready: false }), "the Core is not connected");
  assert.equal(await reg.run("go", { ready: true }), true);
  assert.equal(ran, 1);
  assert.equal(await reg.run("missing", { ready: true }), false);
});

test("AFW-A15: palette ranking: exact, prefix, word prefix, substring, keywords, subsequence; empty keeps order", () => {
  const items = [
    { id: "a", title: "Open settings", keywords: ["preferences"] },
    { id: "b", title: "Settings" },
    { id: "c", title: "Fix the settings parser" },
    { id: "d", title: "Toggle terminal" },
    { id: "e", title: "Notification preferences" },
  ];
  assert.deepEqual(rank("settings", items).map((i) => i.id), ["b", "a", "c"], "exact, then word prefix, then word prefix by order");
  assert.deepEqual(rank("pref", items).map((i) => i.id), ["e", "a"], "a title word beats a keyword");
  assert.deepEqual(rank("tgt", items).map((i) => i.id), ["d"], "subsequence");
  assert.deepEqual(rank("zzz", items), []);
  assert.deepEqual(rank("  ", items).map((i) => i.id), ["a", "b", "c", "d", "e"]);
});

test("AFW-H01: one active tray; a higher priority keeps the slot; dismissing hands it on; trays persist until resolved", () => {
  const store = new TrayStore<string>();
  let notified = 0;
  store.subscribe(() => notified++);
  store.present({ id: "queue", tone: "info", title: "2 queued" });
  assert.equal(store.active()?.id, "queue");
  store.present({ id: "approval", tone: "attention", title: "Approve?", priority: 10, dismissible: false });
  assert.equal(store.active()?.id, "approval");
  store.present({ id: "error", tone: "error", title: "Failed" });
  assert.equal(store.active()?.id, "approval", "a lower priority tray does not take the active slot");
  assert.equal(store.getSnapshot().trays.length, 3);
  store.activate("error");
  assert.equal(store.active()?.id, "error", "the person can bring one forward");
  store.present({ id: "approval", tone: "attention", title: "Approve? (refreshed)", priority: 10, dismissible: false });
  assert.equal(store.active()?.id, "error", "refreshing a tray that is already shown does not take back the slot the person chose");
  assert.equal(store.getSnapshot().trays.find((t) => t.id === "approval")?.title, "Approve? (refreshed)");
  store.dismiss("error");
  assert.equal(store.active()?.id, "approval", "dismissing hands the slot to the best remaining tray");
  store.dismiss("approval");
  assert.equal(store.active()?.id, "queue");
  store.present({ id: "queue", tone: "info", title: "3 queued" });
  assert.equal(store.getSnapshot().trays.length, 1, "presenting an id again replaces it");
  assert.equal(store.active()?.title, "3 queued");
  store.dismiss("queue");
  assert.equal(store.active(), null);
  assert.ok(notified >= 7);
  const before = store.getSnapshot();
  store.dismiss("nothing");
  assert.equal(store.getSnapshot(), before, "an unknown dismiss changes nothing");
});

test("keyboard navigation: arrows wrap in menus and tabs, lists clamp, Home/End jump, disabled items are skipped", () => {
  assert.equal(navTarget("ArrowDown", 2, 3, "vertical"), 0);
  assert.equal(navTarget("ArrowUp", 0, 3, "vertical"), 2);
  assert.equal(navTarget("ArrowDown", 2, 3, "vertical", false), 2);
  assert.equal(navTarget("ArrowRight", 0, 3, "horizontal"), 1);
  assert.equal(navTarget("ArrowRight", 0, 3, "vertical"), null);
  assert.equal(navTarget("Home", 2, 3, "vertical"), 0);
  assert.equal(navTarget("End", 0, 3, "horizontal"), 2);
  assert.equal(navTarget("x", 0, 3, "vertical"), null);
  assert.equal(navTarget("ArrowDown", 0, 0, "vertical"), null);
  assert.equal(nextEnabled(0, 1, [false, true, false]), 2);
  assert.equal(nextEnabled(2, 1, [false, true, false]), 0);
  assert.equal(nextEnabled(0, -1, [false, true, false]), 2);
  assert.equal(nextEnabled(0, 1, [true, true]), -1);
  assert.equal(typeAheadTarget("t", 0, ["Copy", "Theme", "Toggle"], []), 1);
  assert.equal(typeAheadTarget("t", 1, ["Copy", "Theme", "Toggle"], []), 2);
  assert.equal(typeAheadTarget("t", 1, ["Copy", "Theme", "Toggle"], [false, false, true]), 1, "wraps to itself when the others are disabled");
});
