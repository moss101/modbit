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
import { _electron as electron, expect, test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PALETTES } from "../../../packages/design-tokens/src/index.ts";
import { agentRow, expectTaskState, openTask, runTask, setDensity } from "./support/new-task.ts";
import { accessible, appDir, box, closeApp, focusedTestId, launch, liftMinimumSize, makeRepo, MOD, scriptedModel, setContentSize } from "./support/ui-harness.ts";

/** `#rrggbb` as the browser reports a computed colour. The window-root colour of each theme is read from the one token source (DR-PX-2026-10-24-016 changed the palette). */
const rgb = (hex: string) => `rgb(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)})`;

const near = (actual: number, expected: number, tol: number, what: string) => expect(Math.abs(actual - expected), `${what}: ${actual} vs ${expected}`).toBeLessThanOrEqual(tol);

test("DR-PX-2026-11-04-020: started the way `pnpm start` starts it, the app finds the Core with no override", async () => {
  const { MODBIT_CORE_BIN: _ignored, ...env } = process.env;
  const app = await electron.launch({ args: [appDir], env: { ...env, MODBIT_DATA_DIR: mkdtempSync(join(tmpdir(), "modbit-e2e-shell-start-")), OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_QUIT_PROMPT: "off" } as Record<string, string> });
  try {
    const page = await app.firstWindow();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  } finally {
    await closeApp(app);
  }
});

test("AFW-A10: Settings replaces the left column with its own navigation, holds doc 10's list, says plainly what the desktop cannot do yet, and Back returns", async () => {
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-shell-settings-view-")));
  try {
    await setContentSize(app, page, 1280, 800);
    await page.getByTestId("open-settings").click();
    const nav = page.getByTestId("settings-nav");
    await expect(nav).toBeVisible();
    await expect(page.getByTestId("agents-new-task"), "the agent list has given way").toHaveCount(0);
    await expect(nav.getByTestId("core-status"), "the footer stays pinned").toContainText("Core connected");
    await expect(page.getByTestId("open-settings")).toHaveAttribute("aria-pressed", "true");
    // Doc 10's list, every one a real section.
    for (const id of ["provider", "locations", "permissions", "browser", "memory", "account"]) {
      await expect(nav.getByTestId(`settings-nav-${id}`)).toBeVisible();
      await expect(page.getByTestId(`settings-section-${id}`)).toBeVisible();
    }
    const column = await box(page, "settings");
    expect(column.width, "one centred column of about 680").toBeLessThanOrEqual(681);
    // The provider can be changed from here, and what is not built is said, never drawn as if it worked.
    await expect(page.getByTestId("provider-key")).toBeVisible();
    await expect(page.getByTestId("settings-cloud-state")).toContainText("Not available from this app yet");
    await expect(page.getByTestId("settings-account-state")).toContainText("no sign-in yet");
    // A control does what it says: the theme applies, the toggle is a real switch of 32 x 18.
    await page.getByTestId("settings-theme-dark").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await page.getByTestId("settings-theme-light").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    const toggle = await box(page, "notify-attention");
    near(toggle.width, 32, 1, "toggle width");
    near(toggle.height, 18, 1, "toggle height");
    await page.getByTestId("notify-attention").check();
    await expect(page.getByTestId("notify-attention")).toBeChecked();
    // Search narrows the navigation and the page together; a miss says so.
    await page.getByTestId("settings-search").fill("cloud");
    await expect(page.getByTestId("settings-section-locations")).toBeVisible();
    await expect(page.getByTestId("settings-section-appearance")).toHaveCount(0);
    await expect(nav.getByTestId("settings-nav-appearance")).toHaveCount(0);
    await page.getByTestId("settings-search").fill("zzzzqqq");
    await expect(page.getByTestId("settings-no-match")).toBeVisible();
    await page.getByTestId("settings-search").fill("");
    await accessible(page, "the Settings view");
    await page.getByTestId("nav-back").click();
    await expect(nav).toHaveCount(0);
    await expect(page.getByTestId("agents-new-task")).toBeVisible();
    await expect(page.getByTestId("goal")).toBeVisible();
    // The person's name opens the other menu, and the gear is not hidden inside it.
    await page.getByTestId("overflow-menu").click();
    await expect(page.getByTestId("menu-item-open-worktrees")).toBeVisible();
    await expect(page.getByTestId("menu-item-settings.open")).toHaveCount(0);
    await page.keyboard.press("Escape");
  } finally {
    await closeApp(app);
  }
});

