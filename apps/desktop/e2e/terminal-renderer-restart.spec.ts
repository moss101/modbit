/**
 * PX-043 on the real stack: a long-running agent terminal survives the
 * renderer's death. The packaged Electron app, a real Core, a real
 * `modbit-execd` and a real PTY; the scripted model holds the task Running.
 * The renderer's process is crashed under it; main opens a replacement
 * window; the terminal is the same handle in the same Core (same pid), still
 * running, its screen is rebuilt from the Core's replay and the new view
 * types into the same live process.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ECHO_LOOP, expectTerminals, gate, nodeTerminal, steppedModel, type Step } from "./support/apps-harness.ts";
import { closeApp, launch, makeRepo, MOD } from "./support/ui-harness.ts";

test("a background terminal survives a crashed renderer: same handle, same Core, replayed screen, live input", async () => {
  const modelGate = gate();
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "keep a terminal", expected_files: ["notes.txt"] } }] },
    { calls: [nodeTerminal(ECHO_LOOP)] },
    async () => {
      await modelGate.wait;
      return { calls: [{ name: "task.complete", args: { summary: "done", self_review: { findings: [] } } }] };
    },
  ];
  const model = await steppedModel(script);
  const repo = mkdtempSync(join(tmpdir(), "modbit-term-restart-repo-"));
  makeRepo(repo);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-term-restart-"));
  const { app, page: page1 } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await page1.getByTestId("goal").fill("Keep a terminal running");
    await page1.getByTestId("workspace").fill(repo);
    await page1.getByTestId("run").click();
    const card = page1.getByTestId("task-card").first();
    const taskId = (await card.getAttribute("data-task-id"))!;
    await card.getByTestId("task-start").click();
    await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
    await expectTerminals(page1, taskId, 1);
    const before = (await page1.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals), taskId))[0]!;
    const coreBefore = await page1.evaluate(() => window.modbit.debugCoreInfo());

    // Open the terminal, wait for the child's banner, then type once so the log holds a line the new view can only get from the Core.
    await card.focus();
    await page1.keyboard.press(`${MOD}+e`);
    await page1.getByTestId("tab-terminal").click();
    await page1.locator(`[data-testid="terminal-row"][data-terminal-id="${before.terminalId}"] [data-testid="terminal-pick"]`).click();
    await expect(page1.getByTestId("terminal-screen")).toContainText("READY", { timeout: 30_000 });
    await page1.getByTestId("terminal-screen").click();
    await page1.keyboard.type("before-crash");
    await page1.keyboard.press("Enter");
    await expect(page1.getByTestId("terminal-screen")).toContainText("got:before-crash", { timeout: 30_000 });

    // The renderer's process is killed under it.
    const old = await app.evaluate(({ BrowserWindow }) => {
      const wc = BrowserWindow.getAllWindows()[0]!.webContents;
      return { webContentsId: wc.id, pid: wc.getOSProcessId() };
    });
    expect(old.pid).toBeGreaterThan(0);
    if (process.platform === "win32") process.kill(old.pid);
    else await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.webContents.forcefullyCrashRenderer());
    await expect
      .poll(() => app.evaluate(({ BrowserWindow }, id) => BrowserWindow.getAllWindows().filter((w) => !w.isDestroyed() && w.webContents.id !== id).length, old.webContentsId), { timeout: 60_000, message: "main never opened a replacement window" })
      .toBe(1);
    const handedOver = () => app.windows().filter((p) => p !== page1 && !p.isClosed());
    await expect.poll(() => handedOver().length, { timeout: 60_000, message: "Playwright never exposed the replacement page" }).toBe(1);
    const page: Page = handedOver()[0]!;
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });

    // The same Core, the same terminal handle, still running, nothing lost from its log.
    const coreAfter = await page.evaluate(() => window.modbit.debugCoreInfo());
    expect(coreAfter?.pid).toBe(coreBefore?.pid);
    const after = (await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals), taskId))[0]!;
    expect(after.terminalId).toBe(before.terminalId);
    expect(after.state).toBe(before.state);
    expect(BigInt(after.bytesSoFar)).toBeGreaterThanOrEqual(BigInt(before.bytesSoFar));

    // The new view: reopen the panel if the crash closed it, pick the terminal; the screen is rebuilt from the Core's replay.
    if ((await page.getByTestId("region-apps").count()) === 0) {
      await page.getByTestId("task-card").first().focus();
      await page.keyboard.press(`${MOD}+e`);
    }
    await page.getByTestId("tab-terminal").click();
    await page.locator(`[data-testid="terminal-row"][data-terminal-id="${before.terminalId}"] [data-testid="terminal-pick"]`).click();
    const screen = page.getByTestId("terminal-screen");
    await expect(screen).toContainText("got:before-crash", { timeout: 30_000 });
    await expect(screen).toContainText("READY");

    // And it is the same live process: typing in the new view reaches it.
    await screen.click();
    await page.keyboard.type("after-crash");
    await page.keyboard.press("Enter");
    await expect(screen).toContainText("got:after-crash", { timeout: 30_000 });
  } finally {
    modelGate.open();
    await closeApp(app);
    model.server.close();
  }
});
