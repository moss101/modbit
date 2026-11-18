/**
 * QUAL-PX-208 / PX-E2E-208 (DR-PX-2026-11-11-021 MWS-14): Review and Settings in the page shape, on the real Electron build, a
 * real Core and a scripted model. Two tasks change files in two repositories and wait for review.
 *  - Review: the navigation row opens the Review page; it lists both tasks' local changes (no pull request exists), and for the
 *    chosen task shows the change set and the findings the Core's review bundle carries, equal to what the Core answers; the page
 *    has the page shape (a head the bar's height, rows of at least 44 px with a 1 px edge). Open review shows the task's own
 *    review over the page, and a hunk decision still works through the same path (`review:decide` with the rejected hunk): Git
 *    holds exactly the choice, and Back returns to the Review page.
 *  - Appearance: density, theme, zoom, the task rows' second line and the grouping each take effect at once (no reload: a marker
 *    set on the window survives) and persist across an app restart; the list's own options read the same choice.
 *  - Shortcuts: the section equals the registry and the key maps, binding for binding (the spec builds the same sheet from the
 *    same modules and compares), and every listed key, pressed in the running app in its own context, is taken by the app.
 * Fail conditions, each with a retained mutation the check is shown to catch:
 *  - a listed shortcut that is not bound: a row for "m" added to the section is reported by `unboundListed`;
 *  - a setting that belongs to one task shown as global: a Run mode control added to Settings is reported by `perTaskAsGlobal`;
 *  - a density change that needs a reload: the density left unapplied after Compact is chosen is reported by `densityDrift`.
 * Bridge calls are read from a tap on main's handlers added by the test (as in PX-201 and PX-209); the product is not changed.
 * With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { CommandDef } from "@modbit/ui/logic";
import { routedModel, type ModelPart } from "./support/conversation-harness.ts";
import { agentRow, expectTaskState, gearMenu, runTask, setDensity } from "./support/new-task.ts";
import { accessible, box, closeApp, git, launch, setContentSize } from "./support/ui-harness.ts";
import { shellRegistry } from "../src/renderer/shell/commands.ts";
import { SCOPES, shortcutSheet, type KeyProbe } from "../src/renderer/settings/shortcut-sheet.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL_A = "Edit the px208 notes";
const GOAL_B = "Tidy the px208 readme";
const OPEN_FINDING = "The second edit keeps an old line on purpose, as asked";
const DONE_FINDING = "Checked the first edit against the notes";
const ORIGINAL = Array.from({ length: 16 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
const CANDIDATE = ORIGINAL.replace("line 2\n", "line 2 changed\n").replace("line 15\n", "line 15 changed\n");

function repoWith(file: string, text: string): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-e2e-px208-repo-"));
  writeFileSync(join(repo, file), text);
  git(repo, "init", "-q", "-b", "main");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "fixture-noop", argv: ["git", "--version"] }] }));
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

function script(file: string, content: string, findings: { text: string; resolved: boolean }[]): ModelPart[][] {
  return [
    [{ call: { name: "plan.update", args: { outcome: `edit ${file}`, expected_files: [file] } } }],
    [{ call: { name: "fs.read", args: { path: file } } }],
    [{ call: { name: "change.apply", args: { path: file, op: "replace", content } } }],
    [{ call: { name: "task.complete", args: { summary: "edited", self_review: { findings } } } }],
  ];
}

// ---------------------------------------------------------------- the bridge's log (a tap in main, as PX-209)

async function tapBridge(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ ipcMain }) => {
    const g = globalThis as unknown as { __bridgeLog?: unknown[] };
    if (g.__bridgeLog) return;
    const log: { channel: string; args: unknown[] }[] = [];
    g.__bridgeLog = log;
    const handlers = (ipcMain as unknown as { _invokeHandlers: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    for (const [channel, fn] of [...handlers.entries()])
      handlers.set(channel, async (event: unknown, ...args: unknown[]) => {
        log.push({ channel, args });
        return fn(event, ...args);
      });
  });
}
const callsOn = (app: ElectronApplication, channel: string) => app.evaluate((_e, c) => ((globalThis as unknown as { __bridgeLog?: { channel: string; args: unknown[] }[] }).__bridgeLog ?? []).filter((x) => x.channel === c).map((x) => x.args), channel);

// ---------------------------------------------------------------- checks

/** What breaks the page shape of MWS-14 on the Review page (empty when it holds). */
async function pageShapeDrift(page: Page, bar: number): Promise<string[]> {
  const bad: string[] = [];
  const head = await page.locator(".rp-head").boundingBox();
  if (!head || Math.abs(head.height - bar) > 1) bad.push(`the page's head is ${head?.height} high, not ${bar}`);
  const rows = await page.getByTestId("review-page-row").evaluateAll((els) => els.map((e) => ({ h: e.getBoundingClientRect().height, edge: getComputedStyle(e).borderBottomWidth })));
  for (const r of rows) {
    if (r.h < 44 - 0.5) bad.push(`a row is ${r.h} high, under 44`);
    if (r.edge !== "1px") bad.push(`a row's edge is ${r.edge}, not 1px`);
  }
  const primary = await page.locator(".rp-head button[data-variant='primary']").count();
  if (primary !== 1) bad.push(`the head has ${primary} primary actions, not one`);
  return bad;
}

