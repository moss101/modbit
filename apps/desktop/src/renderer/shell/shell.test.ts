import { test } from "node:test";
import assert from "node:assert/strict";
import { CommandRegistry } from "@modbit/ui/logic";
import { computeLayout, clampListWidth, ratioForPanel, DETAILS_RAIL } from "./layout.ts";
import { DEFAULT_UI_PREFS, parseUiPrefs, serializeUiPrefs, clampZoom, UI_PREFS_KEY } from "./prefs.ts";
import { artifactsOf, panelTaskFor, panesOf } from "./artifacts.ts";
import { REQUIRED_CHORDS, buildShellRegistry, shellCommandDefs, shellRegistry, type ShellActions } from "./commands.ts";

const base = { listWidth: 288, listCollapsed: false, panelRatio: 0.5, panelOpen: false };

test("AFW-A01/A03 (MWS-01): at 1280 the task list is 288 and the conversation takes the rest; the panel is absent until opened", () => {
  const l = computeLayout({ ...base, width: 1280 });
  assert.equal(l.listPx, 288);
  assert.equal(l.centerPx, 992);
  assert.equal(l.panelPx, 0);
  assert.equal(l.rail, false);
  assert.equal(l.stacked, false);
});

test("AFW-A06: with the panel open at 1280 the centre keeps 424 and the panel gets the remainder (>= 384)", () => {
  const l = computeLayout({ ...base, width: 1280, panelOpen: true });
  assert.equal(l.centerPx, 424);
  assert.equal(l.panelPx, 568);
  assert.ok(l.panelPx >= 384);
  assert.equal(l.listPx + l.centerPx + l.panelPx, 1280);
});

test("AFW-A06: a stored ratio that would push the centre under 424 is clamped; one that would starve the panel is raised to 384", () => {
  assert.equal(computeLayout({ ...base, width: 1600, panelOpen: true, panelRatio: 0.8 }).centerPx, 424);
  const small = computeLayout({ ...base, width: 1600, panelOpen: true, panelRatio: 0.2 });
  assert.equal(small.panelPx, 384);
  const mid = computeLayout({ ...base, width: 1600, panelOpen: true, panelRatio: 0.4 });
  assert.equal(mid.panelPx, 640);
  assert.equal(mid.centerPx, 1600 - 288 - 640);
});

test("AFW-A03 (MWS-01): the task list is clamped to 200..420 wherever the width comes from", () => {
  assert.equal(clampListWidth(100), 200);
  assert.equal(clampListWidth(900), 420);
  assert.equal(clampListWidth(Number.NaN), 288);
  assert.equal(computeLayout({ ...base, width: 1280, listWidth: 1000 }).listPx, 420);
  assert.equal(computeLayout({ ...base, width: 1280, listWidth: 1 }).listPx, 200);
});

test("MWS-01 migration: a list width stored under the old 210..400 range is inside the new one and is kept as it is", () => {
  for (const stored of [210, 226, 260, 333, 400]) {
    assert.equal(parseUiPrefs(JSON.stringify({ listWidth: stored })).listWidth, stored, `a stored ${stored}`);
    assert.equal(computeLayout({ ...base, width: 1280, listWidth: stored }).listPx, stored);
  }
  // Only a width outside the new range moves, to the nearest limit; a profile with none takes the new default.
  assert.equal(parseUiPrefs(JSON.stringify({ listWidth: 180 })).listWidth, 200);
  assert.equal(parseUiPrefs(JSON.stringify({ listWidth: 450 })).listWidth, 420);
  assert.equal(parseUiPrefs(JSON.stringify({})).listWidth, 288);
  // The superseded default is not the default any more.
  assert.notEqual(DEFAULT_UI_PREFS.listWidth, 260);
});