test("PX-045, PX-198: geometry at 1280 x 800, keyboard-resizable splits inside 200..420, the layout and the density persisted across a restart", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-geom-"));
  let { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    near((await box(page, "region-agents")).width, 288, 2, "agent list");
    near((await box(page, "top-bar")).height, 45, 1, "top bar");
    expect(await page.getByTestId("region-apps").count(), "the apps panel is absent until a task has an artifact").toBe(0);
    // MWS-05: a toggle stands for a pane the open task can show; with no task there is none (the one disabled "Apps panel" toggle is gone).
    await expect(page.getByTestId("pane-toggles")).toHaveCount(0);
    await expect(page.getByTestId("toggle-apps-panel")).toHaveCount(0);
    expect((await box(page, "region-conversation")).width).toBeGreaterThanOrEqual(424);

    const sep = page.getByTestId("splitter-list");
    await expect(sep).toHaveAttribute("role", "separator");
    await expect(sep).toHaveAttribute("aria-valuenow", "288");
    await expect(sep).toHaveAttribute("aria-valuemin", "200");
    await expect(sep).toHaveAttribute("aria-valuemax", "420");
    await sep.focus();
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "336");
    await page.keyboard.press("Home");
    await expect(sep).toHaveAttribute("aria-valuenow", "200");
    await page.keyboard.press("ArrowLeft");
    await expect(sep, "the minimum holds").toHaveAttribute("aria-valuenow", "200");
    await page.keyboard.press("End");
    await expect(sep).toHaveAttribute("aria-valuenow", "420");
    await page.keyboard.press("ArrowRight");
    await expect(sep, "the maximum holds").toHaveAttribute("aria-valuenow", "420");
    near((await box(page, "region-agents")).width, 420, 2, "agent list at its maximum");
    await page.keyboard.press("Enter");
    await expect(sep, "Enter restores the default").toHaveAttribute("aria-valuenow", "288");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "320");
    // A pointer drag moves it too.
    const b = (await sep.boundingBox())!;
    await page.mouse.move(b.x + b.width / 2, b.y + 100);
    await page.mouse.down();
    await page.mouse.move(b.x + b.width / 2 + 40, b.y + 100, { steps: 4 });
    await page.mouse.up();
    const dragged = Number(await sep.getAttribute("aria-valuenow"));
    expect(dragged).toBeGreaterThan(320);
    expect(dragged).toBeLessThanOrEqual(420);
    await sep.focus();
    await page.keyboard.press("Home");
    await page.keyboard.press("ArrowRight");
    await expect(sep).toHaveAttribute("aria-valuenow", "216");

    // MWS-03: the density is one setting under Settings, Appearance, stored with the layout and separate from zoom.
    await setDensity(page, "compact");
    near((await box(page, "top-bar")).height, 40, 1, "top bar in Compact");
    near((await box(page, "agents-new-task")).height, 30, 0.5, "navigation row in Compact");
    expect(await page.evaluate(() => getComputedStyle(document.body).fontSize), "chrome text in Compact").toBe("13px");
    expect(await page.evaluate(() => document.documentElement.style.zoom), "the density is not the zoom").toBe("");
    near((await box(page, "region-agents")).width, 216, 2, "the density leaves the list's width alone");

    // The layout and the density survive an app restart: they are renderer-local preferences, not task state.
    await closeApp(app);
    ({ app, page } = await launch(dataDir));
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("splitter-list")).toHaveAttribute("aria-valuenow", "216");
    near((await box(page, "region-agents")).width, 216, 2, "agent list after the restart");
    await expect(page.locator("html"), "the density after the restart").toHaveAttribute("data-density", "compact");
    near((await box(page, "top-bar")).height, 40, 1, "top bar after the restart");
    near((await box(page, "agents-new-task")).height, 30, 0.5, "navigation row after the restart");
    await page.getByTestId("open-settings").click();
    await expect(page.getByTestId("settings-density-compact")).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("settings-density-comfortable")).toHaveAttribute("aria-checked", "false");
    await accessible(page, "Settings with the density control, in Compact", '[data-testid="settings-section-appearance"]');
    // Back to Comfortable: the override leaves the root, and the measured numbers return.
    await page.getByTestId("settings-density-comfortable").click();
    await expect(page.locator("html")).not.toHaveAttribute("data-density", /.+/);
    await page.getByTestId("nav-back").click();
    near((await box(page, "top-bar")).height, 45, 1, "top bar back in Comfortable");
    near((await box(page, "agents-new-task")).height, 32, 0.5, "navigation row back in Comfortable");
    expect(await page.evaluate(() => getComputedStyle(document.body).fontSize), "chrome text in Comfortable").toBe("14px");
  } finally {
    await closeApp(app);
  }
});