/** A setting that belongs to one task, drawn as a control on the Settings page (empty when none is). */
async function perTaskAsGlobal(page: Page): Promise<string[]> {
  return page.getByTestId("settings").evaluate((root) => {
    const perTask = /\b(run mode|send behaviou?r|allowlist|model|objective|budget|auto-?fix|merge when ready)\b/i;
    const out: string[] = [];
    for (const el of root.querySelectorAll<HTMLElement>("input, select, textarea, [role='radio'], [role='radiogroup'], [role='switch'], [role='combobox']")) {
      const row = el.closest(".settings-row")?.querySelector(".settings-row-text > div:first-child")?.textContent ?? "";
      const label = `${el.getAttribute("aria-label") ?? ""} ${el.closest("[role='radiogroup']")?.getAttribute("aria-label") ?? ""} ${row}`;
      // The provider form names models and keys for the whole app (they are not one task's); it is not a per-task setting.
      if (el.closest("[data-testid='settings-section-provider']")) continue;
      if (perTask.test(label)) out.push(`"${label.trim()}" is a control on the Settings page`);
    }
    return out;
  });
}

/** What shows the density did not apply at once (empty when it did): the root's attribute, the bar and a row, and no reload. */
async function densityDrift(page: Page, density: "comfortable" | "compact"): Promise<string[]> {
  const bad: string[] = [];
  const want = density === "compact" ? { bar: 40, row: 30, text: "13px" } : { bar: 45, row: 32, text: "14px" };
  const got = await page.evaluate(() => ({ attr: document.documentElement.dataset.density ?? "comfortable", marker: (window as unknown as { __px208?: string }).__px208 ?? "", text: getComputedStyle(document.body).fontSize }));
  if (got.attr !== density) bad.push(`the root says ${got.attr} after ${density} was chosen`);
  if (got.marker !== "set") bad.push("the window reloaded");
  if (got.text !== want.text) bad.push(`chrome text is ${got.text}, not ${want.text}`);
  const bar = (await box(page, "top-bar")).height;
  if (Math.abs(bar - want.bar) > 1) bad.push(`the bar is ${bar} px, not ${want.bar}`);
  const row = (await box(page, "settings-nav-appearance")).height;
  if (Math.abs(row - want.row) > 0.5) bad.push(`a navigation row is ${row} px, not ${want.row}`);
  return bad;
}

