/**
 * QUAL-PX-209 / PX-E2E-209 (DR-PX-2026-11-11-021 MWS-15): one search in the palette, on the real Electron build, a real Core
 * and a scripted model. Two tasks run in two folders. From the keyboard alone: a task is found by its title and opens; a
 * conversation is found by text inside it (text only the model wrote) and opens at the row it is in; a file of the open task's
 * folder is found by its name and opens in the Files pane with its text; a command is found by its label and runs. One query
 * shows all four kinds, each under its heading, in order; the arrows, Home and End move through them; Escape closes and focus
 * returns where it was; nothing found says what was searched. A task row has a second action, Open in split view (MWS-11,
 * the item PX-204 left out): shown on the highlighted task row, Alt+Enter takes PX-204's own split entry (the shown task left,
 * the chosen one right with the keyboard), and PX-204's refusals hold (a third task, a window too narrow for two).
 *
 * Fail conditions, each with a retained mutation that the check is shown to catch:
 *  - a file outside the open task's folder is offered: every file the palette offers must be one the Core listed for the open
 *    task (read from a log of the bridge in the main process), and the other task's file is never offered while it is not open;
 *  - search text sent anywhere but the Core's search and file reads: every bridge call (invoke and send) after the palette is
 *    first used is logged with its arguments, and none but `conversation:search`, `files:list` and `files:read` may carry
 *    what was typed.
 *  - a split opened from the palette that is not PX-204's (columns swapped, keyboard elsewhere): `splitDrift`, shown failing
 *    on the columns drawn the other way round.
 * The log is a tap on the main process's handlers added by the test (as in PX-201); the product is not changed.
 * With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { routedModel, type ModelPart } from "./support/conversation-harness.ts";
import { agentRow, runTask, setDensity } from "./support/new-task.ts";
import { accessible, closeApp, focusedTestId, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL_A = "Tidy the files of the px209 ledger";
const GOAL_B = "Draft the beta summary";
const SAID_A = "Done. The quokka ledger reconciles now; I left the widget alone.";
const SAID_B = "The summary is drafted.";
const GOAL_C = "Sweep the gamma chores";
const SAID_C = "Swept.";
const WIDGET = "src/deep/px209widget.ts";
const WIDGET_TEXT = "export const widget = 209;\n";
const OUTSIDE = "px209-outside.txt";
/** The channels that may carry what is typed: the Core's search and the file reads. */
const ALLOWED = new Set(["conversation:search", "files:list", "files:read"]);

// ---------------------------------------------------------------- screenshots

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/** One state in light and dark and in both densities (only when asked for): `show` opens the palette on it again after the density changed. */
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
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
  }
  await setDensity(page, "comfortable");
  await setTheme(page, "light");
}

// ---------------------------------------------------------------- the bridge's log

interface BridgeCall {
  channel: string;
  args: unknown[];
  result: unknown;
  n: number;
}

/**
 * Taps every handler main answers the preload on (invoke) and every message it receives (send), from the test: each call is
 * kept with its arguments, and an invoke with what main answered. The renderer cannot reach this log.
 */
async function tapBridge(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ ipcMain }) => {
    const g = globalThis as unknown as { __bridgeLog?: unknown[] };
    if (g.__bridgeLog) return;
    const log: { channel: string; args: unknown[]; result: unknown; n: number }[] = [];
    g.__bridgeLog = log;
    let n = 0;
    const handlers = (ipcMain as unknown as { _invokeHandlers: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    for (const [channel, fn] of [...handlers.entries()]) {
      handlers.set(channel, async (event: unknown, ...args: unknown[]) => {
        const entry = { channel, args, result: null as unknown, n: n++ };
        log.push(entry);
        entry.result = await fn(event, ...args);
        return entry.result;
      });
    }
    const emit = ipcMain.emit.bind(ipcMain);
    ipcMain.emit = (channel: string | symbol, ...args: unknown[]) => {
      if (typeof channel === "string") log.push({ channel: `send:${channel}`, args: args.slice(1), result: null, n: n++ });
      return emit(channel, ...args);
    };
  });
}

