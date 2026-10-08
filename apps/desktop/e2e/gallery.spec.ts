/**
 * REQ-PX-044 / QUAL-PX-044 (docs/62, docs/65 AFW-A11..A14, AFW-H01, AFW-K02):
 * the model-free state gallery in the real Electron app with NO Core and no
 * model behind it: approvals, generating, error, empty and offline states
 * rendered from fixtures; the axe-core suite over every state in every theme;
 * every primitive operated by keyboard alone; the tray host keeping one active
 * tray whose scoped keys move with it; reduced motion; the shell geometry. The
 * gallery is a QA aid and never proof of a feature (every behaviour is proven
 * against a real Core in the other specs). It is built into a separate output
 * directory by this spec, and the production bundle must not contain it.
 */
import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, appDir, closeApp, focusedTestId, launch, MOD } from "./support/ui-harness.ts";

const OUT = "dist/gallery-build";
const STATES = ["approvals", "generating", "error", "empty", "offline"] as const;
const THEMES = ["light", "dark", "high-contrast"] as const;

test.beforeAll(() => {
  execFileSync("node", ["build.mjs"], { cwd: appDir, env: { ...process.env, MODBIT_GALLERY: "1", MODBIT_OUTDIR: OUT }, stdio: "pipe" });
});

async function openGallery(page: Page): Promise<void> {
  await page.evaluate(() => {
    location.hash = "#gallery";
  });
  await expect(page.getByTestId("gallery")).toBeVisible();
}

async function launchGallery() {
  // No Core: the binary does not exist, so nothing here can be served by one.
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-gallery-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  await openGallery(page);
  return { app, page };
}

test("PX-044: the production bundle contains no gallery; the gallery build is separate", () => {
  const prod = join(appDir, "dist", "renderer", "index.js");
  if (existsSync(prod)) expect(readFileSync(prod, "utf8")).not.toContain("State gallery");
  expect(readFileSync(join(appDir, OUT, "renderer", "index.js"), "utf8")).toContain("State gallery");
});

test("PX-044: approvals, generating, error, empty and offline render from fixtures with no Core; axe passes on every state in light, dark and high contrast", async () => {
  const { app, page } = await launchGallery();
  try {
    const status = await page.evaluate(() => window.modbit.coreStatus());
    expect((status as { state: string }).state, "no Core is connected to this page").not.toBe("connected");
    for (const s of STATES) await expect(page.getByTestId(`gallery-state-${s}`)).toBeVisible();
    await expect(page.getByTestId("gallery-state-generating")).toContainText("Editing src/lib.rs");
    await expect(page.getByTestId("gallery-state-error")).toContainText("The turn failed");
    await expect(page.getByTestId("gallery-state-empty")).toContainText("No tasks yet");
    await expect(page.getByTestId("gallery-state-offline")).toContainText("You are offline");
    await page.getByTestId("present-approval").click();
    await page.getByTestId("present-error").click();
    await expect(page.getByTestId("gallery-state-approvals")).toContainText("Approve this command?");
    for (const theme of THEMES) {
      await page.getByTestId(`theme-${theme}`).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await accessible(page, `the whole gallery in ${theme}`);
      for (const s of STATES) await accessible(page, `${s} in ${theme}`, `[data-testid="gallery-state-${s}"]`);
      await accessible(page, `primitives in ${theme}`, '[data-testid="gallery-primitives"]');
      await accessible(page, `shell geometry in ${theme}`, '[data-testid="gallery-shell"]');
    }
    // Status is never colour alone: every status dot carries its own glyph and a text alternative.
    const dots = await page.locator(".mb-dot").evaluateAll((els) => els.map((e) => ({ status: e.getAttribute("data-status"), svg: e.querySelectorAll("svg").length, text: (e.textContent ?? "").trim() })));
    expect(dots.length).toBeGreaterThan(8);
    for (const d of dots) {
      expect(d.svg, `${d.status} has a glyph`).toBe(1);
      expect(d.text.length, `${d.status} has text`).toBeGreaterThan(0);
    }
    const shapes = await page.locator(".mb-dot-glyph svg").evaluateAll((els) => new Set(els.map((e) => e.innerHTML)).size);
    expect(shapes, "ok, warn, danger, info, running and idle each have their own shape").toBeGreaterThanOrEqual(6);
  } finally {
    await closeApp(app);
  }
});