interface ListedRow {
  id: string;
  scope: string;
  probes: KeyProbe[];
}
const listedRows = (page: Page): Promise<ListedRow[]> => page.getByTestId("settings-shortcut").evaluateAll((els) => els.map((e) => ({ id: e.getAttribute("data-id")!, scope: e.getAttribute("data-scope")!, probes: JSON.parse(e.getAttribute("data-probes") ?? "[]") as KeyProbe[] })));

/** Dispatches one key at `target` (the focused element, or the body) and says whether the app took it (the event was cancelled). */
async function pressTaken(page: Page, p: KeyProbe, target: "body" | "focus"): Promise<boolean> {
  return page.evaluate(
    ([probe, where]) => {
      const el = where === "focus" ? (document.activeElement as HTMLElement) : document.body;
      if (where === "body") (document.activeElement as HTMLElement | null)?.blur?.();
      const ev = new KeyboardEvent("keydown", { key: probe!.key, ctrlKey: probe!.ctrlKey, metaKey: probe!.metaKey, altKey: probe!.altKey, shiftKey: probe!.shiftKey, bubbles: true, cancelable: true });
      return !el.dispatchEvent(ev);
    },
    [p, target] as const,
  );
}

/** The listed keys of `rows` that the app does not take in their context (empty when every one is bound); `prepare` sets the context, `after` undoes the key's effect. */
async function unboundListed(page: Page, rows: ListedRow[], target: "body" | "focus", prepare: () => Promise<void>, after: (row: ListedRow) => Promise<void>): Promise<string[]> {
  const out: string[] = [];
  for (const r of rows)
    for (const p of r.probes) {
      await prepare();
      if (!(await pressTaken(page, p, target))) out.push(`${r.id}: ${p.chord} is listed and not bound`);
      await after(r);
    }
  return out;
}

/** Shows Settings (the gear toggles it, so it is pressed only when Settings is not showing). */
async function toSettings(page: Page): Promise<void> {
  if ((await page.getByTestId("settings-nav").count()) === 0) await gearMenu(page, "settings.open");
  await expect(page.getByTestId("settings-nav")).toBeVisible();
}

/** Shows the Review page (its navigation row toggles it, so it is pressed only when the page is not showing). */
async function toReviewPage(page: Page): Promise<void> {
  if ((await page.getByTestId("settings-nav").count()) > 0) await page.getByTestId("nav-back").click();
  if ((await page.getByTestId("review").count()) > 0) await page.getByTestId("review-close").click();
  if ((await page.getByTestId("review-page").count()) === 0) await page.getByTestId("open-review").click();
  await expect(page.getByTestId("review-page")).toBeVisible();
}

// ---------------------------------------------------------------- screenshots

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

async function shots(page: Page, name: string, show: () => Promise<void>): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const density of ["comfortable", "compact"] as const) {
    await setDensity(page, density);
    await show();
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(SHOTS, `${name}-${density}-${theme}.png`) });
    }
  }
  await setDensity(page, "comfortable");
  await setTheme(page, "light");
}

// ---------------------------------------------------------------- the run

