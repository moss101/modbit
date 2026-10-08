/**
 * REQ-PX-045 / QUAL-PX-045 (docs/62, docs/65 AFW-A01..A16): the agent-first
 * shell in the real Electron app against the real Core. Three regions with the
 * geometry the spec states, resizable splits that persist across an app
 * restart, the command palette and the shortcut registry operated by keyboard
 * alone, the layer rule (Escape closes the innermost layer), focus returned
 * where it was, themes and reduced motion, the apps panel hidden until a task
 * has an artifact, and the accessibility suite on every region. The model is a
 * scripted stand-in only for the one flow that needs a task with a change set.
 */
import { expect, test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, box, closeApp, focusedTestId, launch, makeRepo, MOD, scriptedModel, setContentSize } from "./support/ui-harness.ts";

const near = (actual: number, expected: number, tol: number, what: string) => expect(Math.abs(actual - expected), `${what}: ${actual} vs ${expected}`).toBeLessThanOrEqual(tol);

test("PX-045: geometry at 1280 x 800, keyboard-resizable splits inside 210..400, persisted across a restart", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-geom-"));
  let { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    near((await box(page, "region-agents")).width, 260, 2, "agent list");
    near((await box(page, "top-bar")).height, 40, 1, "top bar");
    expect(await page.getByTestId("region-apps").count(), "the apps panel is absent until a task has an artifact").toBe(0);
    await expect(page.getByTestId("toggle-apps-panel")).toHaveAttribute("aria-disabled", "true");
    expect((await box(page, "region-conversation")).width).toBeGreaterThanOrEqual(424);

    const sep = page.getByTestId("splitter-list");
    await expect(sep).toHaveAttribute("role", "separator");
    await expect(sep).toHaveAttribute("aria-valuenow", "260");
    await expect(sep).toHaveAttribute("aria-valuemin", "210");
    await expect(sep).toHaveAttribute("aria-valuemax", "400");
    await sep.focus();
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "308");
    await page.keyboard.press("Home");
    await expect(sep).toHaveAttribute("aria-valuenow", "210");
    await page.keyboard.press("ArrowLeft");
    await expect(sep, "the minimum holds").toHaveAttribute("aria-valuenow", "210");
    await page.keyboard.press("End");
    await expect(sep).toHaveAttribute("aria-valuenow", "400");
    await page.keyboard.press("ArrowRight");
    await expect(sep, "the maximum holds").toHaveAttribute("aria-valuenow", "400");
    near((await box(page, "region-agents")).width, 400, 2, "agent list at its maximum");
    await page.keyboard.press("Enter");
    await expect(sep, "Enter restores the default").toHaveAttribute("aria-valuenow", "260");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "292");
    // A pointer drag moves it too.
    const b = (await sep.boundingBox())!;
    await page.mouse.move(b.x + b.width / 2, b.y + 100);
    await page.mouse.down();
    await page.mouse.move(b.x + b.width / 2 + 40, b.y + 100, { steps: 4 });
    await page.mouse.up();
    const dragged = Number(await sep.getAttribute("aria-valuenow"));
    expect(dragged).toBeGreaterThan(292);
    expect(dragged).toBeLessThanOrEqual(400);
    await sep.focus();
    await page.keyboard.press("Home");
    await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "226");

    // The layout survives an app restart: it is a renderer-local preference, not task state.
    await closeApp(app);
    ({ app, page } = await launch(dataDir));
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("splitter-list")).toHaveAttribute("aria-valuenow", "226");
    near((await box(page, "region-agents")).width, 226, 2, "agent list after the restart");
  } finally {
    await closeApp(app);
  }
});

