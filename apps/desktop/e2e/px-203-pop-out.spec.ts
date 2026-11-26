/**
 * QUAL-PX-203 / PX-E2E-203 (DR-PX-2026-11-11-021 MWS-10, section 9): pane pop-out on the real Electron build, a real Core and a
 * scripted model. One task edits two files, keeps a terminal running that counts aloud and echoes what is typed, and asks for a
 * protected command (adding a git remote that is a real bare repository on this machine).
 *
 * Terminal and Changes each pop out into a window of their own and keep working there (the same terminal process, typing reaches
 * it; the same file chosen and the list scrolled as it was). The hardening guard holds for each pane window as for the app's
 * window: the same web preferences and preload, the default session and the same Content-Security-Policy in force, no Node in
 * the page, a navigation and a new window refused (and a window built with Node in the page, one in a session without the
 * policy, and one without the refusals each fail it). A pane is in one place at a time (a pane drawn in both places fails). No
 * approval can be decided from a pane window: main refuses the channels, the Core still lists the approval as pending (and an
 * approval that was decided fails that check). The quit question counts the work pane windows show. The app's window reloads
 * and the pane windows go on. The approval is then allowed on the app window's card; Commit and push started in the Changes
 * window waits for its approval on that card and goes on in the window once allowed (a real push to the bare remote). Closing
 * each window brings its pane back to the dock as it was; a pane window whose renderer dies closes and its pane comes back; and
 * quitting closes every pane window. Axe on each pane window. With MODBIT_SHOTS_DIR set, the run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Gate } from "./support/stream-model.ts";
import { nodeTerminal } from "./support/apps-harness.ts";
import { routedModel, sessionOf, type ModelPart } from "./support/conversation-harness.ts";
import { expectTaskState, openTask, runTask, showPane } from "./support/new-task.ts";
import { accessible, appDir, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { guardDrift, referenceDrift, windowFacts, type WindowFacts } from "./support/window-guard.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL = "Tidy the notes and write the long list";
/** Echoes each typed line and counts aloud: the count never starts again unless the process does. */
const TICKER = "let n = 0; process.stdout.write('READY\\n'); process.stdin.setEncoding('utf8'); process.stdin.on('data', (d) => process.stdout.write('got:' + d.trim() + '\\n')); setInterval(() => process.stdout.write('tick ' + (++n) + '\\n'), 2000)";
const LONG = Array.from({ length: 240 }, (_, i) => `item ${i + 1}`).join("\n") + "\n";

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** An evidence screenshot of a window of the running app, light and dark (only when asked for). */
async function shots(page: Page, name: string): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await setTheme(page, "light");
}

/** The pane window main opens next, as Playwright sees it, once it shows its pane. */
async function nextPaneWindow(app: ElectronApplication, kind: string, click: () => Promise<void>): Promise<Page> {
  const opened = app.waitForEvent("window", { timeout: 30_000 });
  await click();
  const w = await opened;
  await expect(w.getByTestId("pane-window")).toHaveAttribute("data-kind", kind, { timeout: 30_000 });
  return w;
}

/**
 * Clicks a control that closes its own window (Back to the dock): the window can be gone before Playwright finishes the click,
 * which is the click having worked. Waits for the page to close.
 */
async function clickAndClose(w: Page, testId: string): Promise<void> {
  const closed = w.waitForEvent("close", { timeout: 30_000 });
  await w.getByTestId(testId).click({ noWaitAfter: true }).catch((e: Error) => {
    if (!/closed/i.test(e.message)) throw e;
  });
  await closed;
}

const windowCount = (app: ElectronApplication): Promise<number> => app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().filter((w) => !w.isDestroyed()).length);

/**
 * Where a pane is shown in two places (empty when none is): a pane window's pane that the dock of the app's window also draws for
 * the same task, or two pane windows for the same pane.
 */