test("PX-045: below a 448 centre the shell is single-pane with the 40 px rail; the list can also be collapsed", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-rail-"));
  const { app, page } = await launch(dataDir);
  try {
    // The window cannot be made this small (PX-049 keeps 900 x 600); the layout below that width is still the renderer's to get right.
    await liftMinimumSize(app);
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
    near((await box(page, "region-agents")).width, 288, 2, "the expanded list");
  } finally {
    await closeApp(app);
  }
});

test("PX-045: at 1000 x 700 and 900 x 600 the list keeps 288, the top bar 45 and the centre stays above the single-pane threshold with nothing overflowing", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-sizes-"));
  const { app, page } = await launch(dataDir);
  try {
    for (const [w, h] of [[1000, 700], [900, 600]] as const) {
      await setContentSize(app, page, w, h);
      near((await box(page, "region-agents")).width, 288, 2, `agent list at ${w}`);
      near((await box(page, "top-bar")).height, 45, 1, `top bar at ${w}`);
      expect((await box(page, "region-conversation")).width, `centre at ${w}`).toBeGreaterThanOrEqual(448);
      await expect(page.getByTestId("agent-rail"), `no rail at ${w}`).toHaveCount(0);
      await expect(page.getByTestId("shell")).not.toHaveAttribute("data-single-pane", "true");
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `no horizontal overflow at ${w}`).toBe(true);
      await accessible(page, `shell at ${w} x ${h}`);
    }
  } finally {
    await closeApp(app);
  }
});