test("PX-045: below a 448 centre the shell is single-pane with the 40 px rail; the list can also be collapsed", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-rail-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 600, 700);
    await expect(page.getByTestId("agent-rail")).toBeVisible();
    await expect(page.getByTestId("shell")).toHaveAttribute("data-single-pane", "true");
    near((await box(page, "region-agents")).width, 40, 1, "the rail");
    await expect(page.getByTestId("splitter-list")).toHaveCount(0);
    await accessible(page, "single pane with the rail");
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("agent-rail")).toHaveCount(0);
    // Collapsing by the person gives the same rail, and expanding it again restores the list.
    await page.getByTestId("toggle-agent-list").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("agent-rail")).toBeVisible();
    near((await box(page, "region-agents")).width, 40, 1, "the collapsed rail");
    await page.getByTestId("rail-expand").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("agent-rail")).toHaveCount(0);
    near((await box(page, "region-agents")).width, 260, 2, "the expanded list");
  } finally {
    await closeApp(app);
  }
});

test("PX-045: the command palette and the shortcut registry by keyboard alone; Escape closes the innermost layer; focus returns", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-palette-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    // A task to find: created from the keyboard.
    await page.keyboard.press(`${MOD}+n`);
    expect(await focusedTestId(page)).toBe("goal");
    await page.keyboard.type("Palette findable task");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    const card = page.getByTestId("task-card").filter({ hasText: "Palette findable task" });
    await expect(card).toHaveCount(1, { timeout: 30_000 });
    const taskId = (await card.getAttribute("data-task-id"))!;

    // Open with the chord, focus is in the field; Escape closes and focus returns to where it was.
    await page.keyboard.press("Escape");
    await page.keyboard.press("/");
    expect(await focusedTestId(page)).toBe("search");
    await page.keyboard.press(`${MOD}+k`);
    await expect(page.getByTestId("palette")).toBeVisible();
    expect(await focusedTestId(page)).toBe("palette-input");
    await expect(page.getByTestId("palette")).toHaveAttribute("aria-modal", "true");
    await accessible(page, "command palette (empty query)");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    expect(await focusedTestId(page), "focus returns to where it was").toBe("search");
    await page.keyboard.press("Escape");

    // It finds a task by title and goes to it.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("findable");
    const agentOption = page.getByTestId("palette-option").first();
    await expect(agentOption).toHaveAttribute("data-kind", "agent");
    await expect(agentOption).toContainText("Palette findable task");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="task-card"]')?.getAttribute("data-task-id") ?? "")).toBe(taskId);

    // It finds a setting and focuses it.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("notification");
    const settingOption = page.getByTestId("palette-option").first();
    await expect(settingOption).toHaveAttribute("data-kind", "setting");
    await page.keyboard.press("Enter");
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="notification-preferences"]') !== null)).toBe(true);

    // It runs a theme command, and the theme is applied and remembered by the shell.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("theme dark");
    await expect(page.getByTestId("palette-option").first()).toContainText("Theme: Dark");
    await page.keyboard.press("Enter");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    // Arrow keys move the active option; an unavailable command says why and keeps the palette open.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("show changes");
    const changes = page.getByTestId("palette-option").first();
    await expect(changes).toContainText("Show changes");
    await expect(changes).toHaveAttribute("aria-disabled", "true");
    await expect(changes).toContainText("unavailable");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("palette")).toBeVisible();
    await expect(page.getByTestId("shell-notice")).toContainText("unavailable");
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.press("Backspace");
    await page.keyboard.press("ArrowDown");
    await expect(page.getByTestId("palette-option").nth(1)).toHaveAttribute("data-active", "true");
    await expect(page.getByTestId("palette-input")).toHaveAttribute("aria-activedescendant", /.+/);
    await page.keyboard.type("zzzzqqq");
    await expect(page.getByTestId("palette-empty")).toBeVisible();
    await page.keyboard.press("Escape");

    // The layer rule: with a menu open and the palette above it, Escape closes the palette first, then the menu, and focus returns to the menu's trigger.
    await page.getByTestId("overflow-menu").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("menu")).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press(`${MOD}+k`);
    await expect(page.getByTestId("palette")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect(page.getByRole("menu"), "the menu is the next layer").toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toHaveCount(0);
    expect(await focusedTestId(page)).toBe("overflow-menu");

    // Shortcut help: the visible list comes from the registry; every default chord of the spec is on it.
    await page.keyboard.press("Control+Shift+Slash");
    await expect(page.getByTestId("help")).toBeVisible();
    for (const id of ["task.new", "palette.open", "app.changes", "app.terminal", "app.browser", "app.files", "settings.open", "help.shortcuts", "zoom.in", "zoom.out", "zoom.reset"]) await expect(page.getByTestId("help").locator(`[data-command="${id}"]`)).toHaveCount(1);
    await accessible(page, "shortcut help");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("help")).toHaveCount(0);

    // Every remaining chord fires its command.
    await page.keyboard.press(`${MOD}+Comma`);
    await expect.poll(() => focusedTestId(page)).toBe("settings");
    await page.keyboard.press(`${MOD}+n`);
    await expect.poll(() => focusedTestId(page)).toBe("goal");
    for (const [chord, what] of [[`${MOD}+e`, "Show changes"], [`${MOD}+j`, "Toggle terminal"], [`${MOD}+Shift+b`, "Show browser"], [`${MOD}+g`, "Show files"]] as const) {
      await page.keyboard.press(chord);
      await expect(page.getByTestId("shell-notice"), `${chord} ran ${what}`).toContainText(`${what} is unavailable`);
    }
    await page.keyboard.press(`${MOD}+Equal`);
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("1.08");
    await page.keyboard.press(`${MOD}+Minus`);
    await page.keyboard.press(`${MOD}+Minus`);
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("0.92");
    await page.keyboard.press(`${MOD}+0`);
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("");
  } finally {
    await closeApp(app);
  }
});

