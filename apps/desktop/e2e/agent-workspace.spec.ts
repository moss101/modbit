/**
 * Wave 3 together (REQ-PX-046..049), on the real stack: the packaged Electron
 * app, a real Core, a real `modbit-execd` broker and PTY, a real Git checkout
 * and a scripted OpenAI-compatible model (the model stand-in only). One task
 * is started from the UI; it appears in the agent list; selecting it shows its
 * conversation streaming in the centre AND its apps panel (Terminal, Changes,
 * Evidence) beside it; and quitting with the task running asks first.
 * Every wait is for a condition the Core or the page reports.
 */
import { expect, test, type ElectronApplication } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ECHO_LOOP, expectTerminals, gate, nodeTerminal, steppedModel, type Step } from "./support/apps-harness.ts";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

async function recordDialog(app: ElectronApplication, answer: number): Promise<void> {
  await app.evaluate(({ dialog }, response) => {
    const g = globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[] }[] };
    g.__asked = [];
    (dialog as unknown as { showMessageBox: unknown }).showMessageBox = async (...args: unknown[]) => {
      const o = (args.find((a) => typeof a === "object" && a !== null && "message" in (a as object)) ?? {}) as { message: string; detail: string; buttons: string[] };
      g.__asked.push({ message: o.message, detail: o.detail, buttons: o.buttons });
      return { response, checkboxChecked: false };
    };
  }, answer);
}
const asked = (app: ElectronApplication) => app.evaluate(() => (globalThis as unknown as { __asked: { message: string; detail: string; buttons: string[] }[] }).__asked);
const exited = (app: ElectronApplication) => new Promise<void>((resolve) => (app.process().exitCode !== null ? resolve() : app.process().once("exit", () => resolve())));

test("the agent list, the conversation and the apps panel work together on one running task, and quitting with it running asks first", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-w3-repo-")));
  const hold = gate();
  const script: Step[] = [
    { text: "Looking at the notes first.", calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } }] },
    { text: "Edited the notes; a shell is waiting for input.", calls: [nodeTerminal(ECHO_LOOP)] },
    async () => {
      await hold.wait;
      return { text: "Nothing further." };
    },
  ];
  const { server, url } = await steppedModel(script);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-w3-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url, MODBIT_QUIT_PROMPT: "" } });
  let quitDone = false;
  try {
    await setContentSize(app, page, 1280, 800);

    // Start a task from the UI. A draft has no artifact, so there is no apps panel, even with its conversation open.
    await page.getByTestId("goal").fill("Edit the notes");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    const taskId = (await card.getAttribute("data-task-id"))!;

    // It appears in the agent list, as a Draft by the Core's class; selecting it opens its conversation in the centre.
    const row = page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-class", "DRAFT");
    await row.click();
    const conversation = page.getByTestId("conversation");
    await expect(conversation).toHaveAttribute("data-task-id", taskId);
    await expect(row).toHaveAttribute("data-selected", "true");
    expect(await page.getByTestId("region-apps").count(), "the panel is absent until the task has an artifact").toBe(0);

    // Start it (the conversation's own control). The agent list follows the Core's class, the conversation streams.
    await page.getByTestId("conv-start").click();
    await expect(row).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });
    await expect(conversation.getByTestId("conv-assistant").filter({ hasText: "Looking at the notes first." }).first()).toBeVisible({ timeout: 60_000 });
    await expect(conversation.getByTestId("conv-assistant").filter({ hasText: "Edited the notes; a shell is waiting for input." }).first()).toBeVisible({ timeout: 60_000 });
    await expect(conversation.getByTestId("conv-tail")).toBeVisible();

    // The same selection drives the apps panel: it appears once there is an artifact, for this task, with the conversation still in the centre.
    await expectTerminals(page, taskId, 1);
    await page.getByTestId("toggle-apps-panel").click();
    const panel = page.getByTestId("region-apps");
    await expect(panel).toBeVisible();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", taskId);
    await expect(conversation).toBeVisible();
    await expect(conversation).toHaveAttribute("data-task-id", taskId);

    // Terminal: the command the agent ran, live, and the person can type into it.
    await page.getByTestId("tab-terminal").click();
    const termRow = page.getByTestId("terminal-row").first();
    await expect(termRow.getByTestId("terminal-state")).toContainText("running", { timeout: 30_000 });
    await termRow.getByTestId("terminal-pick").click();
    await expect(page.getByTestId("terminal-view")).toBeVisible();
    await expect(page.getByTestId("terminal-screen")).toContainText("READY", { timeout: 30_000 });

    // Changes: the live diff of the agent's edit while the task is still running.
    await page.getByTestId("tab-changes").click();
    await expect(page.getByTestId("app-changes")).toHaveAttribute("data-task-state", "Running");
    await expect(page.getByTestId("changes-file")).toHaveCount(1);
    await expect(page.getByTestId("changes-hunk")).toContainText("line 2 changed");

    // Evidence: the receipts shown are the Core's own list for this task.
    await page.getByTestId("tab-evidence").click();
    await expect(page.getByTestId("app-evidence")).toBeVisible();
    await expect(page.getByTestId("evidence-receipts")).toBeVisible({ timeout: 30_000 });
    const fromCore = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    await expect(page.getByTestId("evidence-receipt")).toHaveCount(fromCore.receipts.length);
    await accessible(page, "agent list, conversation and apps panel together", '[data-testid="shell"]');

    // Per-task tab memory holds with the conversation: leave to the Fleet board and come back, the task's tab is as it was.
    await page.getByTestId("tab-changes").click();
    await page.getByTestId("agents-fleet").click();
    await expect(conversation).toHaveCount(0);
    await row.click();
    await expect(conversation).toHaveAttribute("data-task-id", taskId);
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", taskId);
    await expect(page.getByTestId("app-changes")).toBeVisible();

    // Quit with the task running: the question names it and the default answer keeps everything.
    await recordDialog(app, 0);
    await app.evaluate(({ app: a }) => a.quit());
    await expect.poll(async () => (await asked(app)).length).toBe(1);
    const q = (await asked(app))[0]!;
    expect(q.message).toContain("1 task is running");
    expect(q.buttons[0]).toBe("Keep Modbit open");
    await expect(page.getByTestId("core-status")).toContainText("Core connected");
    await expect(row).toHaveAttribute("data-class", "RUNNING");
    expect(app.process().exitCode).toBeNull();

    await recordDialog(app, 1);
    const done = exited(app);
    await app.evaluate(({ app: a }) => a.quit());
    await done;
    quitDone = true;
  } finally {
    hold.open();
    if (!quitDone) await closeApp(app);
    server.close();
  }
});