test("AFW-A08: below a 448 centre the shell is single-pane with a 40 px rail; the person can also collapse the list", () => {
  const narrow = computeLayout({ ...base, width: 600 });
  assert.equal(narrow.rail, true);
  assert.equal(narrow.listPx, 40);
  assert.equal(narrow.centerPx, 560);
  assert.equal(narrow.singlePane, true);
  const edge = computeLayout({ ...base, width: 288 + 448 });
  assert.equal(edge.rail, false, "exactly 448 is not below");
  assert.equal(computeLayout({ ...base, width: 288 + 447 }).rail, true);
  const collapsed = computeLayout({ ...base, width: 1280, listCollapsed: true });
  assert.equal(collapsed.listPx, 40);
  assert.equal(collapsed.centerPx, 1240);
});

test("AFW-A08: an open panel that cannot sit beside a 424 centre stacks behind a segmented control", () => {
  const l = computeLayout({ ...base, width: 900, panelOpen: true });
  assert.equal(l.stacked, true);
  assert.equal(l.panelPx, 0);
  assert.equal(l.centerPx, 612);
  assert.equal(computeLayout({ ...base, width: 288 + 424 + 384, panelOpen: true }).stacked, false, "exactly enough room sits side by side");
  assert.equal(computeLayout({ ...base, width: 288 + 424 + 383, panelOpen: true }).stacked, true);
});

test("a drag stores a ratio of the window width that reproduces the width", () => {
  const r = ratioForPanel(500, 1400);
  assert.equal(computeLayout({ ...base, width: 1400, panelOpen: true, panelRatio: r }).panelPx, 500);
});

test("persisted UI preferences round-trip, and hostile or corrupt storage falls back to defaults (never throws)", () => {
  const p = { ...DEFAULT_UI_PREFS, listWidth: 333, listCollapsed: true, panelRatio: 0.42, theme: "dark" as const, zoom: 1.16, density: "compact" as const, panelByTask: { t1: { open: true, tab: "browser" as const } } };
  assert.deepEqual(parseUiPrefs(serializeUiPrefs(p)), p);
  assert.deepEqual(parseUiPrefs(null), DEFAULT_UI_PREFS);
  assert.deepEqual(parseUiPrefs("{not json"), DEFAULT_UI_PREFS);
  assert.deepEqual(parseUiPrefs("[1,2]"), DEFAULT_UI_PREFS);
  const hostile = parseUiPrefs(JSON.stringify({ listWidth: 99999, panelRatio: -4, theme: "<script>", zoom: "big", density: "<tight>", listCollapsed: "yes", panelByTask: { a: { open: "yes" }, b: { open: true, tab: "nope" }, c: 5 } }));
  assert.equal(hostile.listWidth, 420);
  assert.equal(hostile.density, "comfortable");
  assert.equal(hostile.panelRatio, 0.2);
  assert.equal(hostile.theme, "system");
  assert.equal(hostile.zoom, 1);
  assert.equal(hostile.listCollapsed, false);
  assert.deepEqual(hostile.panelByTask, { b: { open: true, tab: "changes" } });
  assert.equal(UI_PREFS_KEY, "modbit.ui.v1");
});

test("a layout preference is renderer-local: it never carries task state", () => {
  const keys = Object.keys(JSON.parse(serializeUiPrefs(DEFAULT_UI_PREFS)));
  assert.deepEqual(keys.sort(), ["density", "detailsOpen", "listCollapsed", "listWidth", "panelByTask", "panelRatio", "theme", "zoom"]);
});

test("MWS-03: density is one layout preference with a default, separate from zoom, and a profile stored before it reads as Comfortable", () => {
  assert.equal(DEFAULT_UI_PREFS.density, "comfortable");
  assert.equal(parseUiPrefs(JSON.stringify({ listWidth: 300, zoom: 1.16 })).density, "comfortable");
  const compact = parseUiPrefs(JSON.stringify({ density: "compact", zoom: 1.16 }));
  assert.equal(compact.density, "compact");
  assert.equal(compact.zoom, 1.16, "choosing a density leaves the zoom alone");
  assert.equal(parseUiPrefs(JSON.stringify({ density: "compact" })).zoom, 1);
  for (const junk of ["dense", 2, null, ["compact"]]) assert.equal(parseUiPrefs(JSON.stringify({ density: junk })).density, "comfortable");
});

