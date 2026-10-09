/**
 * REQ-PX-049 on the real stack: the packaged Electron app and a real Core.
 * The window's minimum size, the quit question when a task is running (the
 * native dialog is replaced in the main process by a recorder, because a test
 * cannot press a native button; everything it is asked is real), what the
 * question says quitting does, and the recovery counts a restart shows.
 */
import { expect, test, type ElectronApplication } from "@playwright/test";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { gate, steppedModel, type Step } from "./support/apps-harness.ts";

/** Replaces the main process's message box with a recorder that answers `answer`. */
async function recordDialog(app: ElectronApplication, answer: number): Promise<void> {
  await app.evaluate(({ dialog }, response) => {
    const g = globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[]; type: string }[] };
    g.__asked = [];
    (dialog as unknown as { showMessageBox: unknown }).showMessageBox = async (...args: unknown[]) => {
      const o = (args.find((a) => typeof a === "object" && a !== null && "message" in (a as object)) ?? {}) as { message: string; detail: string; buttons: string[]; type: string };
      g.__asked.push({ message: o.message, detail: o.detail, buttons: o.buttons, type: o.type });
      return { response, checkboxChecked: false };
    };
  }, answer);
}
const asked = (app: ElectronApplication) => app.evaluate(() => (globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[] }[] }).__asked);
const exited = (app: ElectronApplication) => new Promise<void>((resolve) => (app.process().exitCode !== null ? resolve() : app.process().once("exit", () => resolve())));

test("PX-049: the window opens at 1280 x 800 and cannot be made smaller than 900 x 600", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-minsize-"));
  const { app } = await launch(dataDir);
  try {
    const got = await app.evaluate(({ BrowserWindow, screen }) => {
      const w = BrowserWindow.getAllWindows()[0]!;
      const initial = w.getSize();
      // The frame (border and shadow on Windows, none on macOS and Linux) is constant: the window size less the content size.
      const initialContent = w.getContentSize();
      const frame: [number, number] = [initial[0]! - initialContent[0]!, initial[1]! - initialContent[1]!];
      const workArea = screen.getDisplayMatching(w.getBounds()).workAreaSize;
      w.setSize(300, 200);
      const clamped = w.getSize();
      w.setContentSize(100, 100);
      return { minimum: w.getMinimumSize(), initial, workArea, clamped, content: w.getContentSize(), frame };
    });
    expect(got.minimum).toEqual([900, 600]);
    // 1280 x 800, unless the screen's work area is smaller (a CI runner's
    // 1024 x 768 display): the OS fits the window to it, and nothing else may.
    expect(got.initial, `work area ${got.workArea.width} x ${got.workArea.height}`).toEqual([
      Math.min(1280, got.workArea.width),
      Math.min(800, got.workArea.height),
    ]);
    expect(got.clamped[0]).toBeGreaterThanOrEqual(900);
    expect(got.clamped[1]).toBeGreaterThanOrEqual(600);
    // The 900 x 600 minimum applies to the window including its frame (on Windows the content area is ~16 px
    // narrower), so the exact bound on the content is the minimum less the frame.
    expect(got.content[0], `a content size below the minimum is clamped too (frame ${got.frame[0]} x ${got.frame[1]})`).toBeGreaterThanOrEqual(900 - got.frame[0]);
    expect(got.content[1]).toBeGreaterThanOrEqual(600 - got.frame[1]);
  } finally {
    await closeApp(app);
  }
});

test("PX-049: quitting with nothing running does not ask", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-quit-idle-"));
  const { app } = await launch(dataDir, { env: { MODBIT_QUIT_PROMPT: "" } });
  await recordDialog(app, 0);
  const proc = app.process();
  const done = exited(app);
  await app.evaluate(({ app: a }) => a.quit());
  await done;
  expect(proc.exitCode, "it quit by itself, without a question").toBe(0);
});

