import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { chromeOptions, chromeQuery, controlsPosition, controlsReserve, displaySignature, effectiveTranslucent, isVisible, readTranslucencyOptIn, validateRestore, WindowStateStore, type DisplayInfo } from "./window-chrome.ts";

const one: DisplayInfo[] = [{ bounds: { x: 0, y: 0, width: 1920, height: 1080 }, workArea: { x: 0, y: 25, width: 1920, height: 1055 }, primary: true }];
const two: DisplayInfo[] = [...one, { bounds: { x: 1920, y: 0, width: 2560, height: 1440 }, workArea: { x: 1920, y: 0, width: 2560, height: 1440 } }];
const FALLBACK = { width: 1280, height: 800 };
const MIN = { width: 900, height: 600 };
const saved = (x: number, y: number, w = 1200, h = 800) => ({ bounds: { x, y, width: w, height: h }, maximized: false, fullscreen: false });

test("PX-094: a saved window on a display that exists is restored exactly", () => {
  const r = validateRestore(saved(2100, 80), two, FALLBACK, MIN);
  assert.deepEqual(r.bounds, { x: 2100, y: 80, width: 1200, height: 800 });
  assert.equal(r.moved, "none");
});

test("PX-094: a saved window on a removed display opens on a visible screen, centred on the primary", () => {
  const r = validateRestore(saved(2100, 80), one, FALLBACK, MIN);
  assert.equal(r.moved, "off-screen");
  assert.ok(isVisible(r.bounds, one), JSON.stringify(r.bounds));
  assert.deepEqual(r.bounds, { x: 360, y: 153, width: 1200, height: 800 });
});

test("PX-094: a window above every screen (its bar unreachable), far below, or with nonsense numbers is brought back", () => {
  for (const s of [saved(100, -400), saved(100, 5000), saved(-9000, 100), saved(Number.NaN, 0)]) {
    const r = validateRestore(s, one, FALLBACK, MIN);
    assert.ok(isVisible(r.bounds, one), `${JSON.stringify(s.bounds)} -> ${JSON.stringify(r.bounds)}`);
    assert.notEqual(r.moved, "none");
  }
});

test("PX-094: a window larger than the screen is shrunk to it when it has to move, and never below the minimum", () => {
  const r = validateRestore(saved(9000, 0, 6000, 4000), one, FALLBACK, MIN);
  assert.ok(r.bounds.width <= 1920 && r.bounds.height <= 1055);
  const tiny = validateRestore(saved(10, 40, 10, 10), one, FALLBACK, MIN);
  assert.ok(tiny.bounds.width >= MIN.width && tiny.bounds.height >= MIN.height);
});

test("PX-094: nothing saved opens the default size, centred", () => {
  const r = validateRestore(undefined, one, FALLBACK, MIN);
  assert.equal(r.moved, "no-record");
  assert.deepEqual(r.bounds, { x: 320, y: 153, width: 1280, height: 800 });
});

test("PX-094: maximised and fullscreen survive a restore; a fullscreen window that had to move is not restored fullscreen", () => {
  const ok = validateRestore({ ...saved(100, 100), maximized: true, fullscreen: true }, one, FALLBACK, MIN);
  assert.equal(ok.maximized, true);
  assert.equal(ok.fullscreen, true);
  const moved = validateRestore({ ...saved(5000, 100), maximized: true, fullscreen: true }, one, FALLBACK, MIN);
  assert.equal(moved.fullscreen, false);
});

test("PX-094: the store keeps a window per display configuration and falls back to the last one under a new configuration", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-wstate-"));
  const file = join(dir, "window-state.json");
  const a = new WindowStateStore(file);
  assert.equal(a.get(one), undefined);
  a.set(one, saved(10, 40));
  a.set(two, saved(2100, 80));
  const b = new WindowStateStore(file);
  assert.deepEqual(b.get(one)?.bounds, { x: 10, y: 40, width: 1200, height: 800 });
  assert.deepEqual(b.get(two)?.bounds, { x: 2100, y: 80, width: 1200, height: 800 });
  const third: DisplayInfo[] = [{ bounds: { x: 0, y: 0, width: 1440, height: 900 }, workArea: { x: 0, y: 25, width: 1440, height: 875 }, primary: true }];
  assert.deepEqual(b.get(third)?.bounds, { x: 2100, y: 80, width: 1200, height: 800 }, "a new configuration is offered the last saved window (validated on restore)");
  assert.notEqual(displaySignature(one), displaySignature(two));
  assert.equal(displaySignature([...two].reverse()), displaySignature(two), "the key does not depend on the order the OS lists displays in");
});