test("zoom moves in 8% steps inside its clamp", () => {
  assert.equal(clampZoom(1.08), 1.08);
  assert.equal(clampZoom(1 + 0.08 * 20), 1.64);
  assert.equal(clampZoom(0.1), 0.68);
});

test("AFW-A01, REQ-PX-048: the apps panel exists only for a task with an artifact; a started task has its change set in every state", () => {
  const ctx = { browsingTaskId: null };
  // PX-048 changed this on purpose: the change set, workspace and evidence exist from the moment a task has run, not only at review.
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Queued" }, ctx), []);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Created" }, ctx), []);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Running" }, ctx), ["changes", "files", "evidence"]);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "ReadyForReview" }, ctx), ["changes", "files", "evidence"]);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Completed" }, { browsingTaskId: "a" }), ["changes", "browser", "files", "evidence"]);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Running" }, { browsingTaskId: null, terminalTaskIds: new Set(["a"]) }), ["changes", "terminal", "files", "evidence"]);
  assert.deepEqual(artifactsOf({ taskId: "a", state: "Running" }, { browsingTaskId: null, terminalTaskIds: new Set(["b"]) }), ["changes", "files", "evidence"], "another task's terminal is not this task's artifact");
  assert.deepEqual(artifactsOf(undefined, ctx), []);
  const tasks = [
    { taskId: "old", state: "Completed", createdAtMs: 1 },
    { taskId: "new", state: "ReadyForReview", createdAtMs: 5 },
    { taskId: "queued", state: "Queued", createdAtMs: 9 },
  ];
  assert.equal(panelTaskFor(tasks, "queued", ctx), "new", "a selected task with nothing to show yields to the latest one that has something");
  assert.equal(panelTaskFor(tasks, "old", ctx), "old");
  assert.equal(panelTaskFor([{ taskId: "queued", state: "Queued", createdAtMs: 9 }], "queued", ctx), null);
  assert.equal(panelTaskFor([{ taskId: "run", state: "Running", createdAtMs: 9 }], "run", ctx), "run");
});

test("AFW-A16: every default chord of the spec is bound to its command, and none collides", () => {
  for (const [id, chord] of Object.entries(REQUIRED_CHORDS)) {
    const def = shellRegistry.get(id);
    assert.ok(def, id);
    assert.ok(def.keys?.includes(chord), `${id} is bound to ${chord}`);
  }
  // Every key of every command is unique across the registry (the constructor refuses a collision).
  const seen = new Map<string, string>();
  for (const d of shellCommandDefs()) for (const k of d.keys ?? []) assert.equal(seen.set(k, d.id).size > 0, true);
  assert.equal(new Set(seen.keys()).size, seen.size);
  assert.throws(() => {
    const r = buildShellRegistry();
    r.register({ id: "dup", title: "Dup", group: "General", keys: ["mod+k"], run: () => {} });
  }, /already bound to "palette.open"/);
});

test("AFW-A16: every registered command runs its action, and an unavailable one refuses with its reason", async () => {
  const calls: string[] = [];
  const spy = new Proxy({} as ShellActions, {
    get: (_t, name: string) => {
      if (name === "zoomAvailability") return () => true;
      if (name === "appAvailability") return () => true;
      if (name === "canGoBack") return () => true;
      if (name === "computerStopAvailability") return () => true;
      if (name === "showApp") return (k: string) => (calls.push(`showApp:${k}`), true);
      return (...args: unknown[]) => void calls.push(`${name}${args.length ? `:${args.join(",")}` : ""}`);
    },
  });
  const registry: CommandRegistry<ShellActions> = buildShellRegistry();
  for (const d of registry.all()) assert.equal(await registry.run(d.id, spy), true, d.id);
  assert.ok(calls.includes("focusNewTask") && calls.includes("openPalette") && calls.includes("toggleHelp") && calls.includes("showApp:terminal") && calls.includes("zoom:0") && calls.includes("setTheme:high-contrast"));
  const blocked = new Proxy(spy, { get: (t, n: string) => (n === "zoomAvailability" ? () => "Zoom is unavailable while the browser view is open" : Reflect.get(t, n)) });
  assert.equal(registry.availability("zoom.in", blocked), "Zoom is unavailable while the browser view is open");
  assert.equal(await registry.run("zoom.in", blocked), false);
});