async function bothPlaces(app: ElectronApplication, page: Page): Promise<string[]> {
  const inWindows = await app.evaluate(async ({ BrowserWindow }) => {
    const out: { taskId: string; kind: string }[] = [];
    for (const w of BrowserWindow.getAllWindows()) {
      if (w.isDestroyed() || !w.webContents.getURL().endsWith("#pane")) continue;
      const r = (await w.webContents.executeJavaScript("(() => { const e = document.querySelector('[data-testid=pane-window]'); return e ? { taskId: e.getAttribute('data-task-id') || '', kind: e.getAttribute('data-kind') || '' } : null; })()").catch(() => null)) as { taskId: string; kind: string } | null;
      if (r) out.push(r);
    }
    return out;
  });
  const bad: string[] = [];
  const seen = new Set<string>();
  for (const w of inWindows) {
    const key = `${w.taskId}:${w.kind}`;
    if (seen.has(key)) bad.push(`${w.kind} of ${w.taskId.slice(0, 8)} is in two windows`);
    seen.add(key);
    const inDock = await page.locator(`[data-testid="apps-panel"][data-task-id="${w.taskId}"] [data-testid="pane-${w.kind}"]`).count();
    if (inDock > 0) bad.push(`${w.kind} of ${w.taskId.slice(0, 8)} is in its window and in the dock`);
  }
  return bad;
}

interface Pending {
  approvalId: string;
  intentHash: string;
  taskId: string;
}
const pendingOf = (page: Page, sessionId: string): Promise<Pending[]> => page.evaluate((s) => window.modbit.pendingApprovals(s), sessionId) as Promise<Pending[]>;
const same = (a: string, b: string) => a.replace(/-/g, "").toLowerCase() === b.replace(/-/g, "").toLowerCase();

/**
 * Why the approval is no longer the Core's to ask (empty while it still is): the pane window's attempts to decide it were
 * answered (not refused by main), or the Core no longer lists it as pending.
 */
async function decidedFromPane(attempts: { channel: string; outcome: string }[], page: Page, sessionId: string, approvalId: string): Promise<string[]> {
  const bad = attempts.filter((a) => !a.outcome.includes("SENDER_REFUSED")).map((a) => `${a.channel} from the pane window was answered: ${a.outcome}`);
  if (!(await pendingOf(page, sessionId)).some((p) => same(p.approvalId, approvalId))) bad.push("the Core no longer lists the approval as pending");
  return bad;
}

interface CoreTerminal {
  terminalId: string;
  state: string;
  startedAtMs: number;
  bytesSoFar: string;
}
const coreTerminal = (page: Page, taskId: string, terminalId: string): Promise<CoreTerminal> => page.evaluate(([t, id]) => window.modbit.terminalList(t!).then((l) => l.terminals.find((x) => x.terminalId === id) as unknown as CoreTerminal), [taskId, terminalId]);
const lastTick = (screen: string): number => Math.max(0, ...[...screen.matchAll(/tick (\d+)/g)].map((m) => Number(m[1])));

/** Why the terminal shown is not the session it was (empty when it is): another terminal, a process ended or started again, nothing written since, the typed line gone, the count started again. */
function sessionLost(before: { core: CoreTerminal; tick: number; marker: string }, after: { shownId: string | null; core: CoreTerminal; screen: string }): string[] {
  const bad: string[] = [];
  if (after.shownId !== before.core.terminalId) bad.push(`the pane shows terminal ${after.shownId}, not ${before.core.terminalId}`);
  if (after.core.state !== "RUNNING") bad.push(`the Core lists the terminal as ${after.core.state}`);
  if (after.core.startedAtMs !== before.core.startedAtMs) bad.push("the process was started again");
  if (BigInt(after.core.bytesSoFar) <= BigInt(before.core.bytesSoFar)) bad.push("the process wrote nothing since");
  if (!after.screen.includes(before.marker)) bad.push("what was typed before is not on the screen");
  if (lastTick(after.screen) <= before.tick) bad.push(`the count is at ${lastTick(after.screen)}, not past ${before.tick}`);
  return bad;
}

/** Records the quit question instead of showing it, answering `answer`. */
async function recordDialog(app: ElectronApplication, answer: number): Promise<void> {
  await app.evaluate(({ dialog }, a) => {
    const g = globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[] }[] };
    g.__asked ??= [];
    (dialog as unknown as { showMessageBox: (...args: unknown[]) => Promise<{ response: number }> }).showMessageBox = async (...args: unknown[]) => {
      const o = (args.find((x) => typeof x === "object" && x !== null && "message" in (x as object)) ?? {}) as { message: string; detail: string; buttons: string[] };
      g.__asked.push({ message: o.message, detail: o.detail, buttons: o.buttons });
      return { response: a, checkboxChecked: false };
    };
  }, answer);
}
const asked = (app: ElectronApplication) => app.evaluate(() => (globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[] }[] }).__asked);
const exited = (app: ElectronApplication) => new Promise<void>((resolve) => (app.process().exitCode !== null ? resolve() : app.process().once("exit", () => resolve())));