test("PX-094: a corrupt or hostile state file is ignored, not trusted", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-wstate-bad-"));
  for (const body of ["not json", "[]", '{"version":2}', '{"version":1,"last":"x","byDisplays":{"x":{"bounds":{"x":"1"}}}}']) {
    const file = join(dir, "s.json");
    writeFileSync(file, body);
    assert.equal(new WindowStateStore(file).get(one), undefined, body);
  }
});

test("PX-094: the store writes the file whole (no half-written state) and caps the configurations it remembers", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-wstate-cap-"));
  const file = join(dir, "w", "window-state.json");
  const s = new WindowStateStore(file);
  for (let i = 0; i < 12; i++) s.set([{ bounds: { x: 0, y: 0, width: 1000 + i, height: 700 }, workArea: { x: 0, y: 0, width: 1000 + i, height: 700 } }], saved(0, 0, 900, 600));
  assert.equal(s.keys().length, 8);
  assert.equal(JSON.parse(readFileSync(file, "utf8")).version, 1);
});

test("PX-094: translucency is off by default, follows the opt-in, and Reduce transparency always wins", () => {
  assert.equal(effectiveTranslucent(false, false, "darwin"), false);
  assert.equal(effectiveTranslucent(true, false, "darwin"), true);
  assert.equal(effectiveTranslucent(true, true, "darwin"), false, "the operating system's preference overrides the opt-in");
  assert.equal(effectiveTranslucent(true, false, "linux"), false, "a platform with no material stays opaque");
  const dir = mkdtempSync(join(tmpdir(), "modbit-chrome-"));
  const file = join(dir, "window-chrome.json");
  assert.equal(readTranslucencyOptIn(file), false, "absent means off");
  writeFileSync(file, '{"translucency":true}');
  assert.equal(readTranslucencyOptIn(file), true);
  writeFileSync(file, '{"translucency":"yes"}');
  assert.equal(readTranslucencyOptIn(file), false, "only the boolean true opts in");
});

test("PX-094: macOS gets the inset title bar and controls placed from the 40 pt bar; other platforms keep their frame", () => {
  const mac = chromeOptions({ platform: "darwin", osRelease: "27.0.0", translucent: false });
  assert.equal(mac.titleBarStyle, "hidden");
  assert.deepEqual(mac.trafficLightPosition, controlsPosition("27.0.0"));
  assert.equal(mac.vibrancy, undefined);
  assert.equal(mac.backgroundColor, undefined, "opaque by default: the window keeps its own fill");
  assert.deepEqual(chromeOptions({ platform: "win32", osRelease: "10.0", translucent: false }), {});
  assert.deepEqual(chromeOptions({ platform: "linux", osRelease: "6.1", translucent: true }), {});
  const t = chromeOptions({ platform: "darwin", osRelease: "27.0.0", translucent: true });
  assert.equal(t.vibrancy, "sidebar");
  assert.equal(chromeOptions({ platform: "win32", osRelease: "10.0", translucent: true }).backgroundMaterial, "mica");
});

test("PX-094: the controls' position is derived from the bar height and differs on the newer operating system", () => {
  const old = controlsPosition("24.0.0");
  const newer = controlsPosition("25.1.0");
  assert.equal(old.y, Math.round((40 - 14) / 2));
  assert.notDeepEqual(old, newer);
  assert.ok(controlsReserve("24.0.0").width >= old.x + 52);
  assert.ok(controlsReserve("25.0.0").width >= newer.x + 52);
  assert.deepEqual(controlsPosition("garbage"), controlsPosition("0.0.0"));
});

test("PX-094: the renderer is told its chrome through a query and nothing else", () => {
  assert.deepEqual(chromeQuery({ platform: "darwin", osRelease: "27.0.0", translucent: false }), { chrome: "inset", reserve: `${controlsReserve("27.0.0").width}x40` });
  assert.deepEqual(chromeQuery({ platform: "darwin", osRelease: "27.0.0", translucent: true }).translucent, "1");
  assert.deepEqual(chromeQuery({ platform: "linux", osRelease: "6.1", translucent: true }), {});
});