test("PX-044: the tray host keeps one active tray, and the scoped keys move with it", async () => {
  const { app, page } = await launchGallery();
  try {
    await expect(page.getByTestId("tray")).toHaveCount(1);
    await expect(page.getByTestId("tray")).toContainText("2 messages queued");
    const log = page.getByTestId("gallery-log");
    await page.getByTestId("present-approval").click();
    await expect(page.getByTestId("tray")).toHaveCount(1);
    await expect(page.getByTestId("tray")).toContainText("Approve this command?");
    await expect(page.getByTestId("tray-pending")).toHaveCount(1);
    // The approval owns its chord; the queue tray's (none) and a different chord do nothing.
    await page.keyboard.press(`${MOD}+Enter`);
    await expect(log).toContainText("approval-1: approve");
    await page.keyboard.press(`${MOD}+Shift+Enter`);
    await expect(log).not.toContainText("approval-2");
    // A second approval of equal priority becomes the active tray: the keys move with it.
    await page.getByTestId("present-second").click();
    await expect(page.getByTestId("tray")).toHaveCount(1);
    await expect(page.getByTestId("tray")).toContainText("Write outside the workspace");
    await page.keyboard.press(`${MOD}+Shift+Enter`);
    await expect(log).toContainText("approval-2: approve");
    const before = (await log.locator("li").count());
    await page.keyboard.press(`${MOD}+Enter`);
    await page.keyboard.press(`${MOD}+Backspace`);
    expect(await log.locator("li").count(), "the first approval's keys are off while it is not active").toBe(before);
    // The person brings the first one forward from its pending row; its keys return.
    await page.getByTestId("tray-pending").filter({ hasText: "Approve this command?" }).click();
    await expect(page.getByTestId("tray")).toContainText("Approve this command?");
    await page.keyboard.press(`${MOD}+Backspace`);
    await expect(log).toContainText("approval-1: deny");
    // An approval is resolved, not dismissed: no dismiss control; an error tray can be dismissed; Escape dismisses only while focus is inside it.
    await expect(page.getByTestId("tray-dismiss")).toHaveCount(0);
    await page.getByTestId("present-error").click();
    await expect(page.getByTestId("tray")).toContainText("Approve this command?");
    await page.getByTestId("tray-pending").filter({ hasText: "The turn failed" }).click();
    await expect(page.getByTestId("tray")).toContainText("The turn failed");
    await expect(page.getByTestId("tray")).toHaveAttribute("role", "alert");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("tray"), "Escape outside the tray leaves it").toContainText("The turn failed");
    await page.getByTestId("tray-dismiss").focus();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("tray")).not.toContainText("The turn failed");
    await accessible(page, "the tray host with trays", '[data-testid="gallery-state-approvals"]');
  } finally {
    await closeApp(app);
  }
});