test("PX-200 (MWS-05): a task has a toggle for each pane it can show, in tab order; one that has not started has none", () => {
  assert.deepEqual(panesOf([]), []);
  assert.deepEqual(panesOf(artifactsOf({ taskId: "a", state: "Queued" }, { browsingTaskId: null })), []);
  assert.deepEqual(panesOf(artifactsOf({ taskId: "a", state: "Running" }, { browsingTaskId: null })), ["changes", "browser", "files", "evidence"], "the Browser pane is where a session is opened from");
  assert.deepEqual(panesOf(artifactsOf({ taskId: "a", state: "Running" }, { browsingTaskId: "a", terminalTaskIds: new Set(["a"]) })), ["changes", "terminal", "browser", "files", "evidence"]);
  assert.deepEqual(panesOf(["terminal"]), ["terminal"], "a terminal alone does not make a workspace");
});

test("MWS-09 (PX-201): the details rail is 320 inside the task area, never under 280, absent at a task area of 800 or narrower, and gives way to the apps panel", () => {
  assert.deepEqual(DETAILS_RAIL, { width: 320, min: 280, hiddenAtOrBelow: 800 });
  const wide = computeLayout({ ...base, width: 1280, detailsOpen: true });
  assert.equal(wide.detailsPx, 320);
  assert.equal(wide.centerPx, 992, "the rail is inside the centre: the task area keeps its width");
  assert.ok(wide.centerPx - wide.detailsPx >= 424, "the conversation beside the rail keeps the centre's minimum");
  assert.equal(computeLayout({ ...base, width: 288 + 801, detailsOpen: true }).detailsPx, 320, "801 is wider than 800");
  const edge = computeLayout({ ...base, width: 288 + 800, detailsOpen: true });
  assert.equal(edge.detailsPx, 0, "exactly 800 has no rail");
  assert.equal(edge.detailsFits, false);
  assert.equal(computeLayout({ ...base, width: 900, detailsOpen: true }).detailsPx, 0, "the window's minimum size has no rail");
  // The person's choice, and a centre that is not a task's conversation, leave it out; whether it would fit is still told.
  const off = computeLayout({ ...base, width: 1280, detailsOpen: false });
  assert.equal(off.detailsPx, 0);
  assert.equal(off.detailsFits, true);
  assert.equal(computeLayout({ ...base, width: 1280 }).detailsPx, 0);
  // It gives way while the apps panel is open, beside the conversation or stacked.
  const beside = computeLayout({ ...base, width: 1440, panelOpen: true, detailsOpen: true });
  assert.ok(beside.panelPx > 0);
  assert.equal(beside.detailsPx, 0);
  assert.equal(beside.detailsFits, true);
  assert.equal(computeLayout({ ...base, width: 288 + 805, panelOpen: true, detailsOpen: true }).detailsPx, 0, "stacked");
  // With the list on its rail the task area is wider.
  assert.equal(computeLayout({ ...base, width: 900, listCollapsed: true, detailsOpen: true }).detailsPx, 320);
});

test("MWS-09 (PX-201): the rail's choice is stored with the layout preferences; a profile from before it reads as shown", () => {
  assert.equal(DEFAULT_UI_PREFS.detailsOpen, true);
  assert.equal(parseUiPrefs(JSON.stringify({ listWidth: 300 })).detailsOpen, true);
  assert.equal(parseUiPrefs(JSON.stringify({ detailsOpen: false })).detailsOpen, false);
  assert.equal(parseUiPrefs(JSON.stringify({ detailsOpen: "no" })).detailsOpen, true, "a hostile value falls back to the default");
  assert.equal(parseUiPrefs(serializeUiPrefs({ ...DEFAULT_UI_PREFS, detailsOpen: false })).detailsOpen, false);
});