test("PX-044/045: themes, high contrast and reduced motion; the primary button meets AA; axe on every region in every theme", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-theme-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    const ratio = () =>
      page.evaluate(() => {
        const lum = (css: string) => {
          const m = css.match(/\d+(\.\d+)?/g)!.slice(0, 3).map(Number);
          const lin = m.map((v) => ((v / 255) <= 0.04045 ? v / 255 / 12.92 : (((v / 255) + 0.055) / 1.055) ** 2.4));
          return 0.2126 * lin[0]! + 0.7152 * lin[1]! + 0.0722 * lin[2]!;
        };
        const s = getComputedStyle(document.querySelector('[data-testid="run"]')!);
        const a = lum(s.color);
        const b = lum(s.backgroundColor);
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
      });
    for (const theme of ["light", "dark", "high-contrast"] as const) {
      await page.keyboard.press(`${MOD}+k`);
      await page.keyboard.type(`theme ${theme === "high-contrast" ? "high" : theme}`);
      await page.keyboard.press("Enter");
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      expect(await ratio(), `the primary button in ${theme}`).toBeGreaterThanOrEqual(4.5);
      await accessible(page, `fleet in the ${theme} theme`);
      for (const region of ["region-agents", "top-bar", "shell-content", "status-row"]) await accessible(page, `${region} in ${theme}`, `[data-testid="${region}"]`);
    }
    // Follow system resolves from the OS: a dark emulation with no explicit theme.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("theme follow");
    await page.keyboard.press("Enter");
    await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
    await page.emulateMedia({ colorScheme: "dark" });
    await expect.poll(() => page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(18, 20, 26)");
    await accessible(page, "fleet following a dark system");
    await page.emulateMedia({ colorScheme: "light" });
    await expect.poll(() => page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(243, 244, 247)");

    // Reduced motion: no transition or animation runs.
    const motion = () => page.evaluate(() => {
      const el = document.querySelector<HTMLElement>('[data-testid="toggle-agent-list"]')!;
      const s = getComputedStyle(el);
      return { transition: s.transitionDuration, property: s.transitionProperty };
    });
    await page.emulateMedia({ reducedMotion: "no-preference" });
    expect((await motion()).transition).not.toBe("0s");
    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect.poll(async () => (await motion()).transition).toBe("0s");
  } finally {
    await closeApp(app);
  }
});