test("QUAL-PX-208: Review and Settings in the page shape: change sets and findings, a hunk decision, Appearance at once and kept, Shortcuts equal to the key maps", async () => {
  test.setTimeout(480_000);
  const repoA = repoWith("notes.txt", ORIGINAL);
  const repoB = repoWith("readme.txt", "alpha\nbeta\n");
  const model = await routedModel((goal, results) => {
    const s = goal.startsWith(GOAL_A) ? script("notes.txt", CANDIDATE, [{ text: DONE_FINDING, resolved: true }, { text: OPEN_FINDING, resolved: true }]) : script("readme.txt", "alpha\nbeta tidied\n", []);
    return s[results] ?? ["Done."];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px208-"));
  const env = { MODBIT_OPENAI_BASE_URL: model.url };
  let { app, page } = await launch(dataDir, { env });
  try {
    await setContentSize(app, page, 1280, 800);
    await tapBridge(app);

    // Two tasks reach review. The first result opens its review by itself; Escape leaves it.
    const a = await runTask(page, GOAL_A, repoA);
    await expectTaskState(page, a, "ReadyForReview", 90_000);
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("review-close").click();
    await expect(page.getByTestId("review")).toHaveCount(0);
    const b = await runTask(page, GOAL_B, repoB);
    await expectTaskState(page, b, "ReadyForReview", 90_000);
    if ((await page.getByTestId("review").count()) > 0) await page.getByTestId("review-close").click();

    // ---- The Review row opens the Review page in the page shape.
    const nav = await page.getByTestId("agent-nav").locator("button").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
    expect(nav).toEqual(["agents-new-task", "open-inbox", "open-automations", "open-review", "open-customize"]);
    await shots(page, "sidebar-with-review", async () => {
      await page.getByTestId("agents-new-task").click();
    });
    await page.getByTestId("open-review").click();
    await expect(page.getByTestId("review-page")).toBeVisible();
    await expect(page.getByTestId("open-review")).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByTestId("top-bar")).toContainText("Review");
    await expect(page.getByTestId("review-page-pulls"), "no pull request exists, so none is listed").toHaveCount(0);
    await expect(page.getByTestId("review-page-local").getByTestId("review-page-row")).toHaveCount(2);
    await expect(page.getByTestId("review-page-count")).toHaveText("2 tasks have changes");
    expect(await pageShapeDrift(page, 45)).toEqual([]);

    // The chosen task's change set and findings are the Core's review bundle, in words.
    await page.locator(`[data-testid="review-page-row"][data-task-id="${a}"]`).click();
    const detail = page.getByTestId("review-page-detail");
    await expect(detail).toHaveAttribute("data-task-id", a);
    await expect(detail.getByTestId("review-page-file")).toHaveCount(1);
    const core = await page.evaluate((t) => window.modbit.reviewBundle(t), a);
    expect(await detail.getByTestId("review-page-file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path")))).toEqual(core.files.map((f) => f.path));
    await expect(detail.getByTestId("review-page-file")).toContainText("+2");
    await expect(detail.getByTestId("review-page-file")).toContainText("−2");
    // The latest run's checks, counted as the Core recorded them (a skipped check is counted, not a finding).
    const last = core.verificationRuns[core.verificationRuns.length - 1]!.checks;
    const passed = last.filter((c) => c.status === "PASS").length;
    expect(passed, "the Core's completion run passed its checks").toBeGreaterThan(0);
    await expect(detail.getByTestId("review-page-checks")).toContainText(`${passed} `);
    await expect(detail.getByTestId("review-page-finding").filter({ hasText: /configured_command|adv:/ })).toHaveCount(0);
    const findings = detail.getByTestId("review-page-finding");
    await expect(findings).toHaveCount(2);
    // The agent's self-review items as it recorded them (an unresolved one keeps the task from review, so both are resolved here;
    // the order of open before resolved is unit-tested in review-model.test.ts).
    expect(await findings.evaluateAll((els) => els.map((e) => e.textContent))).toEqual([`Self-review, resolved${DONE_FINDING}`, `Self-review, resolved${OPEN_FINDING}`]);
    await expect(detail.getByTestId("review-page-no-comments")).toContainText("once the changes are in a pull request");
    const text = (await page.getByTestId("review-page").innerText()) ?? "";
    expect(text, "plain words: no raw state or code").not.toMatch(/ReadyForReview|[A-Z]{2,}_[A-Z]{2,}|DI-\d/);
    await accessible(page, "the Review page");
    await shots(page, "review-page", async () => {
      await toReviewPage(page);
      await page.locator(`[data-testid="review-page-row"][data-task-id="${a}"]`).click();
      await expect(page.getByTestId("review-page-finding")).toHaveCount(2);
    });
    await toReviewPage(page);
    await page.locator(`[data-testid="review-page-row"][data-task-id="${a}"]`).click();

    // ---- A hunk decision still works: Open review shows the task's review over the page, through review:decide as before.
    const rejectSecond = async () => {
      await toReviewPage(page);
      await page.locator(`[data-testid="review-page-row"][data-task-id="${a}"]`).click();
      await page.getByTestId("review-page-open-review").click();
      await expect(page.getByTestId("review")).toBeVisible();
      await expect(page.getByTestId("review-hunk")).toHaveCount(2);
      const second = page.getByTestId("review-hunk").nth(1);
      await expect(second).toContainText("line 15 changed");
      await second.getByTestId("hunk-reject").check();
      await expect(second).toHaveAttribute("data-rejected", "true");
    };
    await rejectSecond();
    await shots(page, "hunk-decision", rejectSecond);
    if ((await page.getByTestId("review").count()) === 0) await rejectSecond();
    await page.getByTestId("review-note").fill("Keep the second region");
    await page.getByTestId("review-accept").click();
    await expect(page.getByTestId("review-result")).toContainText("1 hunk(s) reverted", { timeout: 30_000 });
    const decided = await callsOn(app, "review:decide");
    expect(decided.length).toBe(1);
    expect(decided[0]!.slice(1, 5)).toEqual([a, "ACCEPT", [{ path: "notes.txt", index: 1 }], "Keep the second region"]);
    await page.getByTestId("review-close").click();
    await expect(page.getByTestId("review-page"), "Back returns to the Review page").toBeVisible();
    await expectTaskState(page, a, "Completed", 30_000);
    expect(readFileSync(join(repoA, "notes.txt"), "utf8")).toBe(ORIGINAL.replace("line 2\n", "line 2 changed\n"));
    expect(git(repoA, "log", "--format=%s", "-1").trim()).toBe("Keep the second region");

    // ---- Settings, Appearance: each option takes effect at once, with no reload.
    await page.evaluate(() => ((window as unknown as { __px208?: string }).__px208 = "set"));
    await toSettings(page);
    await page.getByTestId("settings-nav-appearance").click();
    await page.getByTestId("settings-density-compact").click();
    expect(await densityDrift(page, "compact")).toEqual([]);
    // Retained mutation: the density stored but left for a reload is caught.
    await page.evaluate(() => document.documentElement.removeAttribute("data-density"));
    expect(await densityDrift(page, "compact")).toEqual(expect.arrayContaining(["the root says comfortable after compact was chosen", "the bar is 45 px, not 40"]));
    await page.getByTestId("settings-density-comfortable").click();
    await page.getByTestId("settings-density-compact").click();
    expect(await densityDrift(page, "compact")).toEqual([]);
    await page.getByTestId("settings-theme-dark").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await page.getByTestId("settings-zoom-in").click();
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("1.08");
    await page.getByTestId("settings-row-details-always").click();
    await expect(page.getByTestId("settings-row-details-always")).toHaveAttribute("aria-checked", "true");
    await page.getByTestId("settings-grouping").selectOption("status");
    await page.getByTestId("nav-back").click();
    await expect(page.getByTestId("agent-list"), "the list took the grouping at once").toHaveAttribute("data-grouping", "status");
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "always");
    expect(await page.evaluate(() => (window as unknown as { __px208?: string }).__px208), "no reload").toBe("set");
    // The list's own option reads the same choice: one copy of the list's preferences.
    await page.getByTestId("agents-more").click();
    await expect(page.getByTestId("menu-item-row-details-always")).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Escape");

    // ... and every one is kept across an app restart.
    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env }));
    await setContentSize(app, page, 1280, 800);
    await tapBridge(app);
    await expect(page.locator("html")).toHaveAttribute("data-density", "compact");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    expect(await page.evaluate(() => document.documentElement.style.zoom)).toBe("1.08");
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-grouping", "status");
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "always");
    await toSettings(page);
    await expect(page.getByTestId("settings-density-compact")).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("settings-theme-dark")).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("settings-row-details-always")).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("settings-grouping")).toHaveValue("status");
    // Back to the defaults for what follows.
    await page.getByTestId("settings-density-comfortable").click();
    await page.getByTestId("settings-theme-light").click();
    await page.getByTestId("settings-zoom-reset").click();
    await page.getByTestId("settings-row-details-auto").click();
    await page.getByTestId("settings-grouping").selectOption("repository");
    await accessible(page, "Settings, Appearance", '[data-testid="settings-section-appearance"]');
    await shots(page, "settings-appearance", async () => {
      await toSettings(page);
      await page.getByTestId("settings-nav-appearance").click();
    });

    // ---- A setting that belongs to one task is never shown as global.
    await toSettings(page);
    expect(await perTaskAsGlobal(page)).toEqual([]);
    await expect(page.getByTestId("settings-section-permissions")).toContainText("These belong to each task");
    // Retained mutation: a Run mode control on the Settings page is caught.
    await page.evaluate(() => {
      const g = document.createElement("span");
      g.id = "px208-mutation";
      g.setAttribute("role", "radiogroup");
      g.setAttribute("aria-label", "Run mode");
      g.innerHTML = '<button role="radio" aria-checked="true">Supervised</button>';
      document.querySelector('[data-testid="settings-section-appearance"] .settings-card')?.appendChild(g);
    });
    expect(await perTaskAsGlobal(page)).toEqual(expect.arrayContaining([expect.stringContaining("Run mode")]));
    await page.evaluate(() => document.getElementById("px208-mutation")?.remove());
    expect(await perTaskAsGlobal(page)).toEqual([]);

    // ---- Shortcuts: the section equals the registry and the key maps, binding for binding.
    await page.getByTestId("settings-nav-shortcuts").click();
    await expect(page.getByTestId("settings-section-shortcuts")).toBeVisible();
    const rows = await listedRows(page);
    const isMac = process.platform === "darwin";
    const expected = shortcutSheet(shellRegistry.all() as CommandDef<never>[], (e, m) => shellRegistry.forEvent(e, m)?.id, isMac);
    expect(expected.unbound).toEqual([]);
    expect(rows.map((r) => `${r.scope} ${r.id} ${r.probes.map((p) => p.chord).join(",")}`)).toEqual(SCOPES.flatMap((sc) => expected.rows.filter((r) => r.scope === sc)).map((r) => `${r.scope} ${r.id} ${r.probes.map((p) => p.chord).join(",")}`));
    const registryChords = shellRegistry
      .all()
      .filter((d) => (d.keys?.length ?? 0) > 0)
      .map((d) => `${d.id} ${d.keys!.join(",")}`);
    expect(rows.filter((r) => !r.id.includes(":")).map((r) => `${r.id} ${r.probes.map((p) => p.chord).join(",")}`)).toEqual(registryChords);
    await expect(page.getByTestId("settings-shortcut").first().locator("kbd")).toBeVisible();
    await accessible(page, "Settings, Shortcuts", '[data-testid="settings-section-shortcuts"]');
    await shots(page, "settings-shortcuts", async () => {
      await toSettings(page);
      await page.getByTestId("settings-nav-shortcuts").click();
    });

    // Every listed key is taken by the running app in its context. Anywhere and single keys: pressed with nothing focused, on home.
    const home = async () => {
      if ((await page.getByTestId("settings-nav").count()) > 0) await page.getByTestId("nav-back").click();
      if ((await page.getByTestId("goal").count()) === 0 || !(await page.getByTestId("goal").isVisible())) await page.getByTestId("agents-new-task").click();
      await expect(page.getByTestId("goal")).toBeVisible();
    };
    const settle = async () => {
      for (let i = 0; i < 3 && (await page.locator('[role="dialog"], [data-testid="palette"], [data-testid="confirm"]').count()) > 0; i++) await page.keyboard.press("Escape");
      await expect(page.locator('[role="dialog"], [data-testid="palette"]')).toHaveCount(0);
    };
    const globalRows = rows.filter((r) => r.scope === "anywhere" || r.scope === "keys");
    expect(await unboundListed(page, globalRows, "body", home, settle)).toEqual([]);
    await page.keyboard.press(`${isMac ? "Meta" : "Control"}+0`);

    // On a task in the list: pressed on B's focused row (pin and archive are put back).
    const rowKeys = rows.filter((r) => r.scope === "row");
    const focusB = async () => {
      await home();
      await agentRow(page, a).getByTestId("agent-open").focus();
      await agentRow(page, b).getByTestId("agent-open").focus();
    };
    const undoRow = async (r: ListedRow) => {
      // Archive is the Core's and lands a moment later: wait for its Undo, then take it.
      if (r.id === "row:archive") {
        await expect(agentRow(page, b)).toHaveCount(0, { timeout: 15_000 });
        await page.getByRole("button", { name: "Undo" }).click();
      }
      await expect(agentRow(page, b)).toHaveCount(1);
      if ((await page.locator('[data-testid="agent-preview"]').count()) > 0) await page.keyboard.press("Escape");
    };
    expect(await unboundListed(page, rowKeys, "focus", focusB, undoRow)).toEqual([]);
    await agentRow(page, b).getByTestId("agent-open").focus();
    // Pin was pressed once: press it again to leave the list as it was.
    await page.keyboard.press("p");

    // In a task's review: pressed with B's review open and a change focused.
    await toReviewPage(page);
    await page.locator(`[data-testid="review-page-row"][data-task-id="${b}"]`).click();
    const inReview = async () => {
      if ((await page.getByTestId("review").count()) === 0) await page.getByTestId("review-page-open-review").click();
      await expect(page.getByTestId("review-hunk").first()).toBeVisible();
      await page.getByTestId("review-hunk").first().focus();
    };
    expect(await unboundListed(page, rows.filter((r) => r.scope === "review"), "focus", inReview, async () => {})).toEqual([]);
    await page.getByTestId("review-close").click();

    // While a confirmation is open: c on B asks to cancel; Escape and Enter are taken (Enter confirms: B is cancelled, last).
    const confirmRows = [...rows.filter((r) => r.scope === "confirming" && r.id.endsWith(":back")), ...rows.filter((r) => r.scope === "confirming" && !r.id.endsWith(":back"))];
    const asking = async () => {
      if ((await page.getByTestId("confirm").count()) > 0) return;
      await home();
      await agentRow(page, b).getByTestId("agent-open").focus();
      await page.keyboard.press("c");
      await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "cancel");
    };
    expect(await unboundListed(page, confirmRows, "body", asking, async () => {})).toEqual([]);
    await expectTaskState(page, b, "Cancelled", 30_000);

    // Retained mutation: a listed row whose key nothing binds is caught.
    await toSettings(page);
    await page.getByTestId("settings-nav-shortcuts").click();
    await page.evaluate(() => {
      const li = document.createElement("li");
      li.id = "px208-mutation";
      li.setAttribute("data-testid", "settings-shortcut");
      li.setAttribute("data-id", "key-keys:changeRunMode");
      li.setAttribute("data-scope", "keys");
      li.setAttribute("data-probes", JSON.stringify([{ key: "m", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, chord: "m" }]));
      document.querySelector('[data-testid="settings-shortcut-group"] ul')?.appendChild(li);
    });
    const mutated = (await listedRows(page)).filter((r) => r.id === "key-keys:changeRunMode");
    expect(await unboundListed(page, mutated, "body", async () => {}, async () => {})).toEqual(["key-keys:changeRunMode: m is listed and not bound"]);
    await page.evaluate(() => document.getElementById("px208-mutation")?.remove());
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