test("PX-049: quitting or closing the window with a task running asks first, says what quitting does, and keeps the app open on the default answer", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-quit-repo-")));
  const hold = gate();
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    async () => {
      await hold.wait;
      return { text: "Nothing further." };
    },
  ];
  const { server, url } = await steppedModel(script);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-quit-run-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url, MODBIT_QUIT_PROMPT: "" } });
  let quitDone = false;
  try {
    await setContentSize(app, page, 1280, 800);
    await page.getByTestId("goal").fill("Edit the notes");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await card.getByTestId("task-start").click();
    await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });

    // Quit (the application menu, Cmd+Q): the question names the running task and the truth about what quitting does.
    await recordDialog(app, 0);
    await app.evaluate(({ app: a }) => a.quit());
    await expect.poll(async () => (await asked(app)).length).toBe(1);
    const first = (await asked(app))[0]!;
    expect(first.message).toContain("1 task is running");
    expect(first.detail).toContain("stops the local Core");
    expect(first.detail).toContain("suspended at their last saved step and recovered the next time you open Modbit");
    expect(first.detail).toMatch(/nothing continues while Modbit is closed/i);
    expect(first.buttons[0]).toBe("Keep Modbit open");
    // The default answer keeps everything as it was: the window, the Core and the run.
    await expect(page.getByTestId("core-status")).toContainText("Core connected");
    await expect(card.getByTestId("task-state")).toHaveText("Running");
    expect(app.process().exitCode).toBeNull();

    // Closing the window asks the same question.
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.close());
    await expect.poll(async () => (await asked(app)).length).toBe(2);
    expect((await asked(app))[1]!.message).toContain("1 task is running");
    await expect(page.getByTestId("core-status")).toContainText("Core connected");

    // Choosing to quit quits, and does not wait on anything past the shutdown deadline.
    await recordDialog(app, 1);
    const done = exited(app);
    const t0 = Date.now();
    await app.evaluate(({ app: a }) => a.quit());
    await done;
    quitDone = true;
    expect(Date.now() - t0, "the quit is not held up by shutdown").toBeLessThan(15_000);
    expect(existsSync(join(dataDir, "shutdown-log.json")), "a clean shutdown leaves no failure log").toBe(false);
  } finally {
    hold.open();
    if (!quitDone) await closeApp(app);
    server.close();
  }
});

test("PX-049: a restart after a quit with a task running shows the Core's recovery counts, and the task is there to resume", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-recover-repo-")));
  const hold = gate();
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    async () => {
      await hold.wait;
      return { text: "Nothing further." };
    },
  ];
  const { server, url } = await steppedModel(script);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-recovery-"));
  let { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url, MODBIT_QUIT_PROMPT: "" } });
  try {
    await page.getByTestId("goal").fill("Edit the notes");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await card.getByTestId("task-start").click();
    await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
    expect(await page.getByTestId("recovery-summary").count(), "a first start has nothing to recover").toBe(0);

    // Stop the app the way a crash would: the Core dies with the task running, nothing was asked.
    app.process().kill("SIGKILL");
    await new Promise<void>((r) => (app.process().exitCode !== null || app.process().signalCode !== null ? r() : app.process().once("exit", () => r())));
    hold.open();

    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url, MODBIT_QUIT_PROMPT: "" } }));
    await expect(page.getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    const summary = page.getByTestId("recovery-summary");
    await expect(summary).toBeVisible({ timeout: 30_000 });
    const text = (await summary.textContent())!;
    expect(text).toMatch(/^Recovered at start: (\d+ tasks? resumed|\d+ needs? attention|\d+ recovered idle)/);
    // The counts are the Core's: what the summary says needs attention is what the Needs Attention column shows.
    const attention = await page.getByTestId("column-needsAttention").getByTestId("task-card").count();
    const running = await page.getByTestId("column-running").getByTestId("task-card").count();
    const claimed = (re: RegExp) => Number(re.exec(text)?.[1] ?? 0);
    expect(claimed(/(\d+) needs? attention/)).toBe(attention);
    expect(claimed(/(\d+) tasks? resumed/)).toBe(running);
    expect(readShutdownLog(dataDir)).toBeNull();
  } finally {
    hold.open();
    await closeApp(app);
    server.close();
  }
});

function readShutdownLog(dataDir: string): unknown {
  const f = join(dataDir, "shutdown-log.json");
  return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null;
}