const logLength = (app: ElectronApplication) => app.evaluate(() => ((globalThis as unknown as { __bridgeLog?: unknown[] }).__bridgeLog ?? []).length);

/** The calls from `from` on whose arguments carry one of `needles` on a channel that is not the Core's search or a file read. */
async function leaks(app: ElectronApplication, from: number, needles: readonly string[]): Promise<string[]> {
  return app.evaluate(
    (_e, [start, words, allowed]) => {
      const log = ((globalThis as unknown as { __bridgeLog?: BridgeCall[] }).__bridgeLog ?? []).slice(start as number);
      const out: string[] = [];
      for (const c of log) {
        const text = JSON.stringify(c.args).toLowerCase();
        for (const w of words as string[]) if (text.includes(w.toLowerCase()) && !(allowed as string[]).includes(c.channel)) out.push(`“${w}” went to ${c.channel}`);
      }
      return out;
    },
    [from, needles, [...ALLOWED]] as const,
  );
}

/** Whether the Core's search was asked for `needle` after `from`. */
async function searched(app: ElectronApplication, from: number, needle: string): Promise<boolean> {
  return app.evaluate(
    (_e, [start, w]) => ((globalThis as unknown as { __bridgeLog?: BridgeCall[] }).__bridgeLog ?? []).slice(start as number).some((c) => c.channel === "conversation:search" && String(c.args[1]).toLowerCase() === (w as string).toLowerCase()),
    [from, needle] as const,
  );
}

/** The file paths the Core listed for `taskId` (every `files:list` answer whose first argument is the task). */
async function listedFor(app: ElectronApplication, taskId: string): Promise<string[]> {
  return app.evaluate(
    (_e, t) => {
      const log = (globalThis as unknown as { __bridgeLog?: BridgeCall[] }).__bridgeLog ?? [];
      const out = new Set<string>();
      for (const c of log) if (c.channel === "files:list" && c.args[0] === t && c.result) for (const e of (c.result as { entries: { path: string; kind: string }[] }).entries) if (e.kind === "file") out.add(e.path);
      return [...out];
    },
    taskId,
  );
}

/** The files the palette offers that are not in the open task's folder as the Core listed it (empty when none). */
async function filesOutside(app: ElectronApplication, page: Page, openTaskId: string): Promise<string[]> {
  const offered = await page.locator('[data-testid="palette-option"][data-kind="file"]').evaluateAll((els) => els.map((e) => ({ path: e.getAttribute("data-path") ?? "", task: e.getAttribute("data-task-id") ?? "" })));
  const listed = new Set(await listedFor(app, openTaskId));
  return offered.filter((o) => o.task !== openTaskId || !listed.has(o.path)).map((o) => `${o.path} (offered for ${o.task === openTaskId ? "the open task" : "another task"}, ${listed.has(o.path) ? "listed" : "not listed by the Core for the open task"})`);
}

// ---------------------------------------------------------------- the palette

async function openPalette(page: Page, query = ""): Promise<void> {
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.getByTestId("palette")).toBeVisible();
  await expect.poll(() => focusedTestId(page)).toBe("palette-input");
  if (query) await page.keyboard.type(query);
}