test("PX-045: opening Settings with a running task neither stops nor alters the task", async () => {
  // A model that accepts the request and never answers keeps the task running for the whole test.
  const stalled = createServer(() => undefined);
  await new Promise<void>((r) => stalled.listen(0, "127.0.0.1", r));
  const url = `http://127.0.0.1:${(stalled.address() as { port: number }).port}`;
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-e2e-shell-settings-repo-")));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-settings-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await runTask(page, "Keep running while settings open", repo);
    await expectTaskState(page, taskId, "Running");
    await page.keyboard.press(`${MOD}+Comma`);
    await expect.poll(() => focusedTestId(page)).toBe("settings");
    await page.keyboard.press("Escape");
    await page.getByTestId("nav-back").click();
    await expect(page.getByTestId("conversation"), "the conversation is back").toHaveAttribute("data-task-id", taskId);
    await expect(agentRow(page, taskId), "still running after Settings opened and closed").toHaveAttribute("data-state", "Running");
  } finally {
    await closeApp(app);
    stalled.closeAllConnections();
    stalled.close();
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
    // Past the mode, the model and the repository to send.
    for (let i = 0; i < 4; i++) await page.keyboard.press("Tab");
    expect(await focusedTestId(page)).toBe("run");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", /.+/, { timeout: 30_000 });
    const taskId = (await page.getByTestId("conversation").getAttribute("data-task-id"))!;
    await expect(agentRow(page, taskId)).toContainText("Palette findable task");

    // Open with the chord, focus is in the field; Escape closes and focus returns to where it was.
    await page.getByTestId("agents-search-open").focus();
    await page.keyboard.press(`${MOD}+k`);
    await expect(page.getByTestId("palette")).toBeVisible();
    expect(await focusedTestId(page)).toBe("palette-input");
    await expect(page.getByTestId("palette")).toHaveAttribute("aria-modal", "true");
    await accessible(page, "command palette (empty query)");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    expect(await focusedTestId(page), "focus returns to where it was").toBe("agents-search-open");

    // It finds a task by title and goes to it, from home.
    await page.getByTestId("agents-new-task").click();
    await expect(page.getByTestId("conversation")).toHaveCount(0);
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("findable");
    const agentOption = page.getByTestId("palette-option").first();
    await expect(agentOption).toHaveAttribute("data-kind", "agent");
    await expect(agentOption).toContainText("Palette findable task");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);

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

test("PX-044/045: themes, high contrast and reduced motion; the send button meets AA; axe on every region in every theme", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-shell-theme-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    // Buttons transition their colours: measure the settled colours, never a frame mid-transition.
    const ratio = () =>
      page.evaluate(async () => {
        await Promise.all(document.getAnimations().map((a) => a.finished.catch(() => undefined)));
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
      expect(await ratio(), `the send button in ${theme}`).toBeGreaterThanOrEqual(4.5);
      await accessible(page, `home in the ${theme} theme`);
      for (const region of ["region-agents", "top-bar", "shell-content"]) await accessible(page, `${region} in ${theme}`, `[data-testid="${region}"]`);
    }
    // Follow system resolves from the OS: a dark emulation with no explicit theme.
    await page.keyboard.press(`${MOD}+k`);
    await page.keyboard.type("theme follow");
    await page.keyboard.press("Enter");
    await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
    await page.emulateMedia({ colorScheme: "dark" });
    await expect.poll(() => page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb(PALETTES.dark.bg));
    await accessible(page, "home following a dark system");
    await page.emulateMedia({ colorScheme: "light" });
    await expect.poll(() => page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb(PALETTES.light.bg));

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
    const taskId = await runTask(page, "Edit the notes", repo);
    await expectTaskState(page, taskId, "ReadyForReview");
    // The first result opens the Review itself inside the conversation region; the title follows the task.
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("top-bar-title")).toContainText("Edit the notes");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
    await expect(page.getByTestId("top-bar-title")).toContainText("Edit the notes");
    // MWS-05: the task has run, so it has a toggle per pane it can show, each saying it is closed; the terminal has none because the Core lists no terminal.
    for (const kind of ["changes", "browser", "files", "evidence"]) await expect(page.getByTestId(`pane-toggle-${kind}`), kind).toHaveAttribute("aria-pressed", "false");
    await expect(page.getByTestId("pane-toggle-terminal")).toHaveCount(0);
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
    await accessible(page, "the workspace with the apps panel open");
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
    await expect(page.getByTestId("app-terminal-empty")).toContainText("no terminal");
    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1")!));
    expect(Object.keys(stored.panelByTask)).toHaveLength(1);
    expect(Object.values(stored.panelByTask)[0]).toMatchObject({ open: true, tab: "terminal" });
    expect(JSON.stringify(stored)).not.toContain("Edit the notes");

    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } }));
    await setContentSize(app, page, 1280, 800);
    await expectTaskState(page, taskId, "ReadyForReview", 30_000);
    // The review opens by itself only on the live event; this restart shows home, and the task opens with the remembered panel.
    await openTask(page, taskId);
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
    await expect(page.getByTestId("conversation")).toHaveCount(0);
    await accessible(page, "stacked apps panel");
    await page.getByTestId("segment-conversation").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("conversation")).toBeVisible();

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