test("PX-044: every primitive operates by keyboard alone with its ARIA contract", async () => {
  const { app, page } = await launchGallery();
  try {
    // Menu: opens on Enter with the first item focused; arrows skip disabled items; type-ahead; Escape returns focus.
    const trigger = page.getByTestId("g-menu");
    await expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    await expect(trigger).toHaveAttribute("aria-expanded", "false");
    await trigger.focus();
    await page.keyboard.press("Enter");
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
    const menu = page.getByRole("menu", { name: "Sort by" });
    await expect(menu).toBeVisible();
    await expect(page.getByRole("menuitemradio")).toHaveCount(2);
    expect(await focusedTestId(page)).toBe("menu-item-recent");
    await page.keyboard.press("ArrowDown");
    expect(await focusedTestId(page)).toBe("menu-item-name");
    await page.keyboard.press("ArrowDown");
    expect(await focusedTestId(page), "the disabled item is skipped and the menu wraps").toBe("menu-item-recent");
    await page.keyboard.press("End");
    expect(await focusedTestId(page)).toBe("menu-item-name");
    await page.keyboard.press("Home");
    await page.keyboard.press("n");
    expect(await focusedTestId(page), "type-ahead").toBe("menu-item-name");
    await page.keyboard.press("Escape");
    await expect(menu).toHaveCount(0);
    expect(await focusedTestId(page)).toBe("g-menu");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Space");
    await expect(page.getByTestId("gallery-log")).toContainText("sort: name");
    await expect(menu).toHaveCount(0);
    expect(await focusedTestId(page), "focus returns to the trigger after a selection").toBe("g-menu");
    await page.keyboard.press("ArrowUp");
    expect(await focusedTestId(page), "ArrowUp opens on the last item").toBe("menu-item-name");
    await page.keyboard.press("Tab");
    await expect(menu).toHaveCount(0);

    // Dialog: modal, named, focus trapped, Escape closes, focus returns to the opener.
    const opener = page.getByTestId("g-open-dialog");
    await opener.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Example dialog" });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute("aria-modal", "true");
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press("Tab");
      expect(await page.evaluate(() => document.activeElement?.closest('[role="dialog"]') !== null), `Tab ${i + 1} stays inside the dialog`).toBe(true);
    }
    await page.keyboard.press("Shift+Tab");
    expect(await page.evaluate(() => document.activeElement?.closest('[role="dialog"]') !== null)).toBe(true);
    await accessible(page, "the open dialog");
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    expect(await focusedTestId(page)).toBe("g-open-dialog");

    // Tooltip: shown on keyboard focus and linked by aria-describedby; Escape hides it without moving focus.
    const tip = page.getByTestId("g-tooltip-trigger");
    await tip.focus();
    await expect(page.getByRole("tooltip")).toBeVisible();
    await expect(page.getByRole("tooltip")).toHaveText("Adds a thing");
    const describedBy = await tip.getAttribute("aria-describedby");
    expect(await page.getByRole("tooltip").getAttribute("id")).toBe(describedBy);
    await page.keyboard.press("Escape");
    await expect(page.getByRole("tooltip")).toBeHidden();
    expect(await focusedTestId(page)).toBe("g-tooltip-trigger");

    // Tabs: roving tabindex, arrows select and skip disabled, Home and End.
    await expect(page.getByRole("tablist", { name: "Example tabs" })).toBeVisible();
    await expect(page.getByTestId("tab-changes")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("tabindex", "-1");
    await page.getByTestId("tab-changes").focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    expect(await focusedTestId(page)).toBe("tab-terminal");
    await expect(page.getByRole("tabpanel")).toContainText("Panel for terminal");
    await page.keyboard.press("ArrowRight");
    await expect(page.getByTestId("tab-changes"), "the disabled tab is skipped and the list wraps").toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("End");
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Home");
    await expect(page.getByTestId("tab-changes")).toHaveAttribute("aria-selected", "true");
    expect(await page.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(await page.getByTestId("tab-changes").getAttribute("id"));

    // List and rows: arrows move between rows without wrapping; the row names itself; Enter selects.
    await page.getByTestId("g-row-r1").focus();
    await page.keyboard.press("ArrowDown");
    expect(await focusedTestId(page)).toBe("g-row-r2");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    expect(await focusedTestId(page), "the top stays the top").toBe("g-row-r1");
    await page.keyboard.press("End");
    expect(await focusedTestId(page)).toBe("g-row-r3");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("g-row-r3")).toHaveAttribute("aria-current", "true");
    await expect(page.getByTestId("g-row-r1")).not.toHaveAttribute("aria-current", "true");

    // Names: an icon button is named, a primary button's text meets AA, a kbd shows the platform's spelling.
    await expect(page.getByRole("button", { name: "Add", exact: true })).toBeVisible();
    await expect(page.locator('kbd[data-chord="mod+k"]').first()).toHaveText(process.platform === "darwin" ? "⌘K" : "Ctrl+K");
    await expect(page.getByTestId("gallery-primitives").getByRole("button").first()).toBeVisible();
    const unnamed = await page.evaluate(() => [...document.querySelectorAll('button, [role="tab"], [role="menuitem"], [role="separator"]')].filter((el) => !(el.getAttribute("aria-label") || (el.textContent ?? "").trim())).length);
    expect(unnamed, "every control has an accessible name").toBe(0);
  } finally {
    await closeApp(app);
  }
});

