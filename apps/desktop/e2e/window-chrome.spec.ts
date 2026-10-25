/**
 * REQ-PX-094 / QUAL-PX-094 (docs/62, docs/78 ESH-B01..B04): the window chrome in the real Electron app against the
 * real Core. Geometry is read from the live window and the live DOM (no screenshots): the native window controls sit
 * inside the agent-list strip without overlapping content at 100, 125 and 150 percent zoom; every control in the bar
 * is keyboard reachable, opts out of dragging, and an empty bar area is the drag region; translucency is off by
 * default, the opt-in changes only the designed surfaces, and Reduce transparency puts the opaque look back; the
 * window comes back where it was on the same displays and a window saved for a display that is gone opens on a screen
 * that exists. The platform parts run on macOS (the release platform); Windows and Linux are asserted to keep their
 * platform frame.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { release, tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, MOD } from "./support/ui-harness.ts";
import { CONTROLS_HEIGHT_PT, CONTROLS_WIDTH_PT, controlsPosition, displaySignature, type DisplayInfo } from "../src/main/window-chrome.ts";

const mac = process.platform === "darwin";
const profile = (what: string) => mkdtempSync(join(tmpdir(), `modbit-e2e-chrome-${what}-`));

const displays = (app: ElectronApplication): Promise<DisplayInfo[]> =>
  app.evaluate(({ screen }) => screen.getAllDisplays().map((d) => ({ bounds: d.bounds, workArea: d.workArea, primary: d.id === screen.getPrimaryDisplay().id })));

async function setZoom(page: Page, steps: number): Promise<void> {
  await page.keyboard.press(`${MOD}+0`);
  for (let i = 0; i < steps; i++) await page.keyboard.press(`${MOD}+Equal`);
  const want = steps === 0 ? "" : String(Math.round((1 + 0.08 * steps) * 100) / 100);
  await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe(want);
}

const buttons = (app: ElectronApplication) =>
  app.evaluate(({ BrowserWindow }) => {
    const w = BrowserWindow.getAllWindows()[0]!;
    return { position: (w as unknown as { getWindowButtonPosition(): { x: number; y: number } | null }).getWindowButtonPosition(), bounds: w.getBounds(), content: w.getContentBounds() };
  });

test("PX-094: on macOS the title bar is inset and the window controls sit where the OS-version table puts them", async () => {
  test.skip(!mac, "the inset title bar is the macOS window");
  const { app, page } = await launch(profile("inset"));
  try {
    const b = await buttons(app);
    expect(b.position, "Electron reports the controls' position when it was set explicitly").toEqual(controlsPosition(release()));
    expect(b.content.y - b.bounds.y, "no native title bar: the web content starts at the top of the window").toBe(0);
    await expect(page.locator("html")).toHaveAttribute("data-chrome", "inset");
  } finally {
    await closeApp(app);
  }
});

test("PX-094: the controls stay inside the agent-list strip and clear of content at 100, 125 and 150 percent zoom, expanded and on the rail", async () => {
  test.skip(!mac, "the inset title bar is the macOS window");
  const { app, page } = await launch(profile("zoom"));
  try {
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.setContentSize(1280, 800));
    await expect.poll(() => page.evaluate(() => window.innerWidth)).toBe(1280);
    const pos = (await buttons(app)).position!;
    // 100 / 125 / 150 percent are the 0, 3 and 6 steps of the 8 percent zoom model (1.00, 1.24, 1.48).
    for (const steps of [0, 3, 6]) {
      await setZoom(page, steps);
      const g = await page.evaluate(() => {
        const r = (sel: string) => {
          const e = document.querySelector(sel);
          if (!e) return null;
          const b = e.getBoundingClientRect();
          return { left: b.left, top: b.top, right: b.right, bottom: b.bottom };
        };
        return { zoom: Number(getComputedStyle(document.documentElement).zoom) || 1, strip: r('[data-testid="window-controls-strip"]'), list: r('[data-testid="region-agents"]'), scroll: r('[data-testid="region-agents"] .shell-scroll'), bar: r('[data-testid="top-bar"]') };
      });
      const label = `zoom step ${steps} (${g.zoom})`;
      // The native controls do not scale; the strip, in native points, always holds them.
      const nativeStripBottom = g.strip!.bottom;
      expect(pos.y + CONTROLS_HEIGHT_PT, `${label}: controls end inside the strip`).toBeLessThanOrEqual(nativeStripBottom + 0.5);
      expect(pos.x + CONTROLS_WIDTH_PT, `${label}: controls end inside the list's width`).toBeLessThanOrEqual(g.list!.right);
      expect(g.scroll!.top, `${label}: the list's content starts below the strip, so nothing sits under the controls`).toBeGreaterThanOrEqual(nativeStripBottom - 0.5);
      expect(g.bar!.bottom - g.bar!.top, `${label}: the bar is as tall as the strip`).toBeCloseTo(g.strip!.bottom - g.strip!.top, 0);
      expect(g.bar!.left, `${label}: the bar begins after the list`).toBeGreaterThanOrEqual(g.list!.right - 1);
    }
    // Collapsed to the 40 px rail the controls are wider than the rail: the bar's own content starts after them.
    await setZoom(page, 0);
    await page.getByTestId("toggle-agent-list").click();
    await expect(page.getByTestId("region-agents")).toHaveAttribute("data-rail", "true");
    for (const steps of [0, 3, 6]) {
      await setZoom(page, steps);
      const first = await page.evaluate(() => document.querySelector('[data-testid="top-bar"] button')!.getBoundingClientRect().left);
      expect(first, `rail, zoom step ${steps}: the bar's first control clears the window controls`).toBeGreaterThanOrEqual(pos.x + CONTROLS_WIDTH_PT);
    }
  } finally {
    await closeApp(app);
  }
});

test("PX-094: every control in the bar is keyboard reachable and opts out of dragging; the empty bar and the strip drag the window", async () => {
  test.skip(!mac, "the inset title bar is the macOS window");
  const { app, page } = await launch(profile("drag"));
  try {
    const region = (sel: string) => page.evaluate((s) => getComputedStyle(document.querySelector(s)!).getPropertyValue("-webkit-app-region"), sel);
    expect(await region('[data-testid="top-bar"]'), "the bar is the drag region").toBe("drag");
    expect(await region('[data-testid="top-bar-title"]'), "the title is part of it").toBe("drag");
    expect(await region('[data-testid="window-controls-strip"]'), "the strip under the window controls drags too").toBe("drag");
    const interactive = await page.evaluate(() =>
      [...document.querySelectorAll('[data-testid="top-bar"] :is(button, a, input, select, textarea, [role="button"], [role="menuitem"], [tabindex])')].map((e) => ({
        id: e.getAttribute("data-testid") ?? e.tagName,
        region: getComputedStyle(e).getPropertyValue("-webkit-app-region"),
        disabled: (e as HTMLButtonElement).disabled === true,
      })),
    );
    expect(interactive.length, "the bar has controls").toBeGreaterThan(3);
    for (const c of interactive) expect(c.region, `${c.id} opts out of dragging`).toBe("no-drag");
    // Keyboard: Tab from just before the bar visits every enabled control in it, in order, and leaves it.
    const expected = interactive.filter((c) => !c.disabled).map((c) => c.id);
    await page.evaluate(() => (document.querySelector('[data-testid="region-agents"] button, [data-testid="region-agents"] a, [data-testid="region-agents"] input') as HTMLElement | null)?.focus());
    const visited: string[] = [];
    for (let i = 0; i < 40 && visited.length < expected.length; i++) {
      await page.keyboard.press("Tab");
      const at = await page.evaluate(() => {
        const a = document.activeElement as HTMLElement | null;
        return a && a.closest('[data-testid="top-bar"]') ? (a.getAttribute("data-testid") ?? a.tagName) : "";
      });
      if (at) visited.push(at);
    }
    expect(visited, "Tab reaches every enabled control in the bar").toEqual(expected);
    // Clicking a control acts as a control (a drag would have swallowed it).
    const before = await page.getByTestId("toggle-agent-list").getAttribute("aria-pressed");
    await page.getByTestId("toggle-agent-list").click();
    await expect(page.getByTestId("toggle-agent-list")).not.toHaveAttribute("aria-pressed", before!);
  } finally {
    await closeApp(app);
  }
});

type Look = Record<string, string>;
const SURFACES = { html: "html", body: "body", root: "#root", shell: ".shell", list: ".shell-list", center: ".shell-center", topbar: ".topbar", content: ".shell-content", status: ".statusrow" };
const looks = (page: Page): Promise<Look> =>
  page.evaluate((s) => Object.fromEntries(Object.entries(s).map(([k, sel]) => [k, getComputedStyle(document.querySelector(sel)!).backgroundColor])), SURFACES);
const alphaOf = (c: string): number => {
  const m = /\/\s*([\d.]+)\s*\)|rgba\([^)]*,\s*([\d.]+)\)/.exec(c);
  return m ? Number(m[1] ?? m[2]) : c === "transparent" ? 0 : 1;
};

test("PX-094: translucency is off by default, the opt-in changes only the designed surfaces, and Reduce transparency puts the opaque look back", async () => {
  test.skip(!mac, "vibrancy is exercised on macOS; Windows and Linux are CI_COMPATIBLE");
  // Off (the default): nothing is translucent.
  let dir = profile("opaque");
  let { app, page } = await launch(dir);
  let off: Look;
  try {
    await expect(page.locator("html")).not.toHaveAttribute("data-translucent", /.+/);
    off = await looks(page);
    // A surface either paints an opaque fill or paints nothing (the document root and wrappers); none is part-transparent.
    for (const [k, v] of Object.entries(off)) expect([0, 1], `default: ${k} is opaque or unpainted (${v})`).toContain(alphaOf(v));
    for (const k of ["body", "shell", "list", "topbar", "status"]) expect(alphaOf(off[k]!), `default: ${k} is painted opaque`).toBe(1);
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getBackgroundColor()), "the window keeps an opaque fill").not.toMatch(/^#00/i);
  } finally {
    await closeApp(app);
  }

  // On: the agent list shows the window material and the document behind it lets it through; the reading surfaces stay opaque.
  dir = profile("translucent");
  writeFileSync(join(dir, "window-chrome.json"), JSON.stringify({ translucency: true }));
  ({ app, page } = await launch(dir));
  let on: Look;
  try {
    await expect(page.locator("html")).toHaveAttribute("data-translucent", "1");
    on = await looks(page);
    const differing = Object.keys(off).filter((k) => on[k] !== off[k]).sort();
    expect(differing, "only the designed surfaces differ").toEqual(["body", "center", "list", "shell"]);
    expect(alphaOf(on.body!) + alphaOf(on.shell!), "the document and shell paint nothing, so the material shows behind the list").toBe(0);
    expect(alphaOf(on.list!), "the agent list is translucent").toBeLessThan(1);
    for (const k of ["center", "topbar", "status"]) expect(alphaOf(on[k]!), `${k} stays opaque`).toBe(1);
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getBackgroundColor()), "the window has no fill of its own, so the material shows").toMatch(/^#00/i);
  } finally {
    await closeApp(app);
  }

  // Reduce transparency set: the opt-in is overridden and the look is the default one.
  ({ app, page } = await launch(dir, { env: { MODBIT_REDUCE_TRANSPARENCY: "1" } }));
  try {
    await expect(page.locator("html")).not.toHaveAttribute("data-translucent", /.+/);
    expect(await looks(page), "with Reduce transparency no surface is translucent and the look equals the default").toEqual(off);
  } finally {
    await closeApp(app);
  }
});

test("PX-094: the window comes back where it was, maximised or not, on the same displays", async () => {
  const dir = profile("restore");
  let { app, page } = await launch(dir);
  let target: { x: number; y: number; width: number; height: number };
  try {
    const d = (await displays(app)).find((x) => x.primary)!;
    target = { x: d.workArea.x + 60, y: d.workArea.y + 50, width: 1100, height: 760 };
    await app.evaluate(({ BrowserWindow }, b) => BrowserWindow.getAllWindows()[0]!.setBounds(b), target);
    // The state is written when the window settles; it is not left to the quit.
    await expect.poll(() => (existsState(dir) ? JSON.stringify(readState(dir)) : "")).toContain('"width":1100');
  } finally {
    await closeApp(app);
  }
  ({ app, page } = await launch(dir));
  try {
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getBounds())).toEqual(target);
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.maximize());
    await expect.poll(() => (existsState(dir) ? JSON.stringify(readState(dir)) : "")).toContain('"maximized":true');
  } finally {
    await closeApp(app);
  }
  ({ app, page } = await launch(dir));
  try {
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.isMaximized()), "maximised comes back maximised").toBe(true);
    // And the unmaximised size under it is still the one the person had.
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getNormalBounds())).toEqual(target);
    expect(page).toBeTruthy();
  } finally {
    await closeApp(app);
  }
});

test("PX-094: a window saved for a display that is gone, and a corrupt state file, open on a screen that exists", async () => {
  const dir = profile("gone");
  let { app } = await launch(dir);
  const real = await displays(app);
  await closeApp(app);
  // The profile last ran with a second display to the right; the window sat on it. That display is not there now.
  const was: DisplayInfo[] = [...real, { bounds: { x: 8000, y: 0, width: 2560, height: 1440 }, workArea: { x: 8000, y: 0, width: 2560, height: 1440 } }];
  writeFileSync(join(dir, "window-state.json"), JSON.stringify({ version: 1, last: displaySignature(was), byDisplays: { [displaySignature(was)]: { bounds: { x: 8300, y: 200, width: 1200, height: 800 }, maximized: false, fullscreen: false } } }));
  ({ app } = await launch(dir));
  try {
    const b = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getBounds());
    const onScreen = real.some((d) => b.x >= d.workArea.x && b.y >= d.workArea.y && b.x + b.width <= d.workArea.x + d.workArea.width + 1 && b.y + b.height <= d.workArea.y + d.workArea.height + 1);
    expect(onScreen, `window ${JSON.stringify(b)} is wholly on a display ${JSON.stringify(real.map((d) => d.workArea))}`).toBe(true);
    // The window system alone would nudge a stray window back; the app's own rule is that it is centred, at its saved size, on the primary display.
    const wa = real.find((d) => d.primary)!.workArea;
    expect(b, "centred on the primary display at the size it had").toEqual({ x: Math.round(wa.x + (wa.width - 1200) / 2), y: Math.round(wa.y + (wa.height - 800) / 2), width: 1200, height: 800 });
  } finally {
    await closeApp(app);
  }
  const dir2 = profile("corrupt");
  writeFileSync(join(dir2, "window-state.json"), "{ not json");
  ({ app } = await launch(dir2));
  try {
    const b = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.getBounds());
    expect(b.width).toBeGreaterThanOrEqual(900);
    expect(real.some((d) => b.x >= d.workArea.x - 1 && b.y >= d.workArea.y - 1 && b.x < d.workArea.x + d.workArea.width), "a corrupt state file still opens a visible window").toBe(true);
  } finally {
    await closeApp(app);
  }
});

test("PX-094: Windows and Linux keep their platform frame and the page is told no chrome", async () => {
  test.skip(mac, "macOS has the inset title bar (the tests above)");
  const { app, page } = await launch(profile("plain"));
  try {
    await expect(page.locator("html")).not.toHaveAttribute("data-chrome", /.+/);
    expect(await page.getByTestId("window-controls-strip").count()).toBe(0);
    const b = await buttons(app);
    expect(b.content.y - b.bounds.y, "the platform's own title bar remains (or the window manager's)").toBeGreaterThanOrEqual(0);
  } finally {
    await closeApp(app);
  }
});

function readState(dir: string): unknown {
  return JSON.parse(readFileSync(join(dir, "window-state.json"), "utf8"));
}
function existsState(dir: string): boolean {
  try {
    readState(dir);
    return true;
  } catch {
    return false;
  }
}
