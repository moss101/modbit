/**
 * Packaged-app E2E (docs/32 "Frontend completion gate"): Playwright drives the
 * real Electron app, which spawns the real modbit-core. Verifies the New Task
 * flow renders from durable Core ids, the Fleet reduces Core events, the
 * degraded/recovery states appear when the Core is hard-killed, and state
 * survives a full app restart.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

async function launch(dataDir: string): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin } });
  const page = await app.firstWindow();
  // Diagnosability: the Electron main process relays the Core's stderr; keep it in the test output.
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  app.process().stdout?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  try {
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  } catch (e) {
    const status = await page.evaluate(() => window.modbit.coreStatus()).catch(() => null);
    throw new Error(`Core did not connect: ${JSON.stringify(status)}; ${(e as Error).message}`);
  }
  return { app, page };
}

test("new task renders from Core ids, fleet reduces Core events, and survives Core kill and app restart", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-"));
  const { app, page } = await launch(dataDir);

  // Empty state first.
  await expect(page.getByTestId("fleet-empty")).toBeVisible();

  // New Task: CreateSession then CreateTask; the card appears with the durable id.
  await page.getByTestId("goal").fill("Make the flaky test deterministic");
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  await expect(card).toBeVisible();
  await expect(card).toContainText("Make the flaky test deterministic");
  await expect(card.getByTestId("task-state")).toHaveText("Queued");
  const taskId = await card.getAttribute("data-task-id");
  expect(taskId).toMatch(/^[0-9a-f]{32}$/);
  // Queued tasks sit in Waiting; nothing is invented as Running or Completed.
  await expect(page.getByTestId("column-waiting").getByTestId("task-card")).toHaveCount(1);
  await expect(page.getByTestId("column-running").getByTestId("task-card")).toHaveCount(0);
  await expect(page.getByTestId("column-completed").getByTestId("task-card")).toHaveCount(0);

  // REQ-EV-0190: attach a real PNG through the desktop; the Core normalizes it to the
  // canonical MediaEnvelope (same as a workspace read) and the card reflects the event.
  const png = resolve(appDir, "..", "..", "tests", "fixtures", "media", "label.png");
  const attached = await page.evaluate(async ([tid, path]) => {
    const s = await window.modbit.localState();
    return window.modbit.attachFile(s.sessionId!, tid!, path!);
  }, [taskId, png]);
  expect(attached.kind).toBe("IMAGE");
  expect(attached.mime).toBe("image/png");
  expect(attached.contentRef).toMatch(/^[0-9a-f]{64}$/);
  await expect(card.getByTestId("task-attachments")).toHaveText("1");
  const attachedAgain = await page.evaluate(async ([tid, path]) => {
    const s = await window.modbit.localState();
    return window.modbit.attachFile(s.sessionId!, tid!, path!);
  }, [taskId, png]);
  expect(attachedAgain.replayed).toBe(true);
  await expect(card.getByTestId("task-attachments")).toHaveText("1");

  // Degraded state: hard-kill the Core; the banner shows; main respawns; recovery banner shows;
  // the task is still there because the Core persisted it before answering.
  // PX-026: the honest language labels are visible in the desktop.
  await expect(page.getByTestId("language-labels")).toContainText("Alpha baseline", { timeout: 15_000 });
  await expect(page.getByTestId("language-labels")).toContainText("python");
  const info = await page.evaluate(() => window.modbit.debugCoreInfo());
  expect(info?.pid).toBeGreaterThan(0);
  process.kill(info!.pid, "SIGKILL");
  await expect(page.getByTestId("banner-restarting")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 30_000 });
  await expect(page.getByTestId("banner-recovered")).toBeVisible({ timeout: 15_000 });
  // The recovery banner states what the Core actually recovered (docs/39): one session, one task, chain verified.
  await expect(page.getByTestId("banner-recovered")).toContainText("recovered 1 session(s) and 1 task(s)", { timeout: 15_000 });
  await expect(page.getByTestId("banner-recovered")).toContainText("nothing left to reconcile");
  await expect(page.getByTestId("task-card")).toHaveCount(1);
  await expect(page.getByTestId("task-card").first()).toHaveAttribute("data-task-id", taskId!);

  // A second task after recovery works against the respawned Core.
  await page.getByTestId("goal").fill("Second task after recovery");
  await page.getByTestId("run").click();
  await expect(page.getByTestId("task-card")).toHaveCount(2);
  await app.close();

  // Full app restart: both tasks come back from the Core projection, not from the renderer.
  const again = await launch(dataDir);
  await expect(again.page.getByTestId("task-card")).toHaveCount(2);
  const ids = await again.page.getByTestId("task-card").evaluateAll((els) => els.map((e) => e.getAttribute("data-task-id")));
  expect(ids).toContain(taskId);
  await expect(again.page.getByTestId("column-waiting").getByTestId("task-card")).toHaveCount(2);
  await again.app.close();
});

test("renderer messages with invalid arguments are rejected by main (REQ-EV-0103)", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-"));
  const { app, page } = await launch(dataDir);
  const err = await page.evaluate(async () => {
    try {
      await window.modbit.createTask("not-a-session-id", "x", "y");
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  });
  expect(err).toContain("BAD_ARGUMENT");
  const err2 = await page.evaluate(async () => {
    try {
      await window.modbit.sessionSnapshot("00");
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  });
  expect(err2).toContain("BAD_ARGUMENT");
  await app.close();
});