test("PX-044: with prefers-reduced-motion no transition or animation runs", async () => {
  const { app, page } = await launchGallery();
  try {
    const state = () =>
      page.evaluate(() => {
        const shimmer = getComputedStyle(document.querySelector('[data-testid="shimmer"]')!);
        const spin = getComputedStyle(document.querySelector(".mb-spin")!);
        const btn = getComputedStyle(document.querySelector('[data-testid="g-primary"]')!);
        return { shimmer: shimmer.animationName, spin: spin.animationName, transition: btn.transitionDuration };
      });
    await page.emulateMedia({ reducedMotion: "no-preference" });
    const moving = await state();
    expect(moving.shimmer).not.toBe("none");
    expect(moving.spin).not.toBe("none");
    expect(moving.transition).not.toBe("0s");
    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect.poll(state).toEqual({ shimmer: "none", spin: "none", transition: "0s" });
  } finally {
    await closeApp(app);
  }
});

test("PX-044/045: the shell geometry in the gallery frame: 1280 with the panel, 900 stacked, 600 as a rail", async () => {
  const { app, page } = await launchGallery();
  try {
    const shell = page.getByTestId("gallery-shell").getByTestId("shell");
    const num = async (name: string) => Number(await shell.getAttribute(name));
    await page.getByTestId("shell-width-1280").click();
    await expect.poll(() => num("data-list-px")).toBe(260);
    expect(await num("data-center-px")).toBe(424);
    expect(await num("data-panel-px")).toBe(596);
    await expect(shell).toHaveAttribute("data-stacked", "false");
    await page.getByTestId("shell-panel-toggle").click();
    await expect.poll(() => num("data-panel-px")).toBe(0);
    await expect.poll(() => num("data-center-px")).toBe(1020);
    await page.getByTestId("shell-panel-toggle").click();
    await page.getByTestId("shell-width-900").click();
    await expect(shell).toHaveAttribute("data-stacked", "true");
    await expect(page.getByTestId("gallery-shell").getByTestId("stack-segments")).toBeVisible();
    await page.getByTestId("shell-width-600").click();
    await expect(shell).toHaveAttribute("data-rail", "true");
    expect(await num("data-list-px")).toBe(40);
    await expect(page.getByTestId("gallery-shell").getByTestId("agent-rail")).toBeVisible();
    // Keyboard resize inside the limits, in the gallery frame too.
    await page.getByTestId("shell-width-1280").click();
    await page.getByTestId("gallery-shell").getByTestId("splitter-list").focus();
    await page.keyboard.press("End");
    await expect.poll(() => num("data-list-px")).toBe(400);
    await page.keyboard.press("Home");
    await expect.poll(() => num("data-list-px")).toBe(210);
    await page.getByTestId("gallery-shell").getByTestId("splitter-panel").focus();
    await page.keyboard.press("Home");
    await expect.poll(() => num("data-panel-px")).toBe(384);
    await page.keyboard.press("End");
    await expect.poll(() => num("data-center-px")).toBe(424);
  } finally {
    await closeApp(app);
  }
});