const option = (page: Page, kind: string) => page.locator(`[data-testid="palette-option"][data-kind="${kind}"]`);
const groupsShown = (page: Page) => page.getByTestId("palette-group").evaluateAll((els) => els.map((e) => e.getAttribute("data-group")));
/** Where a split opened from the palette differs from PX-204's rule (empty when it does not): shown task left, chosen right with the keyboard. */
async function splitDrift(page: Page, shown: string, chosen: string): Promise<string[]> {
  const bad: string[] = [];
  if ((await page.getByTestId("shell").getAttribute("data-split")) !== "shown") bad.push("split view is not shown");
  const cols = await page.getByTestId("split-column").evaluateAll((els) => els.map((e) => ({ id: e.getAttribute("data-task-id"), focused: e.getAttribute("data-focused") === "true" })));
  if (cols.length !== 2) return [...bad, `${cols.length} columns`];
  if (cols[0]!.id !== shown || cols[1]!.id !== chosen) bad.push(`the columns are ${cols[0]!.id} | ${cols[1]!.id}, not the shown task then the chosen one`);
  if (!cols.some((c) => c.id === chosen && c.focused)) bad.push("the keyboard is not on the chosen task");
  return bad;
}
const activeIndex = (page: Page) => page.getByTestId("palette-option").evaluateAll((els) => els.findIndex((e) => e.getAttribute("data-active") === "true"));

async function expectActiveIsDescendant(page: Page): Promise<void> {
  const id = await page.getByTestId("palette-input").getAttribute("aria-activedescendant");
  expect(id).toBeTruthy();
  await expect(page.locator(`[id="${id}"]`)).toHaveAttribute("data-active", "true");
}

async function openTaskFromList(page: Page, taskId: string): Promise<void> {
  await agentRow(page, taskId).getByTestId("agent-open").click();
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
}