test("PX-045: the apps panel appears for a task with an artifact, within the geometry; per-task state and width persist; it stacks when it cannot fit", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-shell-repo-")));
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } }] },
    { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-panel-"));
  let { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await page.getByTestId("goal").fill("Edit the notes");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await card.getByTestId("task-start").click();
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 60_000 });
    // The first result opens the Review itself inside the conversation region; the title follows the task.
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("top-bar-title")).toContainText("Edit the notes");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expect(page.getByTestId("top-bar-title")).toHaveText("Fleet");
    await expect(page.getByTestId("toggle-apps-panel")).toHaveAttribute("aria-disabled", "false");
    expect(await page.getByTestId("region-apps").count()).toBe(0);

    // The changes chord opens the panel; the geometry holds: centre >= 424, panel >= 384.
    await page.keyboard.press(`${MOD}+e`);
    await expect(page.getByTestId("region-apps")).toBeVisible();
    await expect(page.getByTestId("app-changes")).toBeVisible();
    const centre = await box(page, "region-conversation");
    const panel = await box(page, "region-apps");
    expect(centre.width).toBeGreaterThanOrEqual(423);
    expect(panel.width).toBeGreaterThanOrEqual(383);
    near(centre.width + panel.width + (await box(page, "region-agents")).width, 1280, 12, "the three regions fill the window");
    await accessible(page, "fleet with the apps panel open");
    await accessible(page, "the apps panel", '[data-testid="region-apps"]');
    // The panel's button leads into the existing Review.
    await page.getByTestId("panel-open-review").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("review")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);

    // Resize the panel from the keyboard; remember a tab per task.
    const sep = page.getByTestId("splitter-panel");
    await expect(sep).toHaveAttribute("role", "separator");
    await sep.focus();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    const narrowed = Number(await sep.getAttribute("aria-valuenow"));
    expect(narrowed).toBeLessThan(panel.width);
    expect(narrowed).toBeGreaterThanOrEqual(384);
    await page.getByTestId("tab-terminal").click();
    await expect(page.getByTestId("app-terminal-pending")).toContainText("PX-048");
    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1")!));
    expect(Object.keys(stored.panelByTask)).toHaveLength(1);
    expect(Object.values(stored.panelByTask)[0]).toMatchObject({ open: true, tab: "terminal" });
    expect(JSON.stringify(stored)).not.toContain("Edit the notes");

    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } }));
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    // The review opens by itself only on the live event; this restart shows the fleet with the remembered panel.
    await page.getByTestId("task-card").first().focus();
    await expect(page.getByTestId("region-apps")).toBeVisible();
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("splitter-panel")).toHaveAttribute("aria-valuenow", String(narrowed));

    // Too narrow to sit beside a 424 centre: Conversation | Apps segments.
    await setContentSize(app, page, 900, 700);
    await expect(page.getByTestId("stack-segments")).toBeVisible();
    await expect(page.getByTestId("region-apps")).toHaveCount(0);
    await page.getByTestId("segment-apps").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("apps-panel")).toBeVisible();
    await expect(page.getByTestId("fleet")).toHaveCount(0);
    await accessible(page, "stacked apps panel");
    await page.getByTestId("segment-conversation").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("fleet")).toBeVisible();

    // Toggling the panel closed is remembered for this task.
    await setContentSize(app, page, 1280, 800);
    await page.getByTestId("toggle-apps-panel").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("region-apps")).toHaveCount(0);
    const after = await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1")!));
    expect(Object.values(after.panelByTask)[0]).toMatchObject({ open: false });
  } finally {
    await closeApp(app);
    server.close();
  }
});