test("PX-203: Terminal and Changes pop out and keep working; the guard holds for pane windows; a pane is in one place; no approval is decided from a pane window; the quit question counts pane-window work; closing returns each pane as it was", async () => {
  test.setTimeout(600_000);
  const repo = mkdtempSync(join(tmpdir(), "modbit-px203-repo-"));
  // Both files are in the base commit, so the change set holds two modified files.
  writeFileSync(join(repo, "list.txt"), "item 1\n");
  makeRepo(repo);
  const bare = mkdtempSync(join(tmpdir(), "modbit-px203-remote-"));
  execFileSync("git", ["init", "--bare", "-q", bare]);
  const hold = new Gate();
  const script: ModelPart[][] = [
    ["I will tidy the notes, write the long list and keep a counter running.", { call: { name: "plan.update", args: { outcome: "the notes are tidy and the list is written", expected_files: ["notes.txt", "list.txt"], protected_effects: ["shell.exec"] } } }],
    [{ call: { name: "fs.read", args: { path: "notes.txt" } } }],
    [{ call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 tidied\nline 3\n" } } }],
    [{ call: { name: "fs.read", args: { path: "list.txt" } } }],
    [{ call: { name: "change.apply", args: { path: "list.txt", op: "replace", content: LONG } } }],
    [{ call: nodeTerminal(TICKER) }],
    ["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin", bare] } } }],
    [{ call: { name: "task.complete", args: { summary: "tidied the notes and wrote the list", self_review: { findings: [] } } } }],
  ];
  const model = await routedModel(async (goal, results) => {
    if (goal !== GOAL) return ["Nothing further."];
    if (results === 6) await hold.opened;
    return script[results] ?? ["Nothing further."];
  });
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px203-")), { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_QUIT_PROMPT: "" } });
  let quit = false;
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    const taskId = await runTask(page, GOAL, repo);
    const sessionId = await sessionOf(page);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
    await expect.poll(() => page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals.length), taskId), { timeout: 90_000 }).toBe(1);
    const ticker = (await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals[0]!), taskId)) as unknown as CoreTerminal;
    await expectTaskState(page, taskId, "Running");

    // ---- The dock first: Changes, Terminal and Evidence open; the second file is chosen and the list scrolled; a line is typed.
    for (const kind of ["changes", "terminal", "evidence"] as const) await showPane(page, kind);
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-panes", "changes terminal evidence");
    const changes = page.getByTestId("pane-changes");
    await expect(changes.getByTestId("changes-file"), `the change set: ${await page.evaluate((t) => window.modbit.reviewBundle(t).then((b) => b.files.map((f) => f.path).join(", ")), taskId).catch((e: Error) => e.message)}`).toHaveCount(2, { timeout: 60_000 });
    const files = await changes.getByTestId("changes-file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path") ?? ""));
    const chosen = files[1]!;
    await changes.locator(`[data-testid="changes-file"][data-path="${chosen}"]`).click();
    await expect(changes.getByTestId("changes-diff")).toHaveAttribute("data-path", chosen);
    // The list.txt diff is long; with it chosen the list scrolls. Choose list.txt if it is not the second file.
    if (chosen !== "list.txt") {
      await changes.locator('[data-testid="changes-file"][data-path="list.txt"]').click();
    }
    const pickedPath = (await changes.getByTestId("changes-diff").getAttribute("data-path"))!;
    const scroller = changes.getByTestId("changes-scroll");
    await scroller.evaluate((el) => (el.scrollTop = 600));
    await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(300);
    const scrolledTo = await scroller.evaluate((el) => el.scrollTop);
    const term = page.getByTestId("pane-terminal");
    await expect(term.getByTestId("terminal-view")).toHaveAttribute("data-terminal-id", ticker.terminalId);
    await expect(term.getByTestId("terminal-screen")).toContainText("READY", { timeout: 30_000 });
    await term.getByTestId("terminal-screen").click();
    await page.keyboard.type("in-the-dock");
    await page.keyboard.press("Enter");
    await expect(term.getByTestId("terminal-screen")).toContainText("got:in-the-dock", { timeout: 30_000 });
    await page.keyboard.press("Shift+Escape");
    const before = { core: await coreTerminal(page, taskId, ticker.terminalId), tick: lastTick((await term.getByTestId("terminal-screen").textContent()) ?? ""), marker: "got:in-the-dock" };
    // Pop out is drawn on the panes that can stand alone, and on no other.
    for (const kind of ["changes", "terminal", "evidence"]) await expect(page.getByTestId(`pane-popout-${kind}`)).toBeVisible();
    expect(await windowCount(app)).toBe(1);

    // ---- Terminal pops out: a second window, made by main, showing the same terminal; the dock no longer draws it.
    const termWin = await nextPaneWindow(app, "terminal", () => page.getByTestId("pane-popout-terminal").click());
    await expect(page.getByTestId("pane-terminal"), "the Terminal pane is no longer in the dock").toHaveCount(0);
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-panes", "changes evidence");
    await expect(page.getByTestId("pane-toggle-terminal")).toHaveAttribute("data-state", "popped");
    await expect(page.getByTestId("pane-toggle-terminal")).toHaveAccessibleName("Terminal pane, in its own window");
    await expect(termWin.getByTestId("pane-window")).toHaveAttribute("data-task-id", taskId);
    await expect(termWin.getByTestId("pane-window-task")).toHaveText(GOAL);
    const wView = termWin.getByTestId("terminal-view");
    const wScreen = termWin.getByTestId("terminal-screen");
    await expect(wView).toHaveAttribute("data-terminal-id", ticker.terminalId, { timeout: 30_000 });
    await expect(wScreen).toContainText("got:in-the-dock", { timeout: 30_000 });
    const afterOf = async (p: Page) => ({ shownId: await p.getByTestId("terminal-view").getAttribute("data-terminal-id"), core: await coreTerminal(page, taskId, ticker.terminalId), screen: (await p.getByTestId("terminal-screen").textContent()) ?? "" });
    await expect.poll(async () => sessionLost(before, await afterOf(termWin)), { timeout: 30_000, message: "the popped-out terminal is the same running session" }).toEqual([]);
    // It keeps working: typing in the window reaches the same process.
    await wScreen.click();
    await termWin.keyboard.type("in-its-window");
    await termWin.keyboard.press("Enter");
    await expect(wScreen).toContainText("got:in-its-window", { timeout: 30_000 });
    await termWin.keyboard.press("Shift+Escape");

    // ---- Changes pops out: the same file chosen and the list scrolled as it was.
    const changesWin = await nextPaneWindow(app, "changes", () => page.getByTestId("pane-popout-changes").click());
    await expect(page.getByTestId("pane-changes")).toHaveCount(0);
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-panes", "evidence");
    await expect(page.getByTestId("pane-toggle-changes")).toHaveAttribute("data-state", "popped");
    await expect(changesWin.getByTestId("changes-file")).toHaveCount(2, { timeout: 30_000 });
    await expect(changesWin.getByTestId("changes-diff")).toHaveAttribute("data-path", pickedPath);
    await expect.poll(() => changesWin.getByTestId("changes-scroll").evaluate((el) => el.scrollTop), { message: "the list is scrolled as it was in the dock" }).toBeGreaterThan(scrolledTo - 40);
    // It keeps working: choosing the other file shows its diff, read from the Core in this window.
    const other = files.find((f) => f !== pickedPath)!;
    await changesWin.locator(`[data-testid="changes-file"][data-path="${other}"]`).click();
    await expect(changesWin.getByTestId("changes-diff")).toHaveAttribute("data-path", other);
    await changesWin.locator(`[data-testid="changes-file"][data-path="${pickedPath}"]`).click();
    await expect(changesWin.getByTestId("changes-diff")).toHaveAttribute("data-path", pickedPath);
    await changesWin.getByTestId("changes-scroll").evaluate((el) => (el.scrollTop = 900));
    const windowScroll = await changesWin.getByTestId("changes-scroll").evaluate((el) => el.scrollTop);
    expect(windowScroll).toBeGreaterThan(600);
    // Popping out again brings the same window forward; it opens no second one.
    expect(await windowCount(app)).toBe(3);

    // ---- A pane is in one place at a time.
    expect(await bothPlaces(app, page), "each popped-out pane is in its window only").toEqual([]);
    // The retained mutation: a Terminal pane drawn in the dock as well fails.
    await page.evaluate(() => {
      const s = document.createElement("section");
      s.setAttribute("data-testid", "pane-terminal");
      s.id = "px203-both";
      document.querySelector('[data-testid="apps-panel"] .dock-col')!.appendChild(s);
    });
    expect(await bothPlaces(app, page), "a pane in both places fails").toEqual([`terminal of ${taskId.slice(0, 8)} is in its window and in the dock`]);
    await page.evaluate(() => document.getElementById("px203-both")?.remove());
    expect(await bothPlaces(app, page)).toEqual([]);

    // ---- The hardening guard: each pane window is as hardened as the app's window.
    const ref = await windowFacts(app, page);
    expect(referenceDrift(ref), "the app's own window").toEqual([]);
    expect(ref.csp).toContain("default-src 'self'");
    for (const [name, w] of [["the Terminal window", termWin], ["the Changes window", changesWin]] as const) {
      const facts = await windowFacts(app, w);
      expect(guardDrift(ref, facts), name).toEqual([]);
    }
    expect(await windowCount(app), "the guard's attempts opened nothing").toBe(3);
    // Retained mutations, each a window the guard must fail: Node in the page; a session where the policy header is not set;
    // a window with the same preferences but neither refusal.
    const preload = join(appDir, "dist", "preload", "preload.cjs");
    const foreign = async (prefs: Record<string, unknown>): Promise<WindowFacts> => {
      const opened = app.waitForEvent("window");
      await app.evaluate(({ BrowserWindow }, [p, pre]) => {
        const w = new BrowserWindow({ show: false, width: 600, height: 400, webPreferences: { preload: pre as string, ...(p as object) } });
        (globalThis as unknown as { __px203Foreign?: Electron.BrowserWindow[] }).__px203Foreign = [...((globalThis as unknown as { __px203Foreign?: Electron.BrowserWindow[] }).__px203Foreign ?? []), w];
        void w.loadURL(BrowserWindow.getAllWindows().find((x) => x !== w && !x.webContents.getURL().endsWith("#pane"))!.webContents.getURL().replace(/#.*$/, "") + "#pane");
      }, [prefs, preload] as const);
      const fp = await opened;
      await fp.waitForLoadState("domcontentloaded");
      return windowFacts(app, fp);
    };
    const nodeWin = await foreign({ nodeIntegration: true, contextIsolation: false, sandbox: false });
    const nodeDrift = guardDrift(ref, nodeWin);
    expect(nodeDrift.some((d) => d.startsWith("webPreferences.nodeIntegration")), `Node in the page fails: ${nodeDrift.join("; ")}`).toBe(true);
    expect(nodeDrift.some((d) => d.startsWith("the page reaches Node")), "and the page does reach Node").toBe(true);
    const partWin = await foreign({ partition: "px203-no-policy", contextIsolation: true, nodeIntegration: false, sandbox: true });
    const partDrift = guardDrift(ref, partWin);
    expect(partDrift.some((d) => d.includes("inline script ran")), `a window without the policy fails: ${partDrift.join("; ")}`).toBe(true);
    expect(partDrift.some((d) => d.includes("not in the default session")), "it is outside the default session").toBe(true);
    const bareWin = await foreign({ contextIsolation: true, nodeIntegration: false, sandbox: true, webSecurity: true });
    const bareDrift = guardDrift(ref, bareWin);
    expect(bareDrift.some((d) => d === "a navigation was not refused") || bareDrift.some((d) => d === "a new window was not refused"), `a window without the refusals fails: ${bareDrift.join("; ")}`).toBe(true);
    await app.evaluate(() => (globalThis as unknown as { __px203Foreign?: Electron.BrowserWindow[] }).__px203Foreign?.forEach((w) => !w.isDestroyed() && w.destroy()));
    await expect.poll(() => windowCount(app)).toBe(3);
    // The pane windows were made by the factory and the foreign windows were refused by main on every channel.
    const foreignAsk = (await page.evaluate(() => window.modbit.debugIpcRefusals())).filter((r) => r.reason === "not the app window");
    expect(foreignAsk.length, "the foreign windows' bridges were refused by main").toBeGreaterThan(0);

    // ---- The quit question counts what the pane windows show; keeping Modbit open keeps them.
    await expectTaskState(page, taskId, "Running");
    await recordDialog(app, 0);
    await app.evaluate(({ app: a }) => a.quit());
    await expect.poll(async () => ((await asked(app)) ?? []).length).toBe(1);
    const q = (await asked(app))[0]!;
    expect(q.message, "the running task and terminal are counted where they are shown").toBe("1 task is running (shown in a pane window) and 1 background terminal is running (shown in a pane window). Quit anyway?");
    expect(q.detail).toContain("The 2 pane windows close with Modbit; their panes are back in the dock when you open Modbit again.");
    expect(q.buttons).toEqual(["Keep Modbit open", "Quit and resume on reopen", "Quit and leave suspended"]);
    expect(await windowCount(app), "keeping Modbit open keeps the pane windows").toBe(3);
    if (SHOTS) {
      mkdirSync(SHOTS, { recursive: true });
      // The question is the main process's native dialog, not a page: what main asked it to show is the evidence.
      writeFileSync(join(SHOTS, "quit-question.txt"), `${q.message}\n\n${q.detail}\n\n[${q.buttons.join("] [")}]\n`);
    }

    // ---- No approval is decided from a pane window. The approval the task asks for next is pending in the Core.
    hold.open();
    const card = page.getByTestId("approval-card");
    await expect(card).toBeVisible({ timeout: 90_000 });
    const pending = (await pendingOf(page, sessionId)).find((p) => p.taskId === taskId)!;
    expect(pending, "the Core lists the approval as pending").toBeTruthy();
    // Neither pane window draws an approval card or anything that answers one.
    for (const w of [termWin, changesWin]) await expect(w.getByTestId("approval-card")).toHaveCount(0);
    const attempts: { channel: string; outcome: string }[] = [];
    for (const w of [termWin, changesWin]) {
      const tries = await w.evaluate(
        async ([s, t, a, h]) => {
          const out: { channel: string; outcome: string }[] = [];
          const run = async (channel: string, f: () => Promise<unknown>) => {
            try {
              out.push({ channel, outcome: `answered ${JSON.stringify(await f())}` });
            } catch (e) {
              out.push({ channel, outcome: (e as Error).message });
            }
          };
          await run("approval:resolve", () => window.modbit.resolveApproval(s!, a!, true, "from a pane window", h!));
          await run("approval:allowAlways", () => window.modbit.allowAlways(s!, a!, h!, { prefix: ["git"], scope: "TASK" } as never));
          await run("question:respond", () => window.modbit.respondToQuestion(s!, t!, "q", "allow", ""));
          await run("modeproposals:decide", () => window.modbit.decideModeProposal(s!, t!, "p", true));
          await run("task:cancel", () => window.modbit.cancelTask(s!, t!));
          await run("changes:state (another task)", () => window.modbit.changesState("f".repeat(32)));
          return out;
        },
        [sessionId, taskId, pending.approvalId, pending.intentHash] as const,
      );
      attempts.push(...tries);
    }
    for (const a of attempts) expect(a.outcome, `${a.channel} from a pane window is refused by main`).toContain("SENDER_REFUSED");
    expect(await decidedFromPane(attempts, page, sessionId, pending.approvalId), "the approval is still the Core's to ask").toEqual([]);
    const refusals = (await page.evaluate(() => window.modbit.debugIpcRefusals())).filter((r) => r.reason.startsWith("pane window:"));
    expect(new Set(refusals.map((r) => r.channel)), "main recorded each refusal").toEqual(new Set(["approval:resolve", "approval:allowAlways", "question:respond", "modeproposals:decide", "task:cancel", "changes:state"]));
    await expect(card, "the card is still in the app's window").toBeVisible();
    await shots(termWin, "terminal-popped-out");
    await shots(changesWin, "changes-popped-out");
    await shots(page, "dock-without-them");

    // ---- Accessibility of each pane window, light and dark.
    for (const [name, w] of [["the Terminal window", termWin], ["the Changes window", changesWin]] as const) {
      for (const theme of ["light", "dark"] as const) {
        await setTheme(w, theme);
        await accessible(w, `${name} (${theme})`);
      }
      await setTheme(w, "light");
    }

    // ---- The app's window reloads: the pane windows go on (their terminal attachment is theirs) and the dock still leaves them out.
    const ticksBefore = lastTick((await wScreen.textContent()) ?? "");
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openTask(page, taskId);
    await expect(page.getByTestId("pane-toggle-terminal")).toHaveAttribute("data-state", "popped");
    await expect(page.getByTestId("pane-toggle-changes")).toHaveAttribute("data-state", "popped");
    await expect(page.getByTestId("pane-terminal")).toHaveCount(0);
    await expect.poll(async () => lastTick((await wScreen.textContent()) ?? ""), { timeout: 30_000, message: "the Terminal window goes on receiving output" }).toBeGreaterThan(ticksBefore);
    expect(await bothPlaces(app, page)).toEqual([]);

    // ---- The approval is answered where approvals are: the app window's card. The check above now fails, as it must.
    await expect(card).toBeVisible({ timeout: 30_000 });
    await card.getByTestId("approval-run").click();
    if (await card.getByTestId("approval-confirm").isVisible().catch(() => false)) await card.getByTestId("approval-confirm-run").click();
    await expect.poll(async () => (await decidedFromPane([], page, sessionId, pending.approvalId)).length, { message: "a decided approval fails the check" }).toBeGreaterThan(0);
    await expect.poll(() => git(repo, "remote", "-v").includes(bare), { timeout: 60_000, message: "the remote was added under the approval" }).toBe(true);
    await expectTaskState(page, taskId, "ReadyForReview", 90_000);

    // ---- Closing each window brings its pane back to the dock as it was: the Terminal by its own Back to the dock, Changes by the window's close.
    await clickAndClose(termWin, "pane-return");
    await expect.poll(() => windowCount(app)).toBe(2);
    await expect(page.getByTestId("pane-terminal")).toBeVisible();
    await expect(page.getByTestId("pane-toggle-terminal")).toHaveAttribute("data-state", "open");
    const back = page.getByTestId("pane-terminal");
    await expect(back.getByTestId("terminal-view")).toHaveAttribute("data-terminal-id", ticker.terminalId);
    await expect(back.getByTestId("terminal-screen")).toContainText("got:in-its-window", { timeout: 30_000 });
    const again = { core: await coreTerminal(page, taskId, ticker.terminalId), tick: lastTick((await back.getByTestId("terminal-screen").textContent()) ?? ""), marker: "got:in-its-window" };
    await expect.poll(async () => sessionLost({ ...again, tick: again.tick - 1, core: { ...again.core, bytesSoFar: String(BigInt(again.core.bytesSoFar) - 1n) } }, await afterOf(page)), { timeout: 30_000, message: "the returned terminal is the same running session" }).toEqual([]);
    await back.getByTestId("terminal-screen").click();
    await page.keyboard.type("back-in-the-dock");
    await page.keyboard.press("Enter");
    await expect(back.getByTestId("terminal-screen")).toContainText("got:back-in-the-dock", { timeout: 30_000 });
    await page.keyboard.press("Shift+Escape");
    await changesWin.locator('[data-testid="changes-file"][data-path="list.txt"]').click();
    const lastFile = (await changesWin.getByTestId("changes-diff").getAttribute("data-path"))!;
    expect(lastFile).toBe("list.txt");
    await changesWin.getByTestId("changes-scroll").evaluate((el) => (el.scrollTop = 500));
    const lastScroll = await changesWin.getByTestId("changes-scroll").evaluate((el) => el.scrollTop);
    await changesWin.waitForTimeout(400);
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().find((w) => w.getTitle().startsWith("Changes"))!.close());
    await expect.poll(() => windowCount(app)).toBe(1);
    await expect(page.getByTestId("pane-changes")).toBeVisible();
    await expect(page.getByTestId("pane-changes").getByTestId("changes-diff")).toHaveAttribute("data-path", lastFile);
    await expect.poll(() => page.getByTestId("pane-changes").getByTestId("changes-scroll").evaluate((el) => el.scrollTop), { message: "the list is scrolled as it was in the window" }).toBeGreaterThan(lastScroll - 40);
    expect(await bothPlaces(app, page)).toEqual([]);
    await shots(page, "returned");
    // Pop out is offered again on both, and the dock holds what it held.
    for (const kind of ["changes", "terminal"]) await expect(page.getByTestId(`pane-popout-${kind}`)).toBeVisible();

    // ---- Commit and push from the Changes window: the approval is answered on the app window's card, and the commit goes on in the window.
    const commitWin = await nextPaneWindow(app, "changes", () => page.getByTestId("pane-popout-changes").click());
    await expect(commitWin.getByTestId("changes-diff"), "popped out again, it shows the file the dock showed").toHaveAttribute("data-path", "list.txt", { timeout: 30_000 });
    await expect(commitWin.getByTestId("changes-stage-all")).toBeEnabled({ timeout: 60_000 });
    await commitWin.getByTestId("changes-stage-all").click();
    await commitWin.getByTestId("changes-commit-message").fill("Tidy the notes and add the list");
    await commitWin.getByTestId("changes-commit-push").click();
    const waiting = commitWin.getByTestId("changes-approval");
    await expect(waiting).toBeVisible({ timeout: 30_000 });
    await expect(commitWin.getByTestId("changes-approve"), "the window draws no answer to the approval").toHaveCount(0);
    await expect(commitWin.getByTestId("changes-deny")).toHaveCount(0);
    await expect(commitWin.getByTestId("changes-approval-elsewhere")).toBeVisible();
    const commitApproval = (await waiting.getAttribute("data-approval-id"))!;
    const tried = await commitWin.evaluate(([s, a, h]) => window.modbit.resolveApproval(s!, a!, true, "from the Changes window", h!).then(() => "answered", (e: Error) => e.message), [sessionId, commitApproval, (await waiting.getAttribute("data-intent-hash"))!] as const);
    expect(tried, "deciding it from the window is refused by main").toContain("SENDER_REFUSED");
    expect(git(bare, "branch", "--list").trim(), "nothing was pushed before the approval").toBe("");
    await commitWin.getByTestId("changes-show-approval").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
    await expect(card).toBeVisible({ timeout: 30_000 });
    await card.getByTestId("approval-run").click();
    // A push cannot be undone, so the card asks twice (AWS-04). The second question renders after the click: wait for it
    // or for the card to go, instead of looking once and moving on before it is drawn.
    await expect
      .poll(async () => ((await card.getByTestId("approval-confirm").isVisible().catch(() => false)) ? "confirm" : (await card.isVisible().catch(() => false)) ? "waiting" : "gone"), { message: "the card asks twice for a push, or is answered", timeout: 15_000 })
      .not.toBe("waiting");
    if (await card.getByTestId("approval-confirm").isVisible().catch(() => false)) await card.getByTestId("approval-confirm-run").click();
    await expect(commitWin.getByTestId("changes-result"), "the commit went on in the window once allowed").toHaveAttribute("data-status", "COMPLETED", { timeout: 60_000 });
    expect(git(bare, "branch", "--list").trim(), "the branch is on the remote").not.toBe("");
    await clickAndClose(commitWin, "pane-return");
    await expect.poll(() => windowCount(app)).toBe(1);
    await expect(page.getByTestId("pane-changes").getByTestId("changes-empty"), "the dock's Changes reads the committed state").toBeVisible({ timeout: 30_000 });

    // ---- Failure: a pane window whose renderer dies closes, and its pane comes back to the dock.
    const evidenceWin = await nextPaneWindow(app, "evidence", () => page.getByTestId("pane-popout-evidence").click());
    await expect(evidenceWin.getByTestId("app-evidence")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("pane-evidence")).toHaveCount(0);
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().find((w) => w.getTitle().startsWith("Evidence"))!.webContents.forcefullyCrashRenderer());
    await expect.poll(() => windowCount(app), { timeout: 30_000 }).toBe(1);
    await expect(page.getByTestId("pane-evidence"), "the pane is back in the dock").toBeVisible();
    await expect(page.getByTestId("core-status")).toContainText("Core connected");

    // ---- Quitting closes every pane window.
    await nextPaneWindow(app, "terminal", () => page.getByTestId("pane-popout-terminal").click());
    expect(await windowCount(app)).toBe(2);
    await recordDialog(app, 1);
    const proc = app.process();
    const done = exited(app);
    await app.evaluate(({ app: a }) => a.quit());
    await done;
    quit = true;
    expect(await asked(app).catch(() => null), "the app is gone").toBeNull();
    expect(proc.exitCode, "it quit, and every pane window with it").toBe(0);
  } finally {
    hold.open();
    if (!quit) await closeApp(app);
    model.server.close();
  }
});