test("PX-209: one search in the palette finds a task by title, a conversation by its text, a file of the open task's folder by name and a command by its label; each opens the right place; the keyboard alone drives it; no file from outside the folder, and the text goes only to the Core's search and file reads", async () => {
  test.setTimeout(420_000);
  const repoA = mkdtempSync(join(tmpdir(), "modbit-px209-a-"));
  mkdirSync(join(repoA, "src", "deep"), { recursive: true });
  mkdirSync(join(repoA, "docs"), { recursive: true });
  writeFileSync(join(repoA, WIDGET), WIDGET_TEXT);
  writeFileSync(join(repoA, "docs", "files-index.md"), "# Index\n");
  writeFileSync(join(repoA, "px209-ledger.md"), "ledger\n");
  makeRepo(repoA);
  const repoB = mkdtempSync(join(tmpdir(), "modbit-px209-b-"));
  writeFileSync(join(repoB, OUTSIDE), "not task A's\n");
  makeRepo(repoB);
  // Each task says its piece once and completes.
  const model = await routedModel((goal, results): ModelPart[] => {
    if (results > 0) return ["Nothing further."];
    const said = goal.includes("px209 ledger") ? SAID_A : goal.includes("beta summary") ? SAID_B : goal.includes("gamma chores") ? SAID_C : "Nothing further.";
    return [said, { call: { name: "task.complete", args: { summary: said, self_review: { findings: [] } } } }];
  });
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px209-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await tapBridge(app);
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");

    // ---- Three tasks, A in its own folder, B and C in another; A is open. Each has said its piece (the text the conversation search must find).
    const taskC = await runTask(page, GOAL_C, repoB);
    await expect(page.getByTestId("conversation").getByText(SAID_C).first()).toBeVisible({ timeout: 60_000 });
    const taskB = await runTask(page, GOAL_B, repoB);
    await expect(page.getByTestId("conversation").getByText(SAID_B).first()).toBeVisible({ timeout: 60_000 });
    const taskA = await runTask(page, GOAL_A, repoA);
    await expect(page.getByTestId("conversation").getByText(SAID_A).first()).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
    const titleA = GOAL_A;
    const from = await logLength(app);
    const typed: string[] = [];

    // ---- Empty: tasks and commands, the keys and what is searched; Escape closes and focus is back where it was.
    await page.getByTestId("composer-input").focus();
    await openPalette(page);
    expect(await groupsShown(page)).toEqual(["tasks", "commands"]);
    await expect(page.getByTestId("palette-scope")).toContainText("every task’s conversation");
    await expect(page.getByTestId("palette-scope")).toContainText("the files of");
    await accessible(page, "the palette, empty");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    expect(await focusedTestId(page), "focus returns to where it was").toBe("composer-input");
    await shots(page, "palette-empty", () => openPalette(page));

    // ---- One query, all four kinds, each under its heading, in order; the keyboard moves through them all.
    typed.push("files");
    await openPalette(page, "files");
    await expect.poll(() => groupsShown(page), { timeout: 30_000, message: "tasks, conversation text, files and commands" }).toEqual(["tasks", "conversations", "files", "commands"]);
    await expect(page.getByTestId("palette-group").locator(".palette-head")).toHaveText(["Tasks", "In conversations", "Files", "Commands"]);
    await expect(option(page, "agent").first()).toHaveAttribute("data-id", `agent:${taskA}`);
    await expect(option(page, "file").first()).toHaveAttribute("data-path", "docs/files-index.md");
    await expect(option(page, "action").filter({ hasText: "Show files" })).toHaveCount(1);
    await accessible(page, "the palette, four kinds");
    const total = await page.getByTestId("palette-option").count();
    expect(await activeIndex(page)).toBe(0);
    for (let i = 1; i < total; i++) {
      await page.keyboard.press("ArrowDown");
      expect(await activeIndex(page)).toBe(i);
    }
    await expectActiveIsDescendant(page);
    await page.keyboard.press("ArrowDown");
    expect(await activeIndex(page), "ArrowDown from the last wraps to the first").toBe(0);
    await page.keyboard.press("ArrowUp");
    expect(await activeIndex(page)).toBe(total - 1);
    await page.keyboard.press("Home");
    expect(await activeIndex(page)).toBe(0);
    await page.keyboard.press("End");
    expect(await activeIndex(page)).toBe(total - 1);
    await expectActiveIsDescendant(page);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await shots(page, "palette-four-kinds", async () => {
      await openPalette(page, "files");
      await expect.poll(() => groupsShown(page), { timeout: 30_000 }).toEqual(["tasks", "conversations", "files", "commands"]);
    });

    // ---- A file by its name: the Files pane opens on it, its folders open, its text read from the Core; the keyboard is on it.
    typed.push("px209widget");
    await openPalette(page, "px209widget");
    const fileHit = option(page, "file").first();
    await expect(fileHit).toHaveAttribute("data-path", WIDGET, { timeout: 30_000 });
    await expect(fileHit.locator("mark")).toHaveText("px209widget");
    await expect.poll(() => filesOutside(app, page, taskA)).toEqual([]);
    await page.keyboard.press("Escape");
    await shots(page, "palette-file-hit", async () => {
      await openPalette(page, "px209widget");
      await expect(option(page, "file").first()).toHaveAttribute("data-path", WIDGET, { timeout: 30_000 });
    });
    await openPalette(page, "px209widget");
    await expect(option(page, "file").first()).toHaveAttribute("data-path", WIDGET, { timeout: 30_000 });
    // Commands rank below files: the file is reached with the arrows when it is not first.
    while ((await page.getByTestId("palette-option").nth(await activeIndex(page)).getAttribute("data-kind")) !== "file") await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect(page.getByTestId("files-viewer")).toHaveAttribute("data-path", WIDGET, { timeout: 30_000 });
    await expect(page.getByTestId("files-text")).toHaveText(WIDGET_TEXT.trimEnd());
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="files-entry"]')?.getAttribute("data-path") ?? "")).toBe(WIDGET);

    // ---- No file from outside the open task's folder: the other task's file is not offered while A is open ...
    typed.push("outside");
    await openPalette(page, "outside");
    await expect(page.getByTestId("palette-empty")).toContainText("Nothing matches", { timeout: 30_000 });
    await expect(page.getByTestId("palette-empty")).toContainText(`the files of “${titleA}”`);
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("px209");
    await expect(option(page, "file")).toHaveCount(2, { timeout: 30_000 });
    expect(await option(page, "file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path")).sort())).toEqual(["px209-ledger.md", WIDGET].sort());
    expect(await filesOutside(app, page, taskA)).toEqual([]);
    // ... and the retained mutation: B's file drawn as an offer while A is open is caught by the check.
    await page.getByTestId("palette-list").evaluate((el, p) => {
      const o = document.createElement("div");
      o.id = "px209-mutation";
      o.setAttribute("role", "option");
      o.setAttribute("data-testid", "palette-option");
      o.setAttribute("data-kind", "file");
      o.setAttribute("data-path", p.path);
      o.setAttribute("data-task-id", p.task);
      o.textContent = p.path;
      el.appendChild(o);
    }, { path: OUTSIDE, task: taskA });
    expect(await filesOutside(app, page, taskA)).toEqual([`${OUTSIDE} (offered for the open task, not listed by the Core for the open task)`]);
    await page.evaluate(() => document.getElementById("px209-mutation")?.remove());
    expect(await filesOutside(app, page, taskA)).toEqual([]);
    await page.keyboard.press("Escape");

    // ---- A task by its title: Enter opens it.
    typed.push("beta summary");
    await openPalette(page, "beta summary");
    await expect(page.getByTestId("palette-option").first()).toHaveAttribute("data-id", `agent:${taskB}`);
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskB);
    // With B open its own folder is searched, and A's files are not.
    typed.push("outside");
    await openPalette(page, "outside");
    await expect(option(page, "file")).toHaveCount(1, { timeout: 30_000 });
    await expect(option(page, "file")).toHaveAttribute("data-path", OUTSIDE);
    await expect(option(page, "file")).toHaveAttribute("data-task-id", taskB);
    expect(await filesOutside(app, page, taskB)).toEqual([]);
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("px209widget");
    await expect.poll(() => page.getByTestId("palette-empty").textContent(), { timeout: 30_000 }).toContain("Nothing matches");
    await page.keyboard.press("Escape");

    // ---- A conversation by text only the model wrote in it: Enter opens it at the row the text is in.
    typed.push("quokka ledger");
    await openPalette(page, "quokka ledger");
    const convHit = option(page, "conversation").first();
    await expect(convHit).toHaveAttribute("data-task-id", taskA, { timeout: 30_000 });
    await expect(convHit.locator("mark").first()).toContainText(/quokka/i);
    await expect(convHit).toContainText(titleA);
    const rowId = (await convHit.getAttribute("data-row-id"))!;
    expect(rowId).toBeTruthy();
    expect(await searched(app, from, "quokka ledger"), "the Core's search was asked").toBe(true);
    await accessible(page, "the palette, a conversation match");
    await page.keyboard.press("Escape");
    await shots(page, "palette-conversation-hit", async () => {
      await openPalette(page, "quokka ledger");
      await expect(option(page, "conversation").first()).toHaveAttribute("data-task-id", taskA, { timeout: 30_000 });
    });
    await openPalette(page, "quokka ledger");
    await expect(option(page, "conversation").first()).toHaveAttribute("data-task-id", taskA, { timeout: 30_000 });
    while ((await page.getByTestId("palette-option").nth(await activeIndex(page)).getAttribute("data-kind")) !== "conversation") await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskA);
    const row = page.getByTestId("conversation").locator(`[data-row-id="${rowId}"]`);
    await expect(row).toHaveAttribute("data-highlight", "true", { timeout: 15_000 });
    await expect(row).toContainText("quokka ledger");

    // ---- A command by its label runs; Escape closes what it opened.
    typed.push("keyboard shortcuts");
    await openPalette(page, "keyboard shortcuts");
    await expect(page.getByTestId("palette-option").first()).toHaveAttribute("data-id", "help.shortcuts");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("help")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("help")).toHaveCount(0);

    // ---- Open in split view (MWS-11; PX-204's left-out item): the highlighted task row shows it, and Alt+Enter takes PX-204's
    // own split entry, so the shown task is on the left and the chosen one on the right with the keyboard.
    await openPalette(page, "beta summary");
    const taskRow = page.getByTestId("palette-option").first();
    await expect(taskRow).toHaveAttribute("data-id", `agent:${taskB}`);
    await expect(taskRow.getByTestId("palette-split-action")).toContainText("Open in split view");
    await expect(page.getByTestId("palette-split-action"), "only the highlighted row shows it").toHaveCount(1);
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("keyboard shortcuts");
    await expect(page.getByTestId("palette-option").first()).toHaveAttribute("data-id", "help.shortcuts");
    await expect(page.getByTestId("palette-split-action"), "no split action on a command").toHaveCount(0);
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("beta summary");
    await expect(page.getByTestId("palette-option").first()).toHaveAttribute("data-id", `agent:${taskB}`);
    await page.keyboard.press("Alt+Enter");
    await expect(page.getByTestId("palette")).toHaveCount(0);
    await expect.poll(() => splitDrift(page, taskA, taskB)).toEqual([]);
    // The retained mutation: the two columns drawn the other way round is caught by the check.
    await page.evaluate(() => {
      const cols = [...document.querySelectorAll<HTMLElement>('[data-testid="split-column"]')];
      const [a, b] = [cols[0]!.getAttribute("data-task-id")!, cols[1]!.getAttribute("data-task-id")!];
      cols[0]!.setAttribute("data-task-id", b);
      cols[1]!.setAttribute("data-task-id", a);
    });
    expect(await splitDrift(page, taskA, taskB)).toEqual([`the columns are ${taskB} | ${taskA}, not the shown task then the chosen one`, "the keyboard is not on the chosen task"]);
    await page.evaluate(() => {
      const cols = [...document.querySelectorAll<HTMLElement>('[data-testid="split-column"]')];
      const [a, b] = [cols[0]!.getAttribute("data-task-id")!, cols[1]!.getAttribute("data-task-id")!];
      cols[0]!.setAttribute("data-task-id", b);
      cols[1]!.setAttribute("data-task-id", a);
    });
    expect(await splitDrift(page, taskA, taskB)).toEqual([]);
    // A third task is refused with PX-204's reason, and the split is unchanged.
    await openPalette(page, "gamma chores");
    await expect(page.getByTestId("palette-option").first()).toHaveAttribute("data-id", `agent:${taskC}`);
    await page.keyboard.press("Alt+Enter");
    await expect(page.getByTestId("shell-notice")).toContainText("Split view holds two tasks");
    expect(await splitDrift(page, taskA, taskB)).toEqual([]);
    await page.locator(`[data-testid="split-column"][data-task-id="${taskB}"]`).getByTestId("split-close").click();
    await expect(page.getByTestId("split-view")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskA);
    // A window too narrow for two is refused with PX-204's reason.
    await setContentSize(app, page, 1100, 800);
    await openPalette(page, "beta summary");
    await page.keyboard.press("Alt+Enter");
    await expect(page.getByTestId("shell-notice")).toContainText("Split view needs");
    await expect(page.getByTestId("split-view")).toHaveCount(0);
    await setContentSize(app, page, 1440, 900);

    // ---- Nothing found says what was searched.
    typed.push("zzqq nothing here");
    await openPalette(page, "zzqq nothing here");
    await expect(page.getByTestId("palette-empty")).toContainText("Nothing matches “zzqq nothing here” in task titles, every task’s conversation, the files of", { timeout: 30_000 });
    await page.keyboard.press("Escape");

    // ---- What was typed went only to the Core's search and the file reads ...
    expect(await leaks(app, from, typed.filter((t) => t.length >= 6))).toEqual([]);
    // ... and the retained mutation: the same text sent on another bridge call is caught by the check.
    await page.evaluate(() => window.modbit.listProjects({ workspaceRoot: "/nowhere/quokka ledger" }).catch(() => null));
    expect(await leaks(app, from, ["quokka ledger"])).toEqual(["“quokka ledger” went to projects:list"]);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
